use crate::{
    CaptureState, Options,
    assets::Assets,
    backend::{Backend, Command, Message},
    capture::Capture,
    design,
    gpu::PreviewTexture,
    smoke::Smoke,
    timeline::{self, Timeline},
};
use egui::{Color32, RichText, Sense, Stroke, Vec2};
use egui_dock::{DockArea, DockState, TabViewer};
use lumeryn_core::{Batch, Edit, MediaTools, Project};
use lumeryn_preview::{PreviewConfig, PreviewPlayer};
use lumeryn_service::ToolConfig;
use serde_json::Value;
use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Settings,
    Activity,
    History,
    Diagnostics,
}
impl Panel {
    fn title(self) -> &'static str {
        match self {
            Self::Settings => "Project",
            Self::Activity => "Activity",
            Self::History => "History",
            Self::Diagnostics => "Diagnostics",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Picker {
    Open,
    Import,
    Export,
    Create,
}
struct Picked {
    epoch: u64,
    kind: Picker,
    path: Option<PathBuf>,
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
    assets: Option<Assets>,
    picker: Option<std::sync::mpsc::Receiver<Picked>>,
    pending_import: Option<(u64, PathBuf)>,
    selected_media: Option<Uuid>,
    recent_media: bool,
    monitor_muted: bool,
    volume: f32,
    pub viewport_size: Vec2,
    pub pixels_per_point: f32,
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
        let (preview, mut error) = match preview {
            Ok(preview) => (Some(preview), None),
            Err(error) => (None, Some(format!("Preview initialization: {error}"))),
        };
        if let Some(preview) = &preview
            && let Err(failure) = preview.set_volume(0.8)
        {
            error = Some(format!("Preview volume: {failure}"));
        }
        let assets = match Assets::new(&cc.egui_ctx, preview_tools.clone()) {
            Ok(assets) => Some(assets),
            Err(failure) => {
                error = Some(format!("Media previews: {failure}"));
                None
            }
        };
        Self {
            backend: Backend::start(cc.egui_ctx.clone(), tools),
            project: None,
            database: None,
            epoch: 1,
            project_input: options
                .project
                .as_ref()
                .map_or_else(String::new, |p| p.display().to_string()),
            media_input: String::new(),
            selected: None,
            trim_source: 0,
            trim_duration: 1,
            preview,
            preview_tools: preview_tools.clone(),
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
            assets,
            picker: None,
            pending_import: None,
            selected_media: None,
            recent_media: false,
            monitor_muted: false,
            volume: 0.8,
            viewport_size: Vec2::ZERO,
            pixels_per_point: 1.0,
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
        self.pending_import = None;
        self.project = None;
        self.last_frame = None;
        self.frame_wait_until = Some(Instant::now() + Duration::from_secs(30));
        if let Some(texture) = &mut self.texture {
            texture.invalidate();
        }
        self.selected = None;
        self.selected_media = None;
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

    pub fn assets_ready(&self) -> bool {
        self.project.as_ref().is_some_and(|project| {
            self.assets.as_ref().is_some_and(|assets| {
                project
                    .media
                    .iter()
                    .take(128)
                    .all(|media| !assets.pending(media.id))
            })
        })
    }
    pub fn asset_evidence(&self) -> Value {
        let mut thumbnails = 0;
        let mut waveforms = 0;
        let mut failures = 0;
        if let (Some(project), Some(assets)) = (&self.project, &self.assets) {
            for media in project.media.iter().take(128) {
                thumbnails += usize::from(assets.thumbnail(media.id).is_some());
                waveforms += usize::from(assets.waveform(media.id).is_some());
                failures += usize::from(assets.error(media.id).is_some());
            }
        }
        serde_json::json!({"thumbnails": thumbnails, "waveforms": waveforms, "failed_assets": failures, "complete": self.assets_ready()})
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let picked = self.picker.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(picked) = picked {
            self.picker = None;
            if let Some(path) = picked.path
                && picked.epoch == self.epoch
            {
                match picked.kind {
                    Picker::Open => self.open(path),
                    Picker::Import => self.pending_import = Some((self.epoch, path)),
                    Picker::Export => self.output_input = path.display().to_string(),
                    Picker::Create => {
                        self.edit_busy = self.send(Command::Create {
                            path,
                            epoch: self.epoch,
                        });
                    }
                }
            }
        }
        if !self.edit_busy
            && self.connected
            && let Some((epoch, path)) = self.pending_import.take()
            && epoch == self.epoch
        {
            self.edit(
                Edit::Import {
                    id: Uuid::new_v4(),
                    path,
                },
                "Import media",
            );
        }
        if let Some(assets) = &mut self.assets {
            assets.poll(ctx);
            if let (Some(project), Some(database)) = (&self.project, &self.database) {
                assets.sync(project, database);
            }
        }

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
                        self.playhead = self
                            .playhead
                            .min(timeline::total_frames(&project).saturating_sub(1));
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
                            self.selected = project
                                .clips
                                .iter()
                                .find(|clip| {
                                    clip.at_frame <= self.playhead
                                        && self.playhead
                                            < clip.at_frame.saturating_add(clip.duration_frames)
                                })
                                .or_else(|| project.clips.first())
                                .map(|clip| clip.id);
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
                    if label == "Create project"
                        && let Some(path) = value.as_str()
                    {
                        self.open(PathBuf::from(path));
                    }

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
        self.edit_batch(
            Batch {
                operations: vec![operation],
            },
            label,
        )
    }
    fn edit_batch(&mut self, batch: Batch, label: &str) -> bool {
        if !self.connected || self.edit_busy {
            return false;
        }
        let Some(project) = &self.project else {
            return false;
        };
        let revision = project.revision;
        if self.send(Command::Edit {
            batch,
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

    fn pick(&mut self, ctx: &egui::Context, kind: Picker) {
        if self.picker.is_some() {
            return;
        }
        #[cfg(windows)]
        {
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let epoch = self.epoch;
            let wake = ctx.clone();
            match std::thread::Builder::new()
                .name("lumeryn-file-dialog".into())
                .spawn(move || {
                    let dialog = rfd::FileDialog::new();
                    let path = match kind {
                        Picker::Open => dialog
                            .set_title("Open Lumeryn project")
                            .add_filter("Lumeryn project", &["lumeryn"])
                            .pick_file(),
                        Picker::Import => dialog
                            .set_title("Import media")
                            .add_filter(
                                "Media",
                                &["mp4", "mov", "mkv", "avi", "webm", "wav", "mp3", "flac"],
                            )
                            .pick_file(),
                        Picker::Export => dialog
                            .set_title("Choose a new export file")
                            .add_filter("MP4 video", &["mp4"])
                            .set_file_name("export.mp4")
                            .save_file(),
                        Picker::Create => dialog
                            .set_title("Create a new Lumeryn project")
                            .add_filter("Lumeryn project", &["lumeryn"])
                            .set_file_name("project.lumeryn")
                            .save_file(),
                    };
                    let _ = tx.send(Picked { epoch, kind, path });
                    wake.request_repaint();
                }) {
                Ok(_) => self.picker = Some(rx),
                Err(error) => self.fail(format!("Open file dialog: {error}")),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (ctx, kind);
            self.fail(
                "File dialogs are Windows-only. Use project tools or drop files into Studio."
                    .into(),
            );
        }
    }
    fn media_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (label, recent) in [("Media", false), ("Recent", true)] {
                let r =
                    ui.selectable_label(self.recent_media == recent, RichText::new(label).strong());
                if r.clicked() {
                    self.recent_media = recent;
                }
                if self.recent_media == recent {
                    ui.painter().line_segment(
                        [r.rect.left_bottom(), r.rect.right_bottom()],
                        Stroke::new(2.0, design::ACCENT),
                    );
                }
            }
        });
        ui.add_space(6.0);
        if design::primary(
            ui,
            "+  Import media",
            self.connected && !self.edit_busy && self.picker.is_none(),
            ui.available_width(),
        )
        .clicked()
        {
            self.pick(ui.ctx(), Picker::Import);
        }
        ui.add_space(10.0);
        let Some(project) = &self.project else {
            ui.label(RichText::new("Your media belongs here").strong());
            ui.label(
                RichText::new("Open a project, then import a video or drop it into this window.")
                    .color(design::MUTED),
            );
            return;
        };
        let mut add = None;
        let width = (ui.available_width() - 12.0) / 2.0;
        let media: Vec<_> = if self.recent_media {
            project.media.iter().rev().collect()
        } else {
            project.media.iter().collect()
        };
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height((ui.available_height() - 70.0).max(80.0))
            .show(ui, |ui| {
                for pair in media.chunks(2) {
                    ui.horizontal_top(|ui| {
                        for media in pair {
                            ui.vertical(|ui| {
                                ui.set_width(width);
                                let (rect, response) = ui.allocate_exact_size(
                                    Vec2::new(width, width * 9.0 / 16.0),
                                    Sense::click(),
                                );
                                ui.painter().rect_filled(rect, 4.0, design::BACKGROUND);
                                if let Some(texture) =
                                    self.assets.as_ref().and_then(|a| a.thumbnail(media.id))
                                {
                                    ui.painter().image(
                                        texture.id(),
                                        rect,
                                        unit_rect(),
                                        Color32::WHITE,
                                    );
                                } else {
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        if media.metadata.video.is_some() {
                                            "Video"
                                        } else {
                                            "Audio"
                                        },
                                        egui::FontId::proportional(13.0),
                                        design::MUTED,
                                    );
                                }
                                ui.painter().rect_stroke(
                                    rect,
                                    4.0,
                                    Stroke::new(
                                        if self.selected_media == Some(media.id) {
                                            2.0
                                        } else {
                                            1.0
                                        },
                                        if self.selected_media == Some(media.id) {
                                            design::ACCENT
                                        } else {
                                            design::BORDER
                                        },
                                    ),
                                    egui::StrokeKind::Inside,
                                );
                                if let Some(video) = &media.metadata.video {
                                    let seconds = video.frames as f64
                                        * f64::from(video.frame_rate.denominator())
                                        / f64::from(video.frame_rate.numerator());
                                    let badge = egui::Rect::from_min_size(
                                        rect.right_bottom() - Vec2::new(48.0, 24.0),
                                        Vec2::new(45.0, 21.0),
                                    );
                                    ui.painter().rect_filled(badge, 3.0, design::BACKGROUND);
                                    ui.painter().text(
                                        badge.center(),
                                        egui::Align2::CENTER_CENTER,
                                        timecode(seconds, false),
                                        egui::FontId::proportional(12.0),
                                        design::TEXT,
                                    );
                                    if response.double_clicked() {
                                        add = Some((media.id, video.frames));
                                    }
                                }
                                if response.clicked() {
                                    self.selected_media = Some(media.id);
                                }
                                response.on_hover_text(
                                    "Select media. Double-click a video to add it to the timeline.",
                                );
                                ui.add(egui::Label::new(media_name(media)).truncate())
                                    .on_hover_text(media.path.display().to_string());
                                if let Some(error) =
                                    self.assets.as_ref().and_then(|a| a.error(media.id))
                                {
                                    ui.label(
                                        RichText::new("Preview unavailable")
                                            .small()
                                            .color(design::WARNING),
                                    )
                                    .on_hover_text(error);
                                }
                            });
                        }
                    });
                    ui.add_space(8.0);
                }
                if project.media.is_empty() {
                    ui.label(RichText::new("No media yet").strong());
                    ui.label(
                        RichText::new("Import a video to start. Your source files stay unchanged.")
                            .color(design::MUTED),
                    );
                }
            });
        if let Some(media) = project
            .media
            .iter()
            .find(|m| Some(m.id) == self.selected_media)
            && let Some(video) = &media.metadata.video
            && ui
                .add_enabled(
                    self.connected && !self.edit_busy,
                    egui::Button::new("Add to timeline")
                        .min_size(Vec2::new(ui.available_width(), 36.0)),
                )
                .clicked()
        {
            add = Some((media.id, video.frames));
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
    fn update_volume(&mut self) {
        if let Some(preview) = &self.preview
            && let Err(error) =
                preview.set_volume(if self.monitor_muted { 0.0 } else { self.volume })
        {
            self.fail(format!("Preview volume: {error}"));
        }
    }
    fn quality_menu(&mut self, ui: &mut egui::Ui) {
        let previous = self.quality;
        egui::ComboBox::from_id_salt("preview-quality")
            .width(74.0)
            .selected_text(["360p", "720p", "1080p"][self.quality])
            .show_ui(ui, |ui| {
                for (index, label) in ["360p", "720p", "1080p"].iter().enumerate() {
                    ui.selectable_value(&mut self.quality, index, *label);
                }
            });
        if previous == self.quality {
            return;
        }
        let (max_width, max_height) = [(640, 360), (1280, 720), (1920, 1080)][self.quality];
        match PreviewPlayer::new(
            self.preview_tools.clone(),
            PreviewConfig {
                max_width,
                max_height,
                audio: true,
                ..Default::default()
            },
        ) {
            Ok(preview) => {
                if let Err(error) =
                    preview.set_volume(if self.monitor_muted { 0.0 } else { self.volume })
                {
                    self.fail(format!("Preview volume: {error}"));
                }
                self.last_frame = None;
                self.frame_wait_until = Some(Instant::now() + Duration::from_secs(30));
                if let Some(texture) = &mut self.texture {
                    texture.invalidate();
                }
                if let (Some(project), Some(database)) = (&self.project, &self.database) {
                    if let Err(error) = preview.load(project.clone(), database.clone()) {
                        self.error = Some(format!("Preview quality: {error}"));
                    }
                    self.pending_seek =
                        (self.playhead != 0).then_some((project.revision, self.playhead));
                }
                self.preview = Some(preview);
            }
            Err(error) => self.fail(format!("Preview quality: {error}")),
        }
    }
    fn preview_panel(&mut self, ui: &mut egui::Ui, expanded: &mut bool) {
        let size = Vec2::new(
            ui.available_width().max(40.0),
            (ui.available_height() - 98.0).max(40.0),
        );
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let current = self.last_frame.is_some_and(|(revision, _)| {
            self.project
                .as_ref()
                .is_some_and(|p| p.revision == revision)
        });
        let source = self.project.as_ref().map_or(Vec2::new(16.0, 9.0), |p| {
            Vec2::new(p.width as f32, p.height as f32)
        });
        let scale = (rect.width() / source.x).min(rect.height() / source.y);
        let image_rect = egui::Rect::from_center_size(rect.center(), source * scale);
        ui.painter()
            .rect_filled(image_rect, 2.0, design::BACKGROUND);
        if let Some((id, _)) = current
            .then(|| self.texture.as_ref().and_then(PreviewTexture::image))
            .flatten()
        {
            ui.painter()
                .image(id, image_rect, unit_rect(), Color32::WHITE);
        } else {
            ui.painter().text(
                image_rect.center(),
                egui::Align2::CENTER_CENTER,
                if self.project.as_ref().is_some_and(|p| !p.clips.is_empty()) {
                    "Loading your preview…"
                } else if self.project.is_some() {
                    "Your video preview"
                } else {
                    ""
                },
                egui::FontId::proportional(20.0),
                design::MUTED,
            );
        }
        if self.project.is_none() && self.project_input.is_empty() {
            let area = egui::Rect::from_center_size(image_rect.center(), Vec2::new(360.0, 180.0));
            let mut welcome = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt("welcome")
                    .max_rect(area)
                    .layout(egui::Layout::top_down(egui::Align::Center)),
            );
            welcome.label(
                RichText::new("Make something worth watching")
                    .size(20.0)
                    .strong(),
            );
            welcome.label(
                RichText::new("Start a local project or pick up your edit.").color(design::MUTED),
            );
            welcome.add_space(12.0);
            if design::primary(&mut welcome, "Open project", self.picker.is_none(), 200.0).clicked()
            {
                self.pick(ui.ctx(), Picker::Open);
            }
            if welcome
                .add_enabled(
                    self.picker.is_none(),
                    egui::Button::new("New project").min_size(Vec2::new(200.0, 36.0)),
                )
                .clicked()
            {
                self.pick(ui.ctx(), Picker::Create);
            }
        }
        ui.painter().rect_stroke(
            image_rect,
            2.0,
            Stroke::new(1.0, design::BORDER),
            egui::StrokeKind::Inside,
        );
        if self
            .project
            .as_ref()
            .is_none_or(|project| project.clips.is_empty())
        {
            return;
        }
        ui.add_space(10.0);
        let maximum = self
            .project
            .as_ref()
            .map_or(0, timeline::total_frames)
            .saturating_sub(1);
        let mut frame = self.playhead.min(maximum);
        ui.scope(|ui| {
            ui.spacing_mut().slider_width = ui.available_width() - 8.0;
            ui.spacing_mut().interact_size.y = 18.0;
            ui.visuals_mut().selection.bg_fill = design::ACCENT;
            if ui
                .add(
                    egui::Slider::new(&mut frame, 0..=maximum)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed()
            {
                self.seek(frame);
            }
        });
        ui.horizontal(|ui| {
            let playing = self.preview.as_ref().is_some_and(|p| p.status().playing);
            if design::icon_button(
                ui,
                if playing {
                    design::Icon::Pause
                } else {
                    design::Icon::Play
                },
                if playing {
                    "Pause - Space"
                } else {
                    "Play - Space"
                },
            )
            .clicked()
            {
                self.toggle_play();
            }
            if design::icon_button(ui, design::Icon::First, "Go to start").clicked() {
                self.seek(0);
            }
            if design::icon_button(ui, design::Icon::Previous, "Previous frame · Left arrow")
                .clicked()
            {
                self.step(-1);
            }
            if design::icon_button(ui, design::Icon::Next, "Next frame · Right arrow").clicked() {
                self.step(1);
            }
            let muted = self.monitor_muted || self.volume == 0.0;
            if design::icon_button(
                ui,
                if muted {
                    design::Icon::Muted
                } else {
                    design::Icon::Speaker
                },
                if muted {
                    "Unmute preview"
                } else {
                    "Mute preview"
                },
            )
            .clicked()
            {
                if muted {
                    self.monitor_muted = false;
                    if self.volume == 0.0 {
                        self.volume = 0.8;
                    }
                } else {
                    self.monitor_muted = true;
                }
                self.update_volume();
            }
            ui.scope(|ui| {
                ui.spacing_mut().slider_width = 88.0;
                ui.spacing_mut().slider_rail_height = 4.0;
                ui.spacing_mut().interact_size.y = 16.0;
                ui.visuals_mut().selection.bg_fill = design::ACCENT;
                if ui
                    .add(
                        egui::Slider::new(&mut self.volume, 0.0..=1.0)
                            .show_value(false)
                            .trailing_fill(true),
                    )
                    .on_hover_text("Preview volume · export audio stays unchanged")
                    .changed()
                {
                    self.monitor_muted = false;
                    self.update_volume();
                }
            });
            let fps = self.project.as_ref().map_or(30.0, |p| {
                f64::from(p.frame_rate.numerator()) / f64::from(p.frame_rate.denominator())
            });
            ui.label(format!(
                "{} / {}",
                timecode(self.playhead as f64 / fps, false),
                timecode((maximum + 1) as f64 / fps, false)
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if design::icon_button(ui, design::Icon::Expand, "Expand preview").clicked() {
                    *expanded = !*expanded;
                }
                self.quality_menu(ui);
            });
        });
    }
    fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Clip").strong().size(17.0));
        ui.add_space(8.0);
        let Some(project) = &self.project else {
            ui.label("Open a project to begin.");
            return;
        };
        let Some(clip) = project.clips.iter().find(|c| Some(c.id) == self.selected) else {
            ui.label(
                RichText::new("Select a clip in the timeline to adjust its trim.")
                    .color(design::MUTED),
            );
            return;
        };
        let id = clip.id;
        let media = project.media.iter().find(|m| m.id == clip.media_id);
        let width = ui.available_width();
        let thumbnail_height = if self.viewport_size.x < 1160.0 {
            width * 0.32
        } else {
            (width * 9.0 / 16.0).min(if self.viewport_size.y < 850.0 {
                100.0
            } else {
                180.0
            })
        };
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, thumbnail_height), Sense::hover());
        ui.painter().rect_filled(rect, 4.0, design::BACKGROUND);
        if let Some(texture) = self
            .assets
            .as_ref()
            .and_then(|a| a.thumbnail(clip.media_id))
        {
            let source = texture.size_vec2();
            let scale = (rect.width() / source.x).min(rect.height() / source.y);
            ui.painter().image(
                texture.id(),
                egui::Rect::from_center_size(rect.center(), source * scale),
                unit_rect(),
                Color32::WHITE,
            );
        }
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, design::BORDER),
            egui::StrokeKind::Inside,
        );
        let fps =
            f64::from(project.frame_rate.numerator()) / f64::from(project.frame_rate.denominator());
        ui.add(
            egui::Label::new(
                RichText::new(media.map_or_else(|| "Clip".into(), media_name)).strong(),
            )
            .truncate(),
        );
        ui.label(
            RichText::new(format!(
                "{} - source audio {}",
                timecode(clip.duration_frames as f64 / fps, false),
                if media.is_some_and(|m| m.metadata.audio.is_some()) {
                    "included"
                } else {
                    "none"
                }
            ))
            .color(design::MUTED)
            .small(),
        );
        ui.add_space(12.0);
        let maximum = media
            .and_then(|m| m.metadata.video.as_ref())
            .map_or(lumeryn_core::model::MAX_FRAMES, |v| v.frames);
        ui.label(RichText::new("Source range").small().color(design::MUTED));
        let mut start = self.trim_source as f64 / fps;
        let mut end = (self.trim_source + self.trim_duration) as f64 / fps;
        egui::Grid::new("clip-times")
            .num_columns(2)
            .spacing([12.0, 14.0])
            .show(ui, |ui| {
                ui.label("Start time");
                let sr = ui.add(
                    egui::DragValue::new(&mut start)
                        .speed(1.0 / fps)
                        .range(0.0..=maximum.saturating_sub(1) as f64 / fps)
                        .custom_formatter(|v, _| timecode(v, true))
                        .custom_parser(parse_timecode),
                );
                ui.end_row();
                ui.label("End time");
                let er = ui.add(
                    egui::DragValue::new(&mut end)
                        .speed(1.0 / fps)
                        .range(1.0 / fps..=maximum as f64 / fps)
                        .custom_formatter(|v, _| timecode(v, true))
                        .custom_parser(parse_timecode),
                );
                ui.end_row();
                if sr.changed() || er.changed() {
                    self.trim_source =
                        ((start * fps).round() as u64).min(maximum.saturating_sub(1));
                    let end_frame =
                        ((end * fps).round() as u64).clamp(self.trim_source + 1, maximum);
                    self.trim_duration = end_frame - self.trim_source;
                }
                ui.label("Length");
                ui.label(timecode(self.trim_duration as f64 / fps, true));
                ui.end_row();
            });
        ui.add_space(14.0);
        let apply =
            design::primary(ui, "Apply trim", self.connected && !self.edit_busy, width).clicked();
        ui.add_space(12.0);
        ui.separator();
        ui.collapsing("Clip details", |ui| {
            ui.label(
                RichText::new(
                    "Trims change the source range. Your original media stays unchanged.",
                )
                .color(design::MUTED),
            );
            if let Some(media) = media
                && let Some(video) = &media.metadata.video
            {
                ui.label(format!(
                    "{} x {} - {} fps",
                    video.width, video.height, video.frame_rate
                ));
            }
        });
        if apply {
            self.trim(id, self.trim_source, self.trim_duration);
        }
    }
    fn timeline_panel(&mut self, ui: &mut egui::Ui) {
        if let (Some(project), Some(assets)) = (&self.project, &self.assets) {
            let actions = self
                .timeline
                .show(ui, project, assets, self.selected, self.playhead);
            for action in actions {
                match action {
                    timeline::Action::Select(id) => self.select(id),
                    timeline::Action::Seek(frame) => self.seek(frame),
                    timeline::Action::Trim { clip, duration } => {
                        let source = self
                            .project
                            .as_ref()
                            .and_then(|p| p.clips.iter().find(|c| c.id == clip))
                            .map_or(0, |c| c.source_in_frame);
                        self.trim(clip, source, duration);
                    }
                }
            }
        } else {
            design::section(
                ui,
                "Your timeline",
                "Import a video and add it here to start editing.",
            );
        }
    }
    fn settings(&mut self, ui: &mut egui::Ui) {
        design::section(
            ui,
            "Project details",
            "Studio and external agents edit this same local project.",
        );
        if let Some(project) = &self.project {
            ui.label(&project.name);
            ui.label(format!(
                "{} x {} - {} fps - revision {}",
                project.width, project.height, project.frame_rate, project.revision
            ));
            if let Some(path) = &self.database {
                ui.label(path.display().to_string());
            }
        }
        ui.separator();
        ui.label("Open an existing project by path");
        ui.add(egui::TextEdit::singleline(&mut self.project_input).desired_width(f32::INFINITY));
        if ui.button("Open project").clicked() && !self.project_input.trim().is_empty() {
            self.open(self.project_input.trim().into());
        }
        ui.separator();
        ui.label("Import media by path");
        ui.add(egui::TextEdit::singleline(&mut self.media_input).desired_width(f32::INFINITY));
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
    }
    fn render_panel(&mut self, ui: &mut egui::Ui) {
        design::section(
            ui,
            "Export your video",
            "MP4 video - MPEG-4 + AAC audio - verified before completion",
        );
        if let Some(project) = &self.project {
            ui.label(format!(
                "{} x {} - {} fps",
                project.width, project.height, project.frame_rate
            ));
        }
        ui.add_space(8.0);
        ui.label("Save to a new file");
        ui.add(egui::TextEdit::singleline(&mut self.output_input).desired_width(f32::INFINITY));
        if ui
            .add_enabled(
                self.picker.is_none(),
                egui::Button::new("Choose location..."),
            )
            .clicked()
        {
            self.pick(ui.ctx(), Picker::Export);
        }
        ui.add_space(8.0);
        if design::primary(
            ui,
            if self.render_busy {
                "Exporting..."
            } else {
                "Export video"
            },
            self.connected && !self.render_busy && !self.output_input.trim().is_empty(),
            ui.available_width(),
        )
        .clicked()
        {
            self.export(self.output_input.trim().into());
        }
        if self.render_busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Rendering in the background");
            });
            ui.label(
                RichText::new(
                    "You can keep editing. This export uses the revision you started with.",
                )
                .color(design::MUTED),
            );
            if ui.button("Cancel export").clicked()
                && let Some(job_id) = self.active_job
            {
                self.send(Command::Cancel { job_id });
            }
        } else if self.last_complete.as_deref() == Some("Export") {
            ui.label(
                RichText::new("Export complete - output decoded and verified")
                    .color(design::ACCENT),
            );
        }
        for event in self
            .events
            .iter()
            .rev()
            .filter(|e| e.kind.starts_with("job_"))
            .take(6)
        {
            ui.label(job_label(&event.kind));
        }
    }
    fn activity(&mut self, ui: &mut egui::Ui) {
        design::section(
            ui,
            "Project activity",
            "Changes from Studio and your external agents appear here.",
        );
        egui::ScrollArea::vertical().show(ui, |ui| {
            for event in self.events.iter().rev() {
                ui.label(format!(
                    "{}{}",
                    job_label(&event.kind),
                    event
                        .revision
                        .map_or(String::new(), |r| format!(" - revision {r}"))
                ));
                ui.separator();
            }
            if self.events.is_empty() {
                ui.label("No recent activity.");
            }
        });
    }
    fn history_panel(&mut self, ui: &mut egui::Ui) {
        design::section(
            ui,
            "Edit history",
            "Committed changes. Undo and redo are not available yet.",
        );
        if ui.button("Refresh").clicked() {
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
                if let Some(operations) = entry.get("operations").and_then(Value::as_array) {
                    for operation in operations {
                        ui.label(match operation.get("operation").and_then(Value::as_str) {
                            Some("import") => "Imported media",
                            Some("add_clip") => "Added a clip to the timeline",
                            Some("trim_clip") => "Adjusted a clip's trim",
                            _ => "Project updated",
                        });
                    }
                }
                ui.push_id(
                    entry.get("revision").and_then(Value::as_u64).unwrap_or(0),
                    |ui| {
                        ui.collapsing("Technical details", |ui| {
                            ui.label(RichText::new(entry.to_string()).monospace().small());
                        });
                    },
                );
                ui.separator();
            }
        });
    }
    fn diagnostics(&mut self, ui: &mut egui::Ui) {
        design::section(
            ui,
            "Diagnostics",
            "Measured locally. GPU memory usage is unknown.",
        );
        ui.label(&self.connection);
        if let Some(texture) = &self.texture {
            ui.label(&texture.adapter);
            ui.label(format!(
                "{} uploads - latest submission {:.3} ms",
                texture.uploads, texture.upload_cpu_ms
            ));
        }
        ui.label(format!(
            "{} UI updates - max {:.3} ms - {} visible clips",
            self.ui_frames, self.max_ui_ms, self.timeline.visible_clips
        ));
        if let Some(preview) = &self.preview {
            let status = preview.status();
            ui.label(format!(
                "Decoded {} - dropped {} - buffered {}",
                status.decoded_frames, status.dropped_frames, status.buffered_frames
            ));
            ui.label(format!(
                "Decode {:.2} ms - seek {:.2} ms - audio {:?}",
                status.decode_time_ms, status.seek_latency_ms, status.audio
            ));
        }
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for log in self.logs.iter().rev() {
                ui.label(RichText::new(log).monospace().small());
            }
        });
    }
}
fn unit_rect() -> egui::Rect {
    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0))
}
fn media_name(media: &lumeryn_core::model::Media) -> String {
    media
        .path
        .file_stem()
        .map_or_else(|| "Media".into(), |n| n.to_string_lossy().replace('_', " "))
}
fn timecode(seconds: f64, precise: bool) -> String {
    let cs = (seconds.max(0.0) * 100.0).round() as u64;
    let min = cs / 6000;
    let sec = cs / 100 % 60;
    if precise {
        format!("{min:02}:{sec:02}.{:02}", cs % 100)
    } else {
        format!("{min:02}:{sec:02}")
    }
}
fn parse_timecode(text: &str) -> Option<f64> {
    let v = if let Some((min, sec)) = text.split_once(':') {
        let min = min.trim().parse::<u64>().ok()?;
        let sec = sec.trim().parse::<f64>().ok()?;
        if !(0.0..60.0).contains(&sec) {
            return None;
        }
        min as f64 * 60.0 + sec
    } else {
        text.trim().parse().ok()?
    };
    (v.is_finite() && v >= 0.0).then_some(v)
}
fn job_label(kind: &str) -> &str {
    match kind {
        "job_queued" => "Export queued",
        "job_running" => "Export running",
        "job_completed" => "Export completed",
        "job_failed" => "Export failed",
        "job_cancelled" => "Export cancelled",
        "revision_committed" | "project_committed" => "Project updated",
        other => other,
    }
}
struct Viewer<'a> {
    session: &'a mut Session,
}
impl TabViewer for Viewer<'_> {
    type Tab = Panel;
    fn id(&mut self, tab: &mut Panel) -> egui::Id {
        egui::Id::new(tab.title())
    }
    fn title(&mut self, tab: &mut Panel) -> egui::WidgetText {
        tab.title().into()
    }
    fn closeable(&mut self, _: &mut Panel) -> bool {
        false
    }
    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Panel) {
        match tab {
            Panel::Settings => self.session.settings(ui),
            Panel::Activity => self.session.activity(ui),
            Panel::History => self.session.history_panel(ui),
            Panel::Diagnostics => self.session.diagnostics(ui),
        }
    }
}

