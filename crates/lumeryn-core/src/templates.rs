//! Original editable starters compiled to ordinary revision-aware edit batches.
use crate::{
    Batch, Edit, Error, Project, Result,
    animation::{Starter, StarterContent, expand_starter},
    model::{VisualPlacement, VisualSource},
    scene_text::{
        Rgba8, SceneContent, SceneDocument, SceneNode, ShapeBox, TextAlignment, TextNode,
        audited_font,
    },
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextStarter {
    CleanTitle,
    LowerThird,
    Callout,
    ChapterCard,
}

impl TextStarter {
    fn starter(self) -> Starter {
        match self {
            Self::CleanTitle => Starter::CleanTitle,
            Self::LowerThird => Starter::LowerThird,
            Self::Callout => Starter::Callout,
            Self::ChapterCard => Starter::ChapterCard,
        }
    }
    fn id(self) -> &'static str {
        match self {
            Self::CleanTitle => "lumeryn.clean_title",
            Self::LowerThird => "lumeryn.lower_third",
            Self::Callout => "lumeryn.callout",
            Self::ChapterCard => "lumeryn.chapter_card",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TemplateRequest {
    Text {
        preset: TextStarter,
        scene_id: Uuid,
        placement_id: Uuid,
        node_ids: [Uuid; 2],
        track_id: Uuid,
        title: String,
        at_frame: u64,
        duration_frames: u64,
        expected_revision: u64,
    },
    PictureInPicture {
        clip_id: Uuid,
        media_id: Uuid,
        track_id: Uuid,
        at_frame: u64,
        duration_frames: u64,
        source: VisualSource,
        fade_in: bool,
        expected_revision: u64,
    },
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TemplateExpansion {
    pub template_id: String,
    pub template_version: u32,
    pub provenance: String,
    pub expected_revision: u64,
    pub object_ids: Vec<Uuid>,
    pub batch: Batch,
}

fn rgba([r, g, b, a]: [u8; 4]) -> Rgba8 {
    Rgba8 { r, g, b, a }
}

/// Pure expansion retains caller IDs and text. The caller must dry-run/commit
/// the returned batch with this revision and an idempotency key through Store.
pub fn expand(project: &Project, request: &TemplateRequest) -> Result<TemplateExpansion> {
    project.validate()?;
    let (template_id, expected_revision, object_ids, operations) = match request {
        TemplateRequest::Text {
            preset,
            scene_id,
            placement_id,
            node_ids,
            track_id,
            title,
            at_frame,
            duration_frames,
            expected_revision,
        } => {
            let objects = expand_starter(
                preset.starter(),
                project.width,
                project.height,
                *duration_frames,
                title,
                node_ids,
            )?;
            let font = audited_font().0;
            let nodes = objects
                .iter()
                .map(|object| {
                    let content = match &object.content {
                        StarterContent::Rectangle { rgba: color } => {
                            SceneContent::Rectangle(ShapeBox {
                                x: object.x as f32,
                                y: object.y as f32,
                                width: object.width as f32,
                                height: object.height as f32,
                                fill: rgba(*color),
                            })
                        }
                        StarterContent::Text {
                            text,
                            font_size,
                            rgba: color,
                        } => SceneContent::Text(TextNode {
                            text: text.clone(),
                            font: font.clone(),
                            x: object.x as f32,
                            y: object.y as f32,
                            width: object.width as f32,
                            height: object.height as f32,
                            font_size: (*font_size).min((object.width / 18).max(12)) as f32,
                            line_height: (*font_size).min((object.width / 18).max(12)) as f32
                                * 1.25,
                            color: rgba(*color),
                            alignment: if matches!(
                                preset,
                                TextStarter::CleanTitle | TextStarter::ChapterCard
                            ) {
                                TextAlignment::Center
                            } else {
                                TextAlignment::Start
                            },
                        }),
                        StarterContent::MediaSlot {} => {
                            return Err(Error::Invalid(
                                "text starter cannot contain a media slot".into(),
                            ));
                        }
                    };
                    Ok(SceneNode {
                        id: object.id,
                        content,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let document = SceneDocument {
                width: project.width,
                height: project.height,
                nodes,
            };
            (
                preset.id(),
                *expected_revision,
                vec![*scene_id, *placement_id, node_ids[0], node_ids[1]],
                vec![
                    Edit::CreateScene {
                        id: *scene_id,
                        name: title.clone(),
                        document,
                    },
                    Edit::AddScenePlacement {
                        id: *placement_id,
                        scene_id: *scene_id,
                        track_id: *track_id,
                        at_frame: *at_frame,
                        duration_frames: *duration_frames,
                    },
                    Edit::SetVisualAnimation {
                        clip_id: *placement_id,
                        animation: objects[0].animation.clone(),
                    },
                ],
            )
        }
        TemplateRequest::PictureInPicture {
            clip_id,
            media_id,
            track_id,
            at_frame,
            duration_frames,
            source,
            fade_in,
            expected_revision,
        } => {
            let media = project
                .media
                .iter()
                .find(|media| media.id == *media_id)
                .ok_or_else(|| Error::NotFound(format!("template media {media_id}")))?;
            if media.metadata.blake3.is_none() {
                return Err(Error::Invalid(
                    "PIP template requires verified imported media".into(),
                ));
            }
            // Blueprint media slot alone: no extra decorative raster placement.
            let objects = expand_starter(
                Starter::PictureInPicture,
                project.width,
                project.height,
                *duration_frames,
                "PIP",
                &[project.id, *clip_id],
            )?;
            let object = &objects[1];
            let mut operations = vec![Edit::AddVisualClip {
                id: *clip_id,
                media_id: *media_id,
                track_id: *track_id,
                at_frame: *at_frame,
                duration_frames: *duration_frames,
                source: source.clone(),
                placement: Some(VisualPlacement {
                    x: object.x as i32,
                    y: object.y as i32,
                    width: object.width,
                    height: object.height,
                }),
            }];
            if *fade_in {
                operations.push(Edit::SetVisualAnimation {
                    clip_id: *clip_id,
                    animation: object.animation.clone(),
                });
            }
            (
                "lumeryn.picture_in_picture",
                *expected_revision,
                vec![*clip_id],
                operations,
            )
        }
    };
    if expected_revision != project.revision {
        return Err(Error::Conflict {
            expected: expected_revision,
            current: project.revision,
        });
    }
    if object_ids.iter().any(Uuid::is_nil) {
        return Err(Error::Invalid("template IDs must be non-nil".into()));
    }
    let mut candidate = project.clone();
    for operation in &operations {
        candidate.apply(operation, None)?;
    }
    Ok(TemplateExpansion{template_id:template_id.into(),template_version:1,provenance:"Original Lumeryn declarative design; no copied third-party artwork or executable expressions".into(),expected_revision,object_ids,batch:Batch{operations}})
}
