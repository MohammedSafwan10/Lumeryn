use crate::{app::Session, backend::Command, timeline};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Debug)]
enum Stage {
    Load,
    Play {
        frames: u64,
        started: Instant,
    },
    Seek {
        target: u64,
    },
    Step {
        target: u64,
    },
    Trim {
        clip: Uuid,
        source: u64,
        original: u64,
        revision: u64,
    },
    Live {
        revision: u64,
        original: u64,
    },
    StartHistory,
    History {
        revision: u64,
        frames: u64,
        clips: usize,
        expected_alias: Option<(Uuid, Option<&'static str>)>,
        label: &'static str,
    },
    Export {
        ui_frames: u64,
    },
    Screenshot,
    Saving,
    Finishing,
    Closed,
}

pub struct Smoke {
    directory: PathBuf,
    started: Instant,
    stage: Stage,
    checks: Vec<Value>,
    screenshot_requested: bool,
    screenshot_saved: bool,
    asset_wait_started: Option<Instant>,
    asset_wait_expired: bool,
    screenshot_assets: Option<Value>,
    history_enabled: bool,
    history_steps: VecDeque<HistoryStep>,
}

struct HistoryStep {
    edit: lumeryn_core::Edit,
    label: &'static str,
    frames: u64,
    clips: usize,
    expected_alias: Option<(Uuid, Option<&'static str>)>,
}

impl Smoke {
    pub fn new(directory: PathBuf, history_enabled: bool) -> Self {
        Self {
            directory,
            started: Instant::now(),
            stage: Stage::Load,
            checks: Vec::new(),
            screenshot_requested: false,
            screenshot_saved: false,
            asset_wait_started: None,
            asset_wait_expired: false,
            screenshot_assets: None,
            history_enabled,
            history_steps: VecDeque::new(),
        }
    }

    fn check(&mut self, name: &str, detail: Value) {
        self.checks
            .push(json!({"check":name,"passed":true,"evidence":detail}));
    }

    fn finish(&mut self, session: &mut Session, passed: bool, error: Option<String>) {
        let mut ui_samples = session.ui_samples_ms.iter().copied().collect::<Vec<_>>();
        ui_samples.sort_by(f64::total_cmp);
        let percentile = |percent: usize| {
            ui_samples
                .get(ui_samples.len().saturating_sub(1) * percent / 100)
                .copied()
        };
        let preview = session.preview.as_ref().map(|preview| {
            let status = preview.status();
            json!({"decoded_frames":status.decoded_frames,"dropped_frames":status.dropped_frames,"decode_ms":status.decode_time_ms,"seek_ms":status.seek_latency_ms,"audio_status":format!("{:?}",status.audio)})
        });
        let report = json!({
            "schema_version":1,"passed":passed,"error":error,"checks":self.checks,
            "viewport":{"logical_width":session.viewport_size.x,"logical_height":session.viewport_size.y,"pixels_per_point":session.pixels_per_point},
            "elapsed_ms":self.started.elapsed().as_secs_f64()*1000.0,"first_frame_ms":session.first_frame_ms,
            "ui_updates":session.ui_frames,"max_ui_update_ms":session.max_ui_ms,"presented_frames":session.presented_frames,
            "ui_update_p50_ms":percentile(50),"ui_update_p95_ms":percentile(95),"ui_update_samples":ui_samples.len(),
            "adapter":session.texture.as_ref().map(|texture|texture.adapter.clone()),
            "frame_uploads":session.texture.as_ref().map(|texture|texture.uploads),
            "last_cpu_upload_submission_ms":session.texture.as_ref().map(|texture|texture.upload_cpu_ms),
            "project_revision":session.project.as_ref().map(|project|project.revision),"preview":preview,
            "native_screenshot":self.directory.join("studio-native.png"),"screenshot_saved":self.screenshot_saved,
            "screenshot_assets":self.screenshot_assets,"asset_capture_wait_expired":self.asset_wait_expired,
            "asset_evidence":session.asset_evidence(),
            "render_result":session.last_render,
            "history_clipboard_acceptance":self.history_enabled,
            "limitations":["Scripted actions call the same UI command paths; pointer usability and native file-dialog cancellation are separate manual acceptance.","CPU upload timing does not measure GPU completion or photon latency.","VRAM consumption is unknown without telemetry.","Audio device status alone does not prove end-to-end A/V tolerance.","Effects, multi-track composition and proxies remain staged; physical clipboard and keyboard checks remain pending. Thumbnail/waveform caches are bounded; readiness/counts are reported independently of workflow success."]
        });
        if session.send(Command::Report {
            path: self.directory.join("studio-smoke.json"),
            value: report,
        }) {
            self.stage = Stage::Finishing;
        } else {
            self.stage = Stage::Closed;
        }
    }

