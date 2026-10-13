//! Native rows for the bounded shared compositor. Thumbnails always come from sources.
use crate::{
    assets::Assets,
    design,
    timeline::{Action, DragPayload, DragSource, Timeline, View},
};
use egui::{Align2, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use lumeryn_core::Project;
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub struct VisualTrim {
    clip: Uuid,
    project: Uuid,
    revision: u64,
    epoch: u64,
    duration: u64,
}

fn snap(project: &Project, raw: u64, playhead: u64, pixels: f32) -> u64 {
    std::iter::once(0)
        .chain(std::iter::once(playhead))
        .chain(
            project
                .clips
                .iter()
                .flat_map(|c| [c.at_frame, c.at_frame + c.duration_frames]),
        )
        .chain(
            project
                .visual_clips
                .iter()
                .flat_map(|c| [c.at_frame, c.at_frame + c.duration_frames]),
        )
        .filter(|f| f.abs_diff(raw) as f32 * pixels <= 8.0)
        .min_by_key(|f| f.abs_diff(raw))
        .unwrap_or(raw)
}

#[derive(Clone, Copy)]
struct SoundBlock {
    id: Uuid,
    media: Uuid,
    at: u64,
    duration: u64,
    source_seconds: f64,
    independent: bool,
}

#[allow(clippy::too_many_arguments)] // One native row shares the canvas/gesture context.
fn sound_lane(
    timeline: &mut Timeline,
    ui: &mut egui::Ui,
    project: &Project,
    assets: &Assets,
    view: View,
    lane: Rect,
    viewport: Rect,
    origin: f32,
    pixels: f32,
    independent: bool,
    actions: &mut Vec<Action>,
) {
    let fps = project.frame_rate.numerator() as f64 / project.frame_rate.denominator() as f64;
    let blocks: Vec<_> = if independent {
        project
            .audio_clips
            .iter()
            .map(|c| SoundBlock {
                id: c.id,
                media: c.media_id,
                at: c.at_frame,
                duration: c.duration_frames,
                source_seconds: c.source_in_sample as f64 / 48_000.0,
                independent: true,
            })
            .collect()
    } else {
        project
            .clips
            .iter()
            .map(|c| SoundBlock {
                id: c.id,
                media: c.media_id,
                at: c.at_frame,
                duration: c.duration_frames,
                source_seconds: c.source_in_frame as f64 / fps,
                independent: false,
            })
            .chain(project.visual_clips.iter().filter_map(|c| {
                if let lumeryn_core::model::VisualSource::Video { source_in_frame } = c.source {
                    Some(SoundBlock {
                        id: c.id,
                        media: c.media_id,
                        at: c.at_frame,
                        duration: c.duration_frames,
                        source_seconds: source_in_frame as f64 / fps,
                        independent: false,
                    })
                } else {
                    None
                }
            }))
            .collect()
    };
    for c in blocks {
        let block = Rect::from_min_max(
            pos2(origin + c.at as f32 * pixels, lane.top() + 2.0),
            pos2(
                origin + (c.at + c.duration) as f32 * pixels,
                lane.bottom() - 2.0,
            ),
        );
        let clipped = block.intersect(viewport);
        if clipped.width() <= 0.0 || clipped.height() <= 0.0 {
            continue;
        }
        let painter = ui.painter().with_clip_rect(clipped);
        painter.rect_filled(block, 4.0, egui::Color32::from_rgb(30, 47, 63));
        if let Some(wave) = assets.waveform(c.media) {
            for x in (clipped.left().ceil() as i32..clipped.right().floor() as i32).step_by(2) {
                let time = c.source_seconds + ((x as f32 - block.left()) / pixels) as f64 / fps;
                let index = (time / wave.duration_seconds * wave.peaks.len() as f64) as usize;
                let peak = wave
                    .peaks
                    .get(index)
                    .copied()
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
                let half =
                    (peak / wave.max_peak.max(0.001)).clamp(0.0, 1.0) * block.height() * 0.38;
                painter.line_segment(
                    [
                        pos2(x as f32, block.center().y - half),
                        pos2(x as f32, block.center().y + half),
                    ],
                    Stroke::new(1.0, egui::Color32::from_rgb(78, 122, 171)),
                );
            }
        }
        if view.selected == Some(c.id) {
            painter.rect_stroke(
                block,
                4.0,
                Stroke::new(2.0, design::ACCENT),
                StrokeKind::Inside,
            );
        }
        let edge_rect = if independent {
            Rect::from_min_max(
                pos2((block.right() - 8.0).max(block.left()), block.top()),
                block.right_bottom(),
            )
        } else {
            Rect::NOTHING
        };
        let body = if independent {
            Rect::from_min_max(
                block.left_top(),
                pos2((block.right() - 8.0).max(block.left()), block.bottom()),
            )
        } else {
            block
        };
        let response = ui.interact(
            body.intersect(viewport),
            ui.id().with(("composition-sound", c.id)),
            Sense::click_and_drag(),
        );
        if response.clicked() {
            actions.push(Action::Select(c.id));
        }
        if view.editable && response.drag_started_by(egui::PointerButton::Primary) {
            let grab = ui.input(|i| i.pointer.press_origin()).map_or(0, |p| {
                ((p.x - block.left()) / pixels).round().max(0.0) as u64
            });
            egui::DragAndDrop::set_payload(
                ui.ctx(),
                DragPayload {
                    project: project.id,
                    revision: project.revision,
                    epoch: view.epoch,
                    source: if c.independent {
                        DragSource::AudioClip {
                            clip: c.id,
                            grab_offset: grab,
                        }
                    } else {
                        DragSource::VisualClip {
                            clip: c.id,
                            grab_offset: grab,
                        }
                    },
                },
            );
        }
        if independent {
            response.context_menu(|ui| {
                if ui
                    .add_enabled(view.editable, egui::Button::new("Delete audio"))
                    .clicked()
                {
                    actions.push(Action::DeleteAudio(c.id));
                    ui.close();
                }
            });
            let edge = ui
                .interact(
                    edge_rect.intersect(viewport),
                    ui.id().with(("composition-audio-trim", c.id)),
                    Sense::click_and_drag(),
                )
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            if edge.drag_started_by(egui::PointerButton::Primary) && view.editable {
                timeline.visual_trim = Some(VisualTrim {
                    clip: c.id,
                    project: project.id,
                    revision: project.revision,
                    epoch: view.epoch,
                    duration: c.duration,
                });
                egui::DragAndDrop::clear_payload(ui.ctx());
            }
            if edge.dragged_by(egui::PointerButton::Primary)
                && let Some(p) = edge.interact_pointer_pos()
                && let Some(trim) = &mut timeline.visual_trim
            {
                trim.duration = (((p.x - origin) / pixels).round().max(0.0) as u64)
                    .saturating_sub(c.at)
                    .clamp(
                        1,
                        project.visual_duration_frames().saturating_sub(c.at).max(1),
                    );
            }
            if edge.drag_stopped_by(egui::PointerButton::Primary)
                && let Some(trim) = timeline.visual_trim.take()
                && view.editable
                && trim.clip == c.id
                && trim.project == project.id
                && trim.revision == project.revision
                && trim.epoch == view.epoch
                && trim.duration != c.duration
            {
                actions.push(Action::TrimAudio {
                    clip: c.id,
                    duration: trim.duration,
                });
            }
        }
    }
    if let Some(payload) = egui::DragAndDrop::payload::<DragPayload>(ui.ctx())
        && let Some(p) = ui.input(|i| i.pointer.interact_pos())
        && lane.intersect(viewport).contains(p)
        && ui.ctx().layer_id_at(p) == Some(ui.layer_id())
    {
        let valid = view.editable
            && payload.project == project.id
            && payload.revision == project.revision
            && payload.epoch == view.epoch;
        let offset = match payload.source {
            DragSource::AudioClip { grab_offset, .. }
            | DragSource::VisualClip { grab_offset, .. } => grab_offset,
            _ => 0,
        };
        let raw = (((p.x - origin) / pixels).round().max(0.0) as u64).saturating_sub(offset);
        let at = snap(project, raw, view.playhead, pixels);
        let compatible = if independent {
            matches!(payload.source, DragSource::AudioClip { .. })
                || matches!(payload.source,DragSource::Media(id)if project.media.iter().any(|m|m.id==id&&m.metadata.video.is_none()&&m.metadata.audio.is_some()))
        } else {
            match payload.source {
                DragSource::VisualClip { clip, .. } => {
                    project.clips.iter().any(|c| c.id == clip)
                        || project.visual_clips.iter().any(|c| {
                            c.id == clip
                                && matches!(
                                    c.source,
                                    lumeryn_core::model::VisualSource::Video { .. }
                                )
                        })
                }
                DragSource::Clip(clip) => project.clips.iter().any(|c| c.id == clip),
                _ => false,
            }
        };
        let accepted = valid && compatible;
        if accepted {
            let x = origin + at as f32 * pixels;
            ui.painter().line_segment(
                [pos2(x, lane.top()), pos2(x, lane.bottom())],
                Stroke::new(2.0, design::ACCENT),
            );
        }
        if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
            egui::DragAndDrop::clear_payload(ui.ctx());
            if accepted {
                if independent {
                    actions.push(Action::DropAudio {
                        payload: *payload,
                        at_frame: at,
                    });
                } else {
                    let id = match payload.source {
                        DragSource::VisualClip { clip, .. } | DragSource::Clip(clip) => clip,
                        _ => return,
                    };
                    if let Some(c) = project.visual_clips.iter().find(|c| c.id == id) {
                        actions.push(Action::DropVisual {
                            payload: *payload,
                            track_id: c.track_id,
                            at_frame: at,
                        });
                    } else {
                        actions.push(Action::MovePrimary {
                            clip_id: id,
                            at_frame: at,
                        });
                    }
                }
            } else {
                actions.push(Action::RejectDrop(
                    "Drop a current compatible source on its audio row.",
                ));
            }
        }
    }
}

