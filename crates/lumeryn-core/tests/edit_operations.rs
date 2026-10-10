use lumeryn_core::{
    Batch, Edit, Error, MediaTools, Project, Result, Store,
    media::{Metadata, Video},
    model::{Clip, MAX_FRAMES, Media},
};
use uuid::Uuid;

fn fixture() -> Result<Project> {
    let mut project = Project::new("Sequence".into(), 1920, 1080, "30000/1001".parse()?)?;
    let media_id = Uuid::new_v4();
    project.media.push(Media {
        id: media_id,
        path: "source.mp4".into(),
        alias: None,
        metadata: Metadata {
            size_bytes: 10,
            blake3: None,
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
    project.clips = [(0, 10, 60), (60, 200, 30), (90, 500, 40)]
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

async fn edit(store: &mut Store, operation: Edit, revision: u64) -> Result<()> {
    store
        .execute(
            &Batch {
                operations: vec![operation],
            },
            Some(revision),
            None,
            false,
            &MediaTools::default(),
        )
        .await?;
    Ok(())
}

fn positions(project: &Project) -> Vec<(Uuid, u64, u64, u64)> {
    project
        .clips
        .iter()
        .map(|c| (c.id, c.at_frame, c.source_in_frame, c.duration_frames))
        .collect()
}

#[tokio::test]
async fn exact_split_duplicate_reorder_and_ripple_delete_preserve_sources() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("source.mp4");
    std::fs::write(&source, b"immutable source fixture")?;
    let source_bytes = std::fs::read(&source)?;
    let project = fixture()?;
    let original_media = serde_json::to_value(&project.media)?;
    let first = project.clips[0].id;
    let second = project.clips[1].id;
    let third = project.clips[2].id;
    let right = Uuid::new_v4();
    let duplicate = Uuid::new_v4();
    let mut store = Store::create(&directory.path().join("project.lumeryn"), &project).await?;
    edit(
        &mut store,
        Edit::SplitClip {
            clip_id: first,
            right_clip_id: right,
            at_frame: 25,
        },
        0,
    )
    .await?;
    assert_eq!(
        positions(&store.inspect().await?),
        vec![
            (first, 0, 10, 25),
            (right, 25, 35, 35),
            (second, 60, 200, 30),
            (third, 90, 500, 40)
        ]
    );
    edit(
        &mut store,
        Edit::DuplicateClip {
            clip_id: right,
            id: duplicate,
        },
        1,
    )
    .await?;
    assert_eq!(
        positions(&store.inspect().await?),
        vec![
            (first, 0, 10, 25),
            (right, 25, 35, 35),
            (duplicate, 60, 35, 35),
            (second, 95, 200, 30),
            (third, 125, 500, 40)
        ]
    );
    edit(
        &mut store,
        Edit::ReorderClip {
            clip_id: third,
            before_clip_id: Some(first),
        },
        2,
    )
    .await?;
    assert_eq!(
        positions(&store.inspect().await?),
        vec![
            (third, 0, 500, 40),
            (first, 40, 10, 25),
            (right, 65, 35, 35),
            (duplicate, 100, 35, 35),
            (second, 135, 200, 30)
        ]
    );
    edit(
        &mut store,
        Edit::ReorderClip {
            clip_id: third,
            before_clip_id: None,
        },
        3,
    )
    .await?;
    edit(
        &mut store,
        Edit::DeleteClip {
            clip_id: duplicate,
            ripple: true,
        },
        4,
    )
    .await?;
    assert_eq!(
        positions(&store.inspect().await?),
        vec![
            (first, 0, 10, 25),
            (right, 25, 35, 35),
            (second, 60, 200, 30),
            (third, 90, 500, 40)
        ]
    );
    let snapshot = store.inspect().await?;
    assert_eq!(snapshot.schema_version, 1);
    assert_eq!(snapshot.frame_rate, project.frame_rate);
    assert_eq!(serde_json::to_value(&snapshot.media)?, original_media);
    assert_eq!(std::fs::read(source)?, source_bytes);
    store.integrity_check().await?;
    Ok(())
}

#[tokio::test]
async fn split_boundaries_uuid_collisions_missing_targets_and_canvas_roll_back() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let project = fixture()?;
    let clip = project.clips[0].id;
    let mut store = Store::create(&directory.path().join("project.lumeryn"), &project).await?;
    let invalid = [
        Edit::SplitClip {
            clip_id: clip,
            right_clip_id: Uuid::new_v4(),
            at_frame: 0,
        },
        Edit::SplitClip {
            clip_id: clip,
            right_clip_id: Uuid::new_v4(),
            at_frame: 60,
        },
        Edit::SplitClip {
            clip_id: clip,
            right_clip_id: project.media[0].id,
            at_frame: 20,
        },
        Edit::DuplicateClip {
            clip_id: clip,
            id: project.id,
        },
        Edit::DuplicateClip {
            clip_id: Uuid::new_v4(),
            id: Uuid::new_v4(),
        },
        Edit::DeleteClip {
            clip_id: Uuid::new_v4(),
            ripple: true,
        },
        Edit::ReorderClip {
            clip_id: clip,
            before_clip_id: Some(clip),
        },
        Edit::ReorderClip {
            clip_id: clip,
            before_clip_id: Some(Uuid::new_v4()),
        },
        Edit::SetCanvas {
            width: 1921,
            height: 1080,
        },
        Edit::SetCanvas {
            width: 8194,
            height: 1080,
        },
    ];
    for operation in invalid {
        let batch = Batch {
            operations: vec![
                Edit::SetCanvas {
                    width: 1080,
                    height: 1920,
                },
                operation,
            ],
        };
        assert!(
            store
                .execute(&batch, Some(0), None, false, &MediaTools::default())
                .await
                .is_err()
        );
        assert_eq!(
            serde_json::to_value(store.inspect().await?)?,
            serde_json::to_value(&project)?
        );
        assert_eq!(store.history().await?.len(), 1);
    }
    store.integrity_check().await?;
    Ok(())
}

#[tokio::test]
async fn edits_dry_run_conflict_and_idempotent_replay_are_transactional() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let project = fixture()?;
    let clip = project.clips[1].id;
    let right = Uuid::new_v4();
    let batch = Batch {
        operations: vec![
            Edit::SplitClip {
                clip_id: clip,
                right_clip_id: right,
                at_frame: 61,
            },
            Edit::SetCanvas {
                width: 1080,
                height: 1920,
            },
        ],
    };
    let tools = MediaTools::default();
    let mut store = Store::create(&path, &project).await?;
    let dry = store
        .execute(&batch, Some(0), Some("cut"), true, &tools)
        .await?;
    assert!(dry.dry_run);
    assert_eq!(dry.revision, 0);
    assert_eq!(dry.object_ids, vec![clip, right, project.id]);
    assert_eq!(store.history().await?.len(), 1);
    assert_eq!(
        serde_json::to_value(store.inspect().await?)?,
        serde_json::to_value(&project)?
    );
    let receipt = store
        .execute(&batch, Some(0), Some("cut"), false, &tools)
        .await?;
    assert_eq!(receipt.revision, 1);
    assert!(!receipt.replayed);
    let replay = store
        .execute(&batch, Some(0), Some("cut"), false, &tools)
        .await?;
    assert!(replay.replayed);
    assert_eq!(replay.object_ids, receipt.object_ids);
    assert_eq!(store.history().await?.len(), 2);
    assert!(matches!(
        store
            .execute(
                &Batch {
                    operations: vec![Edit::SetCanvas {
                        width: 720,
                        height: 1280
                    }]
                },
                Some(0),
                None,
                false,
                &tools
            )
            .await,
        Err(Error::Conflict {
            expected: 0,
            current: 1
        })
    ));
    assert!(matches!(
        store
            .execute(
                &Batch {
                    operations: vec![Edit::SetCanvas {
                        width: 720,
                        height: 1280
                    }]
                },
                Some(1),
                Some("cut"),
                false,
                &tools
            )
            .await,
        Err(Error::Invalid(_))
    ));
    let mut reopened = Store::open(&path).await?;
    assert!(
        reopened
            .execute(&batch, Some(0), Some("cut"), false, &tools)
            .await?
            .replayed
    );
    reopened.integrity_check().await?;
    Ok(())
}

#[tokio::test]
async fn non_ripple_gap_is_explicit_and_sequence_edits_reject_it() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let project = fixture()?;
    let first = project.clips[0].id;
    let second = project.clips[1].id;
    let third = project.clips[2].id;
    let mut store = Store::create(&directory.path().join("project.lumeryn"), &project).await?;
    edit(
        &mut store,
        Edit::DeleteClip {
            clip_id: second,
            ripple: false,
        },
        0,
    )
    .await?;
    let snapshot = store.inspect().await?;
    assert_eq!(
        positions(&snapshot),
        vec![(first, 0, 10, 60), (third, 90, 500, 40)]
    );
    for operation in [
        Edit::DuplicateClip {
            clip_id: first,
            id: Uuid::new_v4(),
        },
        Edit::ReorderClip {
            clip_id: third,
            before_clip_id: Some(first),
        },
        Edit::DeleteClip {
            clip_id: first,
            ripple: true,
        },
    ] {
        assert!(matches!(
            store
                .execute(
                    &Batch {
                        operations: vec![operation]
                    },
                    Some(1),
                    None,
                    false,
                    &MediaTools::default()
                )
                .await,
            Err(Error::Unsupported(_))
        ));
        assert_eq!(
            serde_json::to_value(store.inspect().await?)?,
            serde_json::to_value(&snapshot)?
        );
    }
    let default_delete: Edit =
        serde_json::from_value(serde_json::json!({"operation":"delete_clip","clip_id":first}))?;
    assert!(matches!(
        default_delete,
        Edit::DeleteClip { ripple: true, .. }
    ));
    Ok(())
}

