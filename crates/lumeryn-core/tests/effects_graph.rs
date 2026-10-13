use lumeryn_core::{
    Batch, Edit, MediaTools, Project, Result, Store,
    media::{Metadata, Video},
    model::{Clip, Media},
    visual_effects::{FadeEnvelope, VisualEffects},
    visual_transform::VisualTransform,
};
use uuid::Uuid;

fn fixture(dir: &std::path::Path) -> Result<Project> {
    let mut project = Project::new("Transforms".into(), 320, 180, "30".parse()?)?;
    let bytes = b"immutable source fixture";
    std::fs::write(dir.join("source.fixture"), bytes)?;
    let media_id = Uuid::new_v4();
    project.media.push(Media {
        id: media_id,
        path: "source.fixture".into(),
        alias: None,
        metadata: Metadata {
            size_bytes: bytes.len() as u64,
            blake3: Some(blake3::hash(bytes).to_hex().to_string()),
            video: Some(Video {
                codec: "ffv1".into(),
                width: 320,
                height: 180,
                frame_rate: project.frame_rate,
                frames: 180,
                start_time_micros: 0,
                sample_aspect_ratio: "1:1".into(),
                color_transfer: None,
                constant_rate: true,
            }),
            audio: None,
        },
    });
    project.clips.push(Clip {
        id: Uuid::new_v4(),
        media_id,
        track_id: project.video_track_id,
        at_frame: 0,
        source_in_frame: 0,
        duration_frames: 30,
    });
    project.validate()?;
    Ok(project)
}

#[tokio::test]
async fn effects_atomic_identity_reset_preserves_transform_and_history() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let project = fixture(dir.path())?;
    let id = project.clips[0].id;
    let mut store = Store::create(&dir.path().join("project.lumeryn"), &project).await?;
    let tools = MediaTools::default();
    let transform = VisualTransform {
        opacity_basis_points: 7500,
        ..Default::default()
    };
    let effects = VisualEffects {
        exposure_millistops: 500,
        fades: FadeEnvelope {
            in_frames: 5,
            out_frames: 5,
        },
        ..Default::default()
    };
    let batch = Batch {
        operations: vec![
            Edit::SetVisualTransform {
                clip_id: id,
                transform,
            },
            Edit::SetVisualEffects {
                clip_id: id,
                effects,
            },
        ],
    };
    store.execute(&batch, Some(0), None, true, &tools).await?;
    assert!(store.inspect().await?.visual_settings.is_empty());
    store
        .execute(&batch, Some(0), Some("effects"), false, &tools)
        .await?;
    assert!(
        store
            .execute(&batch, Some(0), Some("effects"), false, &tools)
            .await?
            .replayed
    );
    let edited = store.inspect().await?;
    assert_eq!(edited.visual_settings[&id].effects, Some(effects));
    assert_eq!(edited.visual_settings[&id].transform, transform);
    let clipboard = lumeryn_core::clipboard::Clipboard::copy(&edited, id)?;
    let copy = Uuid::new_v4();
    store
        .execute(
            &Batch {
                operations: vec![Edit::PasteClip {
                    id: copy,
                    at_frame: 30,
                    clipboard,
                }],
            },
            Some(1),
            None,
            false,
            &tools,
        )
        .await?;
    assert_eq!(
        store.inspect().await?.visual_settings[&copy],
        edited.visual_settings[&id]
    );
    store
        .execute(
            &Batch {
                operations: vec![Edit::SetVisualTransform {
                    clip_id: id,
                    transform: Default::default(),
                }],
            },
            Some(2),
            None,
            false,
            &tools,
        )
        .await?;
    assert_eq!(
        store.inspect().await?.visual_settings[&id].effects,
        Some(effects)
    );
    store
        .execute(
            &Batch {
                operations: vec![Edit::SetVisualEffects {
                    clip_id: id,
                    effects: Default::default(),
                }],
            },
            Some(3),
            None,
            false,
            &tools,
        )
        .await?;
    assert!(!store.inspect().await?.visual_settings.contains_key(&id));
    store
        .execute(
            &Batch {
                operations: vec![Edit::Undo {}],
            },
            Some(4),
            None,
            false,
            &tools,
        )
        .await?;
    assert_eq!(
        store.inspect().await?.visual_settings[&id].effects,
        Some(effects)
    );
    store
        .execute(
            &Batch {
                operations: vec![
                    Edit::SetVisualTransform {
                        clip_id: id,
                        transform,
                    },
                    Edit::SetVisualEffects {
                        clip_id: id,
                        effects: Default::default(),
                    },
                ],
            },
            Some(5),
            None,
            false,
            &tools,
        )
        .await?;
    let reset = store.inspect().await?;
    assert_eq!(reset.visual_settings[&id].transform, transform);
    assert_eq!(reset.visual_settings[&id].effects, None);
    Ok(())
}

