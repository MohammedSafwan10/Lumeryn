//! Shared insertion semantics use real SQLite transactions without media workers.
use lumeryn_core::{
    Batch, Edit, Error, MediaTools, Project, Result, Store,
    media::{Metadata, Video},
    model::{Clip, MAX_FRAMES, Media},
};
use uuid::Uuid;

fn fixture() -> Result<Project> {
    let mut project = Project::new("Insertion".into(), 1920, 1080, "30".parse()?)?;
    let media_id = Uuid::new_v4();
    project.media.push(Media {
        id: media_id,
        path: "source.mp4".into(),
        alias: None,
        metadata: Metadata {
            size_bytes: 9,
            blake3: Some(blake3::hash(b"immutable").to_hex().to_string()),
            audio: None,
            video: Some(Video {
                codec: "fixture".into(),
                width: 1920,
                height: 1080,
                frame_rate: project.frame_rate,
                frames: MAX_FRAMES,
                start_time_micros: 0,
                sample_aspect_ratio: "1:1".into(),
                color_transfer: None,
                constant_rate: true,
            }),
        },
    });
    project.clips = [(0, 10, 30), (30, 80, 40), (70, 200, 50)]
        .into_iter()
        .map(|(at_frame, source_in_frame, duration_frames)| Clip {
            id: Uuid::new_v4(),
            media_id,
            track_id: project.video_track_id,
            at_frame,
            source_in_frame,
            duration_frames,
        })
        .collect();
    project.validate()?;
    Ok(project)
}

fn insert(project: &Project, before_clip_id: Option<Uuid>) -> Edit {
    Edit::InsertClip {
        id: Uuid::new_v4(),
        media_id: project.media[0].id,
        before_clip_id,
        source_in_frame: 23,
        duration_frames: 17,
    }
}

fn batch(operation: Edit) -> Batch {
    Batch {
        operations: vec![operation],
    }
}

async fn store_for(project: &Project) -> Result<(tempfile::TempDir, Store)> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("source.mp4"), b"immutable")?;
    let store = Store::create(&dir.path().join("project.lumeryn"), project).await?;
    Ok((dir, store))
}

async fn apply(store: &mut Store, operation: Edit, revision: u64) -> Result<()> {
    store
        .execute(
            &batch(operation),
            Some(revision),
            None,
            false,
            &MediaTools::default(),
        )
        .await?;
    Ok(())
}

fn graph(mut project: Project) -> Result<serde_json::Value> {
    project.revision = 0;
    Ok(serde_json::to_value(project)?)
}

#[tokio::test]
async fn beginning_middle_append_and_empty_preserve_source_ranges() -> Result<()> {
    for position in 0..=3 {
        let original = fixture()?;
        let before = original.clips.get(position).map(|clip| clip.id);
        let operation = insert(&original, before);
        let inserted_id = match &operation {
            Edit::InsertClip { id, .. } => *id,
            _ => unreachable!("fixture produces InsertClip"),
        };
        let (_dir, mut store) = store_for(&original).await?;
        apply(&mut store, operation, 0).await?;
        let result = store.inspect().await?;
        assert_eq!(result.revision, 1);
        assert_eq!(result.clips.len(), 4);
        assert_eq!(result.clips[position].id, inserted_id);
        assert_eq!(result.clips[position].source_in_frame, 23);
        assert_eq!(result.clips[position].duration_frames, 17);
        let mut expected_at = 0;
        for clip in &result.clips {
            assert_eq!(clip.at_frame, expected_at);
            expected_at += clip.duration_frames;
            if clip.id != inserted_id {
                let prior = original.clips.iter().find(|prior| prior.id == clip.id);
                assert!(prior.is_some());
                if let Some(prior) = prior {
                    assert_eq!(clip.media_id, prior.media_id);
                    assert_eq!(clip.source_in_frame, prior.source_in_frame);
                    assert_eq!(clip.duration_frames, prior.duration_frames);
                }
            }
        }
        assert_eq!(expected_at, 137);
    }
    let mut empty = fixture()?;
    empty.clips.clear();
    let (_dir, mut store) = store_for(&empty).await?;
    apply(&mut store, insert(&empty, None), 0).await?;
    assert_eq!(store.inspect().await?.clips[0].at_frame, 0);
    Ok(())
}

#[tokio::test]
async fn insertion_sorts_existing_storage_order_without_changing_ranges() -> Result<()> {
    let mut original = fixture()?;
    original.clips.reverse();
    let before = original.clips[1].id;
    let (_dir, mut store) = store_for(&original).await?;
    apply(&mut store, insert(&original, Some(before)), 0).await?;
    let result = store.inspect().await?;
    assert_eq!(result.clips[2].id, before);
    assert_eq!(result.clips[0].source_in_frame, 10);
    assert_eq!(result.clips[2].source_in_frame, 80);
    assert_eq!(result.clips[3].source_in_frame, 200);
    Ok(())
}

#[tokio::test]
async fn gaps_overlaps_and_nonzero_start_are_rejected_without_repacking() -> Result<()> {
    for (index, at_frame) in [(1, 31), (1, 29), (0, 1)] {
        let mut original = fixture()?;
        original.clips[index].at_frame = at_frame;
        let (_dir, mut store) = store_for(&original).await?;
        assert!(matches!(
            apply(&mut store, insert(&original, None), 0).await,
            Err(Error::Unsupported(_))
        ));
        assert_eq!(graph(store.inspect().await?)?, graph(original)?);
        assert_eq!(store.history().await?.len(), 1);
    }
    Ok(())
}

