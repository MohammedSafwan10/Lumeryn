use crate::design;
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use lumeryn_core::{Project, model::Clip};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Select(Uuid),
    Seek(u64),
    Trim { clip: Uuid, duration: u64 },
}

#[derive(Debug)]
pub struct Timeline {
    pub zoom: f32,
    trim: Option<(Uuid, u64)>,
    pub visible_clips: usize,
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            trim: None,
            visible_clips: 0,
        }
    }
}

pub fn total_frames(project: &Project) -> u64 {
    project
        .clips
        .iter()
        .map(|clip| clip.at_frame.saturating_add(clip.duration_frames))
        .max()
        .unwrap_or(0)
}

pub fn visible(clip: &Clip, first: u64, last: u64) -> bool {
    clip.at_frame < last && clip.at_frame.saturating_add(clip.duration_frames) > first
}

fn frame_at(x: f32, origin: f32, px: f32, maximum: u64) -> u64 {
    (((x - origin) / px).max(0.0).round() as u64).min(maximum.saturating_sub(1))
}

impl Timeline {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        selected: Option<Uuid>,
        playhead: u64,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("EDIT TIMELINE")
                    .strong()
                    .size(11.0)
                    .color(design::MUTED),
            );
            ui.separator();
            ui.label(format!(
                "{} clips  ·  {} fps",
                project.clips.len(),
                project.frame_rate
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::Slider::new(&mut self.zoom, 0.3..=5.0)
                        .text("Zoom")
                        .logarithmic(true),
                );
            });
        });
        ui.label(
            egui::RichText::new(
                "Click a clip to inspect · scrub the ruler · drag a clip’s right edge to trim",
            )
            .size(11.0)
            .color(design::MUTED),
        );
        let total = total_frames(project);
        let fps = project.frame_rate.numerator() as f32 / project.frame_rate.denominator() as f32;
        let pixels_per_frame = (60.0 / fps * self.zoom).max(0.01);
        let header = 112.0;
        let content_width = (header
            + (total.max(5 * fps as u64) as f32 + fps * 2.0) * pixels_per_frame)
            .max(ui.available_width());
        self.visible_clips = 0;
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(content_width, 170.0), Sense::hover());
                let viewport = ui.clip_rect().intersect(rect);
                let origin = rect.left() + header;
                let first = (((viewport.left() - origin) / pixels_per_frame)
                    .max(0.0)
                    .floor() as u64)
                    .min(total);
                let last = ((((viewport.right() - origin) / pixels_per_frame)
                    .max(0.0)
                    .ceil() as u64)
                    + 1)
                .min(total.saturating_add(1));
                let painter = ui.painter();
                painter.rect_filled(rect, 5.0, design::BACKGROUND);
                let ruler = Rect::from_min_max(
                    pos2(origin, rect.top()),
                    pos2(rect.right(), rect.top() + 34.0),
                );
                painter.rect_filled(ruler, 0.0, design::PANEL);
                let seconds_per_tick = if self.zoom < 0.65 { 5 } else { 1 };
                let tick_frames = (fps * seconds_per_tick as f32).round().max(1.0) as u64;
                let first_tick = first / tick_frames * tick_frames;
                for frame in (first_tick..last).step_by(tick_frames as usize) {
                    let x = origin + frame as f32 * pixels_per_frame;
                    painter.line_segment(
                        [pos2(x, rect.top() + 25.0), pos2(x, rect.bottom())],
                        Stroke::new(1.0, design::BORDER.gamma_multiply(0.5)),
                    );
                    painter.text(
                        pos2(x + 5.0, rect.top() + 8.0),
                        Align2::LEFT_TOP,
                        format!("{:.0}s", frame as f64 / f64::from(fps)),
                        FontId::monospace(11.0),
                        design::MUTED,
                    );
                }
                let track = Rect::from_min_max(
                    pos2(rect.left(), rect.top() + 38.0),
                    pos2(rect.right(), rect.top() + 112.0),
                );
                painter.rect_filled(track, 3.0, design::PANEL);
                painter.text(
                    pos2(rect.left() + 12.0, track.top() + 17.0),
                    Align2::LEFT_TOP,
                    "V1  VIDEO",
                    FontId::proportional(12.0),
                    design::TEXT,
                );
                painter.text(
                    pos2(rect.left() + 12.0, track.top() + 39.0),
                    Align2::LEFT_TOP,
                    "Source audio",
                    FontId::proportional(10.0),
                    design::MUTED,
                );
                let ruler_response =
                    ui.interact(ruler, ui.id().with("time-ruler"), Sense::click_and_drag());
                if (ruler_response.clicked() || ruler_response.dragged())
                    && let Some(pointer) = ruler_response.interact_pointer_pos()
                {
                    actions.push(Action::Seek(frame_at(
                        pointer.x,
                        origin,
                        pixels_per_frame,
                        total,
                    )));
                }
                for clip in project
                    .clips
                    .iter()
                    .filter(|clip| visible(clip, first, last))
                {
                    self.visible_clips += 1;
                    let end = self
                        .trim
                        .filter(|(id, _)| *id == clip.id)
                        .map_or(clip.duration_frames, |(_, duration)| duration);
                    let block = Rect::from_min_max(
                        pos2(
                            origin + clip.at_frame as f32 * pixels_per_frame,
                            track.top() + 7.0,
                        ),
                        pos2(
                            origin + (clip.at_frame + end) as f32 * pixels_per_frame,
                            track.bottom() - 7.0,
                        ),
                    );
                    let selected = selected == Some(clip.id);
                    ui.painter().rect_filled(
                        block,
                        4.0,
                        if selected {
                            Color32::from_rgb(48, 101, 127)
                        } else {
                            design::CLIP
                        },
                    );
                    ui.painter().rect_stroke(
                        block,
                        4.0,
                        Stroke::new(
                            if selected { 2.0 } else { 1.0 },
                            if selected {
                                design::ACCENT
                            } else {
                                design::BORDER
                            },
                        ),
                        StrokeKind::Inside,
                    );
                    let label = project
                        .media
                        .iter()
                        .find(|m| m.id == clip.media_id)
                        .and_then(|m| m.path.file_name())
                        .map_or_else(|| "Media".into(), |n| n.to_string_lossy().into_owned());
                    let clip_painter = ui
                        .painter()
                        .with_clip_rect(block.shrink(8.0).intersect(viewport));
                    clip_painter.text(
                        block.left_top() + vec2(10.0, 10.0),
                        Align2::LEFT_TOP,
                        label,
                        FontId::proportional(12.0),
                        design::TEXT,
                    );
                    clip_painter.text(
                        block.left_top() + vec2(10.0, 31.0),
                        Align2::LEFT_TOP,
                        format!("{} frames  ·  source {}", end, clip.source_in_frame),
                        FontId::monospace(10.0),
                        design::MUTED,
                    );
                    let body = ui.interact(block, ui.id().with(clip.id), Sense::click());
                    if body.clicked() {
                        actions.push(Action::Select(clip.id));
                    }
                    if body.double_clicked() {
                        actions.push(Action::Seek(clip.at_frame));
                    }
                    let edge = Rect::from_min_max(
                        pos2(block.right() - 7.0, block.top()),
                        block.right_bottom(),
                    );
                    let handle = ui.interact(edge, ui.id().with((clip.id, "trim")), Sense::drag());
                    if handle.hovered() || handle.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                        ui.painter().rect_filled(
                            edge.shrink2(vec2(2.0, 10.0)),
                            2.0,
                            design::ACCENT,
                        );
                    }
                    if handle.dragged()
                        && let Some(pointer) = handle.interact_pointer_pos()
                    {
                        let duration = ((pointer.x - origin) / pixels_per_frame
                            - clip.at_frame as f32)
                            .round()
                            .max(1.0) as u64;
                        self.trim = Some((clip.id, duration));
                    }
                    if handle.drag_stopped()
                        && let Some((id, duration)) = self.trim.take()
                    {
                        actions.push(Action::Trim { clip: id, duration });
                    }
                }
                let x = origin + playhead as f32 * pixels_per_frame;
                if x >= viewport.left() && x <= viewport.right() {
                    ui.painter().line_segment(
                        [pos2(x, rect.top()), pos2(x, rect.bottom())],
                        Stroke::new(1.5, design::ACCENT),
                    );
                    ui.painter()
                        .circle_filled(pos2(x, rect.top() + 3.0), 4.0, design::ACCENT);
                }
            });
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visible_ranges_and_scrub_bounds() {
        let clip = Clip {
            id: Uuid::nil(),
            media_id: Uuid::nil(),
            track_id: Uuid::nil(),
            at_frame: 100,
            source_in_frame: 0,
            duration_frames: 30,
        };
        assert!(!visible(&clip, 0, 100));
        assert!(visible(&clip, 100, 101));
        assert!(!visible(&clip, 130, 140));
        assert_eq!(frame_at(-100.0, 0.0, 2.0, 100), 0);
        assert_eq!(frame_at(2000.0, 0.0, 2.0, 100), 99);
        assert_eq!(frame_at(25.0, 5.0, 2.0, 100), 10);
    }

    #[test]
    fn benchmark_ten_thousand_clips_visible_paint() -> Result<(), Box<dyn std::error::Error>> {
        use lumeryn_core::{
            media::{Metadata, Video},
            model::Media,
            time::FrameRate,
        };
        let rate = FrameRate::new(30, 1)?;
        let mut project = Project::new("Timeline benchmark".into(), 1920, 1080, rate)?;
        let media_id = Uuid::new_v4();
        project.media.push(Media {
            id: media_id,
            path: "synthetic.mp4".into(),
            metadata: Metadata {
                size_bytes: 1,
                blake3: None,
                audio: None,
                video: Some(Video {
                    codec: "synthetic".into(),
                    width: 1920,
                    height: 1080,
                    frame_rate: rate,
                    frames: 100,
                    start_time_micros: 0,
                    sample_aspect_ratio: "1:1".into(),
                    color_transfer: None,
                    constant_rate: true,
                }),
            },
        });
        project.clips = (0..10_000)
            .map(|index| Clip {
                id: Uuid::new_v4(),
                media_id,
                track_id: project.video_track_id,
                at_frame: index * 30,
                source_in_frame: 0,
                duration_frames: 30,
            })
            .collect();
        project.validate()?;
        let ctx = egui::Context::default();
        crate::design::configure(&ctx);
        let mut timeline = Timeline::default();
        let mut samples = Vec::new();
        for index in 0..110 {
            let started = std::time::Instant::now();
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 300.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                timeline.show(ui, &project, None, 0);
            });
            assert!(!output.shapes.is_empty());
            // This CPU benchmark has no renderer; explicitly discard texture updates.
            output.textures_delta.clear();
            assert!(
                timeline.visible_clips > 0 && timeline.visible_clips < 40,
                "visible {}",
                timeline.visible_clips
            );
            if index >= 10 {
                samples.push(started.elapsed().as_secs_f64() * 1000.0);
            }
        }
        samples.sort_by(f64::total_cmp);
        let report = serde_json::json!({"clips":10_000,"samples":samples.len(),"visible_clips":timeline.visible_clips,"p50_ms":samples[50],"p95_ms":samples[95],"scope":"CPU egui layout/custom paint, no GPU presentation; scans all clip metadata but paints only visible clips"});
        println!("TIMELINE_BENCHMARK {report}");
        if let Some(path) = std::env::var_os("LUMERYN_TIMELINE_BENCHMARK") {
            let mut file = std::fs::File::create_new(path)?;
            use std::io::Write;
            file.write_all(&serde_json::to_vec_pretty(&report)?)?;
        }
        Ok(())
    }
}
