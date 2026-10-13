//! Native creation and editing of the same persisted scenes exposed to the CLI.
use super::*;
use lumeryn_core::scene_text::{
    Rgba8, SceneContent, SceneDocument, SceneNode, ShapeBox, TextAlignment, TextNode,
};

pub(super) struct SceneForm {
    placement: Uuid,
    scene: Uuid,
    revision: u64,
    at: u64,
    duration: u64,
    document: SceneDocument,
}

impl Session {
    pub(super) fn scenes_panel(&mut self, ui: &mut egui::Ui) {
        let Some(project) = self.project.as_ref() else {
            ui.label("Open a project to add text and shapes.");
            return;
        };
        if project.schema_version < 5 {
            ui.label("Migrate this project explicitly to use editable scenes.");
            return;
        }
        let enabled = self.connected && !self.edit_busy;
        let mut create = None;
        let mut starter = None;
        ui.label(RichText::new("Editable overlays").strong());
        ui.label(
            RichText::new("Titles and shapes stay editable in your project.")
                .small()
                .color(design::MUTED),
        );
        for (label, text) in [("Add title", true), ("Add rectangle", false)] {
            if design::primary(ui, label, enabled, ui.available_width()).clicked() {
                create = Some(text);
            }
        }
        ui.separator();
        ui.label(RichText::new("Editable starters").strong());
        for (label, preset) in [
            (
                "Clean title",
                lumeryn_core::templates::TextStarter::CleanTitle,
            ),
            (
                "Lower third",
                lumeryn_core::templates::TextStarter::LowerThird,
            ),
            ("Callout", lumeryn_core::templates::TextStarter::Callout),
            (
                "Chapter card",
                lumeryn_core::templates::TextStarter::ChapterCard,
            ),
        ] {
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(label).min_size(Vec2::new(ui.available_width(), 36.0)),
                )
                .clicked()
            {
                starter = Some(preset);
            }
        }
        ui.separator();
        let mut selected = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for placement in &project.scene_placements {
                if let Some(scene) = project.scenes.iter().find(|s| s.id == placement.scene_id)
                    && ui
                        .selectable_label(self.selected == Some(placement.id), &scene.name)
                        .clicked()
                {
                    selected = Some(placement.id);
                }
            }
        });
        if let Some(id) = selected {
            self.select(id);
        }
        if let Some(text) = create {
            self.create_scene_overlay(text);
        }
        if let Some(preset) = starter {
            self.create_text_starter(preset);
        }
    }

    fn create_text_starter(&mut self, preset: lumeryn_core::templates::TextStarter) {
        let Some(project) = &self.project else {
            return;
        };
        let mut candidate = project.clone();
        let mut operations = Vec::new();
        let track_id = candidate
            .visual_tracks
            .iter()
            .find(|track| {
                !candidate
                    .visual_clips
                    .iter()
                    .any(|c| c.track_id == track.id)
                    && !candidate
                        .scene_placements
                        .iter()
                        .any(|c| c.track_id == track.id)
            })
            .map_or_else(|| Uuid::new_v4(), |track| track.id);
        if !candidate
            .visual_tracks
            .iter()
            .any(|track| track.id == track_id)
        {
            candidate
                .visual_tracks
                .push(lumeryn_core::model::VisualTrack {
                    id: track_id,
                    name: "Titles & shapes".into(),
                });
            operations.push(Edit::AddVisualTrack {
                id: track_id,
                name: "Titles & shapes".into(),
            });
        }
        let placement_id = Uuid::new_v4();
        let request = lumeryn_core::templates::TemplateRequest::Text {
            preset,
            scene_id: Uuid::new_v4(),
            placement_id,
            node_ids: [Uuid::new_v4(), Uuid::new_v4()],
            track_id,
            title: "Your title".into(),
            at_frame: self.playhead,
            duration_frames: (u64::from(project.frame_rate.numerator()) * 5
                / u64::from(project.frame_rate.denominator()))
            .max(2),
            expected_revision: project.revision,
        };
        match lumeryn_core::templates::expand(&candidate, &request) {
            Ok(expansion) => {
                operations.extend(expansion.batch.operations);
                self.selected = Some(placement_id);
                self.scene_form = None;
                self.edit_batch(Batch { operations }, "Add editable starter");
            }
            Err(error) => self.fail(error.to_string()),
        }
    }

    fn create_scene_overlay(&mut self, text: bool) {
        let Some(project) = &self.project else {
            return;
        };
        let (width, height) = (project.width, project.height);
        let scene = Uuid::new_v4();
        let placement = Uuid::new_v4();
        let mut operations = Vec::new();
        // Prefer an unused layer; the shared graph rejects overlap/capacity explicitly.
        let track = project
            .visual_tracks
            .iter()
            .find(|track| {
                !project.visual_clips.iter().any(|c| c.track_id == track.id)
                    && !project
                        .scene_placements
                        .iter()
                        .any(|c| c.track_id == track.id)
            })
            .map_or_else(
                || {
                    let id = Uuid::new_v4();
                    operations.push(Edit::AddVisualTrack {
                        id,
                        name: "Titles & shapes".into(),
                    });
                    id
                },
                |t| t.id,
            );
        let content = if text {
            let (font, _) = lumeryn_core::scene_text::audited_font();
            SceneContent::Text(TextNode {
                text: "Your title".into(),
                font,
                x: width as f32 * 0.1,
                y: height as f32 * 0.12,
                width: width as f32 * 0.8,
                height: height as f32 * 0.5,
                font_size: height as f32 * 0.075,
                line_height: height as f32 * 0.075 * 1.25,
                color: Rgba8 {
                    r: 245,
                    g: 248,
                    b: 255,
                    a: 255,
                },
                alignment: TextAlignment::Center,
            })
        } else {
            SceneContent::Rectangle(ShapeBox {
                x: width as f32 * 0.1,
                y: height as f32 * 0.75,
                width: width as f32 * 0.8,
                height: height as f32 * 0.15,
                fill: Rgba8 {
                    r: 102,
                    g: 245,
                    b: 202,
                    a: 230,
                },
            })
        };
        operations.push(Edit::CreateScene {
            id: scene,
            name: if text { "Title" } else { "Rectangle" }.into(),
            document: SceneDocument {
                width,
                height,
                nodes: vec![SceneNode {
                    id: Uuid::new_v4(),
                    content,
                }],
            },
        });
        let duration =
            project.frame_rate.numerator() as u64 * 5 / project.frame_rate.denominator() as u64;
        operations.push(Edit::AddScenePlacement {
            id: placement,
            scene_id: scene,
            track_id: track,
            at_frame: self.playhead,
            duration_frames: duration.max(1),
        });
        self.selected = Some(placement);
        self.scene_form = None;
        self.edit_batch(Batch { operations }, "Add editable overlay");
    }

    pub(super) fn scene_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            return;
        };
        let Some(placement) = project
            .scene_placements
            .iter()
            .find(|p| Some(p.id) == self.selected)
        else {
            return;
        };
        let Some(scene) = project.scenes.iter().find(|s| s.id == placement.scene_id) else {
            return;
        };
        if self
            .scene_form
            .as_ref()
            .is_none_or(|f| f.placement != placement.id || f.revision != project.revision)
        {
            self.scene_form = Some(SceneForm {
                placement: placement.id,
                scene: scene.id,
                revision: project.revision,
                at: placement.at_frame,
                duration: placement.duration_frames,
                document: scene.document.clone(),
            });
        }
        let Some(form) = &mut self.scene_form else {
            return;
        };
        ui.label(RichText::new(&scene.name).strong());
        ui.label(
            RichText::new("Changes apply to every placement of this scene.")
                .small()
                .color(design::MUTED),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("Start frame");
            ui.add(egui::DragValue::new(&mut form.at).range(0..=lumeryn_core::model::MAX_FRAMES));
            ui.label("Frames");
            ui.add(
                egui::DragValue::new(&mut form.duration).range(1..=lumeryn_core::model::MAX_FRAMES),
            );
        });
        for node in &mut form.document.nodes {
            ui.push_id(node.id, |ui| {
                ui.separator();
                match &mut node.content {
                    SceneContent::Text(text) => {
                        ui.add(
                            egui::TextEdit::multiline(&mut text.text)
                                .desired_width(f32::INFINITY)
                                .desired_rows(3),
                        );
                        let line_ratio = (text.line_height / text.font_size).max(1.0);
                        if ui
                            .add(egui::Slider::new(&mut text.font_size, 4.0..=256.0).text("Size"))
                            .changed()
                        {
                            text.line_height = text.font_size * line_ratio;
                        }
                        egui::ComboBox::from_id_salt("alignment")
                            .selected_text(format!("{:?}", text.alignment))
                            .show_ui(ui, |ui| {
                                for (label, value) in [
                                    ("Left", TextAlignment::Start),
                                    ("Centre", TextAlignment::Center),
                                    ("Right", TextAlignment::End),
                                ] {
                                    ui.selectable_value(&mut text.alignment, value, label);
                                }
                            });
                        color(ui, &mut text.color);
                        bounds(
                            ui,
                            &mut text.x,
                            &mut text.y,
                            &mut text.width,
                            &mut text.height,
                        );
                    }
                    SceneContent::Rectangle(shape) | SceneContent::Ellipse(shape) => {
                        color(ui, &mut shape.fill);
                        bounds(
                            ui,
                            &mut shape.x,
                            &mut shape.y,
                            &mut shape.width,
                            &mut shape.height,
                        );
                    }
                    SceneContent::Connector(line) => {
                        color(ui, &mut line.color);
                        ui.add(
                            egui::DragValue::new(&mut line.stroke_width)
                                .range(0.1..=256.0)
                                .prefix("Stroke "),
                        );
                        egui::Grid::new("connector-endpoints")
                            .num_columns(2)
                            .show(ui, |ui| {
                                for (label, value) in [
                                    ("Start X", &mut line.x1),
                                    ("Start Y", &mut line.y1),
                                    ("End X", &mut line.x2),
                                    ("End Y", &mut line.y2),
                                ] {
                                    ui.label(label);
                                    ui.add(egui::DragValue::new(value));
                                    ui.end_row();
                                }
                            });
                    }
                }
            });
        }
        let last_reference = project
            .scene_placements
            .iter()
            .filter(|p| p.scene_id == form.scene)
            .count()
            == 1;
        let valid = lumeryn_core::scene_text::validate_audited_documents([&form.document]);
        if let Err(error) = &valid {
            ui.label(RichText::new(error.to_string()).color(design::WARNING));
        }
        let enabled = self.connected && !self.edit_busy;
        let apply = design::primary(
            ui,
            "Apply changes",
            enabled && valid.is_ok(),
            ui.available_width(),
        )
        .clicked();
        let delete = ui
            .add_enabled(enabled, egui::Button::new("Remove overlay"))
            .clicked();
        if apply {
            let operations = vec![
                Edit::SetScene {
                    scene_id: form.scene,
                    document: form.document.clone(),
                },
                Edit::SetSceneRange {
                    clip_id: form.placement,
                    at_frame: form.at,
                    duration_frames: form.duration,
                },
            ];
            self.edit_batch(Batch { operations }, "Edit scene");
        } else if delete {
            let clip_id = form.placement;
            let mut operations = vec![Edit::DeleteScenePlacement { clip_id }];
            if last_reference {
                operations.push(Edit::DeleteScene {
                    scene_id: form.scene,
                });
            }
            self.edit_batch(Batch { operations }, "Remove overlay");
        }
        let selected = self.selected;
        if let Some(clip_id) = selected {
            self.animation_inspector(ui, clip_id);
        }
    }
}

fn color(ui: &mut egui::Ui, value: &mut Rgba8) {
    let mut rgba = [value.r, value.g, value.b, value.a];
    ui.horizontal(|ui| {
        ui.label("Colour");
        ui.color_edit_button_srgba_unmultiplied(&mut rgba);
    });
    *value = Rgba8 {
        r: rgba[0],
        g: rgba[1],
        b: rgba[2],
        a: rgba[3],
    };
}
fn bounds(ui: &mut egui::Ui, x: &mut f32, y: &mut f32, width: &mut f32, height: &mut f32) {
    egui::Grid::new("scene-bounds")
        .num_columns(2)
        .show(ui, |ui| {
            for (label, value) in [("X", x), ("Y", y), ("Width", width), ("Height", height)] {
                ui.label(label);
                ui.add(egui::DragValue::new(value).speed(1.0));
                ui.end_row();
            }
        });
}
