use lumeryn_core::{
    Project, Result,
    media::{Metadata, Video},
    model::{Clip, Media, TransitionEdge},
    time_effects::TransitionKind,
};
use uuid::Uuid;
#[test]
fn transition_graph_proves_handles_and_rejects_unsupported_combinations() -> Result<()> {
    let mut p = Project::new("Cut".into(), 64, 64, "30".parse()?)?;
    let mid = Uuid::new_v4();
    p.media.push(Media {
        id: mid,
        path: "source.mkv".into(),
        alias: None,
        metadata: Metadata {
            size_bytes: 4,
            blake3: Some("a".repeat(64)),
            audio: None,
            video: Some(Video {
                codec: "ffv1".into(),
                width: 64,
                height: 64,
                frame_rate: p.frame_rate,
                frames: 20,
                start_time_micros: 0,
                sample_aspect_ratio: "1:1".into(),
                color_transfer: None,
                constant_rate: true,
            }),
        },
    });
    for at in [0, 6] {
        p.clips.push(Clip {
            id: Uuid::new_v4(),
            media_id: mid,
            track_id: p.video_track_id,
            at_frame: at,
            source_in_frame: 2,
            duration_frames: 6,
        });
    }
    p.transitions.push(TransitionEdge {
        id: Uuid::new_v4(),
        outgoing_clip_id: p.clips[0].id,
        incoming_clip_id: p.clips[1].id,
        kind: TransitionKind::CrossDissolve,
        duration_frames: 5,
    });
    p.validate()?;
    assert!(p.uses_composition());
    p.clips[1].source_in_frame = 1;
    assert!(p.validate().is_err());
    p.clips[1].source_in_frame = 2;
    p.clips[1].at_frame = 7;
    assert!(p.validate().is_err());
    p.clips[1].at_frame = 6;
    p.transitions[0].id = p.clips[0].id;
    assert!(p.validate().is_err());
    p.transitions[0].id = Uuid::new_v4();
    p.time_effects.insert(
        p.clips[0].id,
        lumeryn_core::time_effects::TimeEffect::Freeze { source_frame: 0 },
    );
    assert!(p.validate().is_err());
    Ok(())
}

#[tokio::test]
async fn transition_transaction_dry_run_idempotency_delete_and_source_proof() -> Result<()> {
    use lumeryn_core::{Batch, Edit, MediaTools, Store};
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("source.mkv"), b"time")?;
    let mut p = Project::new("Cut transaction".into(), 64, 64, "30".parse()?)?;
    let mid = Uuid::new_v4();
    p.media.push(Media {
        id: mid,
        path: "source.mkv".into(),
        alias: None,
        metadata: Metadata {
            size_bytes: 4,
            blake3: Some(blake3::hash(b"time").to_hex().to_string()),
            audio: None,
            video: Some(Video {
                codec: "ffv1".into(),
                width: 64,
                height: 64,
                frame_rate: p.frame_rate,
                frames: 20,
                start_time_micros: 0,
                sample_aspect_ratio: "1:1".into(),
                color_transfer: None,
                constant_rate: true,
            }),
        },
    });
    for at in [0, 6] {
        p.clips.push(Clip {
            id: Uuid::new_v4(),
            media_id: mid,
            track_id: p.video_track_id,
            at_frame: at,
            source_in_frame: 2,
            duration_frames: 6,
        });
    }
    std::fs::write(dir.path().join("incoming.mkv"), b"next")?;
    let mut incoming = p.media[0].clone();
    incoming.id = Uuid::new_v4();
    incoming.path = "incoming.mkv".into();
    incoming.metadata.blake3 = Some(blake3::hash(b"next").to_hex().to_string());
    p.clips[1].media_id = incoming.id;
    p.media.push(incoming);
    let id = Uuid::new_v4();
    let batch = Batch {
        operations: vec![Edit::SetTransition {
            id,
            outgoing_clip_id: p.clips[0].id,
            incoming_clip_id: p.clips[1].id,
            kind: TransitionKind::CrossDissolve,
            duration_frames: 4,
        }],
    };
    let mut store = Store::create(&dir.path().join("project.lumeryn"), &p).await?;
    store
        .execute(&batch, Some(0), None, true, &MediaTools::default())
        .await?;
    assert!(store.inspect().await?.transitions.is_empty());
    store
        .execute(
            &batch,
            Some(0),
            Some("transition"),
            false,
            &MediaTools::default(),
        )
        .await?;
    store
        .execute(
            &batch,
            Some(0),
            Some("transition"),
            false,
            &MediaTools::default(),
        )
        .await?;
    assert_eq!(store.inspect().await?.revision, 1);
    store
        .execute(
            &Batch {
                operations: vec![Edit::DeleteTransition { transition_id: id }],
            },
            Some(1),
            None,
            false,
            &MediaTools::default(),
        )
        .await?;
    assert!(store.inspect().await?.transitions.is_empty());
    let history = store.history().await?.len();
    let prepared = lumeryn_core::storage::prepare_imports(
        &dir.path().join("project.lumeryn"),
        &batch,
        &MediaTools::default(),
    )
    .await?;
    let incoming = dir.path().join("incoming.mkv");
    let modified = std::fs::metadata(&incoming)?.modified()? + std::time::Duration::from_secs(1);
    std::fs::File::options()
        .write(true)
        .open(&incoming)?
        .set_times(std::fs::FileTimes::new().set_modified(modified))?;
    assert!(
        store
            .execute_prepared(&batch, Some(2), None, false, prepared)
            .await
            .is_err()
    );
    assert_eq!(store.history().await?.len(), history);
    std::fs::write(&incoming, b"fake")?;
    assert!(
        store
            .execute(&batch, Some(2), None, false, &MediaTools::default())
            .await
            .is_err()
    );
    assert_eq!(store.inspect().await?.revision, 2);
    Ok(())
}
