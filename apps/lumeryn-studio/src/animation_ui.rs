//! Editable clip-local animation; commits use the authoritative project service.
use super::*;
use lumeryn_core::animation::{
    Animation, AnimationChannel, AnimationProperty, Interpolation, Keyframe,
};

impl Session {
    pub(super) fn animation_inspector(&mut self, ui: &mut egui::Ui, clip_id: Uuid) {
        let Some(project) = &self.project else {
            return;
        };
        if project.schema_version < 5 {
            return;
        }
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
            })
            .or_else(|| {
                project
                    .scene_placements
                    .iter()
                    .find(|c| c.id == clip_id)
                    .map(|c| c.duration_frames)
            });
        let Some(duration) = duration else {
            return;
        };
        let key = egui::Id::new((
            "animation-form",
            project.id,
            project.revision,
            self.epoch,
            clip_id,
        ));
        let mut animation = ui
            .data_mut(|d| d.get_temp::<Animation>(key))
            .unwrap_or_else(|| {
                project
                    .animations
                    .get(&clip_id)
                    .cloned()
                    .unwrap_or_default()
            });
        let entrance = (u64::from(project.frame_rate.numerator())
            / u64::from(project.frame_rate.denominator())
            / 2)
        .max(1)
        .min(duration);
        let mut apply = false;
        let mut reset = false;
        ui.collapsing("Animation", |ui| {
            if ui.button("Fade entrance").clicked() {
                animation = Animation { channels: vec![AnimationChannel { property: AnimationProperty::Opacity, keys: vec![
                    Keyframe { at_frame: 0, value: 0, interpolation: Interpolation::SmoothStep },
                    Keyframe { at_frame: entrance, value: 1_000_000, interpolation: Interpolation::Hold },
                ] }] };
            }
            for channel in &mut animation.channels {
                ui.push_id(format!("{:?}", channel.property), |ui| {
                    ui.label(RichText::new(format!("{:?}", channel.property)).strong());
                    let mut remove = None;
                    for (index, frame) in channel.keys.iter_mut().enumerate() {
                        ui.push_id(index, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Frame"); ui.add(egui::DragValue::new(&mut frame.at_frame).range(0..=duration));
                                let divisor = match channel.property { AnimationProperty::X | AnimationProperty::Y | AnimationProperty::Rotation => 1000.0,
                                    AnimationProperty::Opacity => 10000.0, _ => 1_000_000.0 };
                                let mut value = frame.value as f64 / divisor;
                                if ui.add(egui::DragValue::new(&mut value).speed(0.1)).changed() { frame.value = (value * divisor).round() as i64; }
                                if ui.small_button("×").on_hover_text("Remove keyframe").clicked() { remove = Some(index); }
                            });
                            egui::ComboBox::from_id_salt("easing").selected_text(format!("{:?}", frame.interpolation)).show_ui(ui, |ui| {
                                for (label, value) in [("Hold", Interpolation::Hold), ("Linear", Interpolation::Linear), ("Smooth", Interpolation::SmoothStep)] {
                                    ui.selectable_value(&mut frame.interpolation, value, label);
                                }
                            });
                        });
                    }
                    if let Some(index) = remove { channel.keys.remove(index); }
                    if channel.keys.len() < lumeryn_core::animation::MAX_KEYFRAMES && ui.button("Add keyframe").clicked() {
                        let next = channel.keys.last().map_or(0, |k| k.at_frame.saturating_add(1)).min(duration);
                        let value = channel.keys.last().map_or(0, |k| k.value);
                        channel.keys.push(Keyframe { at_frame: next, value, interpolation: Interpolation::Linear });
                    }
                });
            }
            animation.channels.retain(|channel| !channel.keys.is_empty());
            let valid = lumeryn_core::animation_backend::validate_supported(&animation, duration);
            if let Err(error) = &valid { ui.label(RichText::new(error.to_string()).small().color(design::WARNING)); }
            ui.label(RichText::new("Position uses canvas pixels, rotation degrees, opacity percent. Apply to preview.").small().color(design::MUTED));
            let enabled = self.connected && !self.edit_busy;
            apply = design::primary(ui, "Apply animation", enabled && valid.is_ok(), ui.available_width()).clicked();
            reset = ui.add_enabled(enabled, egui::Button::new("Remove animation")).clicked();
        });
        ui.data_mut(|d| d.insert_temp(key, animation.clone()));
        if apply || reset {
            self.edit(
                Edit::SetVisualAnimation {
                    clip_id,
                    animation: if reset {
                        Animation::default()
                    } else {
                        animation
                    },
                },
                "Edit animation",
            );
        }
    }
}