pub fn show(
    timeline: &mut Timeline,
    ui: &mut egui::Ui,
    project: &Project,
    assets: &Assets,
    view: View,
) -> Vec<Action> {
    let mut actions = Vec::new();
    if !view.editable
        || ui.input(|i| {
            !i.focused
                || i.key_pressed(egui::Key::Escape)
                || i.pointer.button_pressed(egui::PointerButton::Secondary)
        })
    {
        egui::DragAndDrop::clear_payload(ui.ctx());
        timeline.cancel_gesture();
    }
    if timeline.visual_trim.is_some_and(|t| {
        t.project != project.id || t.revision != project.revision || t.epoch != view.epoch
    }) {
        timeline.visual_trim = None;
    }
    ui.horizontal(|ui| {
        ui.label("Layers");
        if ui.button("Fit").clicked() {
            timeline.fit = true;
        }
        ui.add(egui::Slider::new(&mut timeline.zoom, 0.25..=8.0).text("Zoom"));
    });
    let total = project.visual_duration_frames();
    let header = 112.0;
    let width = ui.available_width();
    let pixels = if timeline.fit {
        (width - header - 8.0).max(1.0) / total.max(1) as f32
    } else {
        2.0 * timeline.zoom
    };
    let canvas_width = (header + pixels * (total.max(1) as f32 + 15.0)).max(width);
    let height = (ui.available_height() - 2.0).max(0.0);
    let rows = project.visual_tracks.len() + 3;
    let canvas_height = (36.0 + rows as f32 * 60.0).max(height);
    timeline.visible_clips = 0;
    let scroll=egui::ScrollArea::both().id_salt(("composition-rows",project.id,view.epoch))
        .scroll_source(egui::scroll_area::ScrollSource {drag:egui::scroll_area::DragScroll::Never,..Default::default()})
        .max_height(height).auto_shrink([false,false]).show(ui,|ui| {
        let (canvas,_) = ui.allocate_exact_size(vec2(canvas_width,canvas_height),Sense::hover());
        let viewport=canvas.intersect(ui.clip_rect());
        let origin=canvas.left()+header;
        let ruler=Rect::from_min_max(pos2(origin,canvas.top()),pos2(canvas.right(),canvas.top()+30.0));
        let painter=ui.painter().with_clip_rect(viewport);
        painter.rect_filled(canvas,0.0,design::BACKGROUND);
        let tick=crate::timeline::nice_tick_frames(project.frame_rate,pixels);
        let first=(((viewport.left()-origin)/pixels).max(0.0) as u64).min(total)/tick*tick;
        let last=((((viewport.right()-origin)/pixels).max(0.0).ceil() as u64)+1).min(total.saturating_add(1));
        for frame in (first..last).step_by(tick as usize) {
            let x=origin+frame as f32*pixels;
            let fractions=(tick as f64)<f64::from(project.frame_rate.numerator())/f64::from(project.frame_rate.denominator());
            painter.text(pos2(x,ruler.top()+3.0),Align2::LEFT_TOP,crate::timeline::timecode(frame,project.frame_rate,fractions),FontId::proportional(12.0),design::MUTED);
            painter.line_segment([pos2(x,ruler.bottom()-7.0),pos2(x,ruler.bottom())],Stroke::new(1.0,design::BORDER));
        }
        let response=ui.interact(ruler.intersect(viewport),ui.id().with("visual-ruler"),Sense::click_and_drag());
        if (response.clicked()||response.dragged()) && let Some(p)=response.interact_pointer_pos() {
            actions.push(Action::Seek(((p.x-origin)/pixels).round().max(0.0) as u64));
        }
        // Draw top layer first so native row order matches what covers the preview.
        let tracks:Vec<(Uuid,&str)> = project.visual_tracks.iter().rev().map(|t|(t.id,t.name.as_str()))
            .chain(std::iter::once((project.video_track_id,"Video 1"))).collect();
        for (row,(track_id,name)) in tracks.iter().enumerate() {
            let lane=Rect::from_min_max(pos2(origin,canvas.top()+34.0+row as f32*60.0),pos2(canvas.right(),canvas.top()+90.0+row as f32*60.0));
            painter.rect_filled(lane,3.0,design::PANEL);
            painter.text(pos2(canvas.left()+12.0,lane.center().y),Align2::LEFT_CENTER,*name,FontId::proportional(13.0),design::TEXT);
            let primary=*track_id==project.video_track_id;
            if !primary && let Some(index)=project.visual_tracks.iter().position(|t|t.id==*track_id) {
                let label_rect=Rect::from_min_max(pos2(canvas.left(),lane.top()),pos2(origin-6.0,lane.bottom())).intersect(viewport);
                ui.interact(label_rect,ui.id().with(("visual-track",track_id)),Sense::click()).on_hover_text("Right-click to reorder or remove an empty layer.").context_menu(|ui|{
                    if ui.add_enabled(view.editable&&index+1<project.visual_tracks.len(),egui::Button::new("Move higher")).clicked(){actions.push(Action::ReorderVisualTrack{track_id:*track_id,before:project.visual_tracks.get(index+2).map(|t|t.id)});ui.close();}
                    if ui.add_enabled(view.editable&&index>0,egui::Button::new("Move lower")).clicked(){actions.push(Action::ReorderVisualTrack{track_id:*track_id,before:index.checked_sub(1).and_then(|i|project.visual_tracks.get(i)).map(|t|t.id)});ui.close();}
                    if ui.add_enabled(view.editable&&!project.visual_clips.iter().any(|c|c.track_id==*track_id)&&!project.scene_placements.iter().any(|c|c.track_id==*track_id),egui::Button::new("Remove empty layer")).clicked(){actions.push(Action::DeleteVisualTrack(*track_id));ui.close();}
                });
            }
            let clips:Vec<_> = if primary {project.clips.iter().map(|c|(c.id,c.media_id,c.at_frame,c.duration_frames)).collect()} else {
                project.visual_clips.iter().filter(|c|c.track_id==*track_id).map(|c|(c.id,c.media_id,c.at_frame,c.duration_frames)).collect()
            };
            for (id,media_id,at,duration) in clips {
                let shown_duration=timeline.visual_trim.filter(|t|t.clip==id).map_or(duration,|t|t.duration);
                let block=Rect::from_min_max(pos2(origin+at as f32*pixels,lane.top()+2.0),pos2(origin+(at+shown_duration) as f32*pixels,lane.bottom()-2.0));
                let visible=block.intersect(viewport);
                if visible.width()<=0.0 || visible.height()<=0.0 {continue;}
                timeline.visible_clips+=1;
                let clip_painter=ui.painter().with_clip_rect(visible);
                clip_painter.rect_filled(block,4.0,design::PANEL);
                if let Some(texture)=assets.thumbnail(media_id) {
                    clip_painter.image(texture.id(),block,Rect::from_min_max(pos2(0.0,0.0),pos2(1.0,1.0)),egui::Color32::WHITE);
                }
                clip_painter.rect_filled(Rect::from_min_max(pos2(block.left(),block.bottom()-20.0),block.right_bottom()),0.0,egui::Color32::from_black_alpha(190));
                let label=project.media.iter().find(|m|m.id==media_id).map_or_else(||"Missing media".into(),|m|m.display_name());
                clip_painter.text(pos2(block.left()+5.0,block.bottom()-4.0),Align2::LEFT_BOTTOM,label,FontId::proportional(12.0),design::TEXT);
                clip_painter.rect_stroke(block,4.0,Stroke::new(if view.selected==Some(id){2.0}else{1.0},if view.selected==Some(id){design::ACCENT}else{design::BORDER}),StrokeKind::Inside);
                let trim_rect=Rect::from_min_max(pos2((block.right()-8.0).max(block.left()),block.top()),block.right_bottom()).intersect(viewport);
                let body=Rect::from_min_max(block.left_top(),pos2((block.right()-8.0).max(block.left()),block.bottom())).intersect(viewport);
                let response=ui.interact(body,ui.id().with(("visual-body",id)),Sense::click_and_drag());
                if response.clicked(){actions.push(Action::Select(id));}
                if view.editable && response.drag_started_by(egui::PointerButton::Primary) {
                    let grab=ui.input(|i|i.pointer.press_origin()).map_or(0,|p|((p.x-block.left())/pixels).max(0.0).round() as u64);
                    egui::DragAndDrop::set_payload(ui.ctx(),DragPayload {project:project.id,revision:project.revision,epoch:view.epoch,
                        source:DragSource::VisualClip {clip:id,grab_offset:grab}});
                }
                response.context_menu(|ui| {
                    if primary {
                        if ui.add_enabled(view.editable&&view.playhead>at&&view.playhead<at+duration,egui::Button::new("Split at playhead")).clicked(){actions.push(Action::Split {clip:id,at_frame:view.playhead});ui.close();}
                        if ui.button("Copy clip").clicked(){actions.push(Action::Copy(id));ui.close();}
                    }
                    if ui.add_enabled(view.editable,egui::Button::new("Delete")).clicked(){
                        actions.push(if primary{Action::Delete(id)}else{Action::DeleteVisual(id)});ui.close();
                    }
                });
                {
                    let edge=ui.interact(trim_rect,ui.id().with(("visual-trim",id)),Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                    if edge.drag_started_by(egui::PointerButton::Primary)&&view.editable {
                        timeline.visual_trim=Some(VisualTrim {clip:id,project:project.id,revision:project.revision,epoch:view.epoch,duration});
                        egui::DragAndDrop::clear_payload(ui.ctx());
                    }
                    if edge.dragged_by(egui::PointerButton::Primary)&&view.editable&&let Some(pointer)=edge.interact_pointer_pos() {
                        let max_duration=project.clips.iter().find(|c|c.id==id).and_then(|c|project.media.iter().find(|m|m.id==media_id).and_then(|m|m.metadata.video.as_ref()).map(|v|v.frames.saturating_sub(c.source_in_frame))).or_else(||project.visual_clips.iter().find(|c|c.id==id).and_then(|c| {
                            match c.source {lumeryn_core::model::VisualSource::Image {}=>Some(lumeryn_core::model::MAX_FRAMES.saturating_sub(at)),lumeryn_core::model::VisualSource::Video {source_in_frame}=>project.media.iter().find(|m|m.id==media_id).and_then(|m|m.metadata.video.as_ref()).map(|v|v.frames.saturating_sub(source_in_frame))}
                        })).unwrap_or(duration);
                        if let Some(trim)=&mut timeline.visual_trim {trim.duration=(((pointer.x-origin)/pixels).round().max(0.0) as u64).saturating_sub(at).clamp(1,max_duration.max(1));}
                    }
                    if edge.drag_stopped_by(egui::PointerButton::Primary)&&let Some(trim)=timeline.visual_trim.take()
                        &&view.editable&&trim.clip==id&&trim.project==project.id&&trim.revision==project.revision&&trim.epoch==view.epoch&&trim.duration!=duration {
                        actions.push(if primary{Action::Trim {clip:id,duration:trim.duration}}else{Action::TrimVisual {clip_id:id,duration:trim.duration}});
                    }
                }
            }
            for placement in project.scene_placements.iter().filter(|p| p.track_id == *track_id) {
                let block = Rect::from_min_max(pos2(origin + placement.at_frame as f32 * pixels, lane.top() + 2.0),
                    pos2(origin + (placement.at_frame + placement.duration_frames) as f32 * pixels, lane.bottom() - 2.0));
                let visible = block.intersect(viewport);
                if visible.width() <= 0.0 || visible.height() <= 0.0 { continue; }
                timeline.visible_clips += 1;
                let p = ui.painter().with_clip_rect(visible);
                p.rect_filled(block, 4.0, design::PANEL);
                p.rect_stroke(block, 4.0, Stroke::new(if view.selected == Some(placement.id) { 2.5 } else { 1.0 },
                    if view.selected == Some(placement.id) { design::ACCENT } else { design::BORDER }), StrokeKind::Inside);
                let name = project.scenes.iter().find(|s| s.id == placement.scene_id).map_or("Scene", |s| s.name.as_str());
                p.text(block.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, name, FontId::proportional(13.0), design::TEXT);
                let body = ui.interact(visible, ui.id().with(("scene-placement", placement.id)), Sense::click_and_drag());
                if body.clicked() { actions.push(Action::Select(placement.id)); }
                if view.editable && body.drag_started_by(egui::PointerButton::Primary) {
                    let grabbed = body.interact_pointer_pos().map_or(0, |point| (((point.x - block.left()) / pixels).max(0.0).round() as u64).min(placement.duration_frames.saturating_sub(1)));
                    body.dnd_set_drag_payload(DragPayload { project: project.id, revision: project.revision, epoch: view.epoch,
                        source: DragSource::VisualClip { clip: placement.id, grab_offset: grabbed } });
                }
            }
            if let Some(payload)=egui::DragAndDrop::payload::<DragPayload>(ui.ctx())
                && let Some(pointer)=ui.input(|i|i.pointer.interact_pos())
                && lane.intersect(viewport).contains(pointer)
                && ui.ctx().layer_id_at(pointer)==Some(ui.layer_id()) {
                let valid=view.editable&&payload.project==project.id&&payload.revision==project.revision&&payload.epoch==view.epoch;
                let offset=match payload.source {DragSource::VisualClip {grab_offset,..}=>grab_offset,_=>0};
                let raw=(((pointer.x-origin)/pixels).max(0.0).round() as u64).saturating_sub(offset);
                let at=if ui.input(|i|i.modifiers.alt){raw}else{snap(project,raw,view.playhead,pixels)};
                let accepted=valid&&match payload.source {DragSource::Media(id)=>project.media.iter().any(|m|m.id==id&&m.metadata.video.is_some()&&(!primary||m.metadata.video.as_ref().is_some_and(|v|v.codec!="png"))),DragSource::VisualClip {clip,..}=>if primary{project.clips.iter().any(|c|c.id==clip)}else{project.visual_clips.iter().any(|c|c.id==clip)||project.scene_placements.iter().any(|c|c.id==clip)},DragSource::Clip(_)=>primary,DragSource::AudioClip {..}=>false};
                ui.ctx().set_cursor_icon(if accepted{egui::CursorIcon::Grabbing}else{egui::CursorIcon::NotAllowed});
                if accepted {let x=origin+at as f32*pixels;painter.line_segment([pos2(x,lane.top()),pos2(x,lane.bottom())],Stroke::new(2.0,design::ACCENT));}
                if ui.input(|i|i.pointer.button_released(egui::PointerButton::Primary)){
                    egui::DragAndDrop::clear_payload(ui.ctx());
                    if accepted {match payload.source {DragSource::VisualClip {clip,..} if primary=>actions.push(Action::MovePrimary {clip_id:clip,at_frame:at}),DragSource::Clip(id)=>actions.push(Action::MovePrimary {clip_id:id,at_frame:at}),_=>actions.push(Action::DropVisual {payload:*payload,track_id:*track_id,at_frame:at})}}
                    else {actions.push(Action::RejectDrop("The layer or project changed. Select a current compatible layer and try again."));}
                }
            }
        }
        for (row,independent) in [false,true].into_iter().enumerate(){
            let top=canvas.top()+34.0+(tracks.len()+row) as f32*60.0;
            let lane=Rect::from_min_max(pos2(origin,top),pos2(canvas.right(),top+56.0));
            painter.text(pos2(canvas.left()+12.0,lane.center().y),Align2::LEFT_CENTER,if independent{"Audio 1"}else{"Source audio"},FontId::proportional(13.0),design::TEXT);
            sound_lane(timeline,ui,project,assets,view,lane,viewport,origin,pixels,independent,&mut actions);
        }
        let x=origin+view.playhead as f32*pixels;
        painter.line_segment([pos2(x,canvas.top()),pos2(x,canvas.bottom())],Stroke::new(1.5,design::ACCENT));
    });
    // Only accepted current visual/audio targets may scroll a held drag.
    if let Some(payload) = egui::DragAndDrop::payload::<DragPayload>(ui.ctx())
        && view.editable
        && payload.project == project.id
        && payload.revision == project.revision
        && payload.epoch == view.epoch
        && let Some(pointer) = ui.input(|i| i.pointer.interact_pos())
        && scroll.inner_rect.contains(pointer)
        && ui.ctx().layer_id_at(pointer) == Some(ui.layer_id())
    {
        let row = (((pointer.y - scroll.inner_rect.top() + scroll.state.offset.y - 34.0) / 60.0)
            .floor() as i32)
            .max(0) as usize;
        let visual_rows = project.visual_tracks.len() + 1;
        let compatible = match payload.source {
            DragSource::AudioClip { .. } => row == visual_rows + 1,
            DragSource::Media(id) => project.media.iter().find(|m| m.id == id).is_some_and(|m| {
                if m.metadata.video.is_some() {
                    row < visual_rows
                        && (row + 1 < visual_rows
                            || m.metadata.video.as_ref().is_some_and(|v| v.codec != "png"))
                } else {
                    m.metadata.audio.is_some() && row == visual_rows + 1
                }
            }),
            DragSource::VisualClip { clip, .. } => {
                if row == visual_rows {
                    project.clips.iter().any(|c| c.id == clip)
                        || project.visual_clips.iter().any(|c| {
                            c.id == clip
                                && matches!(
                                    c.source,
                                    lumeryn_core::model::VisualSource::Video { .. }
                                )
                        })
                } else if row + 1 == visual_rows {
                    project.clips.iter().any(|c| c.id == clip)
                } else {
                    row < visual_rows && project.visual_clips.iter().any(|c| c.id == clip)
                }
            }
            DragSource::Clip(clip) => {
                (row + 1 == visual_rows || row == visual_rows)
                    && project.clips.iter().any(|c| c.id == clip)
            }
        };
        if compatible {
            let mut state = scroll.state;
            let dt = ui.input(|i| i.stable_dt).clamp(0.0, 0.05);
            let edge = 28.0;
            let horizontal = if pointer.x > scroll.inner_rect.right() - edge {
                1.0
            } else if pointer.x < scroll.inner_rect.left() + 112.0 + edge {
                -1.0
            } else {
                0.0
            };
            let vertical = if pointer.y > scroll.inner_rect.bottom() - edge {
                1.0
            } else if pointer.y < scroll.inner_rect.top() + edge {
                -1.0
            } else {
                0.0
            };
            if horizontal != 0.0 || vertical != 0.0 {
                state.offset.x = (state.offset.x + horizontal * 300.0 * dt).clamp(
                    0.0,
                    (scroll.content_size.x - scroll.inner_rect.width()).max(0.0),
                );
                state.offset.y = (state.offset.y + vertical * 240.0 * dt).clamp(
                    0.0,
                    (scroll.content_size.y - scroll.inner_rect.height()).max(0.0),
                );
                state.store(ui.ctx(), scroll.id);
                ui.ctx().request_repaint();
            }
        }
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumeryn_core::{
        Result,
        media::{Audio, Metadata, Video},
        model::{AudioClip, Clip, Media, VisualClip, VisualSource, VisualTrack},
    };

    fn fixture(overlays: bool) -> Result<Project> {
        let mut project = Project::new(
            "Native composition gestures".into(),
            320,
            180,
            "30".parse()?,
        )?;
        for (path, image) in [("video.mkv", false), ("still.png", true)] {
            project.media.push(Media {
                id: Uuid::new_v4(),
                path: path.into(),
                alias: None,
                metadata: Metadata {
                    size_bytes: 1,
                    blake3: Some("a".repeat(64)),
                    video: Some(Video {
                        codec: if image { "png" } else { "ffv1" }.into(),
                        width: 320,
                        height: 180,
                        frame_rate: project.frame_rate,
                        frames: if image { 1 } else { 180 },
                        start_time_micros: 0,
                        sample_aspect_ratio: "1:1".into(),
                        color_transfer: None,
                        constant_rate: true,
                    }),
                    audio: if image {
                        None
                    } else {
                        Some(Audio {
                            codec: "pcm_s16le".into(),
                            sample_rate: 48_000,
                            channels: 2,
                            start_time_micros: 0,
                            canonical_samples_48k: Some(288_000),
                        })
                    },
                },
            });
        }
        project.clips.push(Clip {
            id: Uuid::new_v4(),
            media_id: project.media[0].id,
            track_id: project.video_track_id,
            at_frame: 0,
            source_in_frame: 0,
            duration_frames: 90,
        });
        project.visual_tracks = ["Overlay", "Still"]
            .into_iter()
            .map(|name| VisualTrack {
                id: Uuid::new_v4(),
                name: name.into(),
            })
            .collect();
        if overlays {
            project.visual_clips = [false, true]
                .into_iter()
                .enumerate()
                .map(|(index, image)| VisualClip {
                    id: Uuid::new_v4(),
                    media_id: project.media[usize::from(image)].id,
                    track_id: project.visual_tracks[index].id,
                    at_frame: 10,
                    duration_frames: 30,
                    source: if image {
                        VisualSource::Image {}
                    } else {
                        VisualSource::Video {
                            source_in_frame: 20,
                        }
                    },
                    placement: None,
                })
                .collect();
        }
        project.audio_clips.push(AudioClip {
            id: Uuid::new_v4(),
            media_id: project.media[0].id,
            track_id: project
                .audio_track_id
                .ok_or_else(|| lumeryn_core::Error::Invalid("fixture audio track".into()))?,
            at_frame: 20,
            source_in_sample: 0,
            duration_frames: 30,
            gain_millidecibels: 0,
            muted: false,
        });
        project.validate()?;
        Ok(project)
    }

    struct Gesture {
        ctx: egui::Context,
        timeline: Timeline,
        content_id: Option<egui::Id>,
        media_rect: Rect,
        time: f64,
        epoch: u64,
        editable: bool,
        focused: bool,
        dragged_media: Uuid,
    }

    impl Gesture {
        fn new(project: &Project) -> Self {
            let ctx = egui::Context::default();
            design::configure(&ctx);
            let mut gesture = Self {
                ctx,
                timeline: Timeline::default(),
                content_id: None,
                media_rect: Rect::NOTHING,
                time: 0.0,
                epoch: 1,
                editable: true,
                focused: true,
                dragged_media: project.media[0].id,
            };
            gesture.timeline.fit = false;
            gesture
        }

        fn pass(&mut self, project: &Project, events: Vec<egui::Event>) -> Vec<Action> {
            self.time += 1.0 / 60.0;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 650.0))),
                time: Some(self.time),
                events,
                focused: self.focused,
                ..Default::default()
            };
            let mut actions = Vec::new();
            let mut output = self.ctx.run_ui(input, |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(110.0, 40.0), Sense::hover());
                self.media_rect = rect;
                let response =
                    ui.interact(rect, ui.id().with("gesture-media"), Sense::click_and_drag());
                crate::app::media_card_drag(
                    &response,
                    self.editable,
                    DragPayload {
                        project: project.id,
                        revision: project.revision,
                        epoch: self.epoch,
                        source: DragSource::Media(self.dragged_media),
                    },
                );
                ui.add_space(10.0);
                // egui 0.36 ScrollArea and its Frame each create one default child.
                // Read the actual production response rectangles below; no duplicated lane geometry.
                self.content_id = Some(
                    ui.id()
                        .with(egui::IdSalt::new("child"))
                        .with(egui::IdSalt::new("child")),
                );
                actions = self.timeline.show(
                    ui,
                    project,
                    &Assets::empty(),
                    View {
                        selected: None,
                        playhead: 0,
                        epoch: self.epoch,
                        editable: self.editable,
                    },
                );
            });
            output.textures_delta.clear();
            actions
        }

        fn rect(&self, kind: &str, clip: Uuid) -> Rect {
            let id = self
                .content_id
                .expect("rendered test content")
                .with((kind, clip));
            self.ctx
                .read_response(id)
                .expect("actual production response ID exists")
                .rect
        }

        fn ruler(&self) -> Rect {
            let id = self
                .content_id
                .expect("rendered test content")
                .with("visual-ruler");
            self.ctx
                .read_response(id)
                .expect("actual production ruler response")
                .rect
        }

        fn button(position: egui::Pos2, pressed: bool) -> egui::Event {
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }
        }

        fn start(&mut self, project: &Project, position: egui::Pos2) {
            self.pass(project, vec![egui::Event::PointerMoved(position)]);
            self.pass(project, vec![Self::button(position, true)]);
            self.pass(
                project,
                vec![egui::Event::PointerMoved(position + vec2(12.0, 0.0))],
            );
        }

        fn release(&mut self, project: &Project, position: egui::Pos2) -> Vec<Action> {
            self.pass(project, vec![egui::Event::PointerMoved(position)]);
            self.pass(project, vec![Self::button(position, false)])
        }
    }

    #[test]
    fn actual_primary_and_overlay_edges_trim_once_without_body_drag() -> Result<()> {
        let project = fixture(true)?;
        for (id, primary, original_duration) in [
            (project.clips[0].id, true, 90),
            (project.visual_clips[0].id, false, 30),
        ] {
            let mut gesture = Gesture::new(&project);
            gesture.pass(&project, vec![]);
            let edge = gesture.rect("visual-trim", id);
            let body = gesture.rect("visual-body", id);
            let pixels = (body.width() + edge.width()) / original_duration as f32;
            gesture.start(&project, edge.center());
            assert!(gesture.timeline.visual_trim.is_some());
            assert!(!egui::DragAndDrop::has_any_payload(&gesture.ctx));
            let destination = pos2(
                body.left() + (original_duration - 10) as f32 * pixels,
                edge.center().y,
            );
            let actions = gesture.release(&project, destination);
            assert!(if primary {
                matches!(actions.as_slice(), [Action::Trim { clip, duration }] if *clip == id && *duration == 80)
            } else {
                matches!(actions.as_slice(), [Action::TrimVisual { clip_id, duration }] if *clip_id == id && *duration == 20)
            });
            assert!(gesture.timeline.visual_trim.is_none());
            assert!(gesture.pass(&project, vec![]).is_empty());
        }
        Ok(())
    }

    #[test]
    fn actual_media_drag_drops_on_overlay_and_stale_revision_is_rejected() -> Result<()> {
        let project = fixture(true)?;
        for stale in [false, true] {
            let mut gesture = Gesture::new(&project);
            gesture.pass(&project, vec![]);
            let target = gesture.rect("visual-body", project.visual_clips[0].id);
            let destination = pos2(gesture.ruler().left() + 120.0, target.center().y);
            gesture.start(&project, gesture.media_rect.center());
            assert!(egui::DragAndDrop::has_payload_of_type::<DragPayload>(
                &gesture.ctx
            ));
            let mut latest = project.clone();
            if stale {
                latest.revision += 1;
            }
            let actions = gesture.release(&latest, destination);
            assert!(if stale {
                matches!(actions.as_slice(), [Action::RejectDrop(_)])
            } else {
                matches!(actions.as_slice(), [Action::DropVisual { payload, track_id, at_frame: 60 }] if payload.source == DragSource::Media(project.media[0].id) && *track_id == project.visual_tracks[0].id)
            });
            assert!(!egui::DragAndDrop::has_any_payload(&gesture.ctx));
        }
        Ok(())
    }

    #[test]
    fn actual_visual_body_movement_retains_grabbed_frame_offset() -> Result<()> {
        let project = fixture(true)?;
        let id = project.visual_clips[0].id;
        let mut gesture = Gesture::new(&project);
        gesture.pass(&project, vec![]);
        let body = gesture.rect("visual-body", id);
        let grabbed = pos2(body.left() + 20.0, body.center().y);
        gesture.start(&project, grabbed);
        let payload = egui::DragAndDrop::payload::<DragPayload>(&gesture.ctx)
            .expect("real body drag payload");
        assert!(
            matches!(payload.source, DragSource::VisualClip { clip, grab_offset: 10 } if clip == id)
        );
        let destination = pos2(gesture.ruler().left() + 120.0, body.center().y);
        assert!(matches!(gesture.release(&project, destination).as_slice(),
            [Action::DropVisual { at_frame: 50, track_id, .. }] if *track_id == project.visual_tracks[0].id));
        Ok(())
    }

    #[test]
    fn actual_cancel_and_stale_epoch_discard_trim_and_media_payloads() -> Result<()> {
        let project = fixture(true)?;
        for cancel in [0, 1, 2, 3, 4] {
            let mut gesture = Gesture::new(&project);
            let mut current = project.clone();
            gesture.pass(&project, vec![]);
            let id = project.visual_clips[0].id;
            let edge = gesture.rect("visual-trim", id).center();
            gesture.start(&project, edge);
            assert!(gesture.timeline.visual_trim.is_some());
            let events = match cancel {
                0 => vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                1 => {
                    gesture.focused = false;
                    vec![]
                }
                2 => {
                    gesture.epoch += 1;
                    vec![]
                }
                3 => {
                    gesture.editable = false;
                    vec![]
                }
                _ => {
                    current.id = Uuid::new_v4();
                    vec![]
                }
            };
            assert!(gesture.pass(&current, events).is_empty());
            assert!(gesture.timeline.visual_trim.is_none());
            assert!(gesture.release(&current, edge - vec2(20.0, 0.0)).is_empty());
        }
        Ok(())
    }

    #[test]
    fn actual_empty_overlay_tracks_preserve_audio_move_and_right_trim() -> Result<()> {
        let project = fixture(false)?;
        let id = project.audio_clips[0].id;
        let mut gesture = Gesture::new(&project);
        gesture.pass(&project, vec![]);
        let body = gesture.rect("composition-sound", id);
        gesture.start(&project, pos2(body.left() + 20.0, body.center().y));
        assert!(
            matches!(egui::DragAndDrop::payload::<DragPayload>(&gesture.ctx).map(|payload| payload.source), Some(DragSource::AudioClip { clip, grab_offset: 10 }) if clip == id)
        );
        let destination = pos2(gesture.ruler().left() + 120.0, body.center().y);
        assert!(matches!(
            gesture.release(&project, destination).as_slice(),
            [Action::DropAudio { at_frame: 50, .. }]
        ));
        let edge = gesture.rect("composition-audio-trim", id).center();
        gesture.start(&project, edge);
        assert!(!egui::DragAndDrop::has_any_payload(&gesture.ctx));
        let target = pos2(body.left() + 40.0, edge.y);
        assert!(
            matches!(gesture.release(&project, target).as_slice(), [Action::TrimAudio { clip, duration: 20 }] if *clip == id)
        );
        Ok(())
    }

    #[test]
    fn actual_png_primary_drop_and_cancelled_media_drag_never_commit() -> Result<()> {
        let project = fixture(true)?;
        let mut png = Gesture::new(&project);
        png.dragged_media = project.media[1].id;
        png.pass(&project, vec![]);
        let target = png.rect("visual-body", project.clips[0].id);
        let destination = pos2(png.ruler().left() + 120.0, target.center().y);
        png.start(&project, png.media_rect.center());
        assert!(matches!(
            png.release(&project, destination).as_slice(),
            [Action::RejectDrop(_)]
        ));
        for focus_loss in [false, true] {
            let mut gesture = Gesture::new(&project);
            gesture.pass(&project, vec![]);
            gesture.start(&project, gesture.media_rect.center());
            assert!(egui::DragAndDrop::has_any_payload(&gesture.ctx));
            let events = if focus_loss {
                gesture.focused = false;
                vec![]
            } else {
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]
            };
            gesture.pass(&project, events);
            assert!(!egui::DragAndDrop::has_any_payload(&gesture.ctx));
            assert!(gesture.release(&project, destination).is_empty());
        }
        Ok(())
    }

    #[test]
    fn actual_still_drop_on_linked_audio_is_rejected() -> Result<()> {
        let project = fixture(true)?;
        let image = project
            .visual_clips
            .iter()
            .find(|c| matches!(c.source, VisualSource::Image {}))
            .ok_or_else(|| lumeryn_core::Error::Invalid("missing still fixture".into()))?;
        let mut gesture = Gesture::new(&project);
        gesture.pass(&project, vec![]);
        let body = gesture.rect("visual-body", image.id);
        let target = gesture.rect("composition-sound", project.clips[0].id);
        gesture.start(&project, body.center());
        assert!(matches!(
            gesture.release(&project, target.center()).as_slice(),
            [Action::RejectDrop(_)]
        ));
        Ok(())
    }

    #[test]
    fn actual_secondary_release_cannot_commit_primary_trim() -> Result<()> {
        let project = fixture(true)?;
        let id = project.visual_clips[0].id;
        let mut gesture = Gesture::new(&project);
        gesture.pass(&project, vec![]);
        let body = gesture.rect("visual-body", id);
        let edge = gesture.rect("visual-trim", id).center();
        gesture.start(&project, edge);
        let destination = pos2(body.left() + 40.0, edge.y);
        gesture.pass(&project, vec![egui::Event::PointerMoved(destination)]);
        for pressed in [true, false] {
            let actions = gesture.pass(
                &project,
                vec![egui::Event::PointerButton {
                    pos: destination,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            assert!(actions.is_empty());
        }
        let actions = gesture.release(&project, destination);
        assert!(
            actions.is_empty()
                || matches!(actions.as_slice(),
            [Action::TrimVisual { clip_id, duration: 20 }] if *clip_id == id)
        );
        // Mixed-button input may explicitly cancel; an abandoned draft must never linger.
        assert!(gesture.timeline.visual_trim.is_none());
        Ok(())
    }
}
