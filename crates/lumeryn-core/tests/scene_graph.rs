use lumeryn_core::{
    Batch, Edit, Error, MediaTools, Project, Result, Store,
    model::{SceneAsset, ScenePlacement, VisualTrack},
    scene_text::{Rgba8, SceneContent, SceneDocument, SceneNode, ShapeBox},
};
use uuid::Uuid;

fn document(id: Uuid) -> SceneDocument {
    SceneDocument {
        width: 320,
        height: 180,
        nodes: vec![SceneNode {
            id,
            content: SceneContent::Rectangle(ShapeBox {
                x: 20.0,
                y: 20.0,
                width: 100.0,
                height: 60.0,
                fill: Rgba8 {
                    r: 93,
                    g: 246,
                    b: 190,
                    a: 128,
                },
            }),
        }],
    }
}

fn fixture() -> Result<Project> {
    let mut project = Project::new("Scene".into(), 320, 180, "30".parse()?)?;
    project.visual_tracks.push(VisualTrack {
        id: Uuid::new_v4(),
        name: "Titles".into(),
    });
    project.scenes.push(SceneAsset {
        id: Uuid::new_v4(),
        name: "Editable title".into(),
        document: document(Uuid::new_v4()),
    });
    project.scene_placements.push(ScenePlacement {
        id: Uuid::new_v4(),
        scene_id: project.scenes[0].id,
        track_id: project.visual_tracks[0].id,
        at_frame: 10,
        duration_frames: 30,
    });
    project.validate()?;
    Ok(project)
}

#[test]
fn real_scene_identity_ranges_and_composition_bounds() -> Result<()> {
    let project = fixture()?;
    assert!(project.media.is_empty());
    assert!(project.uses_composition());
    assert_eq!(project.visual_duration_frames(), 40);
    let mut invalid = project.clone();
    invalid.scenes[0].document.nodes[0].id = invalid.id;
    assert!(invalid.validate().is_err());
    invalid = project.clone();
    invalid.scene_placements[0].scene_id = Uuid::new_v4();
    assert!(invalid.validate().is_err());
    invalid = project.clone();
    invalid.scene_placements[0].duration_frames = 0;
    assert!(invalid.validate().is_err());
    invalid = project.clone();
    invalid.scenes[0].document.width = 100;
    assert!(invalid.validate().is_err());
    invalid = project.clone();
    invalid.schema_version = 4;
    assert!(invalid.validate().is_err());
    let mut adjacent = project.clone();
    let mut second = adjacent.scene_placements[0].clone();
    second.id = Uuid::new_v4();
    second.at_frame = 40;
    adjacent.scene_placements.push(second);
    adjacent.validate()?;
    adjacent.scene_placements[1].at_frame = 39;
    assert!(adjacent.validate().is_err());
    let mut overlapping = project.clone();
    overlapping.visual_tracks.push(VisualTrack {
        id: Uuid::new_v4(),
        name: "Other".into(),
    });
    let mut second = overlapping.scene_placements[0].clone();
    second.id = Uuid::new_v4();
    second.track_id = overlapping.visual_tracks[1].id;
    overlapping.scene_placements.push(second);
    assert!(matches!(overlapping.validate(), Err(Error::Unsupported(_))));
    Ok(())
}

