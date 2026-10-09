use crate::{app::Session, backend::Command, timeline};
use serde_json::{Value, json};
use std::{
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
}

impl Smoke {
    pub fn new(directory: PathBuf) -> Self {
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
            "limitations":["Scripted actions call the same UI command paths; pointer usability and native file-dialog cancellation are separate manual acceptance.","CPU upload timing does not measure GPU completion or photon latency.","VRAM consumption is unknown without telemetry.","Audio device status alone does not prove end-to-end A/V tolerance.","Effects, multi-track composition, undo/redo and proxies remain staged. Thumbnail/waveform caches are bounded; readiness/counts are reported independently of workflow success."]
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
                if session.export(self.directory.join("studio-smoke-export.mp4")) {
                    self.stage = Stage::Export {
                        ui_frames: session.ui_frames,
                    };
                }
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