pub struct Studio {
    session: Session,
    tools: DockState<Panel>,
    tools_open: bool,
    export_open: bool,
    inspector_open: bool,
    expanded: bool,
    smoke: Option<Smoke>,
    capture: Option<Capture>,
    capture_frame: u64,
    capture_state: CaptureState,
    capture_seek_revision: Option<u64>,
}
impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>, options: Options) -> Self {
        design::configure(&cc.egui_ctx);
        if options.capture_only || options.smoke {
            cc.egui_ctx.style_mut_of(egui::Theme::Dark, |style| {
                style.interaction.tooltip_delay = 3600.0;
                style.interaction.tooltip_grace_time = 0.0;
            });
        }
        let mut session = Session::new(cc, &options);
        if let Some(path) = &options.project {
            session.open(path.clone());
            session.playhead = options.capture_frame;
        }
        let smoke = if options.smoke {
            options.artifact_dir.clone().map(Smoke::new)
        } else {
            None
        };
        let capture = if options.capture_only {
            options.artifact_dir.map(Capture::new)
        } else {
            None
        };
        Self {
            session,
            tools: DockState::new(vec![
                Panel::History,
                Panel::Activity,
                Panel::Settings,
                Panel::Diagnostics,
            ]),
            tools_open: options.capture_only && options.capture_state == CaptureState::Tools,
            export_open: options.capture_only && options.capture_state == CaptureState::Export,
            inspector_open: options.capture_only
                && options.capture_state == CaptureState::Inspector,
            expanded: false,
            smoke,
            capture,
            capture_frame: options.capture_frame,
            capture_state: options.capture_state,
            capture_seek_revision: None,
        }
    }
    fn header(&mut self, ui: &mut egui::Ui) {
        let compact = ui.available_width() < 1160.0;
        ui.horizontal(|ui| {
            design::logo(ui);
            ui.label(RichText::new("L U M E R Y N").strong().size(19.0));
            ui.add_space(12.0);
            ui.separator();
            let name = self
                .session
                .project
                .as_ref()
                .map_or("Open a project", |p| p.name.as_str())
                .to_owned();
            let title = if name.chars().count() > 25 {
                format!("{}…", name.chars().take(24).collect::<String>())
            } else {
                name.clone()
            };
            let menu = ui
                .menu_button(RichText::new(format!("{title}   ")).strong(), |ui| {
                    if ui.button("Open project…").clicked() {
                        self.session.pick(ui.ctx(), Picker::Open);
                        ui.close();
                    }
                    if ui.button("New project…").clicked() {
                        self.session.pick(ui.ctx(), Picker::Create);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Project details").clicked() {
                        self.tools_open = true;
                        self.tools = DockState::new(vec![
                            Panel::Settings,
                            Panel::History,
                            Panel::Activity,
                            Panel::Diagnostics,
                        ]);
                        ui.close();
                    }
                })
                .response;
            let caret = menu.rect.right_center() - Vec2::new(12.0, 0.0);
            ui.painter().line_segment(
                [caret + Vec2::new(-3.0, -2.0), caret + Vec2::new(0.0, 2.0)],
                Stroke::new(1.3, design::TEXT),
            );
            ui.painter().line_segment(
                [caret + Vec2::new(0.0, 2.0), caret + Vec2::new(3.0, -2.0)],
                Stroke::new(1.3, design::TEXT),
            );
            menu.on_hover_text(&name);
            if self.session.edit_busy {
                ui.spinner();
                ui.label(RichText::new("Saving…").small().color(design::MUTED));
            } else if self.session.connected {
                ui.label(RichText::new("Saved").small().color(design::MUTED))
                    .on_hover_text(&self.session.connection);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if design::primary(
                    ui,
                    if self.session.render_busy {
                        "Exporting…"
                    } else {
                        "Export"
                    },
                    self.session
                        .project
                        .as_ref()
                        .is_some_and(|p| !p.clips.is_empty()),
                    112.0,
                )
                .clicked()
                {
                    self.export_open = true;
                }
                ui.menu_button("•••", |ui| {
                    if ui.button("History & project tools").clicked() {
                        self.tools_open = true;
                        ui.close();
                    }
                    if ui.button("Open project…").clicked() {
                        self.session.pick(ui.ctx(), Picker::Open);
                        ui.close();
                    }
                    if ui.button("New project…").clicked() {
                        self.session.pick(ui.ctx(), Picker::Create);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Reset view").clicked() {
                        self.expanded = false;
                        self.inspector_open = false;
                        self.session.timeline = Timeline::default();
                        ui.close();
                    }
                    ui.label(
                        RichText::new(
                            "Space: play / pause\n← →: step one frame\nDrop files to import",
                        )
                        .small()
                        .color(design::MUTED),
                    );
                });
                if compact && ui.button("Clip").clicked() {
                    self.inspector_open = !self.inspector_open;
                }
            });
        });
    }
}
fn pane(
    ui: &mut egui::Ui,
    id: &str,
    rect: egui::Rect,
    margin: i8,
    draw: impl FnOnce(&mut egui::Ui),
) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    egui::Frame::new()
        .fill(design::PANEL)
        .stroke(Stroke::new(1.0, design::BORDER))
        .inner_margin(margin)
        .show(&mut child, |ui| {
            ui.set_min_size(rect.size() - Vec2::splat(2.0 * f32::from(margin) + 2.0));
            draw(ui);
        });
}
impl eframe::App for Studio {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let started = Instant::now();
        let ctx = ui.ctx().clone();
        self.session.viewport_size = ctx.content_rect().size();
        self.session.pixels_per_point = ctx.pixels_per_point();
        self.session.ui_frames += 1;
        self.session.poll(&ctx);
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        if !dropped.is_empty() {
            let paths = dropped
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect::<Vec<_>>();
            let projects = paths
                .iter()
                .filter(|path| path.extension().is_some_and(|ext| ext == "lumeryn"))
                .count();
            if projects == 1 && paths.len() == 1 {
                if let Some(path) = paths.into_iter().next() {
                    self.session.open(path);
                }
            } else if projects != 0 {
                self.session
                    .fail("Drop one project at a time. Open it before importing media.".into());
            } else {
                let batch = Batch {
                    operations: paths
                        .into_iter()
                        .map(|path| Edit::Import {
                            id: Uuid::new_v4(),
                            path,
                        })
                        .collect(),
                };
                if !self.session.edit_batch(batch, "Import media") {
                    self.session.fail(
                        "Open a project and wait for the current edit before importing files."
                            .into(),
                    );
                }
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
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.expanded = false;
            }
        }
        let mut bounds = ui.available_rect_before_wrap();
        let header = egui::Rect::from_min_size(bounds.min, Vec2::new(bounds.width(), 58.0));
        pane(ui, "header", header, 8, |ui| self.header(ui));
        bounds.min.y = header.bottom();
        if let Some(error) = self.session.error.clone() {
            let r = egui::Rect::from_min_size(bounds.min, Vec2::new(bounds.width(), 44.0));
            pane(ui, "error", r, 4, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Dismiss").clicked() {
                        self.session.error = None;
                    }
                    ui.add(
                        egui::Label::new(RichText::new(&error).color(design::WARNING)).truncate(),
                    )
                    .on_hover_text(&error);
                });
            });
            bounds.min.y = r.bottom();
        }
        let th = (bounds.height() * 0.30).clamp(240.0, 288.0);
        let upper =
            egui::Rect::from_min_max(bounds.min, egui::pos2(bounds.right(), bounds.bottom() - th));
        let bottom =
            egui::Rect::from_min_max(egui::pos2(bounds.left(), upper.bottom()), bounds.max);
        let compact = bounds.width() < 1160.0;
        if self.expanded {
            pane(ui, "preview", upper, 20, |ui| {
                self.session.preview_panel(ui, &mut self.expanded)
            });
        } else {
            let lw = (bounds.width() * 0.21).clamp(220.0, 330.0);
            let rw = if compact {
                0.0
            } else {
                (bounds.width() * 0.21).clamp(260.0, 330.0)
            };
            let left =
                egui::Rect::from_min_max(upper.min, egui::pos2(upper.left() + lw, upper.bottom()));
            let right =
                egui::Rect::from_min_max(egui::pos2(upper.right() - rw, upper.top()), upper.max);
            let center = egui::Rect::from_min_max(
                egui::pos2(left.right(), upper.top()),
                egui::pos2(right.left(), upper.bottom()),
            );
            pane(ui, "media", left, 18, |ui| self.session.media_panel(ui));
            pane(ui, "preview", center, 18, |ui| {
                self.session.preview_panel(ui, &mut self.expanded)
            });
            if !compact {
                pane(ui, "inspector", right, 18, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.session.inspector(ui));
                });
            }
        }
        pane(ui, "timeline", bottom, 12, |ui| {
            self.session.timeline_panel(ui)
        });
        ui.advance_cursor_after_rect(bounds);
        if self.session.project.is_none() && !self.session.project_input.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.export_open {
            egui::Window::new("Export video")
                .open(&mut self.export_open)
                .collapsible(false)
                .resizable(false)
                .default_width(460.0)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(&ctx, |ui| self.session.render_panel(ui));
        }
        if compact && self.inspector_open {
            egui::Window::new("Clip inspector")
                .open(&mut self.inspector_open)
                .default_size(Vec2::new(
                    300.0,
                    (ctx.content_rect().height() - 100.0).clamp(340.0, 560.0),
                ))
                .anchor(egui::Align2::RIGHT_TOP, Vec2::new(-12.0, 70.0))
                .max_height((ctx.content_rect().height() - 85.0).max(180.0))
                .collapsible(false)
                .show(&ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.session.inspector(ui));
                });
        }
        if self.tools_open {
            egui::Window::new("Project tools")
                .open(&mut self.tools_open)
                .default_size(Vec2::new(700.0, 460.0))
                .show(&ctx, |ui| {
                    DockArea::new(&mut self.tools)
                        .show_close_buttons(false)
                        .show_leaf_close_all_buttons(false)
                        .show_leaf_collapse_buttons(false)
                        .show_inside(
                            ui,
                            &mut Viewer {
                                session: &mut self.session,
                            },
                        );
                });
        }
        if self.capture.is_some() {
            let ready = if self.capture_state == CaptureState::Empty {
                self.session.project.is_none()
            } else if let Some(project) = &self.session.project {
                let revision = project.revision;
                let target = self
                    .capture_frame
                    .min(timeline::total_frames(project).saturating_sub(1));
                let clip = project
                    .clips
                    .iter()
                    .find(|c| c.at_frame <= target && target < c.at_frame + c.duration_frames)
                    .map(|c| c.id);
                if self.capture_seek_revision != Some(revision) && self.session.last_frame.is_some()
                {
                    self.session.seek(target);
                    if let Some(id) = clip {
                        self.session.select(id);
                    }
                    self.capture_seek_revision = Some(revision);
                }
                self.capture_seek_revision == Some(revision)
                    && self.session.last_frame == Some((revision, target))
            } else {
                false
            };
            if let Some(capture) = &mut self.capture {
                capture.tick(&mut self.session, &ctx, ready);
            }
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if let Some(smoke) = &mut self.smoke {
            smoke.tick(&mut self.session, &ctx);
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        self.session.max_ui_ms = self.session.max_ui_ms.max(ms);
        if self.session.ui_samples_ms.len() >= 600 {
            self.session.ui_samples_ms.pop_front();
        }
        self.session.ui_samples_ms.push_back(ms);
        if self
            .session
            .frame_wait_until
            .is_some_and(|d| Instant::now() < d)
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
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
    fn timecode_rounds_and_validates() {
        assert_eq!(timecode(59.999, true), "01:00.00");
        assert_eq!(parse_timecode("01:14.25"), Some(74.25));
        for invalid in ["-1", "NaN", "inf", "01:60", "1:2:3", ""] {
            assert_eq!(parse_timecode(invalid), None);
        }
    }
}
