use crate::{assets::Assets, design};
use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use lumeryn_core::{Project, model::Clip, time::FrameRate};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Select(Uuid),
    Seek(u64),
    Trim {
        clip: Uuid,
        duration: u64,
    },
    Split {
        clip: Uuid,
        at_frame: u64,
    },
    Duplicate(Uuid),
    Copy(Uuid),
    Delete(Uuid),
    Reorder {
        clip: Uuid,
        before: Option<Uuid>,
    },
    Drop {
        payload: DragPayload,
        before: Option<Uuid>,
    },
    RejectDrop(&'static str),
    DropAudio {
        payload: DragPayload,
        at_frame: u64,
    },
    TrimAudio {
        clip: Uuid,
        duration: u64,
    },
    DeleteAudio(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragSource {
    Media(Uuid),
    Clip(Uuid),
    AudioClip { clip: Uuid, grab_offset: u64 },
}

/// Captured at drag start. Never rebase a gesture onto a later committed graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragPayload {
    pub project: Uuid,
    pub revision: u64,
    pub epoch: u64,
    pub source: DragSource,
}

#[derive(Debug, Clone, Copy)]
pub struct View {
    pub selected: Option<Uuid>,
    pub playhead: u64,
    pub epoch: u64,
    pub editable: bool,
}

/// Midpoints select insertion boundaries, never a position inside an existing clip.
fn insertion_target(project: &Project, frame: f64, moving: Option<Uuid>) -> Option<Uuid> {
    project
        .clips
        .iter()
        .filter(|clip| Some(clip.id) != moving)
        .filter(|clip| frame < clip.at_frame as f64 + clip.duration_frames as f64 / 2.0)
        .min_by_key(|clip| clip.at_frame)
        .map(|clip| clip.id)
}

fn reorder_changes(project: &Project, moving: Uuid, before: Option<Uuid>) -> bool {
    let mut sequence: Vec<_> = project.clips.iter().collect();
    sequence.sort_by_key(|clip| clip.at_frame);
    sequence
        .iter()
        .position(|clip| clip.id == moving)
        .is_some_and(|index| sequence.get(index + 1).map(|clip| clip.id) != before)
        && before != Some(moving)
}

fn snap_audio_frame(project: &Project, frame: u64, playhead: u64, px: f32) -> u64 {
    std::iter::once(0)
        .chain(std::iter::once(playhead))
        .chain(project.clips.iter().flat_map(|clip| {
            [
                clip.at_frame,
                clip.at_frame.saturating_add(clip.duration_frames),
            ]
        }))
        .filter(|candidate| candidate.abs_diff(frame) as f32 * px <= 8.0)
        .min_by_key(|candidate| candidate.abs_diff(frame))
        .unwrap_or(frame)
}

#[derive(Debug)]
pub struct Timeline {
    pub zoom: f32,
    pub fit: bool,
    trim: Option<TrimGesture>,
    audio_trim: Option<TrimGesture>,
    pub visible_clips: usize,
}

#[derive(Debug, Clone, Copy)]
struct TrimGesture {
    clip: Uuid,
    project: Uuid,
    revision: u64,
    epoch: u64,
    duration: u64,
}

impl TrimGesture {
    fn matches(self, project: &Project, epoch: u64) -> bool {
        self.project == project.id && self.revision == project.revision && self.epoch == epoch
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            fit: true,
            trim: None,
            audio_trim: None,
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
    music: Option<Rect>,
}

impl Lanes {
    #[cfg(test)]
    fn new(rect: Rect, origin: f32) -> Self {
        Self::with_audio(rect, origin, false)
    }
    fn with_audio(rect: Rect, origin: f32, independent: bool) -> Self {
        if independent {
            let ruler_height = if rect.height() < 184.0 { 35.0 } else { 43.0 };
            let body = (rect.height() - ruler_height - 21.0).max(0.0);
            let video_height = body * 0.48;
            let linked_height = body * 0.22;
            let ruler = Rect::from_min_max(
                pos2(origin, rect.top()),
                pos2(rect.right(), rect.top() + ruler_height),
            );
            let video = Rect::from_min_max(
                pos2(origin, ruler.bottom() + 4.0),
                pos2(rect.right(), ruler.bottom() + 4.0 + video_height),
            );
            let audio = Rect::from_min_max(
                pos2(origin, video.bottom() + 4.0),
                pos2(rect.right(), video.bottom() + 4.0 + linked_height),
            );
            let music = Rect::from_min_max(
                pos2(origin, audio.bottom() + 4.0),
                pos2(rect.right(), rect.bottom() - 5.0),
            );
            return Self {
                ruler,
                video,
                audio,
                music: Some(music),
            };
        }
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
            music: None,
        }
    }
}

impl Timeline {
    pub fn cancel_gesture(&mut self) {
        self.trim = None;
        self.audio_trim = None;
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        assets: &Assets,
        view: View,
    ) -> Vec<Action> {
        let View {
            selected,
            playhead,
            epoch,
            editable,
        } = view;
        if !editable || ui.input(|input| !input.focused || input.key_pressed(egui::Key::Escape)) {
            egui::DragAndDrop::clear_payload(ui.ctx());
            self.cancel_gesture();
        }
        if self
            .audio_trim
            .is_some_and(|trim| !trim.matches(project, epoch))
        {
            self.audio_trim = None;
        }
        if self.trim.is_some_and(|trim| !trim.matches(project, epoch)) {
            self.trim = None;
        }
        let mut actions = Vec::new();
        let total = total_frames(project);
        let fps = project.frame_rate.numerator() as f32 / project.frame_rate.denominator() as f32;
        let header = 150.0;
        let width = ui.available_width();
        ui.horizontal(|ui| {
            let (tool, response) = ui.allocate_exact_size(vec2(36.0, 32.0), Sense::hover());
            response.on_hover_text("Drag clips to rearrange; drag the right edge to trim");
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
        let mut scroll = egui::ScrollArea::horizontal().id_salt(("studio-timeline-scroll",project.id,epoch)).scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Never, ..Default::default() }).max_height(canvas_height).auto_shrink([false, false]).show(ui, |ui| {
            let height = ui.available_height().min(canvas_height);
            let (rect, _) = ui.allocate_exact_size(vec2(content_width, height), Sense::hover());
            let canvas_viewport = ui.clip_rect().intersect(rect);
            let viewport = Rect::from_min_max(pos2((canvas_viewport.left()+header).min(canvas_viewport.right()),canvas_viewport.top()),canvas_viewport.right_bottom());
            let origin = rect.left() + header;
            let first = (((viewport.left() - origin) / pixels_per_frame).max(0.0).floor() as u64).min(total);
            let last = ((((viewport.right() - origin) / pixels_per_frame).max(0.0).ceil() as u64) + 1).min(total.saturating_add(1));
            let Lanes { ruler, video, audio, music } = Lanes::with_audio(rect, origin,project.audio_track_id.is_some());
            let painter = ui.painter();
            painter.rect_filled(rect, 0.0, design::BACKGROUND);
            painter.line_segment([pos2(rect.left(), ruler.bottom()+1.0), pos2(rect.right(), ruler.bottom()+1.0)], Stroke::new(1.0, design::BORDER));
            painter.line_segment([pos2(rect.left(), video.bottom()+4.0), pos2(rect.right(), video.bottom()+4.0)], Stroke::new(1.0, design::BORDER));
            painter.rect_filled(video, 3.0, design::PANEL);
            painter.rect_filled(audio, 3.0, design::PANEL);
            if let Some(music)=music {
                painter.rect_filled(music,3.0,design::PANEL);

                if project.audio_clips.is_empty() {painter.with_clip_rect(music.intersect(viewport)).text(music.center(),Align2::CENTER_CENTER,"Drag music or voiceover here",FontId::proportional(12.0),design::MUTED);}
            }
            painter.rect_filled(Rect::from_min_max(canvas_viewport.left_top(),pos2(viewport.left(),rect.bottom())),0.0,design::BACKGROUND);
            if let Some(music)=music {
                let label=Rect::from_min_max(pos2(canvas_viewport.left(),music.top()),pos2(viewport.left()-6.0,music.bottom())).intersect(canvas_viewport);
                painter.with_clip_rect(label).text(pos2(canvas_viewport.left()+20.0,music.center().y),Align2::LEFT_CENTER,"Audio 1",FontId::proportional(13.0),design::TEXT);
            }
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
                let duration = self.trim.filter(|trim| trim.clip == clip.id).map_or(clip.duration_frames, |trim| trim.duration);
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
                let label = project.media.iter().find(|m| m.id == clip.media_id).map_or_else(|| "Media".into(), |media| media.display_name());
                let label_right = if block.width()>180.0 {block.right()-64.0} else {block.right()-6.0};
                let label_painter = clip_painter.with_clip_rect(Rect::from_min_max(footer.left_top(),pos2(label_right,footer.bottom())).intersect(clipped));
                label_painter.text(pos2(block.left()+10.0, footer.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.0), design::TEXT);
                if block.width() > 180.0 { clip_painter.text(pos2(block.right()-10.0, footer.center().y), Align2::RIGHT_CENTER, timecode(duration, project.frame_rate, false), FontId::proportional(12.0), design::TEXT); }
                clip_painter.rect_stroke(block, 4.0, Stroke::new(if is_selected { 2.5 } else { 1.0 }, if is_selected { design::ACCENT } else { design::BORDER }), StrokeKind::Inside);
                let body_rect = if block.width() > 10.0 { Rect::from_min_max(block.left_top(),pos2(block.right()-8.0,block.bottom())).intersect(viewport) } else { clipped };
                let body = ui.interact(body_rect, ui.id().with(clip.id), Sense::click_and_drag());
                if editable && body.drag_started_by(egui::PointerButton::Primary) { body.dnd_set_drag_payload(DragPayload { project: project.id, revision: project.revision, epoch, source: DragSource::Clip(clip.id) }); }
                let body = body.on_hover_text("Click to select · drag to rearrange · double-click to seek · drag the right edge to trim. Source audio moves with its video.");
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
                    if handle.hovered() || handle.dragged_by(egui::PointerButton::Primary) { ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal); ui.painter().rect_filled(edge.shrink2(vec2(2.0,14.0)), 2.0, design::ACCENT); }
                    if editable && handle.drag_started_by(egui::PointerButton::Primary) {self.trim=Some(TrimGesture {clip:clip.id,project:project.id,revision:project.revision,epoch,duration:clip.duration_frames});}
                    if editable && handle.dragged_by(egui::PointerButton::Primary) && let Some(trim)=self.trim.as_mut().filter(|trim|trim.clip==clip.id && trim.matches(project,epoch)) && let Some(pointer)=handle.interact_pointer_pos() {trim.duration=((pointer.x-origin)/pixels_per_frame-clip.at_frame as f32).round().max(1.0) as u64;}
                    if handle.drag_stopped() && let Some(trim)=self.trim.take() && editable && trim.matches(project,epoch) && handle.drag_stopped_by(egui::PointerButton::Primary) {actions.push(Action::Trim {clip:trim.clip,duration:trim.duration});}
                }
                let audio_block = Rect::from_min_max(pos2(left,audio.top()+1.0),pos2(right.max(left+1.0),audio.bottom()-1.0));
                let audio_painter = ui.painter().with_clip_rect(audio_block.intersect(viewport));
                let has_audio = project.media.iter().find(|media| media.id==clip.media_id).is_some_and(|media| media.metadata.audio.is_some());
                if has_audio {
                    audio_painter.rect_filled(audio_block, 3.0, Color32::from_rgb(24,35,49));
                    let response = ui.interact(audio_block.intersect(viewport),ui.id().with((clip.id,"source-audio")),Sense::click_and_drag()).on_hover_text("Drag to move this video and its linked source audio");
                    if editable && response.drag_started_by(egui::PointerButton::Primary) { response.dnd_set_drag_payload(DragPayload { project: project.id, revision: project.revision, epoch, source: DragSource::Clip(clip.id) }); }
                    if response.clicked() { actions.push(Action::Select(clip.id)); }
                    if let Some(waveform) = assets.waveform(clip.media_id) {
                        response.on_hover_text(format!("Linked source audio · waveform from {} Hz mono PCM",waveform.sample_rate));
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
            if let Some(music)=music {
                let mut audio_clips:Vec<_>=project.audio_clips.iter().filter(|clip|clip.at_frame<last && clip.at_frame.saturating_add(clip.duration_frames)>first).collect();
                audio_clips.sort_by_key(|clip|(selected==Some(clip.id),clip.at_frame));
                for clip in audio_clips {
                    let duration=self.audio_trim.filter(|trim|trim.clip==clip.id).map_or(clip.duration_frames,|trim|trim.duration);
                    let block=Rect::from_min_max(pos2(origin+clip.at_frame as f32*pixels_per_frame,music.top()+1.0),pos2(origin+clip.at_frame.saturating_add(duration) as f32*pixels_per_frame,music.bottom()-1.0));
                    let clipped=block.intersect(viewport);
                    let paint=ui.painter().with_clip_rect(clipped);
                    paint.rect_filled(block,3.0,if clip.muted {Color32::from_rgb(36,42,50)} else {Color32::from_rgb(28,58,76)});
                    if let Some(wave)=assets.waveform(clip.media_id) {
                        let source_seconds=clip.source_in_sample as f64/48000.0;
                        let duration_seconds=duration as f64/f64::from(fps);
                        for column in 0..(clipped.width()/3.0).ceil().max(0.0) as usize {
                            let x=clipped.left()+column as f32*3.0;
                            let fraction=((x-block.left())/block.width().max(1.0)).clamp(0.0,1.0);
                            let bin=((source_seconds+f64::from(fraction)*duration_seconds)/wave.duration_seconds*wave.peaks.len() as f64) as usize;
                            if let Some(peak)=wave.peaks.get(bin) {
                                let height=(peak/wave.max_peak.max(0.001)).clamp(0.0,1.0)*block.height()*0.30;
                                paint.line_segment([pos2(x,block.center().y-height),pos2(x,block.center().y+height)],Stroke::new(1.0,Color32::from_rgb(77,135,171)));
                            }
                        }
                    }
                    let name=project.media.iter().find(|media|media.id==clip.media_id).map_or_else(||"Audio".into(),|media|media.display_name());
                    paint.rect_filled(Rect::from_min_max(pos2(block.left(),block.bottom()-18.0),block.right_bottom()),0.0,Color32::from_black_alpha(125));
                    paint.text(pos2(block.left()+7.0,block.bottom()-9.0),Align2::LEFT_CENTER,format!("{name}{}",if clip.muted {" · muted"} else {""}),FontId::proportional(11.0),design::TEXT);
                    paint.rect_stroke(block,3.0,Stroke::new(if selected==Some(clip.id) {2.0} else {1.0},if selected==Some(clip.id) {design::ACCENT} else {design::BORDER}),StrokeKind::Inside);
                    let body=Rect::from_min_max(block.left_top(),pos2((block.right()-8.0).max(block.left()),block.bottom())).intersect(viewport);
                    let response=ui.interact(body,ui.id().with((clip.id,"audio-body")),Sense::click_and_drag()).on_hover_text("Drag independently · right edge trims · overlaps mix additively");
                    if response.clicked() {actions.push(Action::Select(clip.id));}
                    if editable && response.drag_started_by(egui::PointerButton::Primary) && let Some(pointer)=ui.input(|input|input.pointer.press_origin()) {
                        let grab_offset=((pointer.x-block.left())/pixels_per_frame).round().max(0.0) as u64;
                        response.dnd_set_drag_payload(DragPayload {project:project.id,revision:project.revision,epoch,source:DragSource::AudioClip {clip:clip.id,grab_offset}});
                    }
                    response.context_menu(|ui|{ if ui.button("Delete audio").clicked() {actions.push(Action::DeleteAudio(clip.id));ui.close();} });
                    if block.width()>10.0 {
                        let edge=Rect::from_min_max(pos2(block.right()-8.0,block.top()),block.right_bottom()).intersect(viewport);
                        let handle=ui.interact(edge,ui.id().with((clip.id,"audio-trim")),Sense::drag());
                        if editable && handle.drag_started_by(egui::PointerButton::Primary) {
                            self.audio_trim=Some(TrimGesture {clip:clip.id,project:project.id,revision:project.revision,epoch,duration:clip.duration_frames});
                        }
                        if editable && handle.dragged_by(egui::PointerButton::Primary) && let Some(trim)=self.audio_trim.as_mut().filter(|trim|trim.clip==clip.id && trim.matches(project,epoch)) && let Some(pointer)=handle.interact_pointer_pos() {
                            trim.duration=((pointer.x-origin)/pixels_per_frame-clip.at_frame as f32).round().max(1.0) as u64;
                        }
                        if handle.hovered() || handle.dragged_by(egui::PointerButton::Primary) {ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);}
                        if handle.drag_stopped() && let Some(trim)=self.audio_trim.take() && editable && trim.matches(project,epoch) && handle.drag_stopped_by(egui::PointerButton::Primary) {
                            actions.push(Action::TrimAudio {clip:trim.clip,duration:trim.duration});
                        }
                    }
                }
            }
            if total == 0 {
                ui.painter().with_clip_rect(video.intersect(viewport)).text(video.center(),Align2::CENTER_CENTER,"Drag a video here to start",FontId::proportional(14.0),design::MUTED);
            }
            if let Some(payload) = egui::DragAndDrop::payload::<DragPayload>(ui.ctx()) {
                let released = ui.input(|input| input.pointer.primary_released());
                let pointer = ui.input(|input| if released {input.pointer.interact_pos()} else {input.pointer.hover_pos()});
                if let Some(pointer) = pointer {
                    let music_zone=music.map(|rect|rect.intersect(viewport));
                    if let Some(zone)=music_zone && zone.contains(pointer) && ui.ctx().layer_id_at(pointer)==Some(ui.layer_id()) {
                        let stale=payload.project!=project.id || payload.revision!=project.revision || payload.epoch!=epoch;
                        let accepted=match payload.source {DragSource::Media(id)=>project.media.iter().any(|media|media.id==id && media.metadata.audio.is_some()),DragSource::AudioClip {clip,..}=>project.audio_clips.iter().any(|item|item.id==clip),DragSource::Clip(_)=>false};
                        let raw=((pointer.x-origin)/pixels_per_frame).round().max(0.0) as u64;
                        let offset=match payload.source {DragSource::AudioClip {grab_offset,..}=>grab_offset,_=>0};
                        let maximum=match payload.source {DragSource::AudioClip {clip,..}=>project.audio_clips.iter().find(|item|item.id==clip).map_or(0,|item|total.saturating_sub(item.duration_frames)),_=>total.saturating_sub(1)};
                        let candidate=raw.saturating_sub(offset).min(maximum);
                        let at_frame=snap_audio_frame(project,candidate,playhead,pixels_per_frame).min(maximum);
                        if !editable || stale || !accepted || total==0 {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed);
                            if released {egui::DragAndDrop::clear_payload(ui.ctx());actions.push(Action::RejectDrop(if stale {"The project changed during this drag. Drag again."} else {"Add a video first, then drop an audio source on Audio 1."}));}
                        } else {
                            let x=origin+at_frame as f32*pixels_per_frame;
                            ui.painter().with_clip_rect(zone).line_segment([pos2(x,zone.top()),pos2(x,zone.bottom())],Stroke::new(3.0,design::ACCENT));
                            if released {egui::DragAndDrop::clear_payload(ui.ctx());actions.push(Action::DropAudio {payload:*payload,at_frame});}
                        }
                        // Music rectangles are disjoint from video and linked audio.
                    }
                    let video_zone = video.intersect(viewport);
                    let linked_zone = audio.intersect(viewport);
                    let over_video = video_zone.contains(pointer);
                    let over_linked = linked_zone.contains(pointer);
                    if (over_video || over_linked) && ui.ctx().layer_id_at(pointer) == Some(ui.layer_id()) {
                        let stale = payload.project != project.id || payload.revision != project.revision || payload.epoch != epoch;
                        let playable = match payload.source { DragSource::Media(id) => project.media.iter().any(|media| media.id == id && media.metadata.video.is_some()), DragSource::Clip(id) => project.clips.iter().any(|clip| clip.id == id), DragSource::AudioClip {..}=>false };
                        let error = if stale {Some("The project changed during this drag. Drag again from the current project.")} else if !editable {Some("Wait for the current edit or connection before dropping.")} else if !playable {Some("Drop audio on Audio 1; upgrade legacy projects from the Project panel.")} else if over_linked && matches!(payload.source,DragSource::Media(_)) {Some("Drop video on the Video 1 lane. This waveform lane is linked source audio.")} else {None};
                        if let Some(error) = error {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed);
                            ui.painter().rect_stroke(if over_video {video_zone} else {linked_zone},3.0,Stroke::new(2.0,design::WARNING),StrokeKind::Inside);
                            if released { egui::DragAndDrop::clear_payload(ui.ctx()); actions.push(Action::RejectDrop(error)); }
                        } else {
                            let moving = match payload.source {DragSource::Clip(id)=>Some(id),_=>None};
                            let before = insertion_target(project,f64::from((pointer.x-origin)/pixels_per_frame),moving);
                            let frame = before.and_then(|id|project.clips.iter().find(|clip|clip.id==id)).map_or(total,|clip|clip.at_frame);
                            let x = (origin+frame as f32*pixels_per_frame).clamp(viewport.left()+2.0,viewport.right()-2.0);
                            let marker = ui.painter().with_clip_rect(viewport);
                            marker.line_segment([pos2(x,video.top()),pos2(x,audio.bottom())],Stroke::new(3.0,design::ACCENT));
                            marker.circle_filled(pos2(x,video.top()+3.0),5.0,design::ACCENT);
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                            if released {
                                egui::DragAndDrop::clear_payload(ui.ctx());
                                if moving.is_none_or(|id| reorder_changes(project,id,before)) { actions.push(Action::Drop {payload:*payload,before}); }
                            }
                        }
                    }
                }
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
        // Edge scrolling only changes the viewport, never project state. Fit has no overflow.
        if !self.fit
            && editable
            && let Some(payload) = egui::DragAndDrop::payload::<DragPayload>(ui.ctx())
            && payload.project == project.id
            && payload.revision == project.revision
            && payload.epoch == epoch
            && let Some(pointer) = ui.input(|input| input.pointer.hover_pos())
            && scroll.inner_rect.contains(pointer)
            && pointer.x >= scroll.inner_rect.left() + header
            && ui.ctx().layer_id_at(pointer) == Some(ui.layer_id())
        {
            let lanes = Lanes::with_audio(
                scroll.inner_rect,
                scroll.inner_rect.left() + header,
                project.audio_track_id.is_some(),
            );
            let over_video = lanes.video.contains(pointer);
            let over_linked = lanes.audio.contains(pointer);
            let accepts = match payload.source {
                DragSource::Media(id) => {
                    (over_video
                        && project
                            .media
                            .iter()
                            .any(|media| media.id == id && media.metadata.video.is_some()))
                        || (lanes.music.is_some_and(|music| music.contains(pointer))
                            && project
                                .media
                                .iter()
                                .any(|media| media.id == id && media.metadata.audio.is_some()))
                }
                DragSource::Clip(id) => {
                    (over_video || over_linked) && project.clips.iter().any(|clip| clip.id == id)
                }
                DragSource::AudioClip { clip, .. } => {
                    lanes.music.is_some_and(|music| music.contains(pointer))
                        && project.audio_clips.iter().any(|item| item.id == clip)
                }
            };
            let edge = 36.0;
            let left = scroll.inner_rect.left() + header;
            let speed = if pointer.x < left + edge {
                -((left + edge - pointer.x) / edge).clamp(0.0, 1.0)
            } else if pointer.x > scroll.inner_rect.right() - edge {
                ((pointer.x - scroll.inner_rect.right() + edge) / edge).clamp(0.0, 1.0)
            } else {
                0.0
            };
            if accepts && speed != 0.0 {
                let dt = ui.input(|input| input.stable_dt).min(0.05);
                let previous = scroll.state.offset.x;
                scroll.state.offset.x = (scroll.state.offset.x + speed * 600.0 * dt).clamp(
                    0.0,
                    (scroll.content_size.x - scroll.inner_rect.width()).max(0.0),
                );
                scroll.state.store(ui.ctx(), scroll.id);
                if previous != scroll.state.offset.x {
                    ui.ctx().request_repaint();
                }
            }
        }
        actions
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn drag_project() -> lumeryn_core::Result<Project> {
        let rate = FrameRate::new(30, 1)?;
        let mut project = Project::new("Drag fixture".into(), 320, 180, rate)?;
        let media = Uuid::new_v4();
        project.media.push(lumeryn_core::model::Media {
            id: media,
            path: "synthetic.mkv".into(),
            alias: None,
            metadata: lumeryn_core::media::Metadata {
                size_bytes: 1,
                blake3: None,
                audio: None,
                video: Some(lumeryn_core::media::Video {
                    codec: "synthetic".into(),
                    width: 320,
                    height: 180,
                    frame_rate: rate,
                    frames: 90,
                    start_time_micros: 0,
                    sample_aspect_ratio: "1:1".into(),
                    color_transfer: None,
                    constant_rate: true,
                }),
            },
        });
        project.clips = (0..3)
            .map(|index| Clip {
                id: Uuid::new_v4(),
                media_id: media,
                track_id: project.video_track_id,
                at_frame: index * 30,
                source_in_frame: index * 30,
                duration_frames: 30,
            })
            .collect();
        project.validate()?;
        Ok(project)
    }

    struct Gesture {
        ctx: egui::Context,
        timeline: Timeline,
        source: Rect,
        lane: Rect,
        linked: Rect,
        music: Option<Rect>,
        pixels_per_frame: f32,
        time: f64,
        focused: bool,
        editable: bool,
        scroll_id: Option<egui::Id>,
    }
    impl Gesture {
        fn new() -> Self {
            let ctx = egui::Context::default();
            crate::design::configure(&ctx);
            Self {
                ctx,
                timeline: Timeline::default(),
                source: Rect::NOTHING,
                lane: Rect::NOTHING,
                linked: Rect::NOTHING,
                music: None,
                pixels_per_frame: 0.0,
                time: 0.0,
                focused: true,
                editable: true,
                scroll_id: None,
            }
        }
        fn pass(&mut self, project: &Project, epoch: u64, events: Vec<egui::Event>) -> Vec<Action> {
            self.time += 1.0 / 60.0;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 350.0))),
                time: Some(self.time),
                events,
                focused: self.focused,
                ..Default::default()
            };
            let mut actions = Vec::new();
            let mut output = self.ctx.run_ui(input, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(vec2(110.0, 50.0), Sense::click_and_drag());
                self.source = rect;
                if response.drag_started_by(egui::PointerButton::Primary) {
                    response.dnd_set_drag_payload(DragPayload {
                        project: project.id,
                        revision: project.revision,
                        epoch,
                        source: DragSource::Media(project.media[0].id),
                    });
                }
                ui.add_space(10.0);
                let canvas = Rect::from_min_size(
                    pos2(ui.cursor().left(), ui.cursor().top() + 39.0),
                    vec2(
                        ui.available_width(),
                        (ui.available_height() - 39.0).clamp(0.0, 204.0),
                    ),
                );
                let lanes = Lanes::with_audio(
                    canvas,
                    canvas.left() + 150.0,
                    project.audio_track_id.is_some(),
                );
                self.lane = lanes.video;
                self.linked = lanes.audio;
                self.music = lanes.music;
                self.pixels_per_frame = if self.timeline.fit {
                    (ui.available_width() - 172.0) / total_frames(project).max(1) as f32
                } else {
                    2.0 * self.timeline.zoom
                };
                self.scroll_id = Some(ui.make_persistent_id(egui::IdSalt::new((
                    "studio-timeline-scroll",
                    project.id,
                    epoch,
                ))));
                actions = self.timeline.show(
                    ui,
                    project,
                    &Assets::empty(),
                    View {
                        selected: None,
                        playhead: 0,
                        epoch,
                        editable: self.editable,
                    },
                );
            });
            output.textures_delta.clear();
            actions
        }
        fn button(pos: egui::Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
            egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }
        }
        fn start_media(&mut self, project: &Project) {
            self.pass(project, 1, vec![]);
            let source = self.source.center();
            self.pass(project, 1, vec![egui::Event::PointerMoved(source)]);
            self.pass(
                project,
                1,
                vec![Self::button(source, egui::PointerButton::Primary, true)],
            );
            self.pass(
                project,
                1,
                vec![egui::Event::PointerMoved(source + vec2(15.0, 0.0))],
            );
            assert!(egui::DragAndDrop::has_payload_of_type::<DragPayload>(
                &self.ctx
            ));
        }
    }

    #[test]
    fn actual_media_pointer_drag_inserts_once_and_snaps() -> lumeryn_core::Result<()> {
        let project = drag_project()?;
        let mut gesture = Gesture::new();
        gesture.start_media(&project);
        let destination = pos2(gesture.lane.left() + 330.0, gesture.lane.center().y);
        assert!(
            gesture
                .pass(&project, 1, vec![egui::Event::PointerMoved(destination)])
                .is_empty()
        );
        let actions = gesture.pass(
            &project,
            1,
            vec![Gesture::button(
                destination,
                egui::PointerButton::Primary,
                false,
            )],
        );
        assert!(
            matches!(actions.as_slice(),[Action::Drop {payload:DragPayload {source:DragSource::Media(id),..},before:Some(before)}] if *id==project.media[0].id && *before==project.clips[1].id)
        );
        assert!(gesture.pass(&project, 1, vec![]).is_empty());
        assert!(!egui::DragAndDrop::has_any_payload(&gesture.ctx));
        Ok(())
    }

    #[test]
    fn cancelled_outside_secondary_and_stale_drops_never_edit() -> lumeryn_core::Result<()> {
        let mut project = drag_project()?;
        let mut gesture = Gesture::new();
        gesture.start_media(&project);
        let destination = gesture.lane.center();
        gesture.pass(&project, 1, vec![egui::Event::PointerMoved(destination)]);
        let escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert!(gesture.pass(&project, 1, vec![escape]).is_empty());
        assert!(
            gesture
                .pass(
                    &project,
                    1,
                    vec![Gesture::button(
                        destination,
                        egui::PointerButton::Primary,
                        false
                    )]
                )
                .is_empty()
        );
        assert!(!egui::DragAndDrop::has_any_payload(&gesture.ctx));
        gesture.start_media(&project);
        let outside = pos2(900.0, 20.0);
        gesture.pass(&project, 1, vec![egui::Event::PointerMoved(outside)]);
        assert!(
            gesture
                .pass(
                    &project,
                    1,
                    vec![Gesture::button(
                        outside,
                        egui::PointerButton::Primary,
                        false
                    )]
                )
                .is_empty()
        );
        gesture.start_media(&project);
        gesture.pass(&project, 1, vec![egui::Event::PointerMoved(destination)]);
        assert!(
            gesture
                .pass(
                    &project,
                    1,
                    vec![Gesture::button(
                        destination,
                        egui::PointerButton::Secondary,
                        false
                    )]
                )
                .is_empty()
        );
        gesture.pass(
            &project,
            1,
            vec![Gesture::button(
                destination,
                egui::PointerButton::Primary,
                false,
            )],
        );
        gesture.start_media(&project);
        project.revision += 1;
        gesture.pass(&project, 1, vec![egui::Event::PointerMoved(destination)]);
        assert!(matches!(
            gesture
                .pass(
                    &project,
                    1,
                    vec![Gesture::button(
                        destination,
                        egui::PointerButton::Primary,
                        false
                    )]
                )
                .as_slice(),
            [Action::RejectDrop(_)]
        ));
        gesture.start_media(&project);
        gesture.pass(&project, 2, vec![egui::Event::PointerMoved(destination)]);
        assert!(matches!(
            gesture
                .pass(
                    &project,
                    2,
                    vec![Gesture::button(
                        destination,
                        egui::PointerButton::Primary,
                        false
                    )]
                )
                .as_slice(),
            [Action::RejectDrop(_)]
        ));
        Ok(())
    }

    #[test]
    fn actual_clip_body_drag_reorders_without_trimming() -> lumeryn_core::Result<()> {
        let project = drag_project()?;
        let mut gesture = Gesture::new();
        gesture.pass(&project, 1, vec![]);
        let from = pos2(gesture.lane.left() + 40.0, gesture.lane.center().y);
        gesture.pass(&project, 1, vec![egui::Event::PointerMoved(from)]);
        gesture.pass(
            &project,
            1,
            vec![Gesture::button(from, egui::PointerButton::Primary, true)],
        );
        gesture.pass(
            &project,
            1,
            vec![egui::Event::PointerMoved(from + vec2(15.0, 0.0))],
        );
        assert!(
            matches!(egui::DragAndDrop::payload::<DragPayload>(&gesture.ctx).map(|p|p.source),Some(DragSource::Clip(id)) if id==project.clips[0].id)
        );
        let to = pos2(gesture.lane.right() - 8.0, from.y);
        gesture.pass(&project, 1, vec![egui::Event::PointerMoved(to)]);
        assert!(
            matches!(gesture.pass(&project,1,vec![Gesture::button(to,egui::PointerButton::Primary,false)]).as_slice(),[Action::Drop {payload:DragPayload {source:DragSource::Clip(id),..},before:None}] if *id==project.clips[0].id)
        );
        assert!(gesture.timeline.trim.is_none());
        Ok(())
    }
    fn start_pointer(
        gesture: &mut Gesture,
        project: &Project,
        from: egui::Pos2,
        button: egui::PointerButton,
    ) {
        assert!(
            gesture
                .pass(project, 1, vec![egui::Event::PointerMoved(from)])
                .is_empty()
        );
        gesture.pass(project, 1, vec![Gesture::button(from, button, true)]);
        gesture.pass(
            project,
            1,
            vec![egui::Event::PointerMoved(from + vec2(15.0, 0.0))],
        );
    }

    #[test]
    fn actual_trim_edge_drag_does_not_create_reorder_payload() -> lumeryn_core::Result<()> {
        let project = drag_project()?;
        let mut gesture = Gesture::new();
        gesture.pass(&project, 1, vec![]);
        let from = pos2(
            gesture.lane.left() + 30.0 * gesture.pixels_per_frame - 3.0,
            gesture.lane.center().y,
        );
        start_pointer(&mut gesture, &project, from, egui::PointerButton::Primary);
        assert!(!egui::DragAndDrop::has_any_payload(&gesture.ctx));
        let destination = from - vec2(45.0, 0.0);
        assert!(
            gesture
                .pass(&project, 1, vec![egui::Event::PointerMoved(destination)])
                .is_empty()
        );
        assert!(gesture.timeline.trim.is_some());
        let actions = gesture.pass(
            &project,
            1,
            vec![Gesture::button(
                destination,
                egui::PointerButton::Primary,
                false,
            )],
        );
        assert!(
            matches!(actions.as_slice(), [Action::Trim { clip, duration }] if *clip == project.clips[0].id && *duration < 30)
        );
        assert!(gesture.timeline.trim.is_none());
        Ok(())
    }

    #[test]
    fn actual_video_trim_secondary_button_never_starts_or_commits() -> lumeryn_core::Result<()> {
        let project = drag_project()?;
        let mut gesture = Gesture::new();
        gesture.pass(&project, 1, vec![]);
        let edge = pos2(
            gesture.lane.left() + 30.0 * gesture.pixels_per_frame - 3.0,
            gesture.lane.center().y,
        );
        start_pointer(&mut gesture, &project, edge, egui::PointerButton::Secondary);
        assert!(gesture.timeline.trim.is_none());
        let destination = edge - vec2(50.0, 0.0);
        assert!(
            gesture
                .pass(&project, 1, vec![egui::Event::PointerMoved(destination)])
                .is_empty()
        );
        assert!(
            gesture
                .pass(
                    &project,
                    1,
                    vec![Gesture::button(
                        destination,
                        egui::PointerButton::Secondary,
                        false
                    )]
                )
                .is_empty()
        );
        assert!(gesture.timeline.trim.is_none());
        Ok(())
    }

    #[test]
    fn actual_video_trim_cancels_on_escape_disabled_focus_overlay_or_stale_identity()
    -> lumeryn_core::Result<()> {
        for reason in [
            "escape", "disabled", "focus", "overlay", "revision", "epoch", "project",
        ] {
            let mut project = drag_project()?;
            let mut gesture = Gesture::new();
            gesture.pass(&project, 1, vec![]);
            let edge = pos2(
                gesture.lane.left() + 30.0 * gesture.pixels_per_frame - 3.0,
                gesture.lane.center().y,
            );
            start_pointer(&mut gesture, &project, edge, egui::PointerButton::Primary);
            assert!(gesture.timeline.trim.is_some(), "{reason}");
            let destination = edge - vec2(50.0, 0.0);
            let mut epoch = 1;
            let events = match reason {
                "escape" => vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                "disabled" => {
                    gesture.editable = false;
                    vec![]
                }
                "focus" => {
                    gesture.focused = false;
                    vec![]
                }
                "overlay" => {
                    gesture.timeline.cancel_gesture();
                    vec![]
                }
                "revision" => {
                    project.revision += 1;
                    vec![]
                }
                "epoch" => {
                    epoch = 2;
                    vec![]
                }
                "project" => {
                    project.id = Uuid::new_v4();
                    vec![]
                }
                _ => {
                    return Err(lumeryn_core::Error::Invalid(
                        "Unexpected test cancellation".into(),
                    ));
                }
            };
            assert!(gesture.pass(&project, epoch, events).is_empty(), "{reason}");
            assert!(gesture.timeline.trim.is_none(), "{reason}");
            gesture.editable = true;
            gesture.focused = true;
            assert!(
                gesture
                    .pass(
                        &project,
                        epoch,
                        vec![egui::Event::PointerMoved(destination)]
                    )
                    .is_empty(),
                "{reason}"
            );
            assert!(
                gesture
                    .pass(
                        &project,
                        epoch,
                        vec![Gesture::button(
                            destination,
                            egui::PointerButton::Primary,
                            false
                        )]
                    )
                    .is_empty(),
                "{reason}"
            );
            assert!(gesture.timeline.trim.is_none(), "{reason}");
        }
        Ok(())
    }

    #[test]
    fn actual_empty_timeline_drop_inserts_without_existing_target() -> lumeryn_core::Result<()> {
        let mut project = drag_project()?;
        project.clips.clear();
        project.validate()?;
        let mut gesture = Gesture::new();
        gesture.start_media(&project);
        let destination = gesture.lane.center();
        assert!(
            gesture
                .pass(&project, 1, vec![egui::Event::PointerMoved(destination)])
                .is_empty()
        );
        assert!(
            matches!(gesture.pass(&project, 1, vec![Gesture::button(destination, egui::PointerButton::Primary, false)]).as_slice(), [Action::Drop { payload: DragPayload { source: DragSource::Media(id), .. }, before: None }] if *id == project.media[0].id)
        );
        assert!(gesture.pass(&project, 1, vec![]).is_empty());
        Ok(())
    }

    #[test]
    fn actual_focus_loss_cancels_held_drag_after_resume() -> lumeryn_core::Result<()> {
        let project = drag_project()?;
        let mut gesture = Gesture::new();
        gesture.start_media(&project);
        let destination = gesture.lane.center();
        gesture.pass(&project, 1, vec![egui::Event::PointerMoved(destination)]);
        gesture.focused = false;
        assert!(
            gesture
                .pass(&project, 1, vec![egui::Event::PointerGone])
                .is_empty()
        );
        assert!(!egui::DragAndDrop::has_any_payload(&gesture.ctx));
        gesture.focused = true;
        assert!(
            gesture
                .pass(&project, 1, vec![egui::Event::PointerMoved(destination)])
                .is_empty()
        );
        assert!(
            gesture
                .pass(
                    &project,
                    1,
                    vec![Gesture::button(
                        destination,
                        egui::PointerButton::Primary,
                        false
                    )]
                )
                .is_empty()
        );
        assert!(!egui::DragAndDrop::has_any_payload(&gesture.ctx));
        Ok(())
    }

    #[test]
    fn actual_secondary_initiation_never_creates_edit_payload() -> lumeryn_core::Result<()> {
        let project = drag_project()?;
        for card in [true, false] {
            let mut gesture = Gesture::new();
            gesture.pass(&project, 1, vec![]);
            let from = if card {
                gesture.source.center()
            } else {
                pos2(gesture.lane.left() + 40.0, gesture.lane.center().y)
            };
            start_pointer(&mut gesture, &project, from, egui::PointerButton::Secondary);
            assert!(!egui::DragAndDrop::has_any_payload(&gesture.ctx));
            let destination = pos2(gesture.lane.right() - 8.0, gesture.lane.center().y);
            assert!(
                gesture
                    .pass(&project, 1, vec![egui::Event::PointerMoved(destination)])
                    .is_empty()
            );
            assert!(
                gesture
                    .pass(
                        &project,
                        1,
                        vec![Gesture::button(
                            destination,
                            egui::PointerButton::Secondary,
                            false
                        )]
                    )
                    .is_empty()
            );
            assert!(!egui::DragAndDrop::has_any_payload(&gesture.ctx));
        }
        Ok(())
    }

    #[test]
    fn actual_same_slot_drop_is_noop() -> lumeryn_core::Result<()> {
        let project = drag_project()?;
        let mut gesture = Gesture::new();
        gesture.pass(&project, 1, vec![]);
        let from = pos2(gesture.lane.left() + 40.0, gesture.lane.center().y);
        start_pointer(&mut gesture, &project, from, egui::PointerButton::Primary);
        assert!(egui::DragAndDrop::has_payload_of_type::<DragPayload>(
            &gesture.ctx
        ));
        let destination = from + vec2(30.0, 0.0);
        assert!(
            gesture
                .pass(&project, 1, vec![egui::Event::PointerMoved(destination)])
                .is_empty()
        );
        assert!(
            gesture
                .pass(
                    &project,
                    1,
                    vec![Gesture::button(
                        destination,
                        egui::PointerButton::Primary,
                        false
                    )]
                )
                .is_empty()
        );
        assert!(!egui::DragAndDrop::has_any_payload(&gesture.ctx));
        Ok(())
    }

    #[test]
    fn actual_audio_only_media_drop_is_explicitly_rejected() -> lumeryn_core::Result<()> {
        let mut project = drag_project()?;
        project.clips.clear();
        project.media[0].metadata.video = None;
        project.media[0].metadata.blake3 = Some("a".repeat(64));
        project.media[0].metadata.audio = Some(lumeryn_core::media::Audio {
            canonical_samples_48k: None,
            codec: "pcm_s16le".into(),
            sample_rate: 48_000,
            channels: 2,
            start_time_micros: 0,
        });
        project.validate()?;
        let mut gesture = Gesture::new();
        gesture.start_media(&project);
        let destination = gesture.lane.center();
        assert!(
            gesture
                .pass(&project, 1, vec![egui::Event::PointerMoved(destination)])
                .is_empty()
        );
        assert!(
            matches!(gesture.pass(&project, 1, vec![Gesture::button(destination, egui::PointerButton::Primary, false)]).as_slice(), [Action::RejectDrop(message)] if message.contains("Audio 1"))
        );
        assert!(!egui::DragAndDrop::has_any_payload(&gesture.ctx));
        Ok(())
    }

    #[test]
    fn actual_linked_audio_drag_works_before_waveform_is_available() -> lumeryn_core::Result<()> {
        let mut project = drag_project()?;
        project.media[0].metadata.audio = Some(lumeryn_core::media::Audio {
            canonical_samples_48k: None,
            codec: "pcm_s16le".into(),
            sample_rate: 48_000,
            channels: 2,
            start_time_micros: 0,
        });
        project.validate()?;
        let mut gesture = Gesture::new();
        gesture.pass(&project, 1, vec![]);
        let from = pos2(gesture.lane.left() + 40.0, gesture.linked.center().y);
        start_pointer(&mut gesture, &project, from, egui::PointerButton::Primary);
        assert!(
            matches!(egui::DragAndDrop::payload::<DragPayload>(&gesture.ctx).map(|payload| payload.source), Some(DragSource::Clip(id)) if id == project.clips[0].id)
        );
        let destination = pos2(gesture.lane.right() - 8.0, from.y);
        assert!(
            gesture
                .pass(&project, 1, vec![egui::Event::PointerMoved(destination)])
                .is_empty()
        );
        assert!(
            matches!(gesture.pass(&project, 1, vec![Gesture::button(destination, egui::PointerButton::Primary, false)]).as_slice(), [Action::Drop { payload: DragPayload { source: DragSource::Clip(id), .. }, before: None }] if *id == project.clips[0].id)
        );
        Ok(())
    }

    fn independent_audio_fixture() -> lumeryn_core::Result<Project> {
        let mut project = drag_project()?;
        project.media[0].metadata.audio = Some(lumeryn_core::media::Audio {
            canonical_samples_48k: Some(144000),
            codec: "pcm_s16le".into(),
            sample_rate: 48000,
            channels: 2,
            start_time_micros: 0,
        });
        project.media[0].metadata.video = None;
        project.media[0].metadata.blake3 = Some("a".repeat(64));
        // Keep the visual sequence on its original video source; add a separate audio asset.
        let audio = project.media[0].clone();
        let video = drag_project()?.media.remove(0);
        project.media[0] = video;
        let video_id = project.media[0].id;
        for clip in &mut project.clips {
            clip.media_id = video_id;
        }
        project.media.insert(0, audio);
        project.validate()?;
        Ok(project)
    }

    #[test]
    fn actual_audio_media_drop_and_independent_body_move_and_trim()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut project = independent_audio_fixture()?;
        let mut gesture = Gesture::new();
        gesture.start_media(&project);
        let music = gesture.music.ok_or("missing audio lane")?;
        let destination = pos2(
            gesture.lane.left() + 30.0 * gesture.pixels_per_frame,
            music.center().y,
        );
        assert!(
            gesture
                .pass(&project, 1, vec![egui::Event::PointerMoved(destination)])
                .is_empty()
        );
        assert!(
            matches!(gesture.pass(&project,1,vec![Gesture::button(destination,egui::PointerButton::Primary,false)]).as_slice(),[Action::DropAudio {payload:DragPayload {source:DragSource::Media(id),..},at_frame:30}] if *id==project.media[0].id)
        );
        let id = Uuid::new_v4();
        project.audio_clips.push(lumeryn_core::model::AudioClip {
            id,
            media_id: project.media[0].id,
            track_id: project.audio_track_id.ok_or("missing track")?,
            at_frame: 30,
            source_in_sample: 0,
            duration_frames: 30,
            gain_millidecibels: 0,
            muted: false,
        });
        project.validate()?;
        gesture.pass(&project, 1, vec![]);
        let from = pos2(
            gesture.lane.left() + 40.0 * gesture.pixels_per_frame,
            music.center().y,
        );
        start_pointer(&mut gesture, &project, from, egui::PointerButton::Primary);
        assert!(
            matches!(egui::DragAndDrop::payload::<DragPayload>(&gesture.ctx).map(|payload|payload.source),Some(DragSource::AudioClip {clip,grab_offset:10}) if clip==id)
        );
        let to = pos2(
            gesture.lane.left() + 70.0 * gesture.pixels_per_frame,
            from.y,
        );
        gesture.pass(&project, 1, vec![egui::Event::PointerMoved(to)]);
        assert!(matches!(
            gesture
                .pass(
                    &project,
                    1,
                    vec![Gesture::button(to, egui::PointerButton::Primary, false)]
                )
                .as_slice(),
            [Action::DropAudio { at_frame: 60, .. }]
        ));
        let edge = pos2(
            gesture.lane.left() + 60.0 * gesture.pixels_per_frame - 3.0,
            music.center().y,
        );
        start_pointer(&mut gesture, &project, edge, egui::PointerButton::Primary);
        assert!(!egui::DragAndDrop::has_any_payload(&gesture.ctx));
        let shortened = pos2(
            gesture.lane.left() + 50.0 * gesture.pixels_per_frame,
            edge.y,
        );
        gesture.pass(&project, 1, vec![egui::Event::PointerMoved(shortened)]);
        assert!(
            matches!(gesture.pass(&project,1,vec![Gesture::button(shortened,egui::PointerButton::Primary,false)]).as_slice(),[Action::TrimAudio {clip,duration:20}] if *clip==id)
        );
        Ok(())
    }

    #[test]
    fn audio_lanes_fit_and_snapping_retains_integer_frames() -> lumeryn_core::Result<()> {
        for height in [131.0, 150.0, 204.0] {
            let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, height));
            let lanes = Lanes::with_audio(rect, 150.0, true);
            let music = lanes
                .music
                .ok_or_else(|| lumeryn_core::Error::Invalid("Missing music lane".into()))?;
            assert!(lanes.video.height() > 30.0);
            assert!(lanes.audio.height() > 15.0);
            assert!(music.height() > 20.0);
            assert!(rect.contains_rect(music));
            assert!(lanes.audio.bottom() < music.top());
        }
        let project = drag_project()?;
        assert_eq!(snap_audio_frame(&project, 29, 0, 3.0), 30);
        assert_eq!(snap_audio_frame(&project, 26, 0, 3.0), 26);
        Ok(())
    }

    fn audio_trim_project() -> lumeryn_core::Result<Project> {
        let mut project = independent_audio_fixture()?;
        project.audio_clips.push(lumeryn_core::model::AudioClip {
            id: Uuid::new_v4(),
            media_id: project.media[0].id,
            track_id: project
                .audio_track_id
                .ok_or_else(|| lumeryn_core::Error::Invalid("Missing audio track".into()))?,
            at_frame: 30,
            source_in_sample: 0,
            duration_frames: 30,
            gain_millidecibels: 0,
            muted: false,
        });
        project.validate()?;
        Ok(project)
    }

    fn audio_trim_edge(
        gesture: &mut Gesture,
        project: &Project,
    ) -> lumeryn_core::Result<egui::Pos2> {
        gesture.pass(project, 1, vec![]);
        Ok(pos2(
            gesture.lane.left() + 60.0 * gesture.pixels_per_frame - 3.0,
            gesture
                .music
                .ok_or_else(|| lumeryn_core::Error::Invalid("Missing audio lane".into()))?
                .center()
                .y,
        ))
    }

    #[test]
    fn actual_audio_trim_secondary_button_never_starts_or_commits() -> lumeryn_core::Result<()> {
        let project = audio_trim_project()?;
        let mut gesture = Gesture::new();
        let edge = audio_trim_edge(&mut gesture, &project)?;
        start_pointer(&mut gesture, &project, edge, egui::PointerButton::Secondary);
        assert!(gesture.timeline.audio_trim.is_none());
        let destination = edge - vec2(50.0, 0.0);
        assert!(
            gesture
                .pass(&project, 1, vec![egui::Event::PointerMoved(destination)])
                .is_empty()
        );
        assert!(
            gesture
                .pass(
                    &project,
                    1,
                    vec![Gesture::button(
                        destination,
                        egui::PointerButton::Secondary,
                        false
                    )]
                )
                .is_empty()
        );
        assert!(gesture.timeline.audio_trim.is_none());
        Ok(())
    }

    #[test]
    fn actual_audio_trim_cancels_on_escape_disabled_focus_overlay_or_stale_identity()
    -> lumeryn_core::Result<()> {
        for reason in [
            "escape", "disabled", "focus", "overlay", "revision", "epoch", "project",
        ] {
            let mut project = audio_trim_project()?;
            let mut gesture = Gesture::new();
            let edge = audio_trim_edge(&mut gesture, &project)?;
            start_pointer(&mut gesture, &project, edge, egui::PointerButton::Primary);
            assert!(gesture.timeline.audio_trim.is_some(), "{reason}");
            let destination = edge - vec2(50.0, 0.0);
            let mut epoch = 1;
            let events = match reason {
                "escape" => vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                "disabled" => {
                    gesture.editable = false;
                    vec![]
                }
                "focus" => {
                    gesture.focused = false;
                    vec![]
                }
                "overlay" => {
                    gesture.timeline.cancel_gesture();
                    vec![]
                }
                "revision" => {
                    project.revision += 1;
                    vec![]
                }
                "epoch" => {
                    epoch = 2;
                    vec![]
                }
                "project" => {
                    project.id = Uuid::new_v4();
                    vec![]
                }
                _ => {
                    return Err(lumeryn_core::Error::Invalid(
                        "Unexpected test cancellation".into(),
                    ));
                }
            };
            assert!(gesture.pass(&project, epoch, events).is_empty(), "{reason}");
            assert!(gesture.timeline.audio_trim.is_none(), "{reason}");
            gesture.editable = true;
            gesture.focused = true;
            assert!(
                gesture
                    .pass(
                        &project,
                        epoch,
                        vec![egui::Event::PointerMoved(destination)]
                    )
                    .is_empty(),
                "{reason}"
            );
            assert!(
                gesture
                    .pass(
                        &project,
                        epoch,
                        vec![Gesture::button(
                            destination,
                            egui::PointerButton::Primary,
                            false
                        )]
                    )
                    .is_empty(),
                "{reason}"
            );
            assert!(gesture.timeline.audio_trim.is_none(), "{reason}");
        }
        Ok(())
    }

    #[test]
    fn actual_audio_body_move_clamps_whole_clip_inside_visual_duration() -> lumeryn_core::Result<()>
    {
        let project = audio_trim_project()?;
        let mut gesture = Gesture::new();
        let edge = audio_trim_edge(&mut gesture, &project)?;
        let from = pos2(
            gesture.lane.left() + 40.0 * gesture.pixels_per_frame,
            edge.y,
        );
        start_pointer(&mut gesture, &project, from, egui::PointerButton::Primary);
        let to = pos2(gesture.lane.right() - 2.0, from.y);
        assert!(
            gesture
                .pass(&project, 1, vec![egui::Event::PointerMoved(to)])
                .is_empty()
        );
        assert!(matches!(
            gesture
                .pass(
                    &project,
                    1,
                    vec![Gesture::button(to, egui::PointerButton::Primary, false)]
                )
                .as_slice(),
            [Action::DropAudio { at_frame: 60, .. }]
        ));
        assert_eq!(
            60 + project.audio_clips[0].duration_frames,
            total_frames(&project)
        );
        Ok(())
    }

    #[test]
    fn actual_edge_hold_scrolls_without_edit_and_preserves_scrolled_drop_mapping()
    -> Result<(), Box<dyn std::error::Error>> {
        let project = drag_project()?;
        let mut gesture = Gesture::new();
        gesture.timeline.fit = false;
        gesture.timeline.zoom = 20.0;
        gesture.start_media(&project);
        let edge = pos2(
            gesture.ctx.content_rect().right() - 6.0,
            gesture.lane.center().y,
        );
        for _ in 0..30 {
            assert!(
                gesture
                    .pass(&project, 1, vec![egui::Event::PointerMoved(edge)])
                    .is_empty()
            );
        }
        let scroll_id = gesture.scroll_id.ok_or("missing scroll ID")?;
        let mut state = egui::scroll_area::State::load(&gesture.ctx, scroll_id)
            .ok_or("missing scroll state")?;
        // Pointer is6px inside the actual1000px viewport:5/6 of maximum
        // 600px/s for30 frames at60Hz yields250px, independent of fit geometry.
        assert!(
            (state.offset.x - 250.0).abs() < 0.01,
            "offset {}",
            state.offset.x
        );
        // 30fps, zoom20 =>40px/frame. A1200px scroll plus320px pointer offset
        // targets frame38, before the second clip's midpoint45; without scroll
        // it would select the first clip. The held pointer is not released here.
        state.offset.x = 1200.0;
        state.store(&gesture.ctx, scroll_id);
        let destination = pos2(gesture.lane.left() + 320.0, gesture.lane.center().y);
        assert!(
            gesture
                .pass(&project, 1, vec![egui::Event::PointerMoved(destination)])
                .is_empty()
        );
        assert!(
            matches!(gesture.pass(&project, 1, vec![Gesture::button(destination, egui::PointerButton::Primary, false)]).as_slice(), [Action::Drop { before: Some(before), .. }] if *before == project.clips[1].id)
        );
        assert!(gesture.pass(&project, 1, vec![]).is_empty());
        Ok(())
    }

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
            alias: None,
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
                timeline.show(
                    ui,
                    &project,
                    &assets,
                    View {
                        selected: None,
                        playhead: 0,
                        epoch: 1,
                        editable: true,
                    },
                );
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
                timeline.show(
                    ui,
                    &project,
                    &assets,
                    View {
                        selected: None,
                        playhead: 0,
                        epoch: 1,
                        editable: true,
                    },
                );
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
