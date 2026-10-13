use lumeryn_core::{
    Batch, Edit, MediaTools, Project, Result, Store,
    media::{Metadata, Video},
    model::{Clip, Media, SCHEMA_VERSION},
    visual_transform::{SourceCrop, VisualTransform},
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
async fn primary_transform_atomic_reset_history_clipboard_and_source_protection() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let project = fixture(dir.path())?;
    let id = project.clips[0].id;
    let mut store = Store::create(&dir.path().join("project.lumeryn"), &project).await?;
    let tools = MediaTools::default();
    let transform = VisualTransform {
        crop: Some(SourceCrop {
            x: 10,
            y: 10,
            width: 100,
            height: 80,
        }),
        rotation_millidegrees: 33000,
        opacity_basis_points: 7500,
        ..Default::default()
    };
    let batch = Batch {
        operations: vec![Edit::SetVisualTransform {
            clip_id: id,
            transform,
        }],
    };
    store.execute(&batch, Some(0), None, true, &tools).await?;
    assert!(store.inspect().await?.visual_settings.is_empty());
    store
        .execute(&batch, Some(0), Some("transform"), false, &tools)
        .await?;
    let changed = store.inspect().await?;
    assert_eq!(changed.visual_settings[&id].transform, transform);
    assert!(changed.uses_composition());
    let clipboard = lumeryn_core::clipboard::Clipboard::copy(&changed, id)?;
    assert_eq!(clipboard.version, lumeryn_core::clipboard::VERSION);
    let pasted = Uuid::new_v4();
    store
        .execute(
            &Batch {
                operations: vec![Edit::PasteClip {
                    id: pasted,
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
        store.inspect().await?.visual_settings[&pasted].transform,
        transform
    );
    store
        .execute(
            &Batch {
                operations: vec![Edit::DeleteClip {
                    clip_id: pasted,
                    ripple: false,
                }],
            },
            Some(2),
            None,
            false,
            &tools,
        )
        .await?;
    assert!(!store.inspect().await?.visual_settings.contains_key(&pasted));
    store
        .execute(
            &Batch {
                operations: vec![Edit::Undo {}],
            },
            Some(3),
            None,
            false,
            &tools,
        )
        .await?;
    assert_eq!(
        store.inspect().await?.visual_settings[&pasted].transform,
        transform
    );
    store
        .execute(
            &Batch {
                operations: vec![Edit::SetVisualTransform {
                    clip_id: id,
                    transform: Default::default(),
                }],
            },
            Some(4),
            None,
            false,
            &tools,
        )
        .await?;
    assert!(!store.inspect().await?.visual_settings.contains_key(&id));
    let before = store.inspect().await?;
    let invalid = VisualTransform {
        crop: Some(SourceCrop {
            x: 319,
            y: 0,
            width: 2,
            height: 10,
        }),
        ..Default::default()
    };
    assert!(
        store
            .execute(
                &Batch {
                    operations: vec![Edit::SetVisualTransform {
                        clip_id: id,
                        transform: invalid
                    }]
                },
                Some(5),
                None,
                false,
                &tools
            )
            .await
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(store.inspect().await?)?,
        serde_json::to_value(before)?
    );
    let prepared = lumeryn_core::storage::prepare_imports(store.path(), &batch, &tools).await?;
    std::fs::write(dir.path().join("source.fixture"), b"changed source")?;
    assert!(
        store
            .execute_prepared(&batch, Some(5), None, false, prepared)
            .await
            .is_err()
    );
    assert_eq!(store.inspect().await?.revision, 5);
    Ok(())
}

#[tokio::test]
async fn explicit_schema_three_migration_preserves_visual_ids_and_all_history() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut project = fixture(dir.path())?;
    project.schema_version = 3;
    let ids = (
        project.id,
        project.clips[0].id,
        project.video_track_id,
        project.audio_track_id,
    );
    let path = dir.path().join("project.lumeryn");
    // Store creation accepts a valid readable legacy graph, but edits require migration.
    let mut store = Store::create(&path, &project).await?;
    assert_eq!(store.inspect().await?.schema_version, 3);
    let before = store.history().await?;
    let result = store.migrate().await?;
    assert_eq!(result["graph_from"], 3);
    assert_eq!(result["graph_to"], SCHEMA_VERSION);
    let changed = store.inspect().await?;
    assert_eq!(
        (
            changed.id,
            changed.clips[0].id,
            changed.video_track_id,
            changed.audio_track_id
        ),
        ids
    );
    assert_eq!(changed.revision, 0);
    assert!(changed.visual_settings.is_empty());
    assert_eq!(store.history().await?, before);
    store.integrity_check().await?;
    Ok(())
}
