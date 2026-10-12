use lumeryn_core::{
    Batch, Edit, Error, MediaTools, Project, Result, Store,
    media::{Metadata, Video},
    model::{Clip, MAX_FRAMES, Media, VisualClip, VisualPlacement, VisualSource, VisualTrack},
};
use uuid::Uuid;

fn fixture(directory: &std::path::Path) -> Result<Project> {
    let mut project = Project::new("Layers".into(), 320, 180, "30".parse()?)?;
    for (filename, codec) in [("video.fixture", "ffv1"), ("image.fixture", "png")] {
        let bytes = filename.as_bytes();
        std::fs::write(directory.join(filename), bytes)?;
        project.media.push(Media {
            id: Uuid::new_v4(),
            path: filename.into(),
            alias: None,
            metadata: Metadata {
                size_bytes: bytes.len() as u64,
                blake3: Some(blake3::hash(bytes).to_hex().to_string()),
                audio: None,
                video: Some(Video {
                    codec: codec.into(),
                    width: 320,
                    height: 180,
                    frame_rate: project.frame_rate,
                    frames: if codec == "png" { 1 } else { 300 },
                    start_time_micros: 0,
                    sample_aspect_ratio: "1:1".into(),
                    color_transfer: None,
                    constant_rate: true,
                }),
            },
        });
    }
    project.clips.push(Clip {
        id: Uuid::new_v4(),
        media_id: project.media[0].id,
        track_id: project.video_track_id,
        at_frame: 0,
        source_in_frame: 0,
        duration_frames: 60,
    });
    project.validate()?;
    Ok(project)
}

fn visual(project: &Project, track_id: Uuid, image: bool, at_frame: u64) -> VisualClip {
    VisualClip {
        id: Uuid::new_v4(),
        media_id: project.media[usize::from(image)].id,
        track_id,
        at_frame,
        duration_frames: 30,
        source: if image {
            VisualSource::Image {}
        } else {
            VisualSource::Video {
                source_in_frame: 20,
            }
        },
        placement: Some(VisualPlacement {
            x: 160,
            y: 90,
            width: 160,
            height: 90,
        }),
    }
}

fn add(clip: &VisualClip) -> Edit {
    Edit::AddVisualClip {
        id: clip.id,
        media_id: clip.media_id,
        track_id: clip.track_id,
        at_frame: clip.at_frame,
        duration_frames: clip.duration_frames,
        source: clip.source.clone(),
        placement: clip.placement.clone(),
    }
}

#[tokio::test]
async fn layered_batch_dry_run_conflict_replay_and_undo_are_atomic() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let project = fixture(directory.path())?;
    let path = directory.path().join("project.lumeryn");
    let mut store = Store::create(&path, &project).await?;
    let tracks = [Uuid::new_v4(), Uuid::new_v4()];
    let moving = visual(&project, tracks[0], false, 10);
    let image = visual(&project, tracks[1], true, 45);
    let batch = Batch {
        operations: vec![
            Edit::AddVisualTrack {
                id: tracks[0],
                name: "Overlay".into(),
            },
            Edit::AddVisualTrack {
                id: tracks[1],
                name: "Image".into(),
            },
            add(&moving),
            add(&image),
        ],
    };
    let tools = MediaTools::default();
    let dry = store.execute(&batch, Some(0), None, true, &tools).await?;
    assert!(dry.dry_run);
    assert!(store.inspect().await?.visual_clips.is_empty());
    let receipt = store
        .execute(&batch, Some(0), Some("layers"), false, &tools)
        .await?;
    assert_eq!(
        receipt.object_ids,
        vec![tracks[0], tracks[1], moving.id, image.id]
    );
    let layered = store.inspect().await?;
    assert!(layered.uses_composition());
    assert_eq!(layered.visual_duration_frames(), 75);
    let conflict = store
        .execute(
            &Batch {
                operations: vec![Edit::DeleteVisualClip { clip_id: moving.id }],
            },
            Some(0),
            None,
            false,
            &tools,
        )
        .await;
    assert!(matches!(conflict, Err(Error::Conflict { .. })));
    store
        .execute(
            &Batch {
                operations: vec![Edit::Undo {}],
            },
            Some(1),
            None,
            false,
            &tools,
        )
        .await?;
    assert!(store.inspect().await?.visual_tracks.is_empty());
    store
        .execute(
            &Batch {
                operations: vec![Edit::Redo {}],
            },
            Some(2),
            None,
            false,
            &tools,
        )
        .await?;
    assert_eq!(store.inspect().await?.visual_clips.len(), 2);
    std::fs::remove_file(directory.path().join("image.fixture"))?;
    assert!(
        store
            .execute(&batch, Some(999), Some("layers"), false, &tools)
            .await?
            .replayed
    );
    store.integrity_check().await?;
    Ok(())
}