#[tokio::test]
async fn invalid_ids_ranges_and_incompatible_media_leave_revision_unchanged() -> Result<()> {
    let original = fixture()?;
    let mut incompatible = original.media[0].clone();
    incompatible.id = Uuid::new_v4();
    if let Some(video) = &mut incompatible.metadata.video {
        video.frame_rate = "24".parse()?;
    }
    let mut original = original;
    original.media.push(incompatible.clone());
    let new_id = Uuid::new_v4();
    let invalid = [
        (
            Uuid::new_v4(),
            original.media[0].id,
            Some(Uuid::new_v4()),
            0,
            17,
        ),
        (new_id, original.media[0].id, Some(new_id), 0, 17),
        (original.clips[0].id, original.media[0].id, None, 0, 17),
        (original.id, original.media[0].id, None, 0, 17),
        (Uuid::new_v4(), Uuid::new_v4(), None, 0, 17),
        (Uuid::new_v4(), incompatible.id, None, 0, 17),
        (Uuid::new_v4(), original.media[0].id, None, 0, 0),
        (Uuid::new_v4(), original.media[0].id, None, MAX_FRAMES, 1),
        (Uuid::new_v4(), original.media[0].id, None, u64::MAX, 1),
    ];
    let (_dir, mut store) = store_for(&original).await?;
    for (id, media_id, before_clip_id, source_in_frame, duration_frames) in invalid {
        assert!(
            apply(
                &mut store,
                Edit::InsertClip {
                    id,
                    media_id,
                    before_clip_id,
                    source_in_frame,
                    duration_frames,
                },
                0,
            )
            .await
            .is_err()
        );
        assert_eq!(graph(store.inspect().await?)?, graph(original.clone())?);
        assert_eq!(store.history().await?.len(), 1);
    }
    Ok(())
}

#[tokio::test]
async fn total_timeline_overflow_rolls_back_insertion() -> Result<()> {
    let mut original = fixture()?;
    original.clips.truncate(1);
    original.clips[0].source_in_frame = 0;
    original.clips[0].duration_frames = MAX_FRAMES;
    let (_dir, mut store) = store_for(&original).await?;
    assert!(matches!(
        apply(&mut store, insert(&original, None), 0).await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(graph(store.inspect().await?)?, graph(original)?);
    Ok(())
}

#[tokio::test]
async fn dry_run_conflict_batch_rollback_and_idempotent_replay() -> Result<()> {
    let original = fixture()?;
    let operation = insert(&original, Some(original.clips[1].id));
    let command = batch(operation.clone());
    let (dir, mut store) = store_for(&original).await?;
    let tools = MediaTools::default();
    let dry = store.execute(&command, Some(0), None, true, &tools).await?;
    assert!(dry.dry_run);
    assert_eq!(dry.revision, 0);
    assert_eq!(graph(store.inspect().await?)?, graph(original.clone())?);
    assert!(matches!(
        store.execute(&command, Some(9), None, false, &tools).await,
        Err(Error::Conflict {
            expected: 9,
            current: 0
        })
    ));
    let invalid_batch = Batch {
        operations: vec![
            operation,
            Edit::ReorderClip {
                clip_id: Uuid::new_v4(),
                before_clip_id: None,
            },
        ],
    };
    assert!(
        store
            .execute(&invalid_batch, Some(0), None, false, &tools)
            .await
            .is_err()
    );
    assert_eq!(graph(store.inspect().await?)?, graph(original.clone())?);
    let first = store
        .execute(&command, Some(0), Some("insertion"), false, &tools)
        .await?;
    assert_eq!(first.revision, 1);
    assert_eq!(first.object_ids.len(), 1);
    apply(&mut store, Edit::Undo {}, 1).await?;
    assert_eq!(graph(store.inspect().await?)?, graph(original.clone())?);
    let replay = store
        .execute(&command, Some(0), Some("insertion"), false, &tools)
        .await?;
    assert!(replay.replayed);
    assert_eq!(replay.object_ids, first.object_ids);
    assert_eq!(replay.revision, 1);
    assert_eq!(store.inspect().await?.revision, 2);
    apply(&mut store, Edit::Redo {}, 2).await?;
    assert_eq!(store.inspect().await?.clips.len(), 4);
    assert_eq!(store.inspect().await?.revision, 3);
    assert_eq!(store.history().await?.len(), 4);
    assert_eq!(std::fs::read(dir.path().join("source.mp4"))?, b"immutable");
    store.integrity_check().await?;
    Ok(())
}

#[test]
fn insertion_json_is_strict_and_has_discoverable_schema() -> Result<()> {
    let original = fixture()?;
    let value = serde_json::to_value(insert(&original, None))?;
    assert_eq!(value["operation"], "insert_clip");
    assert_eq!(value["before_clip_id"], serde_json::Value::Null);
    let mut invalid = value.clone();
    invalid["at_frame"] = 12.into();
    assert!(serde_json::from_value::<Edit>(invalid).is_err());
    let parsed = serde_json::from_value::<Edit>(value)?;
    assert!(matches!(
        parsed,
        Edit::InsertClip {
            before_clip_id: None,
            ..
        }
    ));
    let schema = serde_json::to_value(schemars::schema_for!(Batch))?;
    assert!(schema.to_string().contains("insert_clip"));
    Ok(())
}