#[tokio::test]
async fn invalid_fades_trim_split_and_changed_source_do_not_commit() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let project = fixture(dir.path())?;
    let id = project.clips[0].id;
    let mut store = Store::create(&dir.path().join("project.lumeryn"), &project).await?;
    let tools = MediaTools::default();
    let effects = VisualEffects {
        fades: FadeEnvelope {
            in_frames: 10,
            out_frames: 10,
        },
        ..Default::default()
    };
    let batch = Batch {
        operations: vec![Edit::SetVisualEffects {
            clip_id: id,
            effects,
        }],
    };
    store.execute(&batch, Some(0), None, false, &tools).await?;
    let before = serde_json::to_value(store.inspect().await?)?;
    for operation in [
        Edit::SetVisualEffects {
            clip_id: id,
            effects: VisualEffects {
                version: 2,
                ..Default::default()
            },
        },
        Edit::TrimClip {
            clip_id: id,
            source_in_frame: 0,
            duration_frames: 19,
        },
        Edit::SplitClip {
            clip_id: id,
            right_clip_id: Uuid::new_v4(),
            at_frame: 15,
        },
    ] {
        assert!(
            store
                .execute(
                    &Batch {
                        operations: vec![operation]
                    },
                    Some(1),
                    None,
                    false,
                    &tools
                )
                .await
                .is_err()
        );
        assert_eq!(serde_json::to_value(store.inspect().await?)?, before);
    }
    let prepared = lumeryn_core::storage::prepare_imports(store.path(), &batch, &tools).await?;
    std::fs::write(dir.path().join("source.fixture"), b"changed source")?;
    assert!(
        store
            .execute_prepared(&batch, Some(1), None, false, prepared)
            .await
            .is_err()
    );
    assert_eq!(store.inspect().await?.revision, 1);
    Ok(())
}

#[tokio::test]
async fn overlay_effects_validate_duration_and_atomic_reset() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let project = fixture(dir.path())?;
    let mut store = Store::create(&dir.path().join("project.lumeryn"), &project).await?;
    let tools = MediaTools::default();
    let track = Uuid::new_v4();
    let clip = Uuid::new_v4();
    let effects = VisualEffects {
        temperature_basis_points: 1500,
        ..Default::default()
    };
    store
        .execute(
            &Batch {
                operations: vec![
                    Edit::AddVisualTrack {
                        id: track,
                        name: "Overlay".into(),
                    },
                    Edit::AddVisualClip {
                        id: clip,
                        media_id: project.media[0].id,
                        track_id: track,
                        at_frame: 0,
                        duration_frames: 10,
                        source: lumeryn_core::model::VisualSource::Video {
                            source_in_frame: 30,
                        },
                        placement: None,
                    },
                    Edit::SetVisualEffects {
                        clip_id: clip,
                        effects,
                    },
                ],
            },
            Some(0),
            None,
            false,
            &tools,
        )
        .await?;
    assert_eq!(
        store.inspect().await?.visual_settings[&clip].effects,
        Some(effects)
    );
    assert!(
        store
            .execute(
                &Batch {
                    operations: vec![Edit::SetVisualEffects {
                        clip_id: clip,
                        effects: VisualEffects {
                            fades: FadeEnvelope {
                                in_frames: 11,
                                out_frames: 0
                            },
                            ..effects
                        }
                    }]
                },
                Some(1),
                None,
                false,
                &tools
            )
            .await
            .is_err()
    );
    store
        .execute(
            &Batch {
                operations: vec![Edit::SetVisualEffects {
                    clip_id: clip,
                    effects: Default::default(),
                }],
            },
            Some(1),
            None,
            false,
            &tools,
        )
        .await?;
    assert!(!store.inspect().await?.visual_settings.contains_key(&clip));
    let mut malformed = store.inspect().await?;
    malformed.visual_settings.insert(
        clip,
        lumeryn_core::model::VisualSettings {
            transform: Default::default(),
            effects: Some(Default::default()),
        },
    );
    assert!(malformed.validate().is_err());
    Ok(())
}