#[tokio::test]
async fn used_visual_media_track_and_changed_source_cannot_be_written() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut project = fixture(directory.path())?;
    let track = Uuid::new_v4();
    project.visual_tracks.push(VisualTrack {
        id: track,
        name: "Still".into(),
    });
    let image = visual(&project, track, true, 10);
    project.visual_clips.push(image.clone());
    let mut store = Store::create(&directory.path().join("project.lumeryn"), &project).await?;
    let tools = MediaTools::default();
    for edit in [
        Edit::DeleteVisualTrack { track_id: track },
        Edit::RemoveMedia {
            media_id: image.media_id,
        },
        Edit::SetFrameRate {
            frame_rate: "60".parse()?,
        },
    ] {
        assert!(
            store
                .execute(
                    &Batch {
                        operations: vec![edit]
                    },
                    Some(0),
                    None,
                    false,
                    &tools
                )
                .await
                .is_err()
        );
    }
    std::fs::write(directory.path().join("image.fixture"), b"other.fixture")?;
    assert!(
        store
            .execute(
                &Batch {
                    operations: vec![Edit::SetVisualPlacement {
                        clip_id: image.id,
                        placement: None
                    }]
                },
                Some(0),
                None,
                false,
                &tools
            )
            .await
            .is_err()
    );
    assert_eq!(store.inspect().await?.revision, 0);
    Ok(())
}

#[test]
fn interval_bounds_resource_admission_and_source_kinds_are_checked() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut project = fixture(directory.path())?;
    let track = Uuid::new_v4();
    project.visual_tracks.push(VisualTrack {
        id: track,
        name: "Overlay".into(),
    });
    let first = visual(&project, track, true, 0);
    let second = visual(&project, track, true, 30);
    project.visual_clips = vec![first, second];
    project.validate()?; // Exact half-open adjacency, one active still.
    project.visual_clips[1].at_frame = 29;
    assert!(project.validate().is_err());
    project.visual_clips[1].at_frame = 30;
    project.visual_clips[1].source = VisualSource::Video { source_in_frame: 0 };
    assert!(project.validate().is_err()); // PNG cannot pretend to be video.
    project.visual_clips[1].source = VisualSource::Image {};
    project.visual_clips[1].at_frame = MAX_FRAMES;
    assert!(project.validate().is_err());
    project.visual_clips[1].at_frame = 30;
    project.visual_clips[1]
        .placement
        .as_mut()
        .ok_or_else(|| Error::Invalid("fixture rectangle".into()))?
        .width = 3841;
    assert!(project.validate().is_err());
    project.visual_clips[1].placement = None;
    let extra = visual(&project, track, true, 60);
    project.visual_clips.push(extra);
    assert!(matches!(project.validate(), Err(Error::Unsupported(_))));
    Ok(())
}

#[test]
fn historical_shapes_and_required_nullable_properties_are_explicit() -> Result<()> {
    let mut project = Project::new("Shape".into(), 320, 180, "30".parse()?)?;
    project.schema_version = 2;
    project.validate()?;
    project.audio_track_id = None;
    assert!(project.validate().is_err());
    project.schema_version = 1;
    project.validate()?;
    project.visual_tracks.push(VisualTrack {
        id: Uuid::new_v4(),
        name: "New".into(),
    });
    assert!(project.validate().is_err());
    let id = Uuid::new_v4();
    assert!(
        serde_json::from_value::<Edit>(
            serde_json::json!({"operation":"set_visual_placement","clip_id":id})
        )
        .is_err()
    );
    assert!(matches!(
        serde_json::from_value::<Edit>(
            serde_json::json!({"operation":"set_visual_placement","clip_id":id,"placement":null})
        )?,
        Edit::SetVisualPlacement {
            placement: None,
            ..
        }
    ));
    assert!(
        serde_json::from_value::<VisualSource>(
            serde_json::json!({"kind":"image","source_in_frame":0})
        )
        .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn primary_free_move_preserves_source_and_gap_without_ripple() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let project = fixture(directory.path())?;
    let id = project.clips[0].id;
    let mut store = Store::create(&directory.path().join("project.lumeryn"), &project).await?;
    store
        .execute(
            &Batch {
                operations: vec![Edit::MoveClip {
                    clip_id: id,
                    at_frame: 20,
                }],
            },
            Some(0),
            None,
            false,
            &MediaTools::default(),
        )
        .await?;
    let moved = store.inspect().await?;
    assert!(moved.uses_composition());
    assert_eq!(moved.clips[0].source_in_frame, 0);
    assert_eq!(moved.visual_duration_frames(), 80);
    Ok(())
}

#[tokio::test]
async fn prepared_visual_source_changed_before_commit_is_rejected() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut project = fixture(directory.path())?;
    let track = Uuid::new_v4();
    project.visual_tracks.push(VisualTrack {
        id: track,
        name: "Still".into(),
    });
    let image = visual(&project, track, true, 10);
    let path = directory.path().join("project.lumeryn");
    let mut store = Store::create(&path, &project).await?;
    let batch = Batch {
        operations: vec![add(&image)],
    };
    let prepared =
        lumeryn_core::storage::prepare_imports(&path, &batch, &MediaTools::default()).await?;
    std::fs::write(
        directory.path().join("image.fixture"),
        b"changed source and length",
    )?;
    assert!(
        store
            .execute_prepared(&batch, Some(0), None, false, prepared)
            .await
            .is_err()
    );
    assert!(store.inspect().await?.visual_clips.is_empty());
    assert_eq!(store.inspect().await?.revision, 0);
    Ok(())
}
