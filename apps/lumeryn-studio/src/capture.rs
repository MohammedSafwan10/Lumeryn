//! Native visual evidence without project mutations or workflow smoke actions.
use crate::{app::Session, backend::Command};
use egui::{ColorImage, Context, ViewportCommand};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(30);
const ASSET_TIMEOUT: Duration = Duration::from_secs(8);
const REPORT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug)]
enum Stage {
    Waiting,
    Requested,
    Saving,
    Reporting { started: Instant },
    Closed,
}

#[derive(Debug)]
pub struct Capture {
    directory: PathBuf,
    started: Instant,
    stage: Stage,
    asset_wait_started: Option<Instant>,
    asset_wait_expired: bool,
    evidence: Option<Value>,
}

impl Capture {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            started: Instant::now(),
            stage: Stage::Waiting,
            asset_wait_started: None,
            asset_wait_expired: false,
            evidence: None,
        }
    }

    fn screenshot_path(&self) -> PathBuf {
        self.directory.join("screenshot-capture.png")
    }

    fn report_path(&self) -> PathBuf {
        self.directory.join("studio-capture.json")
    }

    fn finish(&mut self, session: &mut Session, passed: bool, error: Option<String>) {
        let report = json!({
            "schema_version": 1,
            "capture_only": true,
            "passed": passed,
            "error": error,
            "elapsed_ms": self.started.elapsed().as_secs_f64() * 1000.0,
            "native_screenshot": self.screenshot_path(),
            "screenshot_saved": passed,
            "capture": self.evidence,
            "viewport": {
                "logical_width": session.viewport_size.x,
                "logical_height": session.viewport_size.y,
                "pixels_per_point": session.pixels_per_point,
            },
            "project_revision": session.project.as_ref().map(|project| project.revision),
            "playhead_frame": session.playhead,
            "presented_frame": session.last_frame,
            "asset_evidence": session.asset_evidence(),
            "asset_capture_wait_expired": self.asset_wait_expired,
            "session_error": session.error,
            "import_state": session.import_evidence(),
            "limitations": [
                "Capture records actual native pixels; it does not verify editing workflows or pointer usability.",
                "Assets can fail or exceed the bounded wait; readiness and counts are reported independently of capture success.",
                "Concurrent external edits may change the project; this capture path submits no editing commands."
            ]
        });
        self.stage = if session.send(Command::Report {
            path: self.report_path(),
            value: report,
        }) {
            Stage::Reporting {
                started: Instant::now(),
            }
        } else {
            eprintln!("Native capture report could not be queued");
            Stage::Closed
        };
    }

    /// The caller supplies readiness for its selected scene, snapshot and preview frame.
    /// Empty scenes can be ready without a project; media readiness has its own bounded wait.
    pub fn tick(&mut self, session: &mut Session, ctx: &Context, ready: bool) {
        if matches!(self.stage, Stage::Closed) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }
        if let Stage::Reporting { started } = &self.stage {
            let report_saved = session.artifacts.contains(&self.report_path());
            let report_failed = session
                .error
                .as_ref()
                .is_some_and(|error| error.starts_with("Report:"));
            if report_saved || report_failed || started.elapsed() >= REPORT_TIMEOUT {
                if !report_saved {
                    eprintln!("Native capture report was not saved before close");
                }
                self.stage = Stage::Closed;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            return;
        }
        if self.started.elapsed() >= CAPTURE_TIMEOUT {
            self.finish(
                session,
                false,
                Some(format!("Timed out in {:?}", self.stage)),
            );
            return;
        }
        if session
            .error
            .as_ref()
            .is_some_and(|error| error.starts_with("Screenshot:"))
        {
            self.finish(session, false, session.error.clone());
            return;
        }
        match self.stage {
            Stage::Waiting => {
                if !ready || session.viewport_size.x <= 0.0 || session.viewport_size.y <= 0.0 {
                    return;
                }
                if session.project.is_some() {
                    let waiting = self.asset_wait_started.get_or_insert_with(Instant::now);
                    if !session.assets_ready() && waiting.elapsed() < ASSET_TIMEOUT {
                        return;
                    }
                    self.asset_wait_expired = !session.assets_ready();
                }
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(Default::default()));
                self.stage = Stage::Requested;
            }
            Stage::Requested => {
                if !ready {
                    // A live revision/seek can supersede the requested frame before the
                    // screenshot event arrives. Discard that event and capture when ready.
                    self.asset_wait_started = None;
                    self.stage = Stage::Waiting;
                    return;
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
                    self.evidence = Some(json!({
                        "physical_width": image.size[0],
                        "physical_height": image.size[1],
                        "project_revision": session.project.as_ref().map(|project| project.revision),
                        "playhead_frame": session.playhead,
                        "presented_frame": session.last_frame,
                        "viewport": {
                            "logical_width": session.viewport_size.x,
                            "logical_height": session.viewport_size.y,
                            "pixels_per_point": session.pixels_per_point,
                        },
                        "assets": session.asset_evidence(),
                        "native_fullscreen": session.native_fullscreen,
                    }));
                    match screenshot_command(&image, self.screenshot_path()) {
                        Ok(command) => {
                            if session.send(command) {
                                self.stage = Stage::Saving;
                            } else {
                                self.finish(session, false, session.error.clone());
                            }
                        }
                        Err(error) => self.finish(session, false, Some(error)),
                    }
                }
            }
            Stage::Saving => {
                if session.artifacts.contains(&self.screenshot_path()) {
                    self.finish(session, true, None);
                }
            }
            Stage::Reporting { .. } | Stage::Closed => {}
        }
    }
}

fn screenshot_command(image: &ColorImage, path: PathBuf) -> Result<Command, String> {
    let [width, height] = image.size;
    if width == 0 || height == 0 || width.checked_mul(height) != Some(image.pixels.len()) {
        return Err("Native screenshot has invalid dimensions or pixel count".into());
    }
    let width = u32::try_from(width).map_err(|error| error.to_string())?;
    let height = u32::try_from(height).map_err(|error| error.to_string())?;
    Ok(Command::Screenshot {
        path,
        width,
        height,
        rgba: image
            .pixels
            .iter()
            .flat_map(|pixel| pixel.to_array())
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::screenshot_command;
    use crate::backend::Command;
    use egui::{Color32, ColorImage};
    use std::path::PathBuf;

    #[test]
    fn native_pixels_reach_png_worker_without_scaling() -> Result<(), String> {
        let image = ColorImage::new([2, 1], vec![Color32::RED, Color32::GREEN]);
        let command = screenshot_command(&image, PathBuf::from("capture.png"))?;
        let Command::Screenshot {
            width,
            height,
            rgba,
            ..
        } = command
        else {
            return Err("Expected screenshot worker command".into());
        };
        assert_eq!((width, height), (2, 1));
        assert_eq!(rgba, [255, 0, 0, 255, 0, 255, 0, 255]);
        Ok(())
    }

    #[test]
    fn malformed_native_buffer_is_rejected() {
        let mut image = ColorImage::new([1, 1], vec![Color32::BLACK]);
        image.size = [2, 1];
        assert!(screenshot_command(&image, PathBuf::from("capture.png")).is_err());
        image.size = [usize::MAX, 2];
        assert!(screenshot_command(&image, PathBuf::from("capture.png")).is_err());
        image.size = [0, 0];
        assert!(screenshot_command(&image, PathBuf::from("capture.png")).is_err());
    }
}
