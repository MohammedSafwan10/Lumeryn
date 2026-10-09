use crate::{
    Options,
    backend::{Backend, Command, Message},
    design,
    gpu::PreviewTexture,
    smoke::Smoke,
    timeline::{self, Timeline},
};
use egui::{Color32, RichText, Sense, Stroke, Vec2};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use lumeryn_core::{Batch, Edit, MediaTools, Project};
use lumeryn_preview::{PreviewConfig, PreviewPlayer};
use lumeryn_service::ToolConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Panel {
    Media,
    Preview,
    Timeline,
    Inspector,
    Settings,
    Render,
    Activity,
    History,
    Diagnostics,
}

impl Panel {
    fn title(self) -> &'static str {
        match self {
            Self::Media => "Media Library",
            Self::Preview => "Preview Monitor",
            Self::Timeline => "Timeline",
            Self::Inspector => "Inspector",
            Self::Settings => "Project Settings",
            Self::Render => "Render Queue",
            Self::Activity => "Agent Activity",
            Self::History => "Revision History",
            Self::Diagnostics => "Diagnostics",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
enum Workspace {
    #[default]
    Beginner,
    Professional,
}

fn layout(workspace: Workspace) -> DockState<Panel> {
    let mut dock = DockState::new(vec![Panel::Preview]);
    let [main, _] = dock.main_surface_mut().split_below(
        NodeIndex::root(),
        0.66,
        vec![Panel::Timeline, Panel::Render, Panel::Activity],
    );
    let [main, _] = dock
        .main_surface_mut()
        .split_left(main, 0.19, vec![Panel::Media]);
    let right = match workspace {
        Workspace::Beginner => vec![
            Panel::Inspector,
            Panel::History,
            Panel::Settings,
            Panel::Diagnostics,
        ],
        Workspace::Professional => vec![
            Panel::Inspector,
            Panel::Settings,
            Panel::History,
            Panel::Diagnostics,
        ],
    };
    dock.main_surface_mut().split_right(main, 0.75, right);
    dock
}

#[derive(Serialize, Deserialize)]
struct SavedWorkspace {
    version: u32,
    workspace: Workspace,
    dock: DockState<Panel>,
}

pub struct Session {
    backend: Backend,
    pub project: Option<Project>,
    pub database: Option<PathBuf>,
    epoch: u64,
    project_input: String,
    media_input: String,
    pub selected: Option<Uuid>,
    trim_source: u64,
    trim_duration: u64,
    pub preview: Option<PreviewPlayer>,
    preview_tools: MediaTools,
    quality: usize,
    pub texture: Option<PreviewTexture>,
    pub playhead: u64,
    pub presented_frames: u64,
    pub last_frame: Option<(u64, u64)>,
    pending_seek: Option<(u64, u64)>,
    pub first_frame_ms: Option<f64>,
    pub started: Instant,
    connected: bool,
    connection: String,
    pub error: Option<String>,
    logs: VecDeque<String>,
    pub history: Vec<Value>,
    pub events: VecDeque<lumeryn_service::Event>,
    edit_busy: bool,
    pub render_busy: bool,
    active_job: Option<Uuid>,
    output_input: String,
    pub last_complete: Option<String>,
    pub artifacts: Vec<PathBuf>,
    pub ui_frames: u64,
    pub max_ui_ms: f64,
    pub ui_samples_ms: VecDeque<f64>,
    frame_wait_until: Option<Instant>,
    pub timeline: Timeline,
}

impl Session {
    fn new(cc: &eframe::CreationContext<'_>, options: &Options) -> Self {
        let tools = ToolConfig {
            ffmpeg: options.ffmpeg.clone(),
            ffprobe: options.ffprobe.clone(),
            ..Default::default()
        };
        let preview_tools = MediaTools {
            ffmpeg: options.ffmpeg.clone(),
            ffprobe: options.ffprobe.clone(),
            ..Default::default()
        };
        let preview = PreviewPlayer::new(preview_tools.clone(), PreviewConfig::default());
        let (preview, error) = match preview {
            Ok(preview) => (Some(preview), None),
            Err(error) => (None, Some(format!("Preview initialization: {error}"))),
        };
        Self {
            backend: Backend::start(cc.egui_ctx.clone(), tools),
            project: None,
            database: None,
            epoch: 0,
            project_input: options
                .project
                .as_ref()
                .map_or_else(String::new, |p| p.display().to_string()),
            media_input: String::new(),
            selected: None,
            trim_source: 0,
            trim_duration: 1,
            preview,
            preview_tools,
            quality: 1,
            texture: cc.wgpu_render_state.clone().map(PreviewTexture::new),
            playhead: 0,
            presented_frames: 0,
            last_frame: None,
            pending_seek: None,
            first_frame_ms: None,
            started: options.launch_started.unwrap_or_else(Instant::now),
            connected: false,
            connection: "Open a project to begin".into(),
            error,
            logs: VecDeque::new(),
            history: Vec::new(),
            events: VecDeque::new(),
            edit_busy: false,
            render_busy: false,
            active_job: None,
            output_input: String::new(),
            last_complete: None,
            artifacts: Vec::new(),
            ui_frames: 0,
            max_ui_ms: 0.0,
            ui_samples_ms: VecDeque::new(),
            frame_wait_until: None,
            timeline: Timeline::default(),
        }
    }

    pub fn send(&mut self, command: Command) -> bool {
        match self.backend.commands.try_send(command) {
            Ok(()) => true,
            Err(error) => {
                self.fail(format!("Background request queue: {error}"));
                false
            }
        }
    }

    fn note(&mut self, note: impl Into<String>) {
        if self.logs.len() >= 100 {
            self.logs.pop_front();
        }
        self.logs.push_back(note.into());
    }

    fn fail(&mut self, message: String) {
        self.note(message.clone());
        self.error = Some(message);
    }

    pub fn open(&mut self, path: PathBuf) {
        if self.render_busy {
            self.fail("Wait for the current export before switching projects".into());
            return;
        }
        self.epoch += 1;
        self.project = None;
        self.last_frame = None;
        self.frame_wait_until = Some(Instant::now() + Duration::from_secs(30));
        if let Some(texture) = &mut self.texture {
            texture.invalidate();
        }
        self.selected = None;
        self.connected = false;
        self.playhead = 0;
        self.history.clear();
        self.events.clear();
        self.error = None;
        self.edit_busy = false;
        if let Some(preview) = &self.preview
            && let Err(error) = preview.pause()
        {
            self.fail(format!("Pause: {error}"));
        }
        self.output_input = path.parent().map_or_else(
            || "export.mp4".into(),
            |p| p.join("export.mp4").display().to_string(),
        );
        self.project_input = path.display().to_string();
        self.database = Some(path.clone());
        self.send(Command::Open {
            path,
            epoch: self.epoch,
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        for _ in 0..64 {
            let Ok(envelope) = self.backend.messages.try_recv() else {
                break;
            };
            if envelope.epoch != 0 && envelope.epoch != self.epoch {
                continue;
            }
            match envelope.message {
                Message::Snapshot(project) => {
                    let changed = self
                        .project
                        .as_ref()
                        .is_none_or(|p| p.revision != project.revision);
                    self.connected = true;
                    self.connection = "Connected · shared project service".into();
                    if changed {
                        self.note(format!(
                            "Authoritative revision {} received",
                            project.revision
                        ));
                        if let (Some(preview), Some(path)) = (&self.preview, &self.database) {
                            let was_playing = preview.status().playing;
                            if let Err(error) = preview.load(project.clone(), path.clone()) {
                                self.fail(format!("Preview load: {error}"));
                            } else {
                                self.frame_wait_until =
                                    Some(Instant::now() + Duration::from_secs(30));
                                self.pending_seek = (self.playhead != 0)
                                    .then_some((project.revision, self.playhead));
                                if was_playing && let Err(error) = preview.play() {
                                    self.fail(format!("Play: {error}"));
                                }
                            }
                        }
                        if self
                            .selected
                            .is_none_or(|id| !project.clips.iter().any(|clip| clip.id == id))
                        {
                            self.selected = project.clips.first().map(|clip| clip.id);
                        }
                        if let Some(clip) = project
                            .clips
                            .iter()
                            .find(|clip| Some(clip.id) == self.selected)
                        {
                            self.trim_source = clip.source_in_frame;
                            self.trim_duration = clip.duration_frames;
                        }
                        self.project = Some(project);
                    }
                }
                Message::Events(events) => {
                    for event in events {
                        if (event.kind == "job_completed"
                            || event.kind == "job_failed"
                            || event.kind == "job_cancelled")
                            && event
                                .data
                                .get("job_id")
                                .and_then(Value::as_str)
                                .and_then(|value| Uuid::parse_str(value).ok())
                                == self.active_job
                        {
                            self.render_busy = false;
                        }
                        if self.events.len() >= 128 {
                            self.events.pop_front();
                        }
                        self.events.push_back(event);
                    }
                }
                Message::History(history) => self.history = history,
                Message::Complete { label, value } => {
                    self.note(format!("{label}: {value}"));
                    if label == "Export" {
                        self.render_busy = false;
                    } else {
                        self.edit_busy = false;
                    }
                    self.last_complete = Some(label);
                }
                Message::Connection(message) => {
                    self.connected = false;
                    self.connection = message;
                }
                Message::Error { label, message } => {
                    if label == "Export" {
                        self.render_busy = false;
                    }
                    self.edit_busy = false;
                    self.fail(format!("{label}: {message}"));
                }
                Message::Artifact(path) => self.artifacts.push(path),
            }
        }
        if let Some(preview) = &mut self.preview {
            if let Some(frame) = preview.try_frame() {
                self.frame_wait_until = None;
                self.playhead = frame.frame_index;
                self.last_frame = Some((frame.project_revision, frame.frame_index));
                self.presented_frames += 1;
                self.first_frame_ms
                    .get_or_insert(self.started.elapsed().as_secs_f64() * 1000.0);
                if let Some(texture) = &mut self.texture
                    && let Err(error) = texture.upload(&frame)
                {
                    self.error = Some(error);
                }
                ctx.request_repaint();
                if let Some((revision, target)) = self.pending_seek
                    && frame.project_revision == revision
                {
                    self.pending_seek = None;
                    self.frame_wait_until = Some(Instant::now() + Duration::from_secs(30));
                    if let Err(error) = preview.seek(target) {
                        self.error = Some(format!("Seek after reload: {error}"));
                    }
                }
            }
            let status = preview.status();
            if let Some(error) = status.error {
                self.error = Some(format!("Preview: {error}"));
            }
            if status.playing {
                ctx.request_repaint_after(Duration::from_millis(8));
            }
        }
    }

    fn select(&mut self, id: Uuid) {
        self.selected = Some(id);
        if let Some(clip) = self
            .project
            .as_ref()
            .and_then(|p| p.clips.iter().find(|clip| clip.id == id))
        {
            self.trim_source = clip.source_in_frame;
            self.trim_duration = clip.duration_frames;
        }
    }

    pub fn seek(&mut self, frame: u64) {
        self.playhead = frame;
        self.frame_wait_until = Some(Instant::now() + Duration::from_secs(30));
        if let Some(preview) = &self.preview
            && let Err(error) = preview.seek(frame)
        {
            self.fail(format!("Seek: {error}"));
        }
    }

    pub fn trim(&mut self, clip: Uuid, source_in: u64, duration: u64) -> bool {
        self.edit(
            Edit::TrimClip {
                clip_id: clip,
                source_in_frame: source_in,
                duration_frames: duration,
            },
            "Trim clip",
        )
    }

    fn edit(&mut self, operation: Edit, label: &str) -> bool {
        if !self.connected || self.edit_busy {
            return false;
        }
        let Some(project) = &self.project else {
            return false;
        };
        let revision = project.revision;
        if self.send(Command::Edit {
            batch: Batch {
                operations: vec![operation],
            },
            revision,
            label: label.into(),
        }) {
            self.edit_busy = true;
            true
        } else {
            false
        }
    }

    pub fn export(&mut self, output: PathBuf) -> bool {
        if !self.connected || self.render_busy {
            return false;
        }
        let Some(project) = &self.project else {
            return false;
        };
        let revision = project.revision;
        let job_id = Uuid::new_v4();
        if self.send(Command::Render {
            output,
            revision,
            job_id,
        }) {
            self.render_busy = true;
            self.active_job = Some(job_id);
            self.last_complete = None;
            true
        } else {
            false
        }
    }

    fn toggle_play(&mut self) {
        if let Some(preview) = &self.preview {
            let result = if preview.status().playing {
                preview.pause()
            } else {
                preview.play()
            };
            if let Err(error) = result {
                self.fail(format!("Playback: {error}"));
            }
        }
    }

    fn step(&mut self, delta: i64) {
        self.frame_wait_until = Some(Instant::now() + Duration::from_secs(30));
        if let Some(preview) = &self.preview
            && let Err(error) = preview.step(delta)
        {
            self.fail(format!("Frame step: {error}"));
        }
    }

    fn media_panel(&mut self, ui: &mut egui::Ui) {
        design::section(
            ui,
            "Project media",
            "Source files stay unchanged. Import a local video, then add it to the timeline.",
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.media_input)
                .hint_text("Local media path")
                .desired_width(f32::INFINITY),
        );
        if ui
            .add_enabled(
                self.connected && !self.edit_busy && !self.media_input.trim().is_empty(),
                egui::Button::new("Import media"),
            )
            .clicked()
        {
            self.edit(
                Edit::Import {
                    id: Uuid::new_v4(),
                    path: self.media_input.trim().into(),
                },
                "Import media",
            );
        }
        ui.separator();
        let mut add = None;
        if let Some(project) = &self.project {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for media in &project.media {
                    egui::Frame::new()
                        .fill(design::PANEL)
                        .stroke(Stroke::new(1.0, design::BORDER))
                        .corner_radius(5.0)
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(
                                    media.path.file_name().map_or_else(
                                        || "Media".into(),
                                        |name| name.to_string_lossy(),
                                    ),
                                )
                                .strong(),
                            );
                            if let Some(video) = &media.metadata.video {
                                ui.label(
                                    RichText::new(format!(
                                        "{} × {}  ·  {} fps",
                                        video.width, video.height, video.frame_rate
                                    ))
                                    .size(11.0)
                                    .color(design::MUTED),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "{} frames · {}",
                                        video.frames, video.codec
                                    ))
                                    .size(11.0)
                                    .color(design::MUTED),
                                );
                                if ui
                                    .add_enabled(
                                        !self.edit_busy && self.connected,
                                        egui::Button::new("Add to timeline").small(),
                                    )
                                    .clicked()
                                {
                                    add = Some((media.id, video.frames));
                                }
                            }
                            ui.label(
                                RichText::new(media.path.display().to_string())
                                    .size(10.0)
                                    .color(design::MUTED),
                            )
                            .on_hover_text(media.path.display().to_string());
                        });
                    ui.add_space(6.0);
                }
                if project.media.is_empty() {
                    ui.label("No imported media yet.");
                }
            });
        } else {
            ui.label("Open an existing .lumeryn project.");
        }
        if let Some((media_id, duration_frames)) = add {
            let at_frame = self.project.as_ref().map_or(0, timeline::total_frames);
            self.edit(
                Edit::AddClip {
                    id: Uuid::new_v4(),
                    media_id,
                    at_frame,
                    source_in_frame: 0,
                    duration_frames,
                },
                "Add clip",
            );
        }
    }

    fn preview_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("PROGRAM")
                    .size(11.0)
                    .strong()
                    .color(design::MUTED),
            );
            if let Some(project) = &self.project {
                ui.label(
                    RichText::new(format!(
                        "{} × {} · {} fps",
                        project.width, project.height, project.frame_rate
                    ))
                    .size(11.0)
                    .color(design::MUTED),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let previous = self.quality;
                egui::ComboBox::from_id_salt("preview-quality")
                    .selected_text(["360p", "720p", "1080p"][self.quality])
                    .show_ui(ui, |ui| {
                        for (index, label) in ["360p", "720p", "1080p"].iter().enumerate() {
                            ui.selectable_value(&mut self.quality, index, *label);
                        }
                    });
                if previous != self.quality {
                    let (max_width, max_height) =
                        [(640, 360), (1280, 720), (1920, 1080)][self.quality];
                    match PreviewPlayer::new(
                        self.preview_tools.clone(),
                        PreviewConfig {
                            max_width,
                            max_height,
                            ..Default::default()
                        },
                    ) {
                        Ok(preview) => {
                            self.last_frame = None;
                            self.frame_wait_until = Some(Instant::now() + Duration::from_secs(30));
                            if let Some(texture) = &mut self.texture {
                                texture.invalidate();
                            }
                            if let Some(project) = &self.project
                                && let Some(database) = &self.database
                            {
                                let revision = project.revision;
                                if let Err(error) = preview.load(project.clone(), database.clone())
                                {
                                    self.fail(format!("Preview quality: {error}"));
                                }
                                self.pending_seek =
                                    (self.playhead != 0).then_some((revision, self.playhead));
                            }
                            self.preview = Some(preview);
                        }
                        Err(error) => self.fail(format!("Preview quality: {error}")),
                    }
                }
            });
        });
        let size = Vec2::new(
            ui.available_width().max(40.0),
            (ui.available_height() - 110.0).max(40.0),
        );
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        ui.painter().rect_filled(rect, 4.0, Color32::BLACK);
        let current_frame = self.last_frame.is_some_and(|(revision, _)| {
            self.project
                .as_ref()
                .is_some_and(|p| p.revision == revision)
        });
        if let Some((id, source)) = current_frame
            .then(|| self.texture.as_ref().and_then(PreviewTexture::image))
            .flatten()
        {
            let scale = (rect.width() / source.x).min(rect.height() / source.y);
            let image_rect = egui::Rect::from_center_size(rect.center(), source * scale);
            ui.painter().image(
                id,
                image_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if self.project.is_some() {
                    "Waiting for a decoded frame"
                } else {
                    "Open a project to review your video"
                },
                egui::FontId::proportional(16.0),
                design::MUTED,
            );
        }
        ui.horizontal(|ui| {
            if ui.button("|<").on_hover_text("First frame").clicked() {
                self.seek(0);
            }
            if ui
                .button("<")
                .on_hover_text("Previous frame · Left arrow")
                .clicked()
            {
                self.step(-1);
            }
            let playing = self.preview.as_ref().is_some_and(|p| p.status().playing);
            if ui
                .add_sized(
                    [90.0, 30.0],
                    egui::Button::new(if playing { "Pause" } else { "Play" }),
                )
                .on_hover_text("Space")
                .clicked()
            {
                self.toggle_play();
            }
            if ui
                .button(">")
                .on_hover_text("Next frame · Right arrow")
                .clicked()
            {
                self.step(1);
            }
            ui.label(
                RichText::new(format!("Frame {:06}", self.playhead))
                    .monospace()
                    .color(design::ACCENT),
            );
            if let Some(project) = &self.project {
                ui.label(
                    RichText::new(format!(
                        "{:.3}s",
                        self.playhead as f64 * f64::from(project.frame_rate.denominator())
                            / f64::from(project.frame_rate.numerator())
                    ))
                    .monospace()
                    .size(11.0),
                );
            }
        });
        let maximum = self
            .project
            .as_ref()
            .map_or(0, timeline::total_frames)
            .saturating_sub(1);
        let mut frame = self.playhead;
        if ui
            .add(egui::Slider::new(&mut frame, 0..=maximum).show_value(false))
            .changed()
        {
            self.seek(frame);
        }
        if let Some(preview) = &self.preview {
            let status = preview.status();
            ui.label(
                RichText::new(format!("Audio: {:?}", status.audio))
                    .size(10.0)
                    .color(design::MUTED),
            );
        }
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        design::section(
            ui,
            "Clip inspector",
            "Trim changes the visible source range without changing the original file.",
        );
        let clip = self
            .project
            .as_ref()
            .and_then(|p| p.clips.iter().find(|clip| Some(clip.id) == self.selected));
        if let Some(clip) = clip {
            let clip_id = clip.id;
            ui.label(
                RichText::new("SELECTED CLIP")
                    .size(10.0)
                    .color(design::ACCENT),
            );
            ui.label(RichText::new(clip.id.to_string()).monospace().size(10.0));
            ui.add_space(8.0);
            egui::Grid::new("trim-values")
                .spacing([12.0, 12.0])
                .show(ui, |ui| {
                    ui.label("Timeline start");
                    ui.label(format!("{} frames", clip.at_frame));
                    ui.end_row();
                    ui.label("Source in");
                    ui.add(
                        egui::DragValue::new(&mut self.trim_source)
                            .range(0..=lumeryn_core::model::MAX_FRAMES),
                    );
                    ui.end_row();
                    ui.label("Duration");
                    ui.add(
                        egui::DragValue::new(&mut self.trim_duration)
                            .range(1..=lumeryn_core::model::MAX_FRAMES),
                    );
                    ui.end_row();
                });
            ui.add_space(12.0);
            if ui
                .add_enabled(
                    self.connected && !self.edit_busy,
                    egui::Button::new("Apply trim").fill(design::CLIP),
                )
                .clicked()
            {
                self.trim(clip_id, self.trim_source, self.trim_duration);
            }
            ui.separator();
            ui.label(RichText::new("Changes are validated by the shared engine. Stale revisions fail without overwriting agent edits.").size(11.0).color(design::MUTED));
        } else {
            ui.label("Select a clip in the timeline.");
        }
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        design::section(
            ui,
            "Project settings",
            "Committed format used by both native preview and export.",
        );
        if let Some(project) = &self.project {
            egui::Grid::new("project-settings")
                .spacing([12.0, 14.0])
                .show(ui, |ui| {
                    for (key, value) in [
                        ("Name", project.name.clone()),
                        ("Canvas", format!("{} × {}", project.width, project.height)),
                        ("Frame rate", project.frame_rate.to_string()),
                        ("Revision", project.revision.to_string()),
                        ("Schema", project.schema_version.to_string()),
                        ("Project ID", project.id.to_string()),
                    ] {
                        ui.label(RichText::new(key).color(design::MUTED));
                        ui.label(value);
                        ui.end_row();
                    }
                });
            ui.separator();
            ui.label("Format changes are not supported by the current engine.");
            if let Some(database) = &self.database {
                ui.label(
                    RichText::new(database.display().to_string())
                        .monospace()
                        .size(11.0),
                );
            }
        }
    }

    fn render_panel(&mut self, ui: &mut egui::Ui) {
        design::section(
            ui,
            "Export your video",
            "Verified MPEG-4/AAC MP4 · contiguous clips · a new destination file.",
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.output_input)
                .hint_text("New .mp4 destination")
                .desired_width(f32::INFINITY),
        );
        if ui
            .add_enabled(
                self.connected && !self.render_busy && !self.output_input.trim().is_empty(),
                egui::Button::new("Export video"),
            )
            .clicked()
        {
            self.export(self.output_input.trim().into());
        }
        if self.render_busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    "Export runs in the project service. Editing and preview remain available.",
                );
                if ui.button("Cancel export").clicked()
                    && let Some(job_id) = self.active_job
                {
                    self.send(Command::Cancel { job_id });
                }
            });
        }
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for event in self
                .events
                .iter()
                .rev()
                .filter(|e| e.kind.starts_with("job_"))
                .take(30)
            {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&event.kind).color(design::ACCENT));
                    ui.label(event.data.to_string());
                });
            }
        });
    }

    fn activity(&mut self, ui: &mut egui::Ui) {
        design::section(
            ui,
            "Shared project activity",
            "Committed CLI and Studio edits arrive through the same revision event stream.",
        );
        egui::ScrollArea::vertical().show(ui, |ui| {
            for event in self.events.iter().rev() {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("#{:04}", event.sequence))
                            .monospace()
                            .color(design::MUTED),
                    );
                    ui.label(RichText::new(&event.kind).strong());
                    if let Some(revision) = event.revision {
                        ui.label(format!("revision {revision}"));
                    }
                });
                ui.label(
                    RichText::new(event.data.to_string())
                        .size(11.0)
                        .color(design::MUTED),
                );
                ui.separator();
            }
            if self.events.is_empty() {
                ui.label("Waiting for project activity.");
            }
        });
    }

    fn history_panel(&mut self, ui: &mut egui::Ui) {
        design::section(
            ui,
            "Revision history",
            "Read the actual committed commands. Undo/redo is a later engine capability.",
        );
        if ui.button("Refresh history").clicked() {
            self.send(Command::History);
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for entry in self.history.iter().rev() {
                ui.label(
                    RichText::new(format!(
                        "Revision {}",
                        entry.get("revision").unwrap_or(&Value::Null)
                    ))
                    .strong(),
                );
                ui.label(
                    RichText::new(entry.get("operations").unwrap_or(&Value::Null).to_string())
                        .monospace()
                        .size(11.0),
                );
                ui.separator();
            }
        });
    }

    fn diagnostics(&mut self, ui: &mut egui::Ui) {
        design::section(
            ui,
            "Runtime diagnostics",
            "Observed values are measured locally; unknown GPU memory is not estimated.",
        );
        if let Some(texture) = &self.texture {
            ui.label(&texture.adapter);
            ui.label(format!(
                "{} frame uploads · latest CPU submission {:.3} ms",
                texture.uploads, texture.upload_cpu_ms
            ));
        }
        ui.label(format!(
            "{} UI updates · max {:.3} ms",
            self.ui_frames, self.max_ui_ms
        ));
        ui.label(format!(
            "{} visible timeline clips",
            self.timeline.visible_clips
        ));
        ui.label("VRAM usage: unknown · no budget telemetry yet");
        if let Some(preview) = &self.preview {
            let status = preview.status();
            ui.label(format!(
                "Decoded {} · dropped {} · buffered {}",
                status.decoded_frames, status.dropped_frames, status.buffered_frames
            ));
            ui.label(format!(
                "Decode {:.2} ms · seek {:.2} ms",
                status.decode_time_ms, status.seek_latency_ms
            ));
            ui.label(format!("Audio: {:?}", status.audio));
        }
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for log in self.logs.iter().rev() {
                ui.label(RichText::new(log).monospace().size(11.0));
            }
        });
    }
}

