use lumeryn_core::{
    Edit, MediaTools, Project, Result, Store,
    model::VisualTrack,
    scene_text::{SceneContent, SceneRenderer, audited_font},
    templates::{TemplateRequest, TextStarter, expand},
};
use uuid::Uuid;

fn fixture(width: u32, height: u32) -> Result<Project> {
    let mut project = Project::new("Starters".into(), width, height, "30".parse()?)?;
    project.visual_tracks.push(VisualTrack {
        id: Uuid::new_v4(),
        name: "Title".into(),
    });
    Ok(project)
}
fn request(project: &Project, preset: TextStarter) -> TemplateRequest {
    TemplateRequest::Text {
        preset,
        scene_id: Uuid::new_v4(),
        placement_id: Uuid::new_v4(),
        node_ids: [Uuid::new_v4(), Uuid::new_v4()],
        track_id: project.visual_tracks[0].id,
        title: "A useful idea".into(),
        at_frame: 0,
        duration_frames: 90,
        expected_revision: project.revision,
    }
}

#[test]
fn all_original_text_starters_shape_and_expand_repeatably() -> Result<()> {
    for (width, height) in [(320, 180), (1920, 1080), (2160, 3840)] {
        let project = fixture(width, height)?;
        let (font, bytes) = audited_font();
        let mut renderer = SceneRenderer::new(font, bytes)
            .map_err(|e| lumeryn_core::Error::Invalid(e.to_string()))?;
        for preset in [
            TextStarter::CleanTitle,
            TextStarter::LowerThird,
            TextStarter::Callout,
            TextStarter::ChapterCard,
        ] {
            let request = request(&project, preset);
            let expanded = expand(&project, &request)?;
            assert_eq!(
                serde_json::to_value(&expanded)?,
                serde_json::to_value(expand(&project, &request)?)?
            );
            assert_eq!(expanded.batch.operations.len(), 3);
            assert_eq!(expanded.object_ids.len(), 4);
            if let Edit::CreateScene { document, .. } = &expanded.batch.operations[0] {
                assert_eq!(document.nodes.len(), 2);
                for node in &document.nodes {
                    if let SceneContent::Text(text) = &node.content {
                        let layout = renderer
                            .layout(text)
                            .map_err(|e| lumeryn_core::Error::Invalid(e.to_string()))?;
                        assert!(layout.height <= text.height + 0.01);
                        assert!(text.line_height >= text.font_size);
                    }
                }
            } else {
                return Err(lumeryn_core::Error::Invalid(
                    "template did not create an editable scene".into(),
                ));
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn template_atomic_dryrun_and_undo_keep_original_editable_nodes() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let project = fixture(1920, 1080)?;
    let request = request(&project, TextStarter::LowerThird);
    let expanded = expand(&project, &request)?;
    let mut store = Store::create(&dir.path().join("project.lumeryn"), &project).await?;
    store
        .execute(
            &expanded.batch,
            Some(0),
            Some("starter"),
            true,
            &MediaTools::default(),
        )
        .await?;
    assert!(store.inspect().await?.scenes.is_empty());
    store
        .execute(
            &expanded.batch,
            Some(0),
            Some("starter"),
            false,
            &MediaTools::default(),
        )
        .await?;
    let changed = store.inspect().await?;
    assert_eq!(changed.scenes.len(), 1);
    assert_eq!(changed.scene_placements.len(), 1);
    assert_eq!(changed.animations.len(), 1);
    assert!(changed.media.is_empty());
    assert!(
        store
            .execute(
                &expanded.batch,
                Some(0),
                Some("starter"),
                false,
                &MediaTools::default()
            )
            .await?
            .replayed
    );
    store
        .execute(
            &lumeryn_core::Batch {
                operations: vec![Edit::Undo {}],
            },
            Some(1),
            None,
            false,
            &MediaTools::default(),
        )
        .await?;
    assert!(store.inspect().await?.scenes.is_empty());
    Ok(())
}

#[test]
fn stale_ids_unknown_fields_missing_track_and_overflow_are_rejected() -> Result<()> {
    let project = fixture(1920, 1080)?;
    let mut item = request(&project, TextStarter::CleanTitle);
    if let TemplateRequest::Text {
        expected_revision, ..
    } = &mut item
    {
        *expected_revision = 1;
    }
    assert!(expand(&project, &item).is_err());
    item = request(&project, TextStarter::CleanTitle);
    if let TemplateRequest::Text { node_ids, .. } = &mut item {
        node_ids[1] = node_ids[0];
    }
    assert!(expand(&project, &item).is_err());
    item = request(&project, TextStarter::CleanTitle);
    if let TemplateRequest::Text { track_id, .. } = &mut item {
        *track_id = Uuid::new_v4();
    }
    assert!(expand(&project, &item).is_err());
    item = request(&project, TextStarter::CleanTitle);
    if let TemplateRequest::Text { at_frame, .. } = &mut item {
        *at_frame = u64::MAX;
    }
    assert!(expand(&project, &item).is_err());
    let mut json = serde_json::to_value(request(&project, TextStarter::CleanTitle))?;
    json["script"] = serde_json::json!("untrusted");
    assert!(serde_json::from_value::<TemplateRequest>(json).is_err());
    Ok(())
}

#[test]
fn pip_is_one_verified_editable_media_placement_with_explicit_source() -> Result<()> {
    use lumeryn_core::{
        media::{Metadata, Video},
        model::{Media, VisualSource},
    };
    let mut project = fixture(1920, 1080)?;
    let media_id = Uuid::new_v4();
    project.media.push(Media {
        id: media_id,
        path: "source.fixture".into(),
        alias: None,
        metadata: Metadata {
            size_bytes: 1,
            blake3: Some("a".repeat(64)),
            audio: None,
            video: Some(Video {
                codec: "ffv1".into(),
                width: 1920,
                height: 1080,
                frame_rate: project.frame_rate,
                frames: 120,
                start_time_micros: 0,
                sample_aspect_ratio: "1:1".into(),
                color_transfer: None,
                constant_rate: true,
            }),
        },
    });
    let request = TemplateRequest::PictureInPicture {
        clip_id: Uuid::new_v4(),
        media_id,
        track_id: project.visual_tracks[0].id,
        at_frame: 0,
        duration_frames: 60,
        source: VisualSource::Video {
            source_in_frame: 10,
        },
        fade_in: true,
        expected_revision: 0,
    };
    let expanded = expand(&project, &request)?;
    assert_eq!(expanded.batch.operations.len(), 2);
    assert_eq!(expanded.object_ids.len(), 1);
    if let Edit::AddVisualClip {
        source, placement, ..
    } = &expanded.batch.operations[0]
    {
        assert_eq!(
            *source,
            VisualSource::Video {
                source_in_frame: 10
            }
        );
        assert!(
            placement
                .as_ref()
                .is_some_and(|rect| rect.width < project.width && rect.height < project.height)
        );
    } else {
        return Err(lumeryn_core::Error::Invalid("PIP did not add media".into()));
    }
    project.media[0].metadata.blake3 = None;
    assert!(expand(&project, &request).is_err());
    Ok(())
}
