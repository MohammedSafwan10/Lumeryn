//! Bounds-only overlay gestures. Committed preview pixels remain unchanged during drafts.
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use lumeryn_core::{Edit, Project, model::VisualPlacement};
use uuid::Uuid;

#[derive(Clone, Copy)]
enum Handle {
    Move,
    Corner { left: bool, top: bool },
}
struct Draft {
    project: Uuid,
    revision: u64,
    epoch: u64,
    clip: Uuid,
    original: VisualPlacement,
    value: VisualPlacement,
    start: Pos2,
    handle: Handle,
    image: Rect,
}
#[derive(Default)]
pub(super) struct CanvasTransform {
    draft: Option<Draft>,
}

fn screen_rect(value: &VisualPlacement, image: Rect, size: Vec2) -> Rect {
    Rect::from_min_size(
        image.min + Vec2::new(value.x as f32, value.y as f32) * image.size() / size,
        Vec2::new(value.width as f32, value.height as f32) * image.size() / size,
    )
}
fn changed(original: &VisualPlacement, handle: Handle, delta: Vec2) -> VisualPlacement {
    let dx = delta.x.round() as i32;
    let dy = delta.y.round() as i32;
    let mut value = original.clone();
    match handle {
        Handle::Move => {
            value.x = original.x.saturating_add(dx).clamp(-8192, 8192);
            value.y = original.y.saturating_add(dy).clamp(-8192, 8192);
        }
        Handle::Corner { left, top } => {
            let width = (original.width as i32)
                .saturating_add(if left { dx.saturating_neg() } else { dx })
                .clamp(1, 3840);
            let height = (original.height as i32)
                .saturating_add(if top { dy.saturating_neg() } else { dy })
                .clamp(1, 3840);
            value.width = width as u32;
            value.height = height as u32;
            if left {
                value.x = (original.x + original.width as i32 - width).clamp(-8192, 8192);
            }
            if top {
                value.y = (original.y + original.height as i32 - height).clamp(-8192, 8192);
            }
        }
    }
    value
}
fn same(a: &VisualPlacement, b: &VisualPlacement) -> bool {
    (a.x, a.y, a.width, a.height) == (b.x, b.y, b.width, b.height)
}
pub(super) struct CanvasView<'a> {
    pub image: Rect,
    pub project: &'a Project,
    pub selected: Option<Uuid>,
    pub epoch: u64,
    pub editable: bool,
    pub frame: u64,
}
impl CanvasTransform {
    pub(super) fn draw(
        &mut self,
        ui: &mut egui::Ui,
        view: CanvasView<'_>,
    ) -> Result<Option<Edit>, String> {
        let CanvasView {
            image,
            project,
            selected,
            epoch,
            editable,
            frame,
        } = view;
        let cancel = ui.input(|input| !input.focused || input.key_pressed(egui::Key::Escape));
        if cancel
            || !editable
            || self.draft.as_ref().is_some_and(|draft| {
                draft.project != project.id
                    || draft.revision != project.revision
                    || draft.epoch != epoch
                    || Some(draft.clip) != selected
                    || draft.image != image
            })
        {
            self.draft = None;
            return Ok(None);
        }
        let Some(clip) = project.visual_clips.iter().find(|clip| {
            Some(clip.id) == selected
                && clip.at_frame <= frame
                && frame < clip.at_frame.saturating_add(clip.duration_frames)
        }) else {
            self.draft = None;
            return Ok(None);
        };
        let original = clip.placement.clone().unwrap_or(VisualPlacement {
            x: 0,
            y: 0,
            width: project.width,
            height: project.height,
        });
        let size = Vec2::new(project.width as f32, project.height as f32);
        let value = self.draft.as_ref().map_or(&original, |draft| &draft.value);
        let bounds = screen_rect(value, image, size);
        let mut started = None;
        // Corners are registered first and excluded from the move target.
        let corners = [
            (bounds.left_top(), true, true),
            (bounds.right_top(), false, true),
            (bounds.left_bottom(), true, false),
            (bounds.right_bottom(), false, false),
        ];
        for (index, (point, left, top)) in corners.into_iter().enumerate() {
            let target = Rect::from_center_size(point, Vec2::splat(12.)).intersect(image);
            if target.is_positive() {
                let response = ui.interact(
                    target,
                    ui.id().with(("canvas-corner", clip.id, index)),
                    Sense::drag(),
                );
                if response.drag_started_by(egui::PointerButton::Primary) {
                    started = Some(Handle::Corner { left, top });
                }
                ui.painter().rect_filled(
                    Rect::from_center_size(point, Vec2::splat(6.)).intersect(image),
                    1.,
                    Color32::WHITE,
                );
            }
        }
        let body = bounds.shrink(8.).intersect(image);
        if body.is_positive() {
            let response = ui.interact(body, ui.id().with(("canvas-move", clip.id)), Sense::drag());
            if response.drag_started_by(egui::PointerButton::Primary) {
                started = Some(Handle::Move);
            }
        }
        if let Some(handle) = started {
            if let Some(start) = ui.input(|input| input.pointer.press_origin()) {
                self.draft = Some(Draft {
                    project: project.id,
                    revision: project.revision,
                    epoch,
                    clip: clip.id,
                    original: original.clone(),
                    value: original,
                    start,
                    handle,
                    image,
                });
            }
        }
        if let Some(draft) = &mut self.draft {
            if let Some(position) = ui.input(|input| input.pointer.interact_pos()) {
                draft.value = changed(
                    &draft.original,
                    draft.handle,
                    (position - draft.start) * size / image.size(),
                );
            }
        }
        let bounds = self
            .draft
            .as_ref()
            .map_or(bounds, |draft| screen_rect(&draft.value, image, size));
        ui.painter().with_clip_rect(image).rect_stroke(
            bounds,
            0.,
            Stroke::new(1.5, Color32::from_rgb(96, 255, 207)),
            egui::StrokeKind::Inside,
        );
        if self.draft.is_some() {
            ui.painter().text(
                image.left_top() + Vec2::new(8., 8.),
                egui::Align2::LEFT_TOP,
                "Bounds draft · release to apply",
                egui::FontId::proportional(12.),
                Color32::WHITE,
            );
        }
        if ui.input(|input| input.pointer.button_released(egui::PointerButton::Primary)) {
            if let Some(draft) = self.draft.take() {
                if !same(&draft.original, &draft.value) {
                    let mut candidate = project.clone();
                    let target = candidate
                        .visual_clips
                        .iter_mut()
                        .find(|clip| clip.id == draft.clip)
                        .ok_or_else(|| "Selected overlay changed during drag".to_owned())?;
                    target.placement = Some(draft.value.clone());
                    candidate.validate().map_err(|error| error.to_string())?;
                    return Ok(Some(Edit::SetVisualPlacement {
                        clip_id: draft.clip,
                        placement: Some(draft.value),
                    }));
                }
            }
        }
        if !ui.input(|input| input.pointer.button_down(egui::PointerButton::Primary)) {
            self.draft = None;
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_delta_moves_once_and_corner_keeps_opposite_anchor() {
        let original = VisualPlacement {
            x: 20,
            y: 30,
            width: 100,
            height: 80,
        };
        let moved = changed(&original, Handle::Move, Vec2::new(10., -5.));
        assert_eq!((moved.x, moved.y), (30, 25));
        let resized = changed(
            &original,
            Handle::Corner {
                left: true,
                top: true,
            },
            Vec2::new(10., 15.),
        );
        assert_eq!(
            (resized.x, resized.y, resized.width, resized.height),
            (30, 45, 90, 65)
        );
        let bounded = changed(
            &original,
            Handle::Corner {
                left: false,
                top: false,
            },
            Vec2::splat(10000.),
        );
        assert_eq!((bounded.width, bounded.height), (3840, 3840));
        let extreme = changed(
            &original,
            Handle::Corner {
                left: true,
                top: true,
            },
            Vec2::splat(f32::NEG_INFINITY),
        );
        assert_eq!((extreme.width, extreme.height), (3840, 3840));
    }

    fn valid_overlay_project() -> Project {
        use lumeryn_core::{
            media::{Metadata, Video},
            model::{Media, VisualClip, VisualSource, VisualTrack},
        };
        let mut project =
            Project::new("Gesture guards".into(), 320, 180, "30".parse().unwrap()).unwrap();
        project.id = Uuid::from_u128(1);
        project.video_track_id = Uuid::from_u128(2);
        project.audio_track_id = Some(Uuid::from_u128(3));
        project.visual_tracks.push(VisualTrack {
            id: Uuid::from_u128(4),
            name: "Overlay".into(),
        });
        project.media.push(Media {
            id: Uuid::from_u128(5),
            path: "pure-fixture.png".into(),
            alias: None,
            metadata: Metadata {
                size_bytes: 1,
                blake3: Some("a".repeat(64)),
                audio: None,
                video: Some(Video {
                    codec: "png".into(),
                    width: 320,
                    height: 180,
                    frame_rate: project.frame_rate,
                    frames: 1,
                    start_time_micros: 0,
                    sample_aspect_ratio: "1:1".into(),
                    color_transfer: None,
                    constant_rate: true,
                }),
            },
        });
        project.visual_clips.push(VisualClip {
            id: Uuid::from_u128(42),
            media_id: Uuid::from_u128(5),
            track_id: Uuid::from_u128(4),
            at_frame: 0,
            duration_frames: 60,
            source: VisualSource::Image {},
            placement: Some(VisualPlacement {
                x: 20,
                y: 30,
                width: 100,
                height: 80,
            }),
        });
        project.validate().unwrap();
        project
    }

    fn image_rect() -> Rect {
        Rect::from_min_size(egui::pos2(40., 40.), Vec2::new(640., 360.))
    }

    fn button(position: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn run_draw(
        context: &egui::Context,
        gestures: &mut CanvasTransform,
        project: &Project,
        events: Vec<egui::Event>,
        state: (bool, u64, bool, Option<Uuid>),
    ) -> Option<Edit> {
        let (focused, epoch, editable, selected) = state;
        let mut outcome = None;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800., 500.))),
            focused,
            events,
            ..Default::default()
        };
        let _ = context.run_ui(input, |ui| {
            outcome = gestures
                .draw(
                    ui,
                    CanvasView {
                        image: image_rect(),
                        project,
                        selected,
                        epoch,
                        editable,
                        frame: 0,
                    },
                )
                .unwrap();
        });
        outcome
    }

    #[test]
    fn actual_egui_primary_drag_release_commits_once_at_nonunit_mapping() {
        let context = egui::Context::default();
        let project = valid_overlay_project();
        let selected = Some(Uuid::from_u128(42));
        let mut gestures = CanvasTransform::default();
        let start = egui::pos2(150., 160.);
        let end = egui::pos2(190., 180.);
        assert!(
            run_draw(
                &context,
                &mut gestures,
                &project,
                vec![egui::Event::PointerMoved(start)],
                (true, 1, true, selected)
            )
            .is_none()
        );
        assert!(
            run_draw(
                &context,
                &mut gestures,
                &project,
                vec![button(start, true)],
                (true, 1, true, selected)
            )
            .is_none()
        );
        assert!(
            run_draw(
                &context,
                &mut gestures,
                &project,
                vec![egui::Event::PointerMoved(end)],
                (true, 1, true, selected)
            )
            .is_none()
        );
        assert!(
            gestures.draft.is_some(),
            "actual drag must capture a draft before release"
        );
        let edit = run_draw(
            &context,
            &mut gestures,
            &project,
            vec![button(end, false)],
            (true, 1, true, selected),
        );
        match edit {
            Some(Edit::SetVisualPlacement {
                clip_id,
                placement: Some(value),
            }) => {
                assert_eq!(clip_id, Uuid::from_u128(42));
                assert_eq!(
                    (value.x, value.y, value.width, value.height),
                    (40, 40, 100, 80)
                );
            }
            other => panic!("expected one committed placement, got {other:?}"),
        }
        assert!(gestures.draft.is_none());
        assert!(
            run_draw(
                &context,
                &mut gestures,
                &project,
                vec![button(end, false)],
                (true, 1, true, selected)
            )
            .is_none()
        );
        assert_eq!(
            project.visual_clips[0].placement.as_ref().unwrap().x,
            20,
            "draft must not modify the source snapshot"
        );
    }

    #[test]
    fn actual_egui_draw_discards_focus_escape_busy_revision_epoch_and_selection_drafts() {
        let project = valid_overlay_project();
        let clip = Uuid::from_u128(42);
        let image = image_rect();
        let start = egui::pos2(150., 160.);
        // Mode zero proves the same valid clip/draft remains active without a cancellation guard.
        for mode in 0..7 {
            let context = egui::Context::default();
            let placement = project.visual_clips[0].placement.clone().unwrap();
            let mut gestures = CanvasTransform {
                draft: Some(Draft {
                    project: project.id,
                    revision: project.revision,
                    epoch: 1,
                    clip,
                    original: placement.clone(),
                    value: placement,
                    start,
                    handle: Handle::Move,
                    image,
                }),
            };
            let mut events = vec![egui::Event::PointerMoved(start), button(start, true)];
            if mode == 2 {
                events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut changed_project = project.clone();
            if mode == 4 {
                changed_project.revision += 1;
            }
            let outcome = run_draw(
                &context,
                &mut gestures,
                &changed_project,
                events,
                (
                    mode != 1,
                    if mode == 5 { 2 } else { 1 },
                    mode != 3,
                    if mode == 6 { None } else { Some(clip) },
                ),
            );
            assert!(outcome.is_none());
            if mode == 0 {
                assert!(
                    gestures.draft.is_some(),
                    "valid baseline must not cancel via missing-clip or pointer fallback"
                );
            } else {
                assert!(
                    gestures.draft.is_none(),
                    "guard mode {mode} must independently cancel a real placement draft"
                );
            }
        }
    }

    #[test]
    fn screen_mapping_preserves_project_coordinates_at_nonunit_scale() {
        let rect = screen_rect(
            &VisualPlacement {
                x: 20,
                y: 30,
                width: 100,
                height: 80,
            },
            Rect::from_min_size(egui::pos2(10., 15.), Vec2::new(640., 360.)),
            Vec2::new(320., 180.),
        );
        assert_eq!(rect.min, egui::pos2(50., 75.));
        assert_eq!(rect.size(), Vec2::new(200., 160.));
    }
}