struct Viewer<'a> {
    session: &'a mut Session,
}

impl TabViewer for Viewer<'_> {
    type Tab = Panel;
    fn title(&mut self, tab: &mut Panel) -> egui::WidgetText {
        tab.title().into()
    }
    fn id(&mut self, tab: &mut Panel) -> egui::Id {
        egui::Id::new(tab.title())
    }
    fn closeable(&mut self, _tab: &mut Panel) -> bool {
        false
    }
    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Panel) {
        match tab {
            Panel::Media => self.session.media_panel(ui),
            Panel::Preview => self.session.preview_panel(ui),
            Panel::Inspector => self.session.inspector(ui),
            Panel::Settings => self.session.settings(ui),
            Panel::Render => self.session.render_panel(ui),
            Panel::Activity => self.session.activity(ui),
            Panel::History => self.session.history_panel(ui),
            Panel::Diagnostics => self.session.diagnostics(ui),
            Panel::Timeline => {
                if let Some(project) = &self.session.project {
                    let actions = self.session.timeline.show(
                        ui,
                        project,
                        self.session.selected,
                        self.session.playhead,
                    );
                    for action in actions {
                        match action {
                            timeline::Action::Select(id) => self.session.select(id),
                            timeline::Action::Seek(frame) => self.session.seek(frame),
                            timeline::Action::Trim { clip, duration } => {
                                let source = self
                                    .session
                                    .project
                                    .as_ref()
                                    .and_then(|p| p.clips.iter().find(|c| c.id == clip))
                                    .map_or(0, |c| c.source_in_frame);
                                self.session.trim(clip, source, duration);
                            }
                        }
                    }
                } else {
                    design::section(
                        ui,
                        "Your edit timeline",
                        "Open a project to view its committed tracks and clips.",
                    );
                }
            }
        }
    }
}