#[tokio::test]
async fn canvas_presets_keep_fps_and_sequence_overflow_is_rejected() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut project = fixture()?;
    let clip = project.clips[0].id;
    project.clips.truncate(1);
    project.clips[0].source_in_frame = 0;
    project.clips[0].duration_frames = MAX_FRAMES;
    let mut store = Store::create(&directory.path().join("project.lumeryn"), &project).await?;
    assert!(matches!(
        store
            .execute(
                &Batch {
                    operations: vec![Edit::DuplicateClip {
                        clip_id: clip,
                        id: Uuid::new_v4()
                    }]
                },
                Some(0),
                None,
                false,
                &MediaTools::default()
            )
            .await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(store.inspect().await?.clips.len(), 1);
    for (revision, (width, height)) in [
        (1920, 1080),
        (1080, 1920),
        (1080, 1080),
        (1440, 1080),
        (1080, 1350),
        (2520, 1080),
        (1080, 1620),
    ]
    .into_iter()
    .enumerate()
    {
        edit(
            &mut store,
            Edit::SetCanvas { width, height },
            revision as u64,
        )
        .await?;
        let snapshot = store.inspect().await?;
        assert_eq!((snapshot.width, snapshot.height), (width, height));
        assert_eq!(snapshot.frame_rate, project.frame_rate);
        assert_eq!(snapshot.schema_version, 1);
    }
    store.integrity_check().await?;
    Ok(())
}
