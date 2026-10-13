//! Native time and fixed-cut transition controls use the shared graph commands.
use super::*;
use lumeryn_core::{
    model::TransitionEdge,
    time_effects::{PlaybackSpeed, SpeedAudio, TimeEffect, TransitionKind},
};

impl Session {
    pub(super) fn time_inspector(&mut self, ui: &mut egui::Ui, clip_id: Uuid) {
        let Some(project) = &self.project else {
            return;
        };
        if project.schema_version < 5 {
            return;
        }
        let media_id = project
            .clips
            .iter()
            .find(|c| c.id == clip_id)
            .map(|c| c.media_id)
            .or_else(|| {
                project
                    .visual_clips
                    .iter()
                    .find(|c| c.id == clip_id)
                    .map(|c| c.media_id)
            });
        let Some(video) = project
            .media
            .iter()
            .find(|m| Some(m.id) == media_id)
            .and_then(|m| m.metadata.video.as_ref())
        else {
            return;
        };
        if video.codec == "png" {
            return;
        }
        let key = egui::Id::new((
            "time-form",
            project.id,
            project.revision,
            self.epoch,
            clip_id,
        ));
        let mut effect = ui
            .data_mut(|d| d.get_temp::<TimeEffect>(key))
            .unwrap_or_else(|| {
                project
                    .time_effects
                    .get(&clip_id)
                    .copied()
                    .unwrap_or_default()
            });
        let mut apply = false;
        let mut reset = false;
        ui.collapsing("Speed & freeze", |ui| {
            egui::ComboBox::from_id_salt("time-mode").selected_text(match effect {
                TimeEffect::Normal => "Normal", TimeEffect::Speed { speed: PlaybackSpeed::Half, .. } => "0.5×",
                TimeEffect::Speed { speed: PlaybackSpeed::Double, .. } => "2×", TimeEffect::Speed { .. } => "1×", TimeEffect::Freeze { .. } => "Freeze frame",
            }).show_ui(ui, |ui| {
                for (label, value) in [("Normal", TimeEffect::Normal), ("0.5×", TimeEffect::Speed { speed: PlaybackSpeed::Half, audio: SpeedAudio::PreservePitch }),
                    ("2×", TimeEffect::Speed { speed: PlaybackSpeed::Double, audio: SpeedAudio::PreservePitch }), ("Freeze frame", TimeEffect::Freeze { source_frame: 0 })] {
                    ui.selectable_value(&mut effect, value, label);
                }
            });
            if let TimeEffect::Speed { audio, .. } = &mut effect {
                let mut mute = *audio == SpeedAudio::Mute;
                if ui.checkbox(&mut mute, "Mute source audio").changed() { *audio = if mute { SpeedAudio::Mute } else { SpeedAudio::PreservePitch }; }
            }
            if let TimeEffect::Freeze { source_frame } = &mut effect {
                ui.label("Exact source frame"); ui.add(egui::DragValue::new(source_frame).range(0..=video.frames.saturating_sub(1)));
                ui.label(RichText::new("Freeze contributes silence.").small().color(design::MUTED));
            }
            ui.label(RichText::new("Keeps timeline duration. Adjust the range separately; other clips stay in place.").small().color(design::MUTED));
            let enabled = self.connected && !self.edit_busy;
            apply = design::primary(ui, "Apply time effect", enabled, ui.available_width()).clicked();
            reset = ui.add_enabled(enabled, egui::Button::new("Reset time effect")).clicked();
        });
        ui.data_mut(|d| d.insert_temp(key, effect));
        if apply || reset {
            self.edit(
                Edit::SetTimeEffect {
                    clip_id,
                    effect: if reset { TimeEffect::Normal } else { effect },
                },
                "Time effect",
            );
        }
    }

    pub(super) fn transitions_panel(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            ui.label("Open a project to add a transition.");
            return;
        };
        let mut clips: Vec<_> = project.clips.iter().collect();
        clips.sort_by_key(|c| c.at_frame);
        if clips.len() != 2 {
            ui.label("Add two adjacent video clips to begin.");
            ui.label(
                RichText::new(
                    "Transitions require verified source handles on both sides of the cut.",
                )
                .small()
                .color(design::MUTED),
            );
            return;
        }
        let key = egui::Id::new(("transition-form", project.id, project.revision, self.epoch));
        let existing = project.transitions.first();
        let mut kind = ui
            .data_mut(|d| d.get_temp::<TransitionKind>(key))
            .unwrap_or_else(|| existing.map_or(TransitionKind::CrossDissolve, |edge| edge.kind));
        let mut duration = ui
            .data_mut(|d| d.get_temp::<u64>(key.with("duration")))
            .unwrap_or_else(|| existing.map_or(6, |edge| edge.duration_frames));
        ui.label(RichText::new("At the cut").strong());
        for (label, value) in [
            ("Cross dissolve", TransitionKind::CrossDissolve),
            ("Wipe left", TransitionKind::WipeLeft),
            ("Wipe right", TransitionKind::WipeRight),
            ("Wipe up", TransitionKind::WipeUp),
            ("Wipe down", TransitionKind::WipeDown),
        ] {
            ui.selectable_value(&mut kind, value, label);
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Frames");
            ui.add(egui::DragValue::new(&mut duration).range(1..=300));
        });
        let edge = TransitionEdge {
            id: existing.map_or_else(Uuid::new_v4, |edge| edge.id),
            outgoing_clip_id: clips[0].id,
            incoming_clip_id: clips[1].id,
            kind,
            duration_frames: duration,
        };
        let mut candidate = project.clone();
        candidate.transitions = vec![edge.clone()];
        let valid = candidate.validate();
        if let Err(error) = &valid {
            ui.label(
                RichText::new(error.to_string())
                    .small()
                    .color(design::WARNING),
            );
        }
        ui.label(
            RichText::new(
                "Video transition; source audio keeps its cut. Duration stays unchanged.",
            )
            .small()
            .color(design::MUTED),
        );
        let enabled = self.connected && !self.edit_busy;
        let apply = design::primary(
            ui,
            "Apply transition",
            enabled && valid.is_ok(),
            ui.available_width(),
        )
        .clicked();
        let remove = existing.is_some()
            && ui
                .add_enabled(enabled, egui::Button::new("Remove transition"))
                .clicked();
        ui.data_mut(|d| {
            d.insert_temp(key, kind);
            d.insert_temp(key.with("duration"), duration);
        });
        if apply {
            self.edit(
                Edit::SetTransition {
                    id: edge.id,
                    outgoing_clip_id: edge.outgoing_clip_id,
                    incoming_clip_id: edge.incoming_clip_id,
                    kind,
                    duration_frames: duration,
                },
                "Transition",
            );
        } else if remove {
            self.edit(
                Edit::DeleteTransition {
                    transition_id: edge.id,
                },
                "Remove transition",
            );
        }
    }
}
