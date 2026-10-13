use lumeryn_core::{
    Batch, Edit, MediaTools, Project, Result, Store,
    animation::{Animation, AnimationChannel, AnimationProperty, Interpolation, Keyframe},
    media::{Metadata, Video},
    model::{Clip, Media},
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

fn animation() -> Animation {
    Animation {
        channels: vec![AnimationChannel {
            property: AnimationProperty::Opacity,
            keys: vec![
                Keyframe {
                    at_frame: 0,
                    value: 0,
                    interpolation: Interpolation::Linear,
                },
                Keyframe {
                    at_frame: 30,
                    value: 1_000_000,
                    interpolation: Interpolation::Hold,
                },
            ],
        }],
    }
}
async fn apply(store: &mut Store, edit: Edit, revision: u64) -> Result<()> {
    store
        .execute(
            &Batch {
                operations: vec![edit],
            },
            Some(revision),
            None,
            false,
            &MediaTools::default(),
        )
        .await?;
    Ok(())
}
#[tokio::test]
async fn atomic_animation_reset_history_clipboard_and_rejection() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let project = fixture(dir.path())?;
    let id = project.clips[0].id;
    let mut store = Store::create(&dir.path().join("project.lumeryn"), &project).await?;
    let edit = Edit::SetVisualAnimation {
        clip_id: id,
        animation: animation(),
    };
    store
        .execute(
            &Batch {
                operations: vec![edit.clone()],
            },
            Some(0),
            None,
            true,
            &MediaTools::default(),
        )
        .await?;
    assert!(store.inspect().await?.animations.is_empty());
    apply(&mut store, edit, 0).await?;
    let changed = store.inspect().await?;
    assert!(changed.uses_composition());
    assert_eq!(changed.animations[&id], animation());
    let clipboard = lumeryn_core::clipboard::Clipboard::copy(&changed, id)?;
    assert_eq!(clipboard.version, 3);
    let mut old = clipboard.clone();
    old.version = 2;
    assert!(lumeryn_core::clipboard::Clipboard::from_slice(&serde_json::to_vec(&old)?).is_err());
    assert!(
        apply(
            &mut store,
            Edit::SplitClip {
                clip_id: id,
                right_clip_id: Uuid::new_v4(),
                at_frame: 15
            },
            1
        )
        .await
        .is_err()
    );
    assert!(
        apply(
            &mut store,
            Edit::TrimClip {
                clip_id: id,
                source_in_frame: 0,
                duration_frames: 20
            },
            1
        )
        .await
        .is_err()
    );
    let duplicate = Uuid::new_v4();
    apply(
        &mut store,
        Edit::DuplicateClip {
            clip_id: id,
            id: duplicate,
        },
        1,
    )
    .await?;
    assert_eq!(store.inspect().await?.animations[&duplicate], animation());
    apply(
        &mut store,
        Edit::DeleteClip {
            clip_id: duplicate,
            ripple: false,
        },
        2,
    )
    .await?;
    assert!(!store.inspect().await?.animations.contains_key(&duplicate));
    let pasted = Uuid::new_v4();
    apply(
        &mut store,
        Edit::PasteClip {
            id: pasted,
            at_frame: 30,
            clipboard,
        },
        3,
    )
    .await?;
    assert_eq!(store.inspect().await?.animations[&pasted], animation());
    apply(
        &mut store,
        Edit::SetVisualAnimation {
            clip_id: id,
            animation: Animation::default(),
        },
        4,
    )
    .await?;
    assert!(!store.inspect().await?.animations.contains_key(&id));
    apply(&mut store, Edit::Undo {}, 5).await?;
    assert_eq!(store.inspect().await?.animations[&id], animation());
    std::fs::write(dir.path().join("source.fixture"), b"changed")?;
    assert!(
        apply(
            &mut store,
            Edit::SetVisualAnimation {
                clip_id: id,
                animation: animation()
            },
            6
        )
        .await
        .is_err()
    );
    assert_eq!(store.inspect().await?.revision, 6);
    Ok(())
}
#[test]
fn animation_ids_versions_scale_and_empty_map_are_checked() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut project = fixture(dir.path())?;
    let id = project.clips[0].id;
    assert!(
        !serde_json::to_value(&project)?
            .as_object()
            .unwrap()
            .contains_key("animations")
    );
    project.animations.insert(Uuid::new_v4(), animation());
    assert!(project.validate().is_err());
    project.animations.clear();
    project.animations.insert(id, animation());
    project.schema_version = 4;
    assert!(project.validate().is_err());
    project.schema_version = 5;
    project.animations.get_mut(&id).unwrap().channels[0].property = AnimationProperty::ScaleX;
    assert!(project.validate().is_err());
    Ok(())
}

#[tokio::test]
async fn scene_animation_requires_no_fake_media_and_delete_undo_preserves_keys() -> Result<()> {
    use lumeryn_core::{
        model::{SceneAsset, ScenePlacement, VisualTrack},
        scene_text::{Rgba8, SceneContent, SceneDocument, SceneNode, ShapeBox},
    };
    let dir = tempfile::tempdir()?;
    let mut project = Project::new("Animated scene".into(), 320, 180, "30".parse()?)?;
    let track = Uuid::new_v4();
    let scene = Uuid::new_v4();
    let id = Uuid::new_v4();
    project.visual_tracks.push(VisualTrack {
        id: track,
        name: "Titles".into(),
    });
    project.scenes.push(SceneAsset {
        id: scene,
        name: "Card".into(),
        document: SceneDocument {
            width: 320,
            height: 180,
            nodes: vec![SceneNode {
                id: Uuid::new_v4(),
                content: SceneContent::Rectangle(ShapeBox {
                    x: 10.0,
                    y: 10.0,
                    width: 100.0,
                    height: 60.0,
                    fill: Rgba8 {
                        r: 100,
                        g: 200,
                        b: 180,
                        a: 255,
                    },
                }),
            }],
        },
    });
    project.scene_placements.push(ScenePlacement {
        id,
        scene_id: scene,
        track_id: track,
        at_frame: 0,
        duration_frames: 30,
    });
    let mut store = Store::create(&dir.path().join("project.lumeryn"), &project).await?;
    apply(
        &mut store,
        Edit::SetVisualAnimation {
            clip_id: id,
            animation: animation(),
        },
        0,
    )
    .await?;
    assert!(store.inspect().await?.media.is_empty());
    assert!(
        apply(
            &mut store,
            Edit::SetSceneRange {
                clip_id: id,
                at_frame: 0,
                duration_frames: 20
            },
            1
        )
        .await
        .is_err()
    );
    apply(&mut store, Edit::DeleteScenePlacement { clip_id: id }, 1).await?;
    assert!(store.inspect().await?.animations.is_empty());
    apply(&mut store, Edit::Undo {}, 2).await?;
    assert_eq!(store.inspect().await?.animations[&id], animation());
    Ok(())
}
