use crate::{assets::Assets, design};
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use lumeryn_core::{Project, model::Clip, time::FrameRate};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Select(Uuid),
    Seek(u64),
    Trim { clip: Uuid, duration: u64 },
    Split { clip: Uuid, at_frame: u64 },
    Duplicate(Uuid),
    Copy(Uuid),
    Delete(Uuid),
    Reorder { clip: Uuid, before: Option<Uuid> },
}

#[derive(Debug)]
pub struct Timeline {
    pub zoom: f32,
    pub fit: bool,
    trim: Option<(Uuid, u64)>,
    pub visible_clips: usize,
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            fit: true,
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

/// Elapsed timeline time, not SMPTE drop-frame notation. Integer conversion retains rational rates.
fn timecode(frame: u64, rate: FrameRate, fractions: bool) -> String {
    let millis =
        u128::from(frame) * u128::from(rate.denominator()) * 1000 / u128::from(rate.numerator());
    let seconds = millis / 1000;
    if fractions {
        format!(
            "{:02}:{:02}.{:03}",
            seconds / 60,
            seconds % 60,
            millis % 1000
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

fn nice_tick_frames(rate: FrameRate, px_per_frame: f32) -> u64 {
    let fps = f64::from(rate.numerator()) / f64::from(rate.denominator());
    let seconds = [
        0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0,
    ];
    let needed_seconds = 90.0 / (fps * f64::from(px_per_frame));
    let step = seconds
        .into_iter()
        .find(|seconds| seconds * fps * f64::from(px_per_frame) >= 90.0)
        .unwrap_or_else(|| (needed_seconds / 3600.0).ceil() * 3600.0);
    (step * fps).round().max(1.0) as u64
}

fn cover_uv(source: egui::Vec2, target: egui::Vec2) -> Rect {
    let source_aspect = source.x / source.y;
    let target_aspect = target.x / target.y;
    let size = if source_aspect > target_aspect {
        vec2(target_aspect / source_aspect, 1.0)
    } else {
        vec2(1.0, source_aspect / target_aspect)
    };
    Rect::from_center_size(pos2(0.5, 0.5), size)
}

#[derive(Debug)]
struct Lanes {
    ruler: Rect,
    video: Rect,
    audio: Rect,
}

impl Lanes {
    fn new(rect: Rect, origin: f32) -> Self {
        let ruler_height = if rect.height() < 184.0 { 35.0 } else { 43.0 };
        let body_height = (rect.height() - ruler_height - 17.0).max(0.0);
        let audio_maximum = 56.0_f32.min((body_height - 50.0).max(0.0));
        let audio_height = (body_height * 0.4).clamp(28.0_f32.min(audio_maximum), audio_maximum);
        let video_height = body_height - audio_height;
        let ruler = Rect::from_min_max(
            pos2(origin, rect.top()),
            pos2(rect.right(), rect.top() + ruler_height),
        );
        let video = Rect::from_min_max(
            pos2(origin, ruler.bottom() + 4.0),
            pos2(rect.right(), ruler.bottom() + 4.0 + video_height),
        );
        let audio = Rect::from_min_max(
            pos2(origin, video.bottom() + 8.0),
            pos2(rect.right(), rect.bottom() - 5.0),
        );
        Self {
            ruler,
            video,
            audio,
        }
    }
}

fn media_label(path: &std::path::Path) -> String {
    path.file_stem().map_or_else(
        || "Media".into(),
        |name| name.to_string_lossy().replace('_', " "),
    )
}

impl Timeline {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        assets: &Assets,
        selected: Option<Uuid>,
        playhead: u64,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        let total = total_frames(project);
        let fps = project.frame_rate.numerator() as f32 / project.frame_rate.denominator() as f32;
        let header = 150.0;
        let width = ui.available_width();
        ui.horizontal(|ui| {
            let (tool, response) = ui.allocate_exact_size(vec2(36.0, 32.0), Sense::hover());
            response.on_hover_text("Select clips; drag the right edge to trim");
            ui.painter()
                .rect_filled(tool, 3.0, Color32::from_rgb(37, 48, 63));
            let cursor = Color32::from_rgb(133, 187, 213);
            let point = |x, y| tool.left_top() + vec2(x, y);
            ui.painter().add(egui::Shape::convex_polygon(
                vec![point(11.0, 8.0), point(11.0, 24.0), point(25.0, 18.0)],
                cursor,
                Stroke::NONE,
            ));
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    point(14.0, 17.0),
                    point(19.0, 26.0),
                    point(22.0, 24.0),
                    point(17.0, 15.0),
                ],
                cursor,
                Stroke::NONE,
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(egui::Button::new("Fit").selected(self.fit))
                    .on_hover_text("Fit the complete edit in view")
                    .clicked()
                {
                    self.fit = true;
                }
                let previous = self.zoom;
                ui.spacing_mut().interact_size.y = 18.0;
                ui.add(
                    egui::Slider::new(&mut self.zoom, 0.3..=20.0)
                        .show_value(false)
                        .logarithmic(true),
                )
                .on_hover_text("Timeline zoom");
                if previous != self.zoom {
                    self.fit = false;
                }
                ui.label(egui::RichText::new("Zoom").size(12.0).color(design::MUTED));
            });
        });
        ui.add_space(7.0);
        let pixels_per_frame = if self.fit {
            ((width - header - 22.0) / total.max(1) as f32).max(0.000_001)
        } else {
            (60.0 / fps * self.zoom).max(0.01)
        };
        let content_width = if self.fit {
            width
        } else {
            (header + (total as f32 + fps * 0.5) * pixels_per_frame).max(width)
        };
        self.visible_clips = 0;
        // Reserve the real remaining height after the toolbar; scrolling is horizontal only.
        let canvas_height = ui.available_height().clamp(0.0, 204.0);
        egui::ScrollArea::horizontal().id_salt("studio-timeline-scroll").max_height(canvas_height).auto_shrink([false, false]).show(ui, |ui| {
            let height = ui.available_height().min(canvas_height);
            let (rect, _) = ui.allocate_exact_size(vec2(content_width, height), Sense::hover());
            let canvas_viewport = ui.clip_rect().intersect(rect);
            let viewport = Rect::from_min_max(pos2((canvas_viewport.left()+header).min(canvas_viewport.right()),canvas_viewport.top()),canvas_viewport.right_bottom());
            let origin = rect.left() + header;
            let first = (((viewport.left() - origin) / pixels_per_frame).max(0.0).floor() as u64).min(total);
            let last = ((((viewport.right() - origin) / pixels_per_frame).max(0.0).ceil() as u64) + 1).min(total.saturating_add(1));
            let Lanes { ruler, video, audio } = Lanes::new(rect, origin);
            let painter = ui.painter();
            painter.rect_filled(rect, 0.0, design::BACKGROUND);
            painter.line_segment([pos2(rect.left(), ruler.bottom()+1.0), pos2(rect.right(), ruler.bottom()+1.0)], Stroke::new(1.0, design::BORDER));
            painter.line_segment([pos2(rect.left(), video.bottom()+4.0), pos2(rect.right(), video.bottom()+4.0)], Stroke::new(1.0, design::BORDER));
            painter.rect_filled(video, 3.0, design::PANEL);
            painter.rect_filled(audio, 3.0, design::PANEL);
            painter.rect_filled(Rect::from_min_max(canvas_viewport.left_top(),pos2(viewport.left(),rect.bottom())),0.0,design::BACKGROUND);
            // Headers describe the supported single-video-track graph; source audio is linked, not a fake independent track.
            let video_header = painter.with_clip_rect(Rect::from_min_max(pos2(canvas_viewport.left(),video.top()),pos2(viewport.left()-6.0,video.bottom())).intersect(canvas_viewport));
            video_header.text(pos2(canvas_viewport.left()+20.0, video.center().y-8.0), Align2::LEFT_CENTER, "Video 1", FontId::proportional(14.0), design::TEXT);
            video_header.text(pos2(canvas_viewport.left()+20.0, video.center().y+13.0), Align2::LEFT_CENTER, format!("{} clips", project.clips.len()), FontId::proportional(11.0), design::MUTED);
            let audio_header = painter.with_clip_rect(Rect::from_min_max(pos2(canvas_viewport.left(),audio.top()),pos2(viewport.left()-6.0,audio.bottom())).intersect(canvas_viewport));
            audio_header.text(pos2(canvas_viewport.left()+20.0, audio.center().y-if audio.height()>=43.0 {7.0} else {0.0}), Align2::LEFT_CENTER, "Source audio", FontId::proportional(13.0), design::TEXT);
            if audio.height()>=43.0 { audio_header.text(pos2(canvas_viewport.left()+20.0, audio.center().y+11.0), Align2::LEFT_CENTER, "Linked to video", FontId::proportional(11.0), design::MUTED); }
            let ruler_painter = painter.with_clip_rect(viewport);
            let tick_frames = nice_tick_frames(project.frame_rate, pixels_per_frame);
            let first_tick = first / tick_frames * tick_frames;
            for frame in (first_tick..last).step_by(tick_frames as usize) {
                let x = origin + frame as f32 * pixels_per_frame;
                if x >= viewport.left() && x <= viewport.right() {
                    let label = ruler_painter.layout_no_wrap(timecode(frame, project.frame_rate, tick_frames < fps as u64), FontId::proportional(12.0), design::MUTED);
                    let label_x = x.min(viewport.right() - label.size().x).max(viewport.left());
                    ruler_painter.galley(pos2(label_x, ruler.top()+2.0), label, design::MUTED);
                }
                ruler_painter.line_segment([pos2(x, ruler.bottom()-12.0), pos2(x, ruler.bottom()-3.0)], Stroke::new(1.0, design::MUTED));
                for division in 1..5 {
                    let minor_x = x + tick_frames as f32 * pixels_per_frame * division as f32 / 5.0;
                    if minor_x <= viewport.right() { ruler_painter.line_segment([pos2(minor_x, ruler.bottom()-8.0), pos2(minor_x, ruler.bottom()-3.0)], Stroke::new(1.0, design::BORDER)); }
                }
            }
            let ruler_response = ui.interact(ruler.intersect(viewport), ui.id().with("time-ruler"), Sense::click_and_drag());
            if (ruler_response.clicked() || ruler_response.dragged()) && let Some(pointer) = ruler_response.interact_pointer_pos() { actions.push(Action::Seek(frame_at(pointer.x, origin, pixels_per_frame, total))); }
            let mut painted_dense_columns = std::collections::HashSet::new();
            for clip in project.clips.iter().filter(|clip| visible(clip, first, last)) {
                let duration = self.trim.filter(|(id, _)| *id == clip.id).map_or(clip.duration_frames, |(_, duration)| duration);
                let left = origin + clip.at_frame as f32 * pixels_per_frame;
                let right = origin + clip.at_frame.saturating_add(duration) as f32 * pixels_per_frame;
                let is_selected = selected == Some(clip.id);
                // Fit mode can map thousands of clips to fewer pixels: paint at most one unselected block per column.
                let column = left.floor() as i64;
                if right-left < 2.0 && !is_selected && !painted_dense_columns.insert(column) { continue; }
                self.visible_clips += 1;
                let block = Rect::from_min_max(pos2(left, video.top()+2.0), pos2(right.max(left+1.0), video.bottom()-2.0));
                let clipped = block.intersect(viewport);
                let clip_painter = ui.painter().with_clip_rect(clipped);
                clip_painter.rect_filled(block, 4.0, Color32::from_rgb(29, 43, 58));
                if let Some(texture) = assets.thumbnail(clip.media_id) {
                    let image_height = block.height();
                    let tile_width = (image_height*1.7).max(40.0);
                    let first_tile = ((clipped.left()-block.left())/tile_width).floor().max(0.0) as usize;
                    let last_tile = ((clipped.right()-block.left())/tile_width).ceil().max(0.0) as usize;
                    for tile in first_tile..last_tile {
                        let tile_rect = Rect::from_min_size(pos2(block.left()+tile as f32*tile_width, block.top()), vec2(tile_width, image_height));
                        clip_painter.image(texture.id(), tile_rect, cover_uv(texture.size_vec2(), tile_rect.size()), Color32::WHITE);
                    }
                }
                let footer = Rect::from_min_max(pos2(block.left(), block.bottom()-27.0), block.right_bottom());
                clip_painter.rect_filled(footer, 0.0, Color32::from_black_alpha(185));
                let label = project.media.iter().find(|m| m.id == clip.media_id).map_or_else(|| "Media".into(), |media| media_label(&media.path));
                let label_right = if block.width()>180.0 {block.right()-64.0} else {block.right()-6.0};
                let label_painter = clip_painter.with_clip_rect(Rect::from_min_max(footer.left_top(),pos2(label_right,footer.bottom())).intersect(clipped));
                label_painter.text(pos2(block.left()+10.0, footer.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.0), design::TEXT);
                if block.width() > 180.0 { clip_painter.text(pos2(block.right()-10.0, footer.center().y), Align2::RIGHT_CENTER, timecode(duration, project.frame_rate, false), FontId::proportional(12.0), design::TEXT); }
                clip_painter.rect_stroke(block, 4.0, Stroke::new(if is_selected { 2.5 } else { 1.0 }, if is_selected { design::ACCENT } else { design::BORDER }), StrokeKind::Inside);
                let body = ui.interact(clipped, ui.id().with(clip.id), Sense::click());
                let body = body.on_hover_text("Click to select · double-click to seek · drag the right edge to trim. Images are a representative source frame.");
                if body.clicked() { actions.push(Action::Select(clip.id)); }
                if body.double_clicked() { actions.push(Action::Seek(clip.at_frame)); }
                body.context_menu(|ui| {
                    if ui.add_enabled(clip.at_frame < playhead && playhead < clip.at_frame + clip.duration_frames, egui::Button::new("Split at playhead   S")).clicked() { actions.push(Action::Split { clip: clip.id, at_frame: playhead }); ui.close(); }
                    if ui.button("Duplicate   Ctrl+D").clicked() { actions.push(Action::Duplicate(clip.id)); ui.close(); }
                    if ui.button("Copy clip   Ctrl+C").clicked() { actions.push(Action::Copy(clip.id)); ui.close(); }
                    if ui.button("Delete and close gap   Del").clicked() { actions.push(Action::Delete(clip.id)); ui.close(); }
                    ui.separator();
                    if ui.button("Move to beginning").clicked() {
                        let before = project.clips.iter().min_by_key(|item| item.at_frame).map(|item| item.id);
                        actions.push(Action::Reorder { clip: clip.id, before }); ui.close();
                    }
                    if ui.button("Move to end").clicked() { actions.push(Action::Reorder { clip: clip.id, before: None }); ui.close(); }
                });
                if block.width() > 10.0 {
                    let edge = Rect::from_min_max(pos2(block.right()-8.0, block.top()), block.right_bottom());
                    let handle = ui.interact(edge.intersect(viewport), ui.id().with((clip.id,"trim")), Sense::drag());
                    if handle.hovered() || handle.dragged() { ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal); ui.painter().rect_filled(edge.shrink2(vec2(2.0,14.0)), 2.0, design::ACCENT); }
                    if handle.dragged() && let Some(pointer) = handle.interact_pointer_pos() { self.trim = Some((clip.id, ((pointer.x-origin)/pixels_per_frame-clip.at_frame as f32).round().max(1.0) as u64)); }
                    if handle.drag_stopped() && let Some((id,duration)) = self.trim.take() { actions.push(Action::Trim {clip:id,duration}); }
                }
                let audio_block = Rect::from_min_max(pos2(left,audio.top()+1.0),pos2(right.max(left+1.0),audio.bottom()-1.0));
                let audio_painter = ui.painter().with_clip_rect(audio_block.intersect(viewport));
                let has_audio = project.media.iter().find(|media| media.id==clip.media_id).is_some_and(|media| media.metadata.audio.is_some());
                if has_audio {
                    audio_painter.rect_filled(audio_block, 3.0, Color32::from_rgb(24,35,49));
                    if let Some(waveform) = assets.waveform(clip.media_id) {
                        let response = ui.interact(audio_block.intersect(viewport),ui.id().with((clip.id,"source-audio")),Sense::click()).on_hover_text(format!("Linked source audio · normalized waveform display from {} Hz mono PCM",waveform.sample_rate));
                        if response.clicked() { actions.push(Action::Select(clip.id)); }
                        let source_start = clip.source_in_frame as f64 * f64::from(project.frame_rate.denominator()) / f64::from(project.frame_rate.numerator());
                        let clip_seconds = duration as f64 * f64::from(project.frame_rate.denominator()) / f64::from(project.frame_rate.numerator());
                        let visible_audio = audio_block.intersect(viewport);
                        let columns = (visible_audio.width()/2.0).ceil().max(0.0) as usize;
                        let mut envelope = egui::Mesh::default();
                        let mut previous = None;
                        for column in 0..=columns {
                            let x = (visible_audio.left()+column as f32*2.0).min(visible_audio.right());
                            let fraction = ((x-left)/audio_block.width()).clamp(0.0,1.0) as f64;
                            let source_seconds = source_start+fraction*clip_seconds;
                            let bin = (source_seconds/waveform.duration_seconds*waveform.peaks.len() as f64).floor() as usize;
                            if let Some(peak) = waveform.peaks.get(bin) {
                                // Normalize only the display envelope; the decoded audio remains unchanged.
                                let amplitude = (peak / waveform.max_peak.max(0.001)).clamp(0.0,1.0)*(audio_block.height()*0.32);
                                let index = envelope.vertices.len() as u32;
                                let color = Color32::from_rgb(52,89,134);
                                envelope.colored_vertex(pos2(x,audio_block.center().y-amplitude),color);
                                envelope.colored_vertex(pos2(x,audio_block.center().y+amplitude),color);
                                if let Some(previous) = previous {
                                    envelope.add_triangle(previous,previous+1,index);
                                    envelope.add_triangle(index,previous+1,index+1);
                                }
                                previous = Some(index);
                            } else { previous = None; }
                        }
                        audio_painter.add(egui::Shape::mesh(envelope));
                    } else if audio_block.width()>90.0 {
                        audio_painter.text(audio_block.center(),Align2::CENTER_CENTER,if assets.pending(clip.media_id) {"Loading waveform…"} else {"Waveform unavailable"},FontId::proportional(11.0),design::MUTED);
                    }
                } else if audio_block.width()>90.0 { audio_painter.text(audio_block.center(),Align2::CENTER_CENTER,"No source audio",FontId::proportional(11.0),design::MUTED); }
            }
            let x = origin + playhead as f32 * pixels_per_frame;
            if x >= viewport.left() && x <= viewport.right() {
                ui.painter().line_segment([pos2(x,ruler.top()+24.0),pos2(x,rect.bottom())],Stroke::new(1.5,design::ACCENT));
                let text = timecode(playhead,project.frame_rate,false);
                let badge = Rect::from_center_size(pos2(x,ruler.top()+10.0),vec2(55.0,24.0));
                ui.painter().rect_filled(badge,4.0,design::ACCENT);
                ui.painter().text(badge.center(),Align2::CENTER_CENTER,text,FontId::proportional(13.0),design::BACKGROUND);
                ui.painter().add(egui::Shape::convex_polygon(vec![pos2(x-5.0,badge.bottom()),pos2(x+5.0,badge.bottom()),pos2(x,badge.bottom()+6.0)],design::ACCENT,Stroke::NONE));
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
    fn rational_elapsed_labels_and_long_fit_ticks() -> lumeryn_core::Result<()> {
        let rate = FrameRate::new(30_000, 1001)?;
        assert_eq!(timecode(30_000, rate, true), "16:41.000");
        assert_eq!(timecode(15, FrameRate::new(30, 1)?, true), "00:00.500");
        assert_eq!(timecode(720, FrameRate::new(30, 1)?, false), "00:24");
        // Even very long fit timelines retain a bounded number of ruler ticks.
        assert!(nice_tick_frames(FrameRate::new(30, 1)?, 0.000_001) >= 90_000_000);
        let uv = cover_uv(vec2(192.0, 108.0), vec2(100.0, 80.0));
        assert!(uv.width() < 1.0);
        assert_eq!(uv.height(), 1.0);
        Ok(())
    }

    #[test]
    fn compact_lanes_fit_available_canvas() {
        for height in [131.0, 150.0, 180.0, 204.0] {
            let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, height));
            let lanes = Lanes::new(rect, 150.0);
            assert!(lanes.ruler.height() >= 35.0);
            assert!(lanes.video.height() >= 50.0);
            assert!(lanes.audio.height() >= 28.0);
            assert!(rect.contains_rect(lanes.audio));
            assert!(lanes.video.bottom() < lanes.audio.top());
        }
        assert_eq!(
            media_label(std::path::Path::new("moving_fixture_720p.mp4")),
            "moving fixture 720p"
        );
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
        let mut timeline = Timeline {
            fit: false,
            ..Default::default()
        };
        let assets = Assets::empty();
        let mut samples = Vec::new();
        for index in 0..110 {
            let started = std::time::Instant::now();
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 300.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                timeline.show(ui, &project, &assets, None, 0);
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
        let narrow_visible = timeline.visible_clips;
        timeline.fit = true;
        let fit_started = std::time::Instant::now();
        let mut fit_output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 300.0))),
                ..Default::default()
            },
            |ui| {
                timeline.show(ui, &project, &assets, None, 0);
            },
        );
        fit_output.textures_delta.clear();
        assert!(
            timeline.visible_clips < 1000,
            "dense fit must coalesce subpixel clips"
        );
        let report = serde_json::json!({"clips":10_000,"samples":samples.len(),"visible_clips":narrow_visible,"p50_ms":samples[50],"p95_ms":samples[95],"fit_painted_clips":timeline.visible_clips,"fit_single_pass_ms":fit_started.elapsed().as_secs_f64()*1000.0,"scope":"CPU egui layout/custom paint, no GPU presentation; scans all clip metadata but paints only visible clips"});
        println!("TIMELINE_BENCHMARK {report}");
        if let Some(path) = std::env::var_os("LUMERYN_TIMELINE_BENCHMARK") {
            let mut file = std::fs::File::create_new(path)?;
            use std::io::Write;
            file.write_all(&serde_json::to_vec_pretty(&report)?)?;
        }
        Ok(())
    }
}