    pub fn tick(&mut self, session: &mut Session, ctx: &egui::Context) {
        if matches!(self.stage, Stage::Closed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if matches!(self.stage, Stage::Finishing) {
            if self.started.elapsed() > Duration::from_secs(160)
                || session
                    .error
                    .as_ref()
                    .is_some_and(|error| error.starts_with("Report:"))
            {
                self.stage = Stage::Closed;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            if session.artifacts.iter().any(|path| {
                path.file_name()
                    .is_some_and(|name| name == "studio-smoke.json")
            }) {
                self.stage = Stage::Closed;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }
        if self.started.elapsed() > Duration::from_secs(150) {
            self.finish(
                session,
                false,
                Some(format!("Timed out in {:?}", self.stage)),
            );
            return;
        }
        if let Some(error) = &session.error {
            self.finish(session, false, Some(error.clone()));
            return;
        }
        if let Some(preview) = &session.preview
            && let Some(error) = preview.status().error
        {
            self.finish(session, false, Some(error));
            return;
        }
        match self.stage {
            Stage::Load => {
                let Some(project) = &session.project else {
                    return;
                };
                if session
                    .last_frame
                    .is_none_or(|(revision, _)| revision != project.revision)
                {
                    return;
                }
                if project.clips.len() != 1
                    || project
                        .clips
                        .first()
                        .is_none_or(|clip| clip.duration_frames < 8)
                {
                    self.finish(session, false, Some("Smoke requires a disposable single-clip project with at least 8 frames".into()));
                    return;
                }
                self.check("existing_project_loaded", json!({"id":project.id,"revision":project.revision,"clips":project.clips.len()}));
                self.check(
                    "native_gpu_frame",
                    json!({"presented":session.presented_frames,"frame":session.last_frame}),
                );
                if let Some(preview) = &session.preview
                    && let Err(error) = preview.play()
                {
                    self.finish(session, false, Some(error.to_string()));
                    return;
                }
                self.stage = Stage::Play {
                    frames: session.presented_frames,
                    started: Instant::now(),
                };
            }
            Stage::Play { frames, started } => {
                if started.elapsed() < Duration::from_millis(250)
                    || session.presented_frames < frames + 3
                {
                    return;
                }
                self.check("playback_advanced", json!({"new_frames":session.presented_frames-frames,"frame":session.last_frame}));
                if let Some(preview) = &session.preview
                    && let Err(error) = preview.pause()
                {
                    self.finish(session, false, Some(error.to_string()));
                    return;
                }
                let target = session.project.as_ref().map_or(0, timeline::total_frames) / 2;
                session.seek(target);
                self.stage = Stage::Seek { target };
            }
            Stage::Seek { target } => {
                if session.last_frame.is_none_or(|(_, frame)| frame != target)
                    || session.preview.as_ref().is_some_and(|p| p.status().playing)
                {
                    return;
                }
                self.check("pause_and_exact_seek", json!({"frame":target}));
                if let Some(preview) = &session.preview
                    && let Err(error) = preview.step(1)
                {
                    self.finish(session, false, Some(error.to_string()));
                    return;
                }
                self.stage = Stage::Step { target: target + 1 };
            }
            Stage::Step { target } => {
                if session.last_frame.is_none_or(|(_, frame)| frame != target) {
                    return;
                }
                self.check("frame_step", json!({"frame":target}));
                let Some(project) = &session.project else {
                    return;
                };
                let Some(clip) = project.clips.first() else {
                    return;
                };
                let (id, source, original, revision) = (
                    clip.id,
                    clip.source_in_frame,
                    clip.duration_frames,
                    project.revision,
                );
                if session.trim(id, source, original - 1) {
                    self.stage = Stage::Trim {
                        clip: id,
                        source,
                        original,
                        revision,
                    };
                }
            }
            Stage::Trim {
                clip,
                source,
                original,
                revision,
            } => {
                let Some(project) = &session.project else {
                    return;
                };
                if project.revision <= revision
                    || project
                        .clips
                        .first()
                        .is_none_or(|c| c.duration_frames != original - 1)
                {
                    return;
                }
                let revision = project.revision;
                self.check(
                    "studio_trim_committed",
                    json!({"revision":revision,"duration":original-1}),
                );
                if session.send(Command::CliTrim {
                    clip,
                    source_in: source,
                    duration: original,
                    revision,
                }) {
                    self.stage = Stage::Live { revision, original };
                }
            }
            Stage::Live { revision, original } => {
                let Some(project) = &session.project else {
                    return;
                };
                if project.revision <= revision
                    || project
                        .clips
                        .first()
                        .is_none_or(|c| c.duration_frames != original)
                    || session.last_complete.as_deref() != Some("CLI trim")
                {
                    return;
                }
                if !session.events.iter().any(|event| {
                    event.revision.is_some_and(|r| r > revision)
                        && event.kind == "project_committed"
                }) {
                    return;
                }
                self.check(
                    "cli_edit_live_event",
                    json!({"revision":project.revision,"restored_duration":original}),
                );
                if self.history_enabled {
                    let Some(clip) = project.clips.first() else {
                        self.finish(
                            session,
                            false,
                            Some("History fixture clip is missing".into()),
                        );
                        return;
                    };
                    let clipboard = match lumeryn_core::clipboard::Clipboard::copy(project, clip.id)
                    {
                        Ok(clipboard) => clipboard,
                        Err(error) => {
                            self.finish(session, false, Some(error.to_string()));
                            return;
                        }
                    };
                    let pasted = Uuid::new_v4();
                    use lumeryn_core::Edit;
                    self.history_steps = [
                        HistoryStep {
                            edit: Edit::Undo {},
                            label: "Undo",
                            frames: original - 1,
                            clips: 1,
                            expected_alias: None,
                        },
                        HistoryStep {
                            edit: Edit::Redo {},
                            label: "Redo",
                            frames: original,
                            clips: 1,
                            expected_alias: None,
                        },
                        HistoryStep {
                            edit: Edit::PasteClip {
                                id: pasted,
                                at_frame: original,
                                clipboard,
                            },
                            label: "Paste clip",
                            frames: original * 2,
                            clips: 2,
                            expected_alias: None,
                        },
                        HistoryStep {
                            edit: Edit::Undo {},
                            label: "Undo",
                            frames: original,
                            clips: 1,
                            expected_alias: None,
                        },
                        HistoryStep {
                            edit: Edit::Redo {},
                            label: "Redo",
                            frames: original * 2,
                            clips: 2,
                            expected_alias: None,
                        },
                        HistoryStep {
                            edit: Edit::DeleteClip {
                                clip_id: pasted,
                                ripple: true,
                            },
                            label: "Delete clip",
                            frames: original,
                            clips: 1,
                            expected_alias: None,
                        },
                        HistoryStep {
                            edit: Edit::Undo {},
                            label: "Undo",
                            frames: original * 2,
                            clips: 2,
                            expected_alias: None,
                        },
                        HistoryStep {
                            edit: Edit::Redo {},
                            label: "Redo",
                            frames: original,
                            clips: 1,
                            expected_alias: None,
                        },
                    ]
                    .into();
                    self.history_steps.extend([
                        HistoryStep {
                            edit: Edit::RenameMedia {
                                media_id: clip.media_id,
                                alias: Some("Opening shot".into()),
                            },
                            label: "Rename media",
                            frames: original,
                            clips: 1,
                            expected_alias: Some((clip.media_id, Some("Opening shot"))),
                        },
                        HistoryStep {
                            edit: Edit::Undo {},
                            label: "Undo",
                            frames: original,
                            clips: 1,
                            expected_alias: Some((clip.media_id, None)),
                        },
                        HistoryStep {
                            edit: Edit::Redo {},
                            label: "Redo",
                            frames: original,
                            clips: 1,
                            expected_alias: Some((clip.media_id, Some("Opening shot"))),
                        },
                        HistoryStep {
                            edit: Edit::RenameMedia {
                                media_id: clip.media_id,
                                alias: None,
                            },
                            label: "Rename media",
                            frames: original,
                            clips: 1,
                            expected_alias: Some((clip.media_id, None)),
                        },
                    ]);
                    self.stage = Stage::StartHistory;
                    return;
                }
                if session.export(self.directory.join("studio-smoke-export.mp4")) {
                    self.stage = Stage::Export {
                        ui_frames: session.ui_frames,
                    };
                }
            }
            Stage::StartHistory => {
                let Some(step) = self.history_steps.front() else {
                    if session.export(self.directory.join("studio-smoke-export.mp4")) {
                        self.stage = Stage::Export {
                            ui_frames: session.ui_frames,
                        };
                    }
                    return;
                };
                let revision = session
                    .project
                    .as_ref()
                    .map_or(0, |project| project.revision);
                let submitted = match &step.edit {
                    lumeryn_core::Edit::Undo {} => session.undo(),
                    lumeryn_core::Edit::Redo {} => session.redo(),
                    lumeryn_core::Edit::PasteClip { id, clipboard, .. } => {
                        match serde_json::to_vec(clipboard) {
                            Ok(bytes) => session.paste_clip_at_end(&bytes, *id),
                            Err(error) => {
                                self.finish(session, false, Some(error.to_string()));
                                return;
                            }
                        }
                    }
                    lumeryn_core::Edit::RenameMedia { media_id, alias } => {
                        session.rename_media(*media_id, alias.clone())
                    }
                    edit => session.edit(edit.clone(), step.label),
                };
                if submitted && let Some(step) = self.history_steps.pop_front() {
                    self.stage = Stage::History {
                        revision: revision + 1,
                        frames: step.frames,
                        clips: step.clips,
                        expected_alias: step.expected_alias,
                        label: step.label,
                    };
                }
            }
            Stage::History {
                revision,
                frames,
                clips,
                expected_alias,
                label,
            } => {
                let Some(project) = &session.project else {
                    return;
                };
                if project.revision < revision || session.last_complete.as_deref() != Some(label) {
                    return;
                }
                if project.revision != revision
                    || timeline::total_frames(project) != frames
                    || project.clips.len() != clips
                    || expected_alias.is_some_and(|(id, alias)| {
                        project
                            .media
                            .iter()
                            .find(|media| media.id == id)
                            .is_none_or(|media| media.alias.as_deref() != alias)
                    })
                {
                    self.finish(
                        session,
                        false,
                        Some(format!("History state mismatch after {label}")),
                    );
                    return;
                }
                if session
                    .last_frame
                    .is_none_or(|(presented, _)| presented != revision)
                {
                    return;
                }
                self.check("native_shared_history_step",json!({"operation":label,"revision":revision,"frames":frames,"clips":clips,"media":expected_alias.map(|(id,alias)|json!({"id":id,"alias":alias})),"native_presented":session.last_frame}));
                self.stage = Stage::StartHistory;
            }
            Stage::Export { ui_frames } => {
                if session.render_busy || session.last_complete.as_deref() != Some("Export") {
                    return;
                }
                if !session
                    .events
                    .iter()
                    .any(|event| event.kind == "job_completed")
                {
                    return;
                }
                if session.ui_frames <= ui_frames {
                    self.finish(
                        session,
                        false,
                        Some("No UI update occurred during export".into()),
                    );
                    return;
                }
                self.check("background_export", json!({"ui_updates_while_export":session.ui_frames-ui_frames,"output":self.directory.join("studio-smoke-export.mp4"),"service_job_completed":true}));
                self.stage = Stage::Screenshot;
            }
            Stage::Screenshot => {
                if !self.screenshot_requested {
                    let waiting = self.asset_wait_started.get_or_insert_with(Instant::now);
                    if !session.assets_ready() && waiting.elapsed() < Duration::from_secs(8) {
                        return;
                    }
                    self.asset_wait_expired = !session.assets_ready();
                    self.screenshot_assets = Some(session.asset_evidence());
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                    self.screenshot_requested = true;
                }
                let screenshot = ctx.input(|input| {
                    input.events.iter().find_map(|event| {
                        if let egui::Event::Screenshot { image, .. } = event {
                            Some(image.clone())
                        } else {
                            None
                        }
                    })
                });
                if let Some(image) = screenshot {
                    let rgba = image
                        .pixels
                        .iter()
                        .flat_map(|pixel| pixel.to_array())
                        .collect();
                    if session.send(Command::Screenshot {
                        path: self.directory.join("studio-native.png"),
                        width: image.size[0] as u32,
                        height: image.size[1] as u32,
                        rgba,
                    }) {
                        self.stage = Stage::Saving;
                    }
                }
            }
            Stage::Saving => {
                if session.artifacts.iter().any(|path| {
                    path.file_name()
                        .is_some_and(|name| name == "studio-native.png")
                }) {
                    self.screenshot_saved = true;
                    self.check(
                        "native_window_screenshot",
                        json!({"path":self.directory.join("studio-native.png")}),
                    );
                    self.finish(session, true, None);
                }
            }
            Stage::Finishing | Stage::Closed => {}
        }
    }
}
