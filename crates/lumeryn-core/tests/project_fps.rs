use lumeryn_core::{
    Batch, Edit, Error, MediaTools, Project, Result, Store,
    media::{Metadata, Video},
    model::{Clip, Media},
};
use uuid::Uuid;

async fn change(store: &mut Store, fps: &str, revision: u64, dry_run: bool) -> Result<()> {
    store
        .execute(
            &Batch {
                operations: vec![Edit::SetFrameRate {
                    frame_rate: fps.parse()?,
                }],
            },
            Some(revision),
            None,
            dry_run,
            &MediaTools::default(),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn empty_timeline_rate_is_exact_transactional_and_undoable() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let original = Project::new("Exact FPS".into(), 1920, 1080, "30".parse()?)?;
    let mut store = Store::create(&directory.path().join("project.lumeryn"), &original).await?;
    change(&mut store, "60000/1001", 0, true).await?;
    assert_eq!(store.inspect().await?.frame_rate, original.frame_rate);
    change(&mut store, "60000/1001", 0, false).await?;
    let updated = store.inspect().await?;
    assert_eq!(updated.frame_rate.numerator(), 60_000);
    assert_eq!(updated.frame_rate.denominator(), 1001);
    assert_eq!(updated.revision, 1);
    assert!(matches!(
        change(&mut store, "24", 0, false).await,
        Err(Error::Conflict { .. })
    ));
    store
        .execute(
            &Batch {
                operations: vec![Edit::Undo {}],
            },
            Some(1),
            None,
            false,
            &MediaTools::default(),
        )
        .await?;
    assert_eq!(store.inspect().await?.frame_rate, original.frame_rate);
    store
        .execute(
            &Batch {
                operations: vec![Edit::Redo {}],
            },
            Some(2),
            None,
            false,
            &MediaTools::default(),
        )
        .await?;
    assert_eq!(store.inspect().await?.frame_rate, updated.frame_rate);
    store.integrity_check().await?;
    Ok(())
}

#[tokio::test]
async fn populated_timeline_rejects_rate_change_without_altering_clip_timing() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut project = Project::new("Locked FPS".into(), 320, 180, "30000/1001".parse()?)?;
    let media_id = Uuid::new_v4();
    project.media.push(Media {
        id: media_id,
        path: "fixture.bin".into(),
        alias: None,
        metadata: Metadata {
            size_bytes: 1,
            blake3: None,
            audio: None,
            video: Some(Video {
                codec: "fixture".into(),
                width: 320,
                height: 180,
                frame_rate: project.frame_rate,
                frames: 100,
                start_time_micros: 0,
                sample_aspect_ratio: "1:1".into(),
                color_transfer: None,
                constant_rate: true,
            }),
        },
    });
    project.clips.push(Clip {
        id: Uuid::new_v4(),
        media_id,
        track_id: project.video_track_id,
        at_frame: 0,
        source_in_frame: 10,
        duration_frames: 60,
    });
    let expected_clips = serde_json::to_value(&project.clips)?;
    let mut store = Store::create(&directory.path().join("project.lumeryn"), &project).await?;
    assert!(matches!(
        change(&mut store, "60", 0, false).await,
        Err(Error::Unsupported(_))
    ));
    assert_eq!(store.inspect().await?.revision, 0);
    let rejected_batch = store
        .execute(
            &Batch {
                operations: vec![
                    Edit::SetCanvas {
                        width: 640,
                        height: 360,
                    },
                    Edit::SetFrameRate {
                        frame_rate: "60".parse()?,
                    },
                ],
            },
            Some(0),
            None,
            false,
            &MediaTools::default(),
        )
        .await;
    assert!(matches!(rejected_batch, Err(Error::Unsupported(_))));
    let preserved = store.inspect().await?;
    assert_eq!(
        (preserved.width, preserved.height, preserved.revision),
        (320, 180, 0)
    );
    assert_eq!(
        serde_json::to_value(store.inspect().await?.clips)?,
        expected_clips
    );
    change(&mut store, "60000/2002", 0, false).await?;
    assert_eq!(store.inspect().await?.frame_rate, project.frame_rate);
    assert_eq!(
        serde_json::to_value(store.inspect().await?.clips)?,
        expected_clips
    );
    Ok(())
}

#[test]
fn rate_command_rejects_invalid_or_unknown_fields() {
    for command in [
        r#"{"operation":"set_frame_rate","frame_rate":{"numerator":60,"denominator":0}}"#,
        r#"{"operation":"set_frame_rate","frame_rate":{"numerator":60,"denominator":1},"force":true}"#,
    ] {
        assert!(serde_json::from_str::<Edit>(command).is_err());
    }
}
