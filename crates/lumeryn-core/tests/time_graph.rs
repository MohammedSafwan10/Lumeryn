use lumeryn_core::{
    Project, Result,
    media::{Metadata, Video},
    model::{Clip, Media},
    time_effects::{PlaybackSpeed, SpeedAudio, TimeEffect},
};
use uuid::Uuid;
fn fixture() -> Result<Project> {
    let mut p = Project::new("Time".into(), 64, 64, "30".parse()?)?;
    let id = Uuid::new_v4();
    p.media.push(Media {
        id,
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
    p.clips.push(Clip {
        id: Uuid::new_v4(),
        media_id: id,
        track_id: p.video_track_id,
        at_frame: 0,
        source_in_frame: 0,
        duration_frames: 10,
    });
    p.validate()?;
    Ok(p)
}
#[test]
fn speed_and_freeze_graph_bounds_legacy_and_canonical_reset() -> Result<()> {
    let mut p = fixture()?;
    let id = p.clips[0].id;
    p.time_effects.insert(
        id,
        TimeEffect::Speed {
            speed: PlaybackSpeed::Double,
            audio: SpeedAudio::PreservePitch,
        },
    );
    p.validate()?;
    assert!(p.uses_composition());
    p.clips[0].duration_frames = 11;
    assert!(p.validate().is_err());
    p.time_effects
        .insert(id, TimeEffect::Freeze { source_frame: 19 });
    p.validate()?;
    p.time_effects
        .insert(id, TimeEffect::Freeze { source_frame: 20 });
    assert!(p.validate().is_err());
    p.time_effects.insert(id, TimeEffect::Normal);
    assert!(p.validate().is_err());
    p.time_effects.clear();
    p.clips[0].duration_frames = 10;
    p.schema_version = 3;
    p.validate()?;
    p.time_effects
        .insert(id, TimeEffect::Freeze { source_frame: 1 });
    assert!(p.validate().is_err());
    Ok(())
}
#[test]
fn dangling_time_effect_and_png_are_rejected() -> Result<()> {
    let mut p = fixture()?;
    p.time_effects
        .insert(Uuid::new_v4(), TimeEffect::Freeze { source_frame: 0 });
    assert!(p.validate().is_err());
    p.time_effects.clear();
    p.time_effects
        .insert(p.clips[0].id, TimeEffect::Freeze { source_frame: 0 });
    if let Some(v) = p.media[0].metadata.video.as_mut() {
        v.codec = "png".into();
    }
    assert!(p.validate().is_err());
    Ok(())
}

#[tokio::test]
async fn transactional_time_effect_dry_run_reset_and_source_change() -> Result<()> {
    use lumeryn_core::{Batch, Edit, MediaTools, Store};
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("source.mkv"), b"time")?;
    let mut p = fixture()?;
    p.media[0].metadata.blake3 = Some(blake3::hash(b"time").to_hex().to_string());
    let cid = p.clips[0].id;
    let mut store = Store::create(&dir.path().join("project.lumeryn"), &p).await?;
    let batch = Batch {
        operations: vec![Edit::SetTimeEffect {
            clip_id: cid,
            effect: TimeEffect::Speed {
                speed: PlaybackSpeed::Double,
                audio: SpeedAudio::PreservePitch,
            },
        }],
    };
    store
        .execute(
            &batch,
            Some(0),
            None,
            true,
            &lumeryn_core::MediaTools::default(),
        )
        .await?;
    assert!(store.inspect().await?.time_effects.is_empty());
    store
        .execute(
            &batch,
            Some(0),
            Some("speed"),
            false,
            &MediaTools::default(),
        )
        .await?;
    assert_eq!(store.inspect().await?.clips[0].duration_frames, 10);
    assert!(
        store
            .execute(
                &Batch {
                    operations: vec![Edit::SplitClip {
                        clip_id: cid,
                        right_clip_id: Uuid::new_v4(),
                        at_frame: 5
                    }]
                },
                Some(1),
                None,
                false,
                &MediaTools::default()
            )
            .await
            .is_err()
    );
    store
        .execute(
            &Batch {
                operations: vec![Edit::SetTimeEffect {
                    clip_id: cid,
                    effect: TimeEffect::Normal,
                }],
            },
            Some(1),
            None,
            false,
            &MediaTools::default(),
        )
        .await?;
    assert!(store.inspect().await?.time_effects.is_empty());
    std::fs::write(dir.path().join("source.mkv"), b"fake")?;
    assert!(
        store
            .execute(&batch, Some(2), None, false, &MediaTools::default())
            .await
            .is_err()
    );
    assert_eq!(store.inspect().await?.revision, 2);
    Ok(())
}