#[tokio::test]
async fn scene_commands_are_atomic_revisioned_editable_and_undoable() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let mut project = Project::new("Scene".into(), 320, 180, "30".parse()?)?;
    let track = Uuid::new_v4();
    project.visual_tracks.push(VisualTrack {
        id: track,
        name: "Titles".into(),
    });
    let mut store = Store::create(&path, &project).await?;
    let tools = MediaTools::default();
    let scene = Uuid::new_v4();
    let clip = Uuid::new_v4();
    let doc = document(Uuid::new_v4());
    let create = Batch {
        operations: vec![
            Edit::CreateScene {
                id: scene,
                name: "Title".into(),
                document: doc.clone(),
            },
            Edit::AddScenePlacement {
                id: clip,
                scene_id: scene,
                track_id: track,
                at_frame: 0,
                duration_frames: 60,
            },
        ],
    };
    assert!(
        store
            .execute(&create, Some(0), None, true, &tools)
            .await?
            .dry_run
    );
    assert!(store.inspect().await?.scenes.is_empty());
    assert_eq!(
        store
            .execute(&create, Some(0), Some("scene-create"), false, &tools)
            .await?
            .object_ids,
        vec![scene, clip]
    );
    assert!(
        store
            .execute(&create, Some(0), Some("scene-create"), false, &tools)
            .await?
            .replayed
    );
    let rejected = Batch {
        operations: vec![
            Edit::SetSceneRange {
                clip_id: clip,
                at_frame: 10,
                duration_frames: 70,
            },
            Edit::DeleteScene { scene_id: scene },
        ],
    };
    assert!(
        store
            .execute(&rejected, Some(1), None, false, &tools)
            .await
            .is_err()
    );
    assert_eq!(store.inspect().await?.visual_duration_frames(), 60);
    let mut edited = doc.clone();
    if let SceneContent::Rectangle(shape) = &mut edited.nodes[0].content {
        shape.width = 140.0;
    }
    let set = Batch {
        operations: vec![
            Edit::SetScene {
                scene_id: scene,
                document: edited.clone(),
            },
            Edit::MoveScenePlacement {
                clip_id: clip,
                track_id: track,
                at_frame: 10,
            },
            Edit::SetSceneRange {
                clip_id: clip,
                at_frame: 10,
                duration_frames: 50,
            },
        ],
    };
    assert!(matches!(
        store.execute(&set, Some(0), None, false, &tools).await,
        Err(Error::Conflict { .. })
    ));
    store.execute(&set, Some(1), None, false, &tools).await?;
    assert_eq!(store.inspect().await?.scenes[0].document, edited);
    let undo = Batch {
        operations: vec![Edit::Undo {}],
    };
    store.execute(&undo, Some(2), None, false, &tools).await?;
    assert_eq!(store.inspect().await?.scenes[0].document, doc);
    let redo = Batch {
        operations: vec![Edit::Redo {}],
    };
    store.execute(&redo, Some(3), None, false, &tools).await?;
    assert_eq!(store.inspect().await?.scenes[0].document, edited);
    let canvas = Batch {
        operations: vec![Edit::SetCanvas {
            width: 640,
            height: 360,
        }],
    };
    assert!(
        store
            .execute(&canvas, Some(4), None, false, &tools)
            .await
            .is_err()
    );
    let fps = Batch {
        operations: vec![Edit::SetFrameRate {
            frame_rate: "60".parse()?,
        }],
    };
    assert!(
        store
            .execute(&fps, Some(4), None, false, &tools)
            .await
            .is_err()
    );
    let track_delete = Batch {
        operations: vec![Edit::DeleteVisualTrack { track_id: track }],
    };
    assert!(
        store
            .execute(&track_delete, Some(4), None, false, &tools)
            .await
            .is_err()
    );
    drop(store);
    let mut reopened = Store::open(&path).await?;
    assert_eq!(reopened.inspect().await?.scenes[0].document, edited);
    reopened
        .execute(
            &Batch {
                operations: vec![
                    Edit::DeleteScenePlacement { clip_id: clip },
                    Edit::DeleteScene { scene_id: scene },
                ],
            },
            Some(4),
            None,
            false,
            &tools,
        )
        .await?;
    assert!(reopened.inspect().await?.scenes.is_empty());
    Ok(())
}

#[tokio::test]
async fn schema_four_migrates_all_history_without_changing_ids() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let mut legacy = Project::new("Legacy".into(), 320, 180, "30".parse()?)?;
    legacy.schema_version = 4;
    let mut store = Store::create(&path, &legacy).await?;
    assert_eq!(store.inspect().await?.schema_version, 4);
    store.migrate().await?;
    let project = store.inspect().await?;
    assert_eq!(project.schema_version, 5);
    assert_eq!(project.id, legacy.id);
    assert_eq!(project.revision, 0);
    assert!(project.scenes.is_empty());
    store
        .execute(
            &Batch {
                operations: vec![Edit::CreateScene {
                    id: Uuid::new_v4(),
                    name: "New".into(),
                    document: document(Uuid::new_v4()),
                }],
            },
            Some(0),
            None,
            false,
            &MediaTools::default(),
        )
        .await?;
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
    assert_eq!(store.inspect().await?.schema_version, 5);
    assert!(store.inspect().await?.scenes.is_empty());
    Ok(())
}