pub struct Studio {
    session: Session,
    dock: DockState<Panel>,
    workspace: Workspace,
    smoke: Option<Smoke>,
}

impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>, options: Options) -> Self {
        design::configure(&cc.egui_ctx);
        let saved: Option<SavedWorkspace> = if options.smoke {
            None
        } else {
            cc.storage.and_then(|s| eframe::get_value(s, "workspace"))
        };
        let (workspace, dock) = saved.filter(|s| s.version == 1).map_or_else(
            || (Workspace::Beginner, layout(Workspace::Beginner)),
            |s| (s.workspace, s.dock),
        );
        let mut session = Session::new(cc, &options);
        if let Some(path) = &options.project {
            session.open(path.clone());
        }
        let smoke = if options.smoke {
            options.artifact_dir.map(Smoke::new)
        } else {
            None
        };
        Self {
            session,
            dock,
            workspace,
            smoke,
        }
    }
}

impl eframe::App for Studio {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let started = Instant::now();
        let ctx = ui.ctx().clone();
        self.session.ui_frames += 1;
        self.session.poll(&ctx);
        for dropped in ctx.input(|i| i.raw.dropped_files.clone()) {
            let path = dropped.path().to_path_buf();
            if path.extension().is_some_and(|e| e == "lumeryn") {
                self.session.open(path);
            } else {
                self.session.media_input = path.display().to_string();
                self.session.edit(
                    Edit::Import {
                        id: Uuid::new_v4(),
                        path,
                    },
                    "Import media",
                );
            }
        }
        if !ctx.egui_wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
                self.session.toggle_play();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                self.session.step(-1);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                self.session.step(1);
            }
        }
        egui::Frame::new()
            .fill(design::PANEL)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("LUMERYN")
                            .strong()
                            .size(22.0)
                            .color(design::TEXT),
                    );
                    ui.label(RichText::new("STUDIO").size(10.0).color(design::ACCENT));
                    ui.separator();
                    if let Some(project) = &self.session.project {
                        ui.label(RichText::new(&project.name).strong());
                        ui.label(
                            RichText::new(format!("r{}", project.revision))
                                .monospace()
                                .color(design::MUTED),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Reset workspace").clicked() {
                            self.dock = layout(self.workspace);
                        }
                        if ui.button("Professional").clicked() {
                            self.workspace = Workspace::Professional;
                            self.dock = layout(self.workspace);
                        }
                        if ui.button("Beginner").clicked() {
                            self.workspace = Workspace::Beginner;
                            self.dock = layout(self.workspace);
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.session.project_input)
                            .hint_text("Existing .lumeryn project path")
                            .desired_width(480.0),
                    );
                    if ui.button("Open project").clicked()
                        && !self.session.project_input.trim().is_empty()
                    {
                        self.session.open(self.session.project_input.trim().into());
                    }
                    ui.label(
                        RichText::new(&self.session.connection)
                            .color(if self.session.connected {
                                design::ACCENT
                            } else {
                                design::MUTED
                            })
                            .size(11.0),
                    );
                    if self.session.edit_busy || self.session.render_busy {
                        ui.spinner();
                    }
                });
            });
        if let Some(error) = &self.session.error {
            let error = error.clone();
            ui.horizontal(|ui| {
                ui.label(RichText::new(error).color(design::WARNING));
                if ui.small_button("Dismiss").clicked() {
                    self.session.error = None;
                }
            });
        }
        ui.add_space(5.0);
        let mut dock_style = egui_dock::Style::from_egui(ui.style());
        dock_style.tab_bar.height = 32.0;
        dock_style.tab_bar.bg_fill = design::PANEL;
        dock_style.main_surface_border_stroke = Stroke::new(1.0, design::BORDER);
        DockArea::new(&mut self.dock)
            .style(dock_style)
            .show_close_buttons(false)
            .show_inside(
                ui,
                &mut Viewer {
                    session: &mut self.session,
                },
            );
        if let Some(smoke) = &mut self.smoke {
            smoke.tick(&mut self.session, &ctx);
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        let ui_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.session.max_ui_ms = self.session.max_ui_ms.max(ui_ms);
        if self.session.ui_samples_ms.len() >= 600 {
            self.session.ui_samples_ms.pop_front();
        }
        self.session.ui_samples_ms.push_back(ui_ms);
        // Poll only while waiting for a requested decoded frame. Paused idle UI is event-driven.
        if self
            .session
            .frame_wait_until
            .is_some_and(|deadline| Instant::now() < deadline)
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(
            storage,
            "workspace",
            &SavedWorkspace {
                version: 1,
                workspace: self.workspace,
                dock: self.dock.clone(),
            },
        );
    }

    fn on_exit(&mut self) {
        if !self.session.send(Command::Shutdown) {
            eprintln!("Studio worker already stopped during shutdown");
        }
        if let Some(preview) = &self.session.preview {
            preview.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layouts_round_trip_without_losing_functional_panels()
    -> Result<(), Box<dyn std::error::Error>> {
        #[derive(Default)]
        struct MemoryStorage(std::collections::HashMap<String, String>);
        impl eframe::Storage for MemoryStorage {
            fn get_string(&self, key: &str) -> Option<String> {
                self.0.get(key).cloned()
            }
            fn set_string(&mut self, key: &str, value: String) {
                self.0.insert(key.into(), value);
            }
            fn remove_string(&mut self, key: &str) {
                self.0.remove(key);
            }
            fn flush(&mut self) {}
        }
        for workspace in [Workspace::Beginner, Workspace::Professional] {
            let dock = layout(workspace);
            let saved = SavedWorkspace {
                version: 1,
                workspace,
                dock,
            };
            let mut storage = MemoryStorage::default();
            eframe::set_value(&mut storage, "workspace", &saved);
            let decoded: SavedWorkspace =
                eframe::get_value(&storage, "workspace").ok_or("layout persistence failed")?;
            assert_eq!(decoded.version, 1);
            assert_eq!(decoded.dock.iter_all_tabs().count(), 9);
        }
        Ok(())
    }
}
