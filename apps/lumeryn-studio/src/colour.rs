//! Task-focused native controls for persisted, shared colour and visual fades.
use super::*;
use lumeryn_core::visual_effects::{ColourPreset, VisualEffects};

impl Session {
    pub(super) fn colour_inspector(&mut self, ui: &mut egui::Ui, clip_id: Uuid) {
        let Some(project) = &self.project else {
            return;
        };
        let duration = project
            .clips
            .iter()
            .find(|c| c.id == clip_id)
            .map(|c| c.duration_frames)
            .or_else(|| {
                project
                    .visual_clips
                    .iter()
                    .find(|c| c.id == clip_id)
                    .map(|c| c.duration_frames)
            });
        let Some(duration) = duration else {
            return;
        };
        if project.schema_version < 5 {
            return;
        }
        let key = egui::Id::new((
            "colour-form",
            project.id,
            project.revision,
            self.epoch,
            clip_id,
        ));
        let initial = project
            .visual_settings
            .get(&clip_id)
            .and_then(|s| s.effects)
            .unwrap_or_default();
        let mut value = ui
            .data_mut(|d| d.get_temp::<VisualEffects>(key))
            .unwrap_or(initial);
        let mut apply = false;
        let mut reset = false;
        ui.collapsing("Colour & filters", |ui| {
            ui.label(RichText::new("Original looks").small().color(design::MUTED));
            ui.horizontal_wrapped(|ui| {
                for (label, preset) in [
                    ("Neutral", ColourPreset::Neutral),
                    ("Warm story", ColourPreset::WarmStory),
                    ("Cool clarity", ColourPreset::CoolClarity),
                    ("Soft mono", ColourPreset::SoftMono),
                ] {
                    if ui.button(label).clicked() {
                        match VisualEffects::preset(preset, 10000) {
                            Ok(mut recipe) => {
                                recipe.fades = value.fades;
                                value = recipe;
                            }
                            Err(error) => ui
                                .label(RichText::new(error.to_string()).color(design::WARNING))
                                .mark_changed(),
                        }
                    }
                }
            });
            ui.add(
                egui::Slider::new(&mut value.exposure_millistops, -2000..=2000).text("Exposure"),
            );
            ui.add(egui::Slider::new(&mut value.contrast_basis_points, 0..=20000).text("Contrast"));
            ui.add(
                egui::Slider::new(&mut value.saturation_basis_points, 0..=20000).text("Saturation"),
            );
            ui.add(
                egui::Slider::new(&mut value.temperature_basis_points, -10000..=10000)
                    .text("Warmth"),
            );
        });
        ui.collapsing("Visual fade", |ui| {
            ui.label(
                RichText::new("Exact timeline frames")
                    .small()
                    .color(design::MUTED),
            );
            ui.horizontal_wrapped(|ui| {
                ui.label("Fade in");
                ui.add(egui::DragValue::new(&mut value.fades.in_frames).range(0..=duration));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Fade out");
                ui.add(egui::DragValue::new(&mut value.fades.out_frames).range(0..=duration));
            });
        });
        let valid = value.validate(duration);
        if let Err(error) = &valid {
            ui.label(
                RichText::new(error.to_string())
                    .small()
                    .color(design::WARNING),
            );
        }
        ui.label(
            RichText::new("Apply to update preview and export.")
                .small()
                .color(design::MUTED),
        );
        let enabled = self.connected && !self.edit_busy;
        apply |= design::primary(
            ui,
            "Apply colour & fade",
            enabled && valid.is_ok(),
            ui.available_width(),
        )
        .clicked();
        reset |= ui
            .add_enabled(enabled, egui::Button::new("Reset colour & fade"))
            .clicked();
        ui.data_mut(|d| d.insert_temp(key, value));
        if apply || reset {
            self.edit(
                Edit::SetVisualEffects {
                    clip_id,
                    effects: if reset {
                        VisualEffects::default()
                    } else {
                        value
                    },
                },
                "Colour and fade",
            );
        }
    }
}
