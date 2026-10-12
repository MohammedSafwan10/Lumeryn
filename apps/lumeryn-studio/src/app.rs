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
use lumeryn_core::render::{
    Acceleration, RenderOptions, RenderQuality, RenderResolution, VideoCodec,
};
use lumeryn_core::{Batch, Edit, MediaTools, Project};
use lumeryn_preview::{PreviewConfig, PreviewPlayer};
use lumeryn_service::{ImportJob, ImportSource, ImportStage, ToolConfig};
use serde_json::Value;
use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub(crate) struct AudioEdit {
    pub clip: Uuid,
    pub at_frame: u64,
    pub source_in_sample: u64,
    pub duration_frames: u64,
    pub gain_millidecibels: i32,
    pub muted: bool,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeftDrawer {
    Media,
    Library,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RightDrawer {
    Clip,
    Project,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RailSide {
    Left,
    Right,
}
#[derive(Debug, Clone, Copy)]
struct Drawers {
    left: Option<LeftDrawer>,
    right: Option<RightDrawer>,
    active: RailSide,
}
impl Default for Drawers {
    fn default() -> Self {
        Self {
            left: Some(LeftDrawer::Media),
            right: Some(RightDrawer::Clip),
            active: RailSide::Left,
        }
    }
}
impl Drawers {
    fn visible(self, compact: bool) -> (Option<LeftDrawer>, Option<RightDrawer>) {
        if compact {
            match self.active {
                RailSide::Left => (self.left, None),
                RailSide::Right => (None, self.right),
            }
        } else {
            (self.left, self.right)
        }
    }
    fn toggle_left(&mut self, drawer: LeftDrawer, compact: bool) {
        self.left = if self.visible(compact).0 == Some(drawer) {
            None
        } else {
            Some(drawer)
        };
        self.active = RailSide::Left;
    }
    fn toggle_right(&mut self, drawer: RightDrawer, compact: bool) {
        self.right = if self.visible(compact).1 == Some(drawer) {
            None
        } else {
            Some(drawer)
        };
        self.active = RailSide::Right;
    }
}

#[derive(Debug)]
struct EditorLayout {
    left_rail: egui::Rect,
    right_rail: egui::Rect,
    left: Option<egui::Rect>,
    right: Option<egui::Rect>,
    preview: egui::Rect,
    timeline: egui::Rect,
}
impl EditorLayout {
    fn with_visual_rows(mut self, rows: usize) -> Self {
        if rows > 0 {
            let available = self.preview.height() + self.timeline.height();
            let height = (96.0 + rows as f32 * 60.0)
                .min(available * 0.50)
                .max(self.timeline.height());
            let top = self.timeline.bottom() - height;
            self.preview.max.y = top;
            self.timeline.min.y = top;
        }
        self
    }
    fn new(bounds: egui::Rect, drawers: Drawers) -> Self {
        let rail_width = 68.0_f32.min(bounds.width() / 4.0);
        let left_rail = egui::Rect::from_min_max(
            bounds.min,
            egui::pos2(bounds.left() + rail_width, bounds.bottom()),
        );
        let right_rail = egui::Rect::from_min_max(
            egui::pos2(bounds.right() - rail_width, bounds.top()),
            bounds.max,
        );
        let (left, right) = drawers.visible(bounds.width() < 1160.0);
        let available = (bounds.width() - 2.0 * rail_width).max(0.0);
        let left_width = if left.is_some() {
            260.0_f32.min(available * 0.4)
        } else {
            0.0
        };
        let right_width = if right.is_some() {
            280.0_f32.min(available * 0.4)
        } else {
            0.0
        };
        let left = left.map(|_| {
            egui::Rect::from_min_max(
                left_rail.right_top(),
                egui::pos2(left_rail.right() + left_width, bounds.bottom()),
            )
        });
        let right = right.map(|_| {
            egui::Rect::from_min_max(
                egui::pos2(right_rail.left() - right_width, bounds.top()),
                right_rail.left_bottom(),
            )
        });
        let stage = egui::Rect::from_min_max(
            egui::pos2(left_rail.right() + left_width, bounds.top()),
            egui::pos2(right_rail.left() - right_width, bounds.bottom()),
        );
        let timeline_height = (bounds.height() * 0.30)
            .clamp(220.0, 264.0)
            .min(bounds.height() * 0.45);
        let preview = egui::Rect::from_min_max(
            stage.min,
            egui::pos2(stage.right(), stage.bottom() - timeline_height),
        );
        let timeline = egui::Rect::from_min_max(preview.left_bottom(), stage.max);
        Self {
            left_rail,
            right_rail,
            left,
            right,
            preview,
            timeline,
        }
    }
}

fn seek_seconds(
    position: u64,
    rate: lumeryn_core::time::FrameRate,
    seconds: i64,
    total: u64,
) -> u64 {
    let frames = (u128::from(seconds.unsigned_abs()) * u128::from(rate.numerator())
        + u128::from(rate.denominator()) / 2)
        / u128::from(rate.denominator());
    let delta = frames.min(u128::from(u64::MAX)) as u64;
    let position = if seconds < 0 {
        position.saturating_sub(delta)
    } else {
        position.saturating_add(delta)
    };
    position.min(total.saturating_sub(1))
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

struct MediaRename {
    id: Uuid,
    text: String,
}

struct ImportCard {
    media_id: Uuid,
    name: String,
    job_id: Option<Uuid>,
    stage: ImportStage,
    error: Option<String>,
    elapsed_ms: u64,
    quick: Option<lumeryn_core::media::QuickMetadata>,
}

pub struct Session {
    visual_form: Option<VisualForm>,
    target_layer: Option<Uuid>,
    backend: Backend,
    pub project: Option<Project>,
    pub database: Option<PathBuf>,
    epoch: u64,
    project_input: String,
    media_input: String,
    pub selected: Option<Uuid>,
    trim_source: u64,
    trim_duration: u64,
    audio_gain_db: f32,
    audio_muted: bool,
    audio_at_frame: u64,
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
    history_state: Option<lumeryn_core::storage::HistoryState>,
    pub events: VecDeque<lumeryn_service::Event>,
    pub(crate) edit_busy: bool,
    pub render_busy: bool,
    active_job: Option<Uuid>,
    output_input: String,
    export_name: String,
    render_options: RenderOptions,
    pub last_complete: Option<String>,
    pub last_render: Option<Value>,
    pub artifacts: Vec<PathBuf>,
    pub ui_frames: u64,
    pub max_ui_ms: f64,
    pub ui_samples_ms: VecDeque<f64>,
    frame_wait_until: Option<Instant>,
    pub timeline: Timeline,
    assets: Option<Assets>,
    library: crate::library::Library,
    picker: Option<std::sync::mpsc::Receiver<Picked>>,
    pending_import: Option<(u64, PathBuf)>,
    imports: Vec<ImportCard>,
    import_paths: Vec<PathBuf>,
    completed_imports: VecDeque<Uuid>,
    match_first_video: bool,
    selected_media: Option<Uuid>,
    media_rename: Option<MediaRename>,
    recent_media: bool,
    monitor_muted: bool,
    volume: f32,
    pub viewport_size: Vec2,
    pub pixels_per_point: f32,
    pub native_fullscreen: Option<bool>,
}

struct VisualForm {
    clip: Uuid,
    revision: u64,
    at: u64,
    duration: u64,
    source: u64,
    custom: bool,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
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
            visual_form: None,
            target_layer: None,
            trim_source: 0,
            trim_duration: 1,
            audio_gain_db: 0.0,
            audio_muted: false,
            audio_at_frame: 0,
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
            history_state: None,
            events: VecDeque::new(),
            edit_busy: false,
            render_busy: false,
            active_job: None,
            output_input: String::new(),
            export_name: "export.mp4".into(),
            render_options: if options.smoke_uhd {
                RenderOptions {
                    resolution: RenderResolution::Uhd2160,
                    fps: Some(60),
                    codec: VideoCodec::H264,
                    acceleration: Acceleration::Auto,
                    quality: RenderQuality::Balanced,
                    ..Default::default()
                }
            } else if options.smoke {
                RenderOptions::default()
            } else {
                RenderOptions {
                    resolution: RenderResolution::Fhd1080,
                    codec: VideoCodec::H264,
                    acceleration: Acceleration::Auto,
                    ..Default::default()
                }
            },
            last_complete: None,
            last_render: None,
            artifacts: Vec::new(),
            ui_frames: 0,
            max_ui_ms: 0.0,
            ui_samples_ms: VecDeque::new(),
            frame_wait_until: None,
            timeline: Timeline::default(),
            assets,
            library: crate::library::Library::new(
                cc.egui_ctx.clone(),
                options.library.clone(),
                preview_tools.clone(),
            ),
            picker: None,
            pending_import: None,
            imports: Vec::new(),
            import_paths: Vec::new(),
            completed_imports: VecDeque::new(),
            match_first_video: true,
            selected_media: None,
            media_rename: None,
            recent_media: false,
            monitor_muted: false,
            volume: 0.8,
            viewport_size: Vec2::ZERO,
            pixels_per_point: 1.0,
            native_fullscreen: None,
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
        self.imports.clear();
        self.import_paths.clear();
        self.completed_imports.clear();
        self.match_first_video = true;
        self.project = None;
        self.last_frame = None;
        self.frame_wait_until = Some(Instant::now() + Duration::from_secs(30));
        if let Some(texture) = &mut self.texture {
            texture.invalidate();
        }
        self.selected = None;
        self.selected_media = None;
        self.media_rename = None;
        self.export_name = "export.mp4".into();
        self.connected = false;
        self.playhead = 0;
        self.history.clear();
        self.history_state = None;
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
        for (path, epoch) in self.library.poll(ctx) {
            if epoch == self.epoch {
                if self.pending_import.is_none() {
                    self.pending_import = Some((epoch, path));
                } else {
                    self.fail(
                        "An import is pending. Try the library import again after it finishes."
                            .into(),
                    );
                }
            }
        }
        let picked = self.picker.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(picked) = picked {
            self.picker = None;
            if let Some(path) = picked.path
                && picked.epoch == self.epoch
            {
                match picked.kind {
                    Picker::Open => self.open(path),
                    Picker::Import => self.pending_import = Some((self.epoch, path)),
                    Picker::Export => {
                        self.export_name = path.file_name().map_or_else(
                            || "export.mp4".into(),
                            |name| name.to_string_lossy().into_owned(),
                        );
                        self.output_input = path.display().to_string();
                    }
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
            self.import_media(vec![path]);
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
                    if self.project.is_none() {
                        self.match_first_video = project.revision == 0;
                    }
                    self.imports.retain(|card| {
                        !project.media.iter().any(|media| media.id == card.media_id)
                    });
                    let changed = self.project.as_ref().is_none_or(|p| {
                        p.revision != project.revision || p.schema_version != project.schema_version
                    });
                    self.connected = true;
                    self.connection = "Connected · shared project service".into();
                    if changed {
                        egui::DragAndDrop::clear_payload(ctx);
                        self.timeline.cancel_gesture();
                        if self.media_rename.take().is_some() {
                            self.fail(
                                "The project changed. Reopen Rename to use its latest revision."
                                    .into(),
                            );
                        }
                        if self
                            .selected_media
                            .is_some_and(|id| !project.media.iter().any(|media| media.id == id))
                        {
                            self.selected_media = None;
                        }
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
                                self.frame_wait_until = (project.visual_duration_frames() > 0)
                                    .then(|| Instant::now() + Duration::from_secs(30));
                                self.pending_seek = (self.playhead != 0)
                                    .then_some((project.revision, self.playhead));
                                if was_playing && let Err(error) = preview.play() {
                                    self.fail(format!("Play: {error}"));
                                }
                            }
                        }
                        if self.selected.is_none_or(|id| {
                            !project.clips.iter().any(|clip| clip.id == id)
                                && !project.audio_clips.iter().any(|clip| clip.id == id)
                                && !project.visual_clips.iter().any(|clip| clip.id == id)
                        }) {
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
                        } else if let Some(clip) = project
                            .audio_clips
                            .iter()
                            .find(|clip| Some(clip.id) == self.selected)
                        {
                            self.trim_source = clip.source_in_sample;
                            self.trim_duration = clip.duration_frames;
                            self.audio_at_frame = clip.at_frame;
                            self.audio_gain_db = clip.gain_millidecibels as f32 / 1000.0;
                            self.audio_muted = clip.muted;
                        }
                        self.project = Some(project);
                    }
                }
                Message::Events(events) => {
                    for event in events {
                        if event.kind.starts_with("import_")
                            && let Ok(job) = serde_json::from_value::<ImportJob>(event.data.clone())
                        {
                            self.update_import(job);
                        }
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
                Message::History { entries, state } => {
                    self.history = entries;
                    self.history_state = Some(state);
                }
                Message::Import(job) => self.update_import(job),
                Message::ImportRejected { media_ids, message } => {
                    for card in &mut self.imports {
                        if media_ids.contains(&card.media_id) {
                            card.stage = ImportStage::Failed;
                            card.error = Some(message.clone());
                        }
                    }
                    self.fail(format!("Import: {message}"));
                }
                Message::Complete { label, value } => {
                    if label == "Export" {
                        self.last_render = Some(value.clone());
                    }
                    if label == "Create project"
                        && let Some(path) = value.as_str()
                    {
                        self.open(PathBuf::from(path));
                    }

                    self.note(format!("{label}: {value}"));
                    if label == "Export" {
                        self.render_busy = false;
                    } else if label != "Cancel import" {
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
                    if !matches!(label.as_str(), "Cancel import" | "Import status") {
                        self.edit_busy = false;
                    }
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

    pub(crate) fn import_media(&mut self, paths: Vec<PathBuf>) -> bool {
        let Some(project) = &self.project else {
            self.fail("Open a project before importing media.".into());
            return false;
        };
        if !self.connected || self.edit_busy || paths.is_empty() || paths.len() > 32 {
            self.fail("Wait for the current edit, then import up to 32 files together.".into());
            return false;
        }
        if self.imports.iter().any(|card| !card.stage.is_terminal()) {
            self.fail("An import is preparing. You can continue editing, or cancel it before importing more files.".into());
            return false;
        }
        let revision = project.revision;
        self.import_paths = paths.clone();
        let match_first_video = self.match_first_video
            && project.clips.is_empty()
            && project.audio_clips.is_empty()
            && !project.media.iter().any(|media| {
                media
                    .metadata
                    .video
                    .as_ref()
                    .is_some_and(|video| video.codec != "png")
            });
        let sources: Vec<_> = paths
            .into_iter()
            .map(|path| {
                let name = path.file_stem().map_or_else(
                    || "Media".into(),
                    |name| name.to_string_lossy().into_owned(),
                );
                ImportSource {
                    media_id: Uuid::new_v4(),
                    path,
                    name,
                }
            })
            .collect();
        self.imports.retain(|card| !card.stage.is_terminal());
        self.imports.extend(sources.iter().map(|source| ImportCard {
            media_id: source.media_id,
            name: source.name.clone(),
            job_id: None,
            stage: ImportStage::Queued,
            error: None,
            elapsed_ms: 0,
            quick: None,
        }));
        if !self.send(Command::Import {
            sources,
            revision,
            match_first_video,
        }) {
            self.imports.clear();
            return false;
        }
        true
    }

    fn update_import(&mut self, job: ImportJob) {
        for source in &job.sources {
            // A completed job can precede or follow its authoritative snapshot.
            if self.project.as_ref().is_some_and(|project| {
                project
                    .media
                    .iter()
                    .any(|media| media.id == source.media_id)
            }) {
                continue;
            }
            if let Some(card) = self
                .imports
                .iter_mut()
                .find(|card| card.media_id == source.media_id)
            {
                if job.elapsed_ms < card.elapsed_ms
                    || card.stage.is_terminal() && !job.stage.is_terminal()
                {
                    continue;
                }
                card.job_id = Some(job.job_id);
                card.stage = job.stage;
                card.error = job.error.as_ref().map(|error| error.message.clone());
                card.elapsed_ms = job.elapsed_ms;
                card.quick = source.quick.clone();
            } else if !job.stage.is_terminal() && self.imports.len() < 128 {
                self.imports.push(ImportCard {
                    media_id: source.media_id,
                    name: source.name.clone(),
                    job_id: Some(job.job_id),
                    stage: job.stage,
                    error: job.error.as_ref().map(|error| error.message.clone()),
                    elapsed_ms: job.elapsed_ms,
                    quick: source.quick.clone(),
                });
            }
        }
        if job.stage == ImportStage::Completed && !self.completed_imports.contains(&job.job_id) {
            if self.completed_imports.len() >= 32 {
                self.completed_imports.pop_front();
            }
            self.completed_imports.push_back(job.job_id);
            self.last_complete = Some("Import media".into());
        }
    }

    pub(crate) fn import_evidence(&self) -> Value {
        serde_json::json!({
            "pending_cards": self.imports.iter().filter(|card| !card.stage.is_terminal()).count(),
            "editing_busy": self.edit_busy,
            "timeline_frame_rate": self.project.as_ref().map(|project| project.frame_rate),
            "stages": self.imports.iter().map(|card| card.stage).collect::<Vec<_>>(),
        })
    }

    pub(crate) fn select(&mut self, id: Uuid) {
        self.visual_form = None;
        self.selected = Some(id);
        if let Some(clip) = self
            .project
            .as_ref()
            .and_then(|p| p.clips.iter().find(|clip| clip.id == id))
        {
            self.trim_source = clip.source_in_frame;
            self.trim_duration = clip.duration_frames;
        } else if let Some(clip) = self
            .project
            .as_ref()
            .and_then(|project| project.audio_clips.iter().find(|clip| clip.id == id))
        {
            self.trim_source = clip.source_in_sample;
            self.trim_duration = clip.duration_frames;
            self.audio_gain_db = clip.gain_millidecibels as f32 / 1000.0;
            self.audio_muted = clip.muted;
            self.audio_at_frame = clip.at_frame;
        }
    }

    pub(crate) fn rename_media(&mut self, media_id: Uuid, alias: Option<String>) -> bool {
        self.edit(Edit::RenameMedia { media_id, alias }, "Rename media")
    }

    pub(crate) fn remove_media(&mut self, media_id: Uuid) -> bool {
        self.edit(Edit::RemoveMedia { media_id }, "Remove media from project")
    }

    fn rename_panel(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.media_rename.take() else {
            return;
        };
        let Some(media) = self
            .project
            .as_ref()
            .and_then(|project| project.media.iter().find(|media| media.id == form.id))
        else {
            return;
        };
        let path = media.path.display().to_string();
        let changed = media.alias.as_deref() != Some(form.text.as_str());
        let reset_available = media.alias.is_some();
        let mut open = true;
        let mut save = false;
        let mut reset = false;
        egui::Window::new("Rename media")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Name in this project");
                ui.add(
                    egui::TextEdit::singleline(&mut form.text).desired_width(ui.available_width()),
                );
                ui.label(
                    egui::RichText::new("The original filename stays unchanged.")
                        .small()
                        .color(design::MUTED),
                )
                .on_hover_text(&path);
                save = design::primary(
                    ui,
                    "Save name",
                    self.connected && !self.edit_busy && changed,
                    ui.available_width(),
                )
                .clicked();
                reset = ui
                    .add_enabled(
                        self.connected && !self.edit_busy && reset_available,
                        egui::Button::new("Use original name"),
                    )
                    .clicked();
            });
        if save {
            self.rename_media(form.id, Some(form.text));
        } else if reset {
            self.rename_media(form.id, None);
        } else if open {
            self.media_rename = Some(form);
        }
    }

    pub(crate) fn undo(&mut self) -> bool {
        if self
            .history_state
            .as_ref()
            .is_some_and(|state| state.can_undo)
        {
            self.edit(Edit::Undo {}, "Undo")
        } else {
            false
        }
    }
    pub(crate) fn redo(&mut self) -> bool {
        if self
            .history_state
            .as_ref()
            .is_some_and(|state| state.can_redo)
        {
            self.edit(Edit::Redo {}, "Redo")
        } else {
            false
        }
    }
    fn copy_clip(&mut self, ctx: &egui::Context, id: Uuid) {
        if self
            .project
            .as_ref()
            .is_some_and(|p| p.visual_clips.iter().any(|c| c.id == id))
        {
            self.fail(
                "Overlay clipboard transfer is not available yet; add the verified source again."
                    .into(),
            );
            return;
        }
        let result = self
            .project
            .as_ref()
            .ok_or_else(|| lumeryn_core::Error::Invalid("Open a project before copying".into()))
            .and_then(|project| lumeryn_core::clipboard::Clipboard::copy(project, id))
            .and_then(|clipboard| serde_json::to_string(&clipboard).map_err(Into::into));
        match result {
            Ok(text) => ctx.copy_text(text),
            Err(error) => self.fail(format!("Copy clip: {error}")),
        }
    }
    pub(crate) fn paste_clip(&mut self, bytes: &[u8]) -> bool {
        self.paste_clip_at_end(bytes, Uuid::new_v4())
    }
    pub(crate) fn paste_clip_at_end(&mut self, bytes: &[u8], id: Uuid) -> bool {
        let parsed = lumeryn_core::clipboard::Clipboard::from_slice(bytes);
        match parsed {
            Ok(clipboard) => {
                let at_frame = self.project.as_ref().map_or(0, timeline::total_frames);
                self.edit(
                    Edit::PasteClip {
                        id,
                        at_frame,
                        clipboard,
                    },
                    "Paste clip",
                )
            }
            Err(error) => {
                self.fail(format!("Paste needs a copied Lumeryn clip: {error}"));
                false
            }
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

    pub(crate) fn edit(&mut self, operation: Edit, label: &str) -> bool {
        self.edit_batch(
            Batch {
                operations: vec![operation],
            },
            label,
        )
    }
    pub(crate) fn trim_audio(&mut self, clip: Uuid, duration: u64) -> bool {
        let Some(source_in_sample) = self
            .project
            .as_ref()
            .and_then(|project| project.audio_clips.iter().find(|item| item.id == clip))
            .map(|item| item.source_in_sample)
        else {
            return false;
        };
        self.edit(
            Edit::TrimAudioClip {
                clip_id: clip,
                source_in_sample,
                duration_frames: duration,
            },
            "Trim audio",
        )
    }
    pub(crate) fn apply_audio(
        &mut self,
        AudioEdit {
            clip,
            at_frame,
            source_in_sample,
            duration_frames,
            gain_millidecibels,
            muted,
        }: AudioEdit,
    ) -> bool {
        self.edit_batch(
            Batch {
                operations: vec![
                    Edit::SetAudioRange {
                        clip_id: clip,
                        at_frame,
                        source_in_sample,
                        duration_frames,
                    },
                    Edit::SetAudioProperties {
                        clip_id: clip,
                        gain_millidecibels,
                        muted,
                    },
                ],
            },
            "Apply audio",
        )
    }
    pub(crate) fn edit_batch(&mut self, batch: Batch, label: &str) -> bool {
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
            options: self.render_options.clone(),
        }) {
            self.render_busy = true;
            self.active_job = Some(job_id);
            self.last_complete = None;
            self.last_render = None;
            true
        } else {
            false
        }
    }

    fn toggle_play(&mut self) {
        self.library.stop_audition();
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

    fn jump_seconds(&mut self, seconds: i64) {
        if let Some(project) = &self.project {
            let frame = seek_seconds(
                self.playhead,
                project.frame_rate,
                seconds,
                timeline::total_frames(project),
            );
            self.seek(frame);
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
                                &[
                                    "mp4", "mov", "mkv", "avi", "webm", "wav", "mp3", "flac",
                                    "ogg", "png",
                                ],
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
        let mut cancel_import = None;
        let mut dismiss_import = None;
        if !self.imports.is_empty() {
            egui::ScrollArea::vertical()
                .id_salt("pending-imports")
                .max_height((ui.available_height() * 0.4).clamp(100.0, 260.0))
                .show(ui, |ui| {
                    for card in &self.imports {
                        egui::Frame::new()
                            .fill(design::BACKGROUND)
                            .inner_margin(10.0)
                            .corner_radius(5.0)
                            .show(ui, |ui| {
                                ui.set_width((ui.available_width() - 20.0).max(80.0));
                                ui.add(
                                    egui::Label::new(RichText::new(&card.name).strong()).truncate(),
                                )
                                .on_hover_text(&card.name);
                                ui.horizontal(|ui| {
                                    if !card.stage.is_terminal() {
                                        ui.spinner();
                                    }
                                    ui.label(
                                        RichText::new(import_stage_label(card.stage))
                                            .small()
                                            .color(design::MUTED),
                                    );
                                });
                                if let Some(video) =
                                    card.quick.as_ref().and_then(|quick| quick.video.as_ref())
                                {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} × {} · {} · unverified",
                                            video.width,
                                            video.height,
                                            video
                                                .frame_rate
                                                .map_or_else(|| "FPS pending".into(), rate_label)
                                        ))
                                        .small()
                                        .color(design::MUTED),
                                    );
                                }
                                if let Some(error) = &card.error {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(error).small().color(design::MUTED),
                                        )
                                        .wrap(),
                                    );
                                }
                                if matches!(
                                    card.stage,
                                    ImportStage::Queued
                                        | ImportStage::Inspecting
                                        | ImportStage::Hashing
                                        | ImportStage::Validating
                                ) && let Some(job_id) = card.job_id
                                    && ui.small_button("Cancel import").clicked()
                                {
                                    cancel_import = Some(job_id);
                                }
                                if matches!(
                                    card.stage,
                                    ImportStage::Failed | ImportStage::Cancelled
                                ) && ui.small_button("Dismiss").clicked()
                                {
                                    dismiss_import = Some(card.media_id);
                                }
                            });
                        ui.add_space(6.0);
                    }
                });
        }
        if let Some(job_id) = cancel_import {
            self.send(Command::CancelImport { job_id });
        }
        if let Some(media_id) = dismiss_import {
            self.imports.retain(|card| card.media_id != media_id);
        }
        if !self.import_paths.is_empty()
            && self
                .imports
                .iter()
                .any(|card| card.stage == ImportStage::Failed)
            && self.imports.iter().all(|card| card.stage.is_terminal())
            && ui
                .add_enabled(
                    self.connected && !self.edit_busy,
                    egui::Button::new("Retry import"),
                )
                .clicked()
        {
            self.import_media(self.import_paths.clone());
        }
        let Some(project) = &self.project else {
            ui.label(RichText::new("Your media belongs here").strong());
            ui.label(
                RichText::new("Open a project, then import a video or drop it into this window.")
                    .color(design::MUTED),
            );
            return;
        };
        let mut add = None;
        let mut add_audio = None;
        let mut rename = None;
        let mut remove = None;
        let mut add_layer = false;
        let mut add_overlay = None;
        if project.schema_version == lumeryn_core::model::SCHEMA_VERSION {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.connected && !self.edit_busy && project.visual_tracks.len() < 2,
                        egui::Button::new("+ Layer"),
                    )
                    .clicked()
                {
                    add_layer = true;
                }
                egui::ComboBox::from_id_salt("target-visual-layer")
                    .selected_text(
                        project
                            .visual_tracks
                            .iter()
                            .find(|t| Some(t.id) == self.target_layer)
                            .or_else(|| project.visual_tracks.last())
                            .map_or("Choose layer", |t| t.name.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        for track in &project.visual_tracks {
                            ui.selectable_value(
                                &mut self.target_layer,
                                Some(track.id),
                                &track.name,
                            );
                        }
                    });
            });
        }
        let width = (ui.available_width() - 12.0) / 2.0;
        let media: Vec<_> = if self.recent_media {
            project.media.iter().rev().collect()
        } else {
            project.media.iter().collect()
        };
        // A video can offer both primary-timeline and overlay actions. Reserve all
        // of them, including the source-rate label, rather than a fixed one-row footer.
        // Reserve the video case during selection changes so no frame clips a new action.
        let footer_buttons = if project.visual_tracks.is_empty() {
            1
        } else {
            2
        };
        let footer_has_details = project
            .media
            .iter()
            .any(|media| media.metadata.video.is_some());
        let footer_height = media_footer_height(ui, footer_buttons, footer_has_details);
        egui::ScrollArea::vertical()
            .id_salt("project-media-grid")
            .auto_shrink([false, false])
            .max_height((ui.available_height() - footer_height).max(0.0))
            .show(ui, |ui| {
                for pair in media.chunks(2) {
                    ui.horizontal_top(|ui| {
                        for media in pair {
                            ui.vertical(|ui| {
                                ui.set_width(width);
                                let (rect, response) = ui.allocate_exact_size(
                                    Vec2::new(width, width * 9.0 / 16.0),
                                    Sense::click_and_drag(),
                                );
                                media_card_drag(
                                    &response,
                                    self.connected && !self.edit_busy,
                                    timeline::DragPayload {
                                        project: project.id,
                                        revision: project.revision,
                                        epoch: self.epoch,
                                        source: timeline::DragSource::Media(media.id),
                                    },
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
                                } else if let Some(waveform) = self
                                    .assets
                                    .as_ref()
                                    .and_then(|assets| assets.waveform(media.id))
                                {
                                    let plot = rect.shrink2(Vec2::new(8.0, 12.0));
                                    let columns = (plot.width() / 2.0).ceil().max(1.0) as usize;
                                    let paint = ui.painter().with_clip_rect(plot);
                                    for column in 0..columns {
                                        let bin = column * waveform.peaks.len() / columns;
                                        if let Some(peak) = waveform.peaks.get(bin) {
                                            let amplitude = (peak / waveform.max_peak.max(0.001))
                                                .clamp(0.0, 1.0)
                                                * plot.height()
                                                * 0.45;
                                            let x = plot.left() + column as f32 * 2.0;
                                            paint.line_segment(
                                                [
                                                    egui::pos2(x, plot.center().y - amplitude),
                                                    egui::pos2(x, plot.center().y + amplitude),
                                                ],
                                                Stroke::new(1.0, Color32::from_rgb(77, 135, 171)),
                                            );
                                        }
                                    }
                                } else {
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        if still_png(media) {
                                            "Image"
                                        } else if media.metadata.video.is_some() {
                                            "Video"
                                        } else if media.metadata.audio.is_some() {
                                            "Audio"
                                        } else {
                                            "Image"
                                        },
                                        egui::FontId::proportional(13.0),
                                        design::MUTED,
                                    );
                                }
                                if media.metadata.video.is_none()
                                    && let Some(samples) = media
                                        .metadata
                                        .audio
                                        .as_ref()
                                        .and_then(|audio| audio.canonical_samples_48k)
                                {
                                    let badge = egui::Rect::from_min_size(
                                        rect.right_bottom() - Vec2::new(48.0, 24.0),
                                        Vec2::new(45.0, 21.0),
                                    );
                                    ui.painter().rect_filled(badge, 3.0, design::BACKGROUND);
                                    ui.painter().text(
                                        badge.center(),
                                        egui::Align2::CENTER_CENTER,
                                        timecode(samples as f64 / 48000.0, false),
                                        egui::FontId::proportional(12.0),
                                        design::TEXT,
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
                                if !still_png(media)
                                    && let Some(video) = &media.metadata.video
                                    && video.frames > 0
                                {
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
                                } else if response.double_clicked()
                                    && media.metadata.audio.is_some()
                                {
                                    add_audio = Some(media.id);
                                }
                                if response.clicked() {
                                    self.selected_media = Some(media.id);
                                }
                                response.context_menu(|ui| {
                                    let enabled = self.connected && !self.edit_busy;
                                    if ui
                                        .add_enabled(enabled, egui::Button::new("Rename"))
                                        .clicked()
                                    {
                                        rename = Some(MediaRename {
                                            id: media.id,
                                            text: media.display_name(),
                                        });
                                        ui.close();
                                    }
                                    let usage = project
                                        .clips
                                        .iter()
                                        .filter(|clip| clip.media_id == media.id)
                                        .count()
                                        + project
                                            .audio_clips
                                            .iter()
                                            .filter(|clip| clip.media_id == media.id)
                                            .count() + project.visual_clips.iter().filter(|clip|clip.media_id==media.id).count();
                                    if ui
                                        .add_enabled(
                                            enabled && usage == 0,
                                            egui::Button::new("Remove from project"),
                                        )
                                        .clicked()
                                    {
                                        remove = Some(media.id);
                                        ui.close();
                                    }
                                    if usage > 0 {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "Used by {usage} clips. Remove those clips first."
                                            ))
                                            .small()
                                            .color(design::MUTED),
                                        );
                                    }
                                    ui.label(
                                        egui::RichText::new("Original files stay on disk.")
                                            .small()
                                            .color(design::MUTED),
                                    );
                                });
                                response.on_hover_text(if still_png(media) {
                                    "Create a layer, then drag this image onto it or add it at the playhead."
                                } else if media.metadata.video.is_some() {
                                    "Drag to a compatible visual row. Double-click to append to Video 1."
                                } else if media.metadata.audio.is_some() {
                                    "Drag onto Audio 1. Double-click to add at the playhead."
                                } else {
                                    "This source has no supported video or audio stream."
                                });
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
                if project.media.is_empty() && self.imports.is_empty() {
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
            .find(|media| Some(media.id) == self.selected_media)
            && let Some(video) = &media.metadata.video
        {
            ui.label(
                RichText::new(if still_png(media) {
                    format!("Still image - timeline {}", rate_label(project.frame_rate))
                } else {
                    format!(
                        "Source {} - timeline {}",
                        rate_label(video.frame_rate),
                        rate_label(project.frame_rate)
                    )
                })
                .small()
                .color(design::MUTED),
            );
        }
        if let Some(media) = project
            .media
            .iter()
            .find(|m| Some(m.id) == self.selected_media)
            && !still_png(media)
            && let Some(video) = &media.metadata.video
            && video.frames > 0
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
        if let Some(media) = project
            .media
            .iter()
            .find(|m| Some(m.id) == self.selected_media)
            && media.metadata.video.is_some()
            && let Some(track) = self
                .target_layer
                .filter(|id| project.visual_tracks.iter().any(|t| t.id == *id))
                .or_else(|| project.visual_tracks.last().map(|t| t.id))
            && ui
                .add_enabled(
                    self.connected && !self.edit_busy,
                    egui::Button::new(if still_png(media) {
                        "Add image at playhead"
                    } else {
                        "Add video to layer"
                    })
                    .min_size(Vec2::new(ui.available_width(), 36.0)),
                )
                .clicked()
        {
            add_overlay = Some((media.id, track));
        }
        if let Some(rename) = rename {
            self.media_rename = Some(rename);
        }
        if let Some(media) = project
            .media
            .iter()
            .find(|media| Some(media.id) == self.selected_media)
            && media.metadata.video.is_none()
            && media.metadata.audio.is_some()
            && ui
                .add_enabled(
                    self.connected
                        && !self.edit_busy
                        && project.audio_track_id.is_some()
                        && timeline::total_frames(project) > 0,
                    egui::Button::new("Add audio at playhead")
                        .min_size(Vec2::new(ui.available_width(), 36.0)),
                )
                .clicked()
        {
            add_audio = Some(media.id);
        }
        if let Some(id) = remove {
            self.remove_media(id);
        } else if add_layer {
            let id = Uuid::new_v4();
            let name = format!("Layer {}", project.visual_tracks.len() + 1);
            self.target_layer = Some(id);
            self.edit(Edit::AddVisualTrack { id, name }, "Add visual layer");
        } else if let Some((media, track)) = add_overlay {
            if let Some(payload) = self.drag_payload(timeline::DragSource::Media(media)) {
                self.drop_visual(payload, track, self.playhead);
            }
        } else if let Some((media_id, duration_frames)) = add {
            self.edit(
                Edit::InsertClip {
                    id: Uuid::new_v4(),
                    media_id,
                    before_clip_id: None,
                    source_in_frame: 0,
                    duration_frames,
                },
                "Add clip",
            );
        } else if let Some(media_id) = add_audio
            && let Some(payload) = self.drag_payload(timeline::DragSource::Media(media_id))
        {
            self.drop_audio(payload, self.playhead);
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
    fn preview_panel(&mut self, ui: &mut egui::Ui, fullscreen: &mut bool) {
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
                if self
                    .project
                    .as_ref()
                    .is_some_and(|p| p.visual_duration_frames() > 0)
                {
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
            .is_none_or(|project| project.visual_duration_frames() == 0)
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
        let roomy = ui.available_width() >= 720.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
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
            if design::icon_button(ui, design::Icon::BackFive, "Back 5 seconds · Shift+Left")
                .clicked()
            {
                self.jump_seconds(-5);
            }
            if design::icon_button(
                ui,
                design::Icon::ForwardFive,
                "Forward 5 seconds · Shift+Right",
            )
            .clicked()
            {
                self.jump_seconds(5);
            }
            if roomy {
                if design::icon_button(ui, design::Icon::Previous, "Previous frame · Left arrow")
                    .clicked()
                {
                    self.step(-1);
                }
                if design::icon_button(ui, design::Icon::Next, "Next frame · Right arrow").clicked()
                {
                    self.step(1);
                }
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
                ui.spacing_mut().slider_width = if roomy { 88.0 } else { 54.0 };
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
                if design::icon_button(
                    ui,
                    design::Icon::Expand,
                    if *fullscreen {
                        "Exit full screen · Escape"
                    } else {
                        "Full screen preview · F11"
                    },
                )
                .clicked()
                {
                    *fullscreen = !*fullscreen;
                }
                self.quality_menu(ui);
            });
        });
    }
    fn inspector(&mut self, ui: &mut egui::Ui) {
        if self
            .project
            .as_ref()
            .is_some_and(|p| p.visual_clips.iter().any(|c| Some(c.id) == self.selected))
        {
            self.visual_inspector(ui);
            return;
        }
        if self.project.as_ref().is_some_and(|project| {
            project
                .audio_clips
                .iter()
                .any(|clip| Some(clip.id) == self.selected)
        }) {
            self.audio_inspector(ui);
            return;
        }
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
                    "{} × {} · Source {} · Timeline {}",
                    video.width,
                    video.height,
                    rate_label(video.frame_rate),
                    rate_label(project.frame_rate)
                ));
            }
        });
        if apply {
            self.trim(id, self.trim_source, self.trim_duration);
        }
    }
    fn timeline_panel(&mut self, ui: &mut egui::Ui) {
        if let (Some(project), Some(assets)) = (&self.project, &self.assets) {
            let actions = self.timeline.show(
                ui,
                project,
                assets,
                timeline::View {
                    selected: self.selected,
                    playhead: self.playhead,
                    epoch: self.epoch,
                    editable: self.connected && !self.edit_busy,
                },
            );
            for action in actions {
                match action {
                    timeline::Action::ReorderVisualTrack { track_id, before } => {
                        self.edit(
                            Edit::ReorderVisualTrack {
                                track_id,
                                before_track_id: before,
                            },
                            "Reorder layer",
                        );
                    }
                    timeline::Action::DeleteVisualTrack(track_id) => {
                        self.edit(Edit::DeleteVisualTrack { track_id }, "Remove empty layer");
                    }
                    timeline::Action::DropVisual {
                        payload,
                        track_id,
                        at_frame,
                    } => self.drop_visual(payload, track_id, at_frame),
                    timeline::Action::MovePrimary { clip_id, at_frame } => {
                        if !self.project.as_ref().is_some_and(|p| {
                            p.clips
                                .iter()
                                .any(|c| c.id == clip_id && c.at_frame == at_frame)
                        }) {
                            self.edit(Edit::MoveClip { clip_id, at_frame }, "Move clip");
                        }
                    }
                    timeline::Action::DeleteVisual(clip_id) => {
                        self.edit(
                            Edit::DeleteVisualClip { clip_id },
                            "Delete visual layer clip",
                        );
                    }
                    timeline::Action::TrimVisual { clip_id, duration } => {
                        if let Some(clip) = self
                            .project
                            .as_ref()
                            .and_then(|p| p.visual_clips.iter().find(|c| c.id == clip_id))
                        {
                            self.edit(
                                Edit::SetVisualRange {
                                    clip_id,
                                    at_frame: clip.at_frame,
                                    duration_frames: duration,
                                    source: clip.source.clone(),
                                },
                                "Trim layer clip",
                            );
                        }
                    }
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
                    timeline::Action::Split { clip, at_frame } => {
                        self.edit(
                            Edit::SplitClip {
                                clip_id: clip,
                                right_clip_id: Uuid::new_v4(),
                                at_frame,
                            },
                            "Split clip",
                        );
                    }
                    timeline::Action::Duplicate(clip_id) => {
                        self.edit(
                            Edit::DuplicateClip {
                                clip_id,
                                id: Uuid::new_v4(),
                            },
                            "Duplicate clip",
                        );
                    }
                    timeline::Action::Copy(clip_id) => self.copy_clip(ui.ctx(), clip_id),
                    timeline::Action::Delete(clip_id) => {
                        self.edit(
                            Edit::DeleteClip {
                                clip_id,
                                ripple: !self
                                    .project
                                    .as_ref()
                                    .is_some_and(Project::uses_composition),
                            },
                            "Delete clip",
                        );
                    }
                    timeline::Action::Reorder { clip, before } => {
                        self.edit(
                            Edit::ReorderClip {
                                clip_id: clip,
                                before_clip_id: before,
                            },
                            "Reorder clip",
                        );
                    }
                    timeline::Action::RejectDrop(error) => self.fail(error.into()),
                    timeline::Action::DeleteAudio(clip_id) => {
                        self.edit(Edit::DeleteAudioClip { clip_id }, "Delete audio");
                    }
                    timeline::Action::TrimAudio { clip, duration } => {
                        self.trim_audio(clip, duration);
                    }
                    timeline::Action::DropAudio { payload, at_frame } => {
                        self.drop_audio(payload, at_frame);
                    }
                    timeline::Action::Drop { payload, before } => {
                        self.drop_on_timeline(payload, before);
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
    pub(crate) fn drop_visual(
        &mut self,
        payload: timeline::DragPayload,
        track_id: Uuid,
        at_frame: u64,
    ) {
        use lumeryn_core::model::VisualSource;
        let Some(project) = &self.project else {
            return;
        };
        if payload.project != project.id
            || payload.revision != project.revision
            || payload.epoch != self.epoch
            || !self.connected
            || self.edit_busy
        {
            self.fail("The project changed during this drag. Drag again.".into());
            return;
        }
        let operation = match payload.source {
            timeline::DragSource::VisualClip { clip, .. } => {
                if project
                    .visual_clips
                    .iter()
                    .any(|c| c.id == clip && c.track_id == track_id && c.at_frame == at_frame)
                {
                    return;
                }
                Edit::MoveVisualClip {
                    clip_id: clip,
                    track_id,
                    at_frame,
                }
            }
            timeline::DragSource::Media(id) => {
                let Some(media) = project.media.iter().find(|m| m.id == id) else {
                    return;
                };
                let Some(video) = &media.metadata.video else {
                    self.fail("Select a verified video or PNG.".into());
                    return;
                };
                let image = still_png(media);
                if track_id == project.video_track_id {
                    if image {
                        self.fail("Add a layer for the image, then drop it there.".into());
                        return;
                    }
                    Edit::AddClip {
                        id: Uuid::new_v4(),
                        media_id: id,
                        at_frame,
                        source_in_frame: 0,
                        duration_frames: video.frames,
                    }
                } else {
                    let duration = if image {
                        (u64::from(project.frame_rate.numerator()) * 5
                            / u64::from(project.frame_rate.denominator()))
                        .max(1)
                    } else {
                        video.frames
                    };
                    Edit::AddVisualClip {
                        id: Uuid::new_v4(),
                        media_id: id,
                        track_id,
                        at_frame,
                        duration_frames: duration,
                        source: if image {
                            VisualSource::Image {}
                        } else {
                            VisualSource::Video { source_in_frame: 0 }
                        },
                        placement: None,
                    }
                }
            }
            _ => {
                self.fail("Drop a compatible visual source on this layer.".into());
                return;
            }
        };
        self.edit(operation, "Place visual clip");
    }

    fn visual_inspector(&mut self, ui: &mut egui::Ui) {
        use lumeryn_core::model::{VisualPlacement, VisualSource};
        let Some(project) = &self.project else {
            return;
        };
        let Some(clip) = project
            .visual_clips
            .iter()
            .find(|c| Some(c.id) == self.selected)
        else {
            return;
        };
        if self
            .visual_form
            .as_ref()
            .is_none_or(|f| f.clip != clip.id || f.revision != project.revision)
        {
            let rect = clip.placement.clone().unwrap_or(VisualPlacement {
                x: 0,
                y: 0,
                width: project.width,
                height: project.height,
            });
            self.visual_form = Some(VisualForm {
                clip: clip.id,
                revision: project.revision,
                at: clip.at_frame,
                duration: clip.duration_frames,
                source: match clip.source {
                    VisualSource::Video { source_in_frame } => source_in_frame,
                    VisualSource::Image {} => 0,
                },
                custom: clip.placement.is_some(),
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            });
        }
        let id = clip.id;
        let image = matches!(clip.source, VisualSource::Image {});
        let title = project
            .media
            .iter()
            .find(|m| m.id == clip.media_id)
            .map_or_else(|| "Visual clip".into(), |m| m.display_name());
        ui.heading(title);
        ui.add_space(8.0);
        let Some(form) = &mut self.visual_form else {
            return;
        };
        let mut apply = false;
        let mut delete = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(if image { "Image hold" } else { "Video overlay" });
            let fps = f64::from(project.frame_rate.numerator())
                / f64::from(project.frame_rate.denominator());
            let mut start = form.at as f64 / fps;
            let mut length = form.duration as f64 / fps;
            let mut source = form.source as f64 / fps;
            egui::Grid::new("visual-range")
                .num_columns(2)
                .spacing(Vec2::new(10.0, 8.0))
                .show(ui, |ui| {
                    ui.label("Timeline start");
                    ui.add(
                        egui::DragValue::new(&mut start)
                            .speed(1.0 / fps)
                            .range(0.0..=lumeryn_core::model::MAX_FRAMES as f64 / fps)
                            .custom_formatter(|v, _| timecode(v, true))
                            .custom_parser(parse_timecode),
                    );
                    ui.end_row();
                    ui.label("Length");
                    ui.add(
                        egui::DragValue::new(&mut length)
                            .speed(1.0 / fps)
                            .range(1.0 / fps..=lumeryn_core::model::MAX_FRAMES as f64 / fps)
                            .custom_formatter(|v, _| timecode(v, true))
                            .custom_parser(parse_timecode),
                    );
                    ui.end_row();
                    if !image {
                        ui.label("Source start");
                        ui.add(
                            egui::DragValue::new(&mut source)
                                .speed(1.0 / fps)
                                .range(0.0..=lumeryn_core::model::MAX_FRAMES as f64 / fps)
                                .custom_formatter(|v, _| timecode(v, true))
                                .custom_parser(parse_timecode),
                        );
                        ui.end_row();
                    }
                });
            form.at = (start * fps).round() as u64;
            form.duration = (length * fps).round().max(1.0) as u64;
            form.source = (source * fps).round() as u64;
            ui.collapsing("Exact frames", |ui| {
                ui.label(format!(
                    "Start {} · length {} · source {}",
                    form.at, form.duration, form.source
                ));
            });
            ui.separator();
            ui.label(RichText::new("Position & size").strong());
            ui.checkbox(&mut form.custom, "Custom rectangle");
            if form.custom {
                ui.scope(|ui| {
                    egui::Grid::new("visual-rect")
                        .num_columns(2)
                        .spacing(Vec2::new(10.0, 8.0))
                        .show(ui, |ui| {
                            ui.label("Left");
                            ui.add(egui::DragValue::new(&mut form.x).range(-8192..=8192));
                            ui.end_row();
                            ui.label("Top");
                            ui.add(egui::DragValue::new(&mut form.y).range(-8192..=8192));
                            ui.end_row();
                            ui.label("Width");
                            ui.add(egui::DragValue::new(&mut form.width).range(1..=3840));
                            ui.end_row();
                            ui.label("Height");
                            ui.add(egui::DragValue::new(&mut form.height).range(1..=3840));
                            ui.end_row();
                        });
                    if ui.button("Picture in picture").clicked() {
                        form.width = (project.width / 3).max(2);
                        form.height = (project.height / 3).max(2);
                        form.x = (project.width - form.width) as i32;
                        form.y = (project.height - form.height) as i32;
                    }
                });
            }
            ui.label(
                RichText::new(
                    "Aspect ratio is preserved. Clear Custom rectangle to fit the canvas.",
                )
                .small()
                .color(design::MUTED),
            );
            ui.add_space(8.0);
            apply = design::primary(
                ui,
                "Apply changes",
                self.connected && !self.edit_busy,
                ui.available_width(),
            )
            .clicked();
            ui.add_space(8.0);
            delete = ui
                .add_enabled(
                    self.connected && !self.edit_busy,
                    egui::Button::new("Remove from timeline"),
                )
                .clicked();
        });
        if apply {
            let operations = vec![
                Edit::SetVisualRange {
                    clip_id: id,
                    at_frame: form.at,
                    duration_frames: form.duration,
                    source: if image {
                        VisualSource::Image {}
                    } else {
                        VisualSource::Video {
                            source_in_frame: form.source,
                        }
                    },
                },
                Edit::SetVisualPlacement {
                    clip_id: id,
                    placement: form.custom.then_some(VisualPlacement {
                        x: form.x,
                        y: form.y,
                        width: form.width,
                        height: form.height,
                    }),
                },
            ];
            self.edit_batch(Batch { operations }, "Edit visual clip");
        } else if delete {
            self.edit(Edit::DeleteVisualClip { clip_id: id }, "Delete visual clip");
        }
    }
    pub(crate) fn drag_payload(
        &self,
        source: timeline::DragSource,
    ) -> Option<timeline::DragPayload> {
        self.project.as_ref().map(|project| timeline::DragPayload {
            project: project.id,
            revision: project.revision,
            epoch: self.epoch,
            source,
        })
    }
    pub(crate) fn drop_on_timeline(
        &mut self,
        payload: timeline::DragPayload,
        before: Option<Uuid>,
    ) -> bool {
        let Some(project) = &self.project else {
            return false;
        };
        if payload.project != project.id
            || payload.revision != project.revision
            || payload.epoch != self.epoch
        {
            self.fail(
                "The project changed during this drag. Drag again from the current project.".into(),
            );
            return false;
        }
        let operation = match payload.source {
            timeline::DragSource::Clip(clip_id) => Edit::ReorderClip {
                clip_id,
                before_clip_id: before,
            },
            timeline::DragSource::Media(media_id) => {
                let Some(video) = project
                    .media
                    .iter()
                    .find(|media| media.id == media_id)
                    .and_then(|media| media.metadata.video.as_ref())
                else {
                    self.fail("This media cannot be placed on the video track.".into());
                    return false;
                };
                Edit::InsertClip {
                    id: Uuid::new_v4(),
                    media_id,
                    before_clip_id: before,
                    source_in_frame: 0,
                    duration_frames: video.frames,
                }
            }
            timeline::DragSource::AudioClip { .. } | timeline::DragSource::VisualClip { .. } => {
                self.fail("Drop independent audio on Audio 1.".into());
                return false;
            }
        };
        self.edit(operation, "Drag clip")
    }
    pub(crate) fn drop_audio(&mut self, payload: timeline::DragPayload, at_frame: u64) -> bool {
        let Some(project) = &self.project else {
            return false;
        };
        if payload.project != project.id
            || payload.revision != project.revision
            || payload.epoch != self.epoch
        {
            self.fail("The project changed during this audio drag. Drag again.".into());
            return false;
        }
        let operation = match payload.source {
            timeline::DragSource::AudioClip { clip, .. } => {
                if project
                    .audio_clips
                    .iter()
                    .any(|item| item.id == clip && item.at_frame == at_frame)
                {
                    return false;
                }
                Edit::MoveAudioClip {
                    clip_id: clip,
                    at_frame,
                }
            }
            timeline::DragSource::Media(media_id) => {
                if project
                    .media
                    .iter()
                    .any(|media| media.id == media_id && media.metadata.video.is_some())
                {
                    self.fail("Drop an imported audio file here. Detaching a video's audio is not available yet.".into());
                    return false;
                }
                if project
                    .media
                    .iter()
                    .find(|media| media.id == media_id)
                    .and_then(|media| media.metadata.audio.as_ref())
                    .is_some_and(|audio| audio.channels > 2 || audio.start_time_micros != 0)
                {
                    self.fail(
                        "Only zero-origin mono or stereo audio can be placed on Audio 1.".into(),
                    );
                    return false;
                }
                let Some(samples) = project
                    .media
                    .iter()
                    .find(|media| media.id == media_id)
                    .and_then(|media| media.metadata.audio.as_ref())
                    .and_then(|audio| audio.canonical_samples_48k)
                else {
                    self.fail("Reimport this audio source to verify its decoded length before Studio placement.".into());
                    return false;
                };
                let available = timeline::total_frames(project).saturating_sub(at_frame);
                let duration = (u128::from(samples) * u128::from(project.frame_rate.numerator())
                    / (u128::from(project.frame_rate.denominator()) * 48000))
                    .min(u128::from(available)) as u64;
                if duration == 0 {
                    self.fail("Audio needs available visual timeline space.".into());
                    return false;
                }
                Edit::AddAudioClip {
                    id: Uuid::new_v4(),
                    media_id,
                    at_frame,
                    source_in_sample: 0,
                    duration_frames: duration,
                    gain_millidecibels: 0,
                    muted: false,
                }
            }
            timeline::DragSource::Clip(_) | timeline::DragSource::VisualClip { .. } => {
                self.fail("Use an imported audio source on Audio 1.".into());
                return false;
            }
        };
        self.edit(operation, "Drag audio")
    }
    fn audio_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            return;
        };
        let Some(clip) = project
            .audio_clips
            .iter()
            .find(|clip| Some(clip.id) == self.selected)
        else {
            return;
        };
        let id = clip.id;
        let name = project
            .media
            .iter()
            .find(|media| media.id == clip.media_id)
            .map_or_else(|| "Audio".into(), |media| media.display_name());
        ui.label(RichText::new(name).strong());
        ui.label(
            RichText::new("Independent audio · included in export")
                .small()
                .color(design::MUTED),
        );
        ui.add_space(12.0);
        let fps =
            f64::from(project.frame_rate.numerator()) / f64::from(project.frame_rate.denominator());
        let mut start = self.audio_at_frame as f64 / fps;
        let mut source = self.trim_source as f64 / 48000.0;
        let mut duration = self.trim_duration as f64 / fps;
        egui::Grid::new("audio-clip-properties")
            .num_columns(2)
            .spacing(Vec2::new(8.0, 12.0))
            .show(ui, |ui| {
                ui.label("Timeline start");
                ui.add(
                    egui::DragValue::new(&mut start)
                        .speed(1.0 / fps)
                        .range(0.0..=timeline::total_frames(project) as f64 / fps)
                        .custom_formatter(|v, _| timecode(v, true))
                        .custom_parser(parse_timecode),
                );
                ui.end_row();
                ui.label("Source start");
                ui.add(
                    egui::DragValue::new(&mut source)
                        .speed(0.01)
                        .range(0.0..=1e9)
                        .custom_formatter(|v, _| timecode(v, true))
                        .custom_parser(parse_timecode),
                );
                ui.end_row();
                ui.label("Length");
                ui.add(
                    egui::DragValue::new(&mut duration)
                        .speed(1.0 / fps)
                        .range(1.0 / fps..=timeline::total_frames(project) as f64 / fps)
                        .custom_formatter(|v, _| timecode(v, true))
                        .custom_parser(parse_timecode),
                );
                ui.end_row();
            });
        self.audio_at_frame = (start * fps).round() as u64;
        self.trim_source = (source * 48000.0).round() as u64;
        self.trim_duration = (duration * fps).round().max(1.0) as u64;
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label("Volume");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::DragValue::new(&mut self.audio_gain_db)
                        .range(-60.0..=12.0)
                        .speed(0.5)
                        .suffix(" dB"),
                );
            });
        });
        ui.scope(|ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 8.0).max(80.0);
            ui.spacing_mut().slider_rail_height = 4.0;
            ui.spacing_mut().interact_size.y = 20.0;
            ui.visuals_mut().selection.bg_fill = design::ACCENT;
            ui.add(
                egui::Slider::new(&mut self.audio_gain_db, -60.0..=12.0)
                    .show_value(false)
                    .trailing_fill(true),
            )
            .on_hover_text(
                "0 dB retains the original level. -6 dB is about half the signal amplitude.",
            );
        });
        ui.checkbox(&mut self.audio_muted, "Mute this clip");
        ui.add_space(12.0);
        let apply = design::primary(
            ui,
            "Apply audio",
            self.connected && !self.edit_busy,
            ui.available_width(),
        )
        .clicked();
        ui.label(
            RichText::new(
                "Overlapping clips mix together. Video rearrangement keeps this audio's position.",
            )
            .small()
            .color(design::MUTED),
        );
        if apply {
            self.apply_audio(AudioEdit {
                clip: id,
                at_frame: self.audio_at_frame,
                source_in_sample: self.trim_source,
                duration_frames: self.trim_duration,
                gain_millidecibels: (self.audio_gain_db * 1000.0).round() as i32,
                muted: self.audio_muted,
            });
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
            let current = (project.width, project.height);
            let mut choice = current;
            egui::ComboBox::from_id_salt("canvas-ratio")
                .selected_text(format!("Canvas {} × {}", current.0, current.1))
                .show_ui(ui, |ui| {
                    for (size, label) in [
                        ((1920, 1080), "Wide 16:9"),
                        ((1080, 1920), "Vertical 9:16"),
                        ((1080, 1080), "Square 1:1"),
                        ((1440, 1080), "Classic 4:3"),
                        ((1080, 1350), "Social 4:5"),
                        ((2520, 1080), "Cinema 21:9"),
                        ((1080, 1620), "Portrait 2:3"),
                    ] {
                        ui.selectable_value(&mut choice, size, label);
                    }
                });
            if let Some(path) = &self.database {
                ui.label(path.display().to_string());
            }
            if choice != current {
                self.edit(
                    Edit::SetCanvas {
                        width: choice.0,
                        height: choice.1,
                    },
                    "Change canvas",
                );
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
            self.import_media(vec![self.media_input.trim().into()]);
        }
    }
    fn project_panel(&mut self, ui: &mut egui::Ui) {
        if self
            .project
            .as_ref()
            .is_some_and(|project| project.schema_version < lumeryn_core::model::SCHEMA_VERSION)
        {
            ui.label(RichText::new("Upgrade for audio editing").strong());
            ui.label(RichText::new("Creates a local backup, then upgrades every revision without changing your edit history. Older builds cannot edit the upgraded project.").small().color(design::MUTED));
            if ui
                .add_enabled(
                    self.connected && !self.edit_busy,
                    egui::Button::new("Back up & upgrade project"),
                )
                .clicked()
                && self.send(Command::Migrate)
            {
                self.edit_busy = true;
            }
            ui.separator();
        }
        let Some(project) = &self.project else {
            ui.label("Open a project to begin.");
            if ui.button("Choose a project…").clicked() {
                self.pick(ui.ctx(), Picker::Open);
            }
            return;
        };
        ui.add(egui::Label::new(RichText::new(&project.name).strong().size(18.0)).wrap());
        ui.add_space(12.0);
        ui.label(RichText::new("Canvas").small().color(design::MUTED));
        let current = (project.width, project.height);
        let mut choice = current;
        let canvas_label = canvas_choices()
            .iter()
            .find(|(size, _)| {
                u64::from(size.0) * u64::from(current.1) == u64::from(current.0) * u64::from(size.1)
            })
            .map_or("Custom", |(_, label)| *label);
        ui.add_enabled_ui(self.connected && !self.edit_busy, |ui| {
            egui::ComboBox::from_id_salt("project-canvas")
                .width(ui.available_width())
                .selected_text(canvas_label)
                .show_ui(ui, |ui| {
                    for (size, label) in canvas_choices() {
                        ui.selectable_value(&mut choice, size, label);
                    }
                });
        });
        ui.add_space(12.0);
        let current_rate = project.frame_rate;
        let mut selected_rate = current_rate;
        let rate_locked = project.visual_duration_frames() > 0
            || !project.visual_clips.is_empty()
            || !project.audio_clips.is_empty();
        let first_video_eligible = !rate_locked
            && !project.media.iter().any(|media| {
                media
                    .metadata
                    .video
                    .as_ref()
                    .is_some_and(|video| video.codec != "png")
            });
        ui.label(RichText::new("Timeline FPS").small().color(design::MUTED));
        ui.add_enabled_ui(self.connected && !self.edit_busy && !rate_locked, |ui| {
            egui::ComboBox::from_id_salt("project-fps")
                .width(ui.available_width())
                .selected_text(rate_label(selected_rate))
                .show_ui(ui, |ui| {
                    for (numerator, denominator) in [
                        (24000, 1001),
                        (24, 1),
                        (25, 1),
                        (30000, 1001),
                        (30, 1),
                        (50, 1),
                        (60000, 1001),
                        (60, 1),
                    ] {
                        if let Ok(rate) = lumeryn_core::time::FrameRate::new(numerator, denominator)
                        {
                            ui.selectable_value(&mut selected_rate, rate, rate_label(rate));
                        }
                    }
                });
        });
        if rate_locked {
            ui.label(
                RichText::new("Locked while clips are on the timeline.")
                    .small()
                    .color(design::MUTED),
            );
        } else if first_video_eligible {
            ui.checkbox(&mut self.match_first_video, "Match first video").on_hover_text("Use the first verified video's exact FPS. Once clips are added, the timeline rate stays fixed.");
        }
        ui.add_space(12.0);
        egui::Grid::new("project-overview")
            .spacing(Vec2::new(14.0, 12.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Size").color(design::MUTED));
                ui.label(format!("{} × {}", project.width, project.height));
                ui.end_row();
                ui.label(RichText::new("Duration").color(design::MUTED));
                ui.label(timecode(
                    timeline::total_frames(project) as f64
                        * f64::from(project.frame_rate.denominator())
                        / f64::from(project.frame_rate.numerator()),
                    false,
                ));
                ui.end_row();
            });
        ui.add_space(12.0);
        ui.collapsing("File details", |ui| {
            if let Some(path) = &self.database {
                ui.add(egui::Label::new(path.display().to_string()).truncate())
                    .on_hover_text(path.display().to_string());
            }
            ui.label(
                RichText::new(format!("Revision {}", project.revision))
                    .small()
                    .color(design::MUTED),
            );
        });
        if choice != current {
            self.edit(
                Edit::SetCanvas {
                    width: choice.0,
                    height: choice.1,
                },
                "Change canvas",
            );
        } else if selected_rate != current_rate {
            self.match_first_video = false;
            self.edit(
                Edit::SetFrameRate {
                    frame_rate: selected_rate,
                },
                "Change timeline FPS",
            );
        }
    }

    fn render_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .id_salt("export-settings-scroll")
            .max_height((ui.ctx().content_rect().height() - 320.0).clamp(240.0, 440.0))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.add_enabled_ui(!self.render_busy, |ui| {
                    ui.columns(2, |columns| {
                        egui::Frame::new()
                            .fill(design::RAISED)
                            .corner_radius(8)
                            .inner_margin(14)
                            .show(&mut columns[0], |ui| {
                                ui.spacing_mut().interact_size.y = 24.0;
                                ui.spacing_mut().item_spacing.y = 4.0;
                                ui.set_min_width(258.0);
                                ui.label(RichText::new("Resolution").strong());
                                ui.add_space(8.0);
                                for (resolution, name, hint) in [
                                    (RenderResolution::Uhd2160, "4K UHD", "Maximum detail"),
                                    (RenderResolution::Fhd1080, "1080p", "Everyday high quality"),
                                    (RenderResolution::Hd720, "720p", "Smaller file"),
                                    (
                                        RenderResolution::Project,
                                        "Project size",
                                        "Keep the canvas resolution",
                                    ),
                                ] {
                                    if ui
                                        .radio(self.render_options.resolution == resolution, name)
                                        .clicked()
                                    {
                                        self.render_options.resolution = resolution;
                                    }
                                    ui.label(RichText::new(hint).small().color(design::MUTED));
                                    ui.add_space(4.0);
                                }
                            });
                        let ui = &mut columns[1];
                        ui.label(RichText::new("Video settings").strong());
                        ui.add_space(8.0);
                        ui.label(RichText::new("Export FPS").small().color(design::MUTED));
                        egui::ComboBox::from_id_salt("export-fps")
                            .width(ui.available_width())
                            .selected_text(self.render_options.fps.map_or_else(
                                || {
                                    self.project.as_ref().map_or_else(
                                        || "Project frame rate".into(),
                                        |p| format!("{} · project", rate_label(p.frame_rate)),
                                    )
                                },
                                |fps| format!("{fps} fps"),
                            ))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.render_options.fps,
                                    None,
                                    "Project frame rate",
                                );
                                for fps in [24, 25, 30, 60] {
                                    ui.selectable_value(
                                        &mut self.render_options.fps,
                                        Some(fps),
                                        format!("{fps} fps"),
                                    );
                                }
                            });
                        ui.label(RichText::new("Codec").small().color(design::MUTED));
                        egui::ComboBox::from_id_salt("export-codec")
                            .width(ui.available_width())
                            .selected_text(codec_label(self.render_options.codec))
                            .show_ui(ui, |ui| {
                                for codec in [VideoCodec::H264, VideoCodec::H265, VideoCodec::Mpeg4]
                                {
                                    ui.selectable_value(
                                        &mut self.render_options.codec,
                                        codec,
                                        codec_label(codec),
                                    );
                                }
                            });
                        ui.label(RichText::new("Quality").small().color(design::MUTED));
                        egui::ComboBox::from_id_salt("export-quality")
                            .width(ui.available_width())
                            .selected_text(quality_label(self.render_options.quality))
                            .show_ui(ui, |ui| {
                                for quality in [
                                    RenderQuality::High,
                                    RenderQuality::Balanced,
                                    RenderQuality::Draft,
                                ] {
                                    ui.selectable_value(
                                        &mut self.render_options.quality,
                                        quality,
                                        quality_label(quality),
                                    );
                                }
                            });
                    });
                    ui.add_space(10.0);
                    ui.collapsing("Advanced settings", |ui| {
                        ui.label("Encoder");
                        egui::ComboBox::from_id_salt("export-acceleration")
                            .width(ui.available_width())
                            .selected_text(match self.render_options.acceleration {
                                Acceleration::Cpu => "Software",
                                Acceleration::Auto => "Automatic",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.render_options.acceleration,
                                    Acceleration::Auto,
                                    "Automatic · hardware with software fallback",
                                );
                                ui.selectable_value(
                                    &mut self.render_options.acceleration,
                                    Acceleration::Cpu,
                                    "Software",
                                );
                            });
                        let mut custom = self.render_options.bitrate_kbps.is_some();
                        if ui.checkbox(&mut custom, "Custom video bitrate").changed() {
                            self.render_options.bitrate_kbps = custom.then_some(20_000);
                        }
                        if let Some(bitrate) = &mut self.render_options.bitrate_kbps {
                            ui.add(
                                egui::DragValue::new(bitrate)
                                    .range(100..=200_000)
                                    .suffix(" kbps"),
                            );
                        }
                        ui.label(
                            RichText::new(
                                "MP4 output. Installed encoders determine codec availability.",
                            )
                            .small()
                            .color(design::MUTED),
                        );
                    });
                });
            });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(RichText::new("File name").small().color(design::MUTED));
        let name_changed = ui
            .add_enabled(
                !self.render_busy,
                egui::TextEdit::singleline(&mut self.export_name)
                    .desired_width(ui.available_width()),
            )
            .changed();
        let valid_name = export_name_valid(&self.export_name);
        if valid_name && name_changed {
            let parent = std::path::Path::new(&self.output_input)
                .parent()
                .unwrap_or(std::path::Path::new("."));
            self.output_input = parent.join(&self.export_name).display().to_string();
        } else if !valid_name {
            ui.label(
                RichText::new("Use a filename ending in .mp4, without folders.")
                    .small()
                    .color(design::WARNING),
            );
        }
        ui.horizontal(|ui| {
            let parent = std::path::Path::new(&self.output_input).parent();
            let folder = parent.map_or_else(
                || "Choose a folder".into(),
                |path| path.display().to_string(),
            );
            let location = if parent == self.database.as_deref().and_then(std::path::Path::parent) {
                "Project folder".into()
            } else {
                parent.and_then(std::path::Path::file_name).map_or_else(
                    || "Choose a folder".into(),
                    |name| name.to_string_lossy().into_owned(),
                )
            };
            ui.add(
                egui::Label::new(
                    RichText::new(format!("Save to {location}"))
                        .small()
                        .color(design::MUTED),
                )
                .truncate(),
            )
            .on_hover_text(&folder);
            if ui
                .add_enabled(
                    self.picker.is_none() && !self.render_busy,
                    egui::Button::new("Browse…"),
                )
                .clicked()
            {
                self.pick(ui.ctx(), Picker::Export);
            }
        });
        ui.add_space(12.0);
        if let Some(project) = &self.project {
            let (width, height) = self.render_options.dimensions(project);
            let fps = self.render_options.fps.map_or_else(
                || rate_label(project.frame_rate),
                |fps| format!("{fps} fps"),
            );
            ui.label(
                RichText::new(format!("{width} × {height}  ·  {fps}  ·  MP4")).color(design::MUTED),
            );
        }
        if design::primary(
            ui,
            if self.render_busy {
                "Exporting…"
            } else {
                "Export video"
            },
            self.connected && !self.render_busy && valid_name && !self.output_input.is_empty(),
            ui.available_width(),
        )
        .clicked()
        {
            self.export(self.output_input.trim().into());
        }
        if self.render_busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Exporting in the background");
            });
            if ui.button("Cancel export").clicked()
                && let Some(job_id) = self.active_job
            {
                self.send(Command::Cancel { job_id });
            }
            if let Some(event) = self.events.iter().rev().find(|event| {
                event.kind.starts_with("job_")
                    && event
                        .data
                        .get("job_id")
                        .and_then(Value::as_str)
                        .and_then(|id| Uuid::parse_str(id).ok())
                        == self.active_job
            }) {
                ui.label(
                    RichText::new(
                        event
                            .data
                            .get("stage")
                            .and_then(Value::as_str)
                            .unwrap_or_else(|| job_label(&event.kind)),
                    )
                    .small()
                    .color(design::MUTED),
                );
            }
        } else if self.last_complete.as_deref() == Some("Export") {
            ui.label(RichText::new("Video saved").color(design::ACCENT));
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
            "Undo and redo restore edits as new revisions. Your complete history is kept.",
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
                            Some("split_clip") => "Split a clip",
                            Some("duplicate_clip") => "Duplicated a clip",
                            Some("delete_clip") => "Deleted a clip",
                            Some("reorder_clip") => "Reordered clips",
                            Some("set_canvas") => "Changed canvas size",
                            Some("undo") => "Undid an edit",
                            Some("redo") => "Redid an edit",
                            Some("paste_clip") => "Pasted a clip",
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
        ui.collapsing("Interface font licence", |ui| {
            ui.label("Inter Regular · SIL Open Font License 1.1");
            ui.label(include_str!(
                "../../../assets/library/notices/inter.v4.1/License.txt"
            ));
        });
        design::section(
            ui,
            "Diagnostics",
            "Measured locally. GPU memory usage is unknown.",
        );
        ui.label(&self.connection);
        ui.label(format!("{} curated library assets", self.library.count()));
        ui.label(match self.native_fullscreen {
            Some(true) => "Native fullscreen active",
            Some(false) => "Native windowed preview",
            None => "Native fullscreen state unknown",
        });
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
fn canvas_choices() -> [((u32, u32), &'static str); 7] {
    [
        ((1920, 1080), "Wide 16:9"),
        ((1080, 1920), "Vertical 9:16"),
        ((1080, 1080), "Square 1:1"),
        ((1440, 1080), "Classic 4:3"),
        ((1080, 1350), "Social 4:5"),
        ((2520, 1080), "Cinema 21:9"),
        ((1080, 1620), "Portrait 2:3"),
    ]
}
fn rate_label(rate: lumeryn_core::time::FrameRate) -> String {
    if rate.denominator() == 1 {
        format!("{} fps", rate.numerator())
    } else {
        format!(
            "{:.2} fps",
            f64::from(rate.numerator()) / f64::from(rate.denominator())
        )
    }
}
fn import_stage_label(stage: ImportStage) -> &'static str {
    match stage {
        ImportStage::Queued => "Importing…",
        ImportStage::Inspecting => "Reading media…",
        ImportStage::Hashing => "Checking source…",
        ImportStage::Validating => "Verifying timing…",
        ImportStage::Committing => "Adding to project…",
        ImportStage::Completed => "Verified · updating library…",
        ImportStage::Failed => "Import failed",
        ImportStage::Cancelled => "Import cancelled",
    }
}
fn codec_label(codec: VideoCodec) -> &'static str {
    match codec {
        VideoCodec::H264 => "H.264",
        VideoCodec::H265 => "H.265",
        VideoCodec::Mpeg4 => "MPEG-4",
    }
}
fn quality_label(quality: RenderQuality) -> &'static str {
    match quality {
        RenderQuality::High => "High",
        RenderQuality::Balanced => "Balanced",
        RenderQuality::Draft => "Draft",
    }
}
fn export_name_valid(name: &str) -> bool {
    !name.trim().is_empty()
        && name.len() <= 240
        && name.to_ascii_lowercase().ends_with(".mp4")
        && !name
            .chars()
            .any(|character| character.is_control() || "/\\:*?\"<>|".contains(character))
}
fn still_png(media: &lumeryn_core::model::Media) -> bool {
    media.metadata.audio.is_none()
        && media
            .metadata
            .video
            .as_ref()
            .is_some_and(|video| video.codec == "png" && video.frames <= 1)
}
fn media_name(media: &lumeryn_core::model::Media) -> String {
    media.display_name()
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
    drawers: Drawers,
    fullscreen: bool,
    fullscreen_applied: bool,
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
        if options.capture_only && options.capture_state == CaptureState::Library {
            session.library.focus_images();
        }
        if let Some(path) = &options.project {
            session.open(path.clone());
            session.playhead = options.capture_frame;
        }
        let smoke = if options.smoke {
            options.artifact_dir.clone().map(|directory| {
                Smoke::new(
                    directory,
                    options.smoke_history,
                    options.smoke_audio,
                    options.smoke_composition,
                )
            })
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
            drawers: Drawers {
                left: if options.capture_only && options.capture_state == CaptureState::Collapsed {
                    None
                } else if options.capture_only
                    && matches!(
                        options.capture_state,
                        CaptureState::Library | CaptureState::LibraryAudio
                    )
                {
                    Some(LeftDrawer::Library)
                } else {
                    Some(LeftDrawer::Media)
                },
                right: if options.capture_only && options.capture_state == CaptureState::Project {
                    Some(RightDrawer::Project)
                } else if options.capture_only && options.capture_state == CaptureState::Collapsed {
                    None
                } else {
                    Some(RightDrawer::Clip)
                },
                active: if options.capture_only
                    && matches!(
                        options.capture_state,
                        CaptureState::Inspector | CaptureState::Visual | CaptureState::Audio
                    ) {
                    RailSide::Right
                } else {
                    RailSide::Left
                },
            },
            fullscreen: options.capture_only && options.capture_state == CaptureState::Fullscreen,
            fullscreen_applied: false,
            smoke,
            capture,
            capture_frame: options.capture_frame,
            capture_state: options.capture_state,
            capture_seek_revision: None,
        }
    }
    fn header(&mut self, ui: &mut egui::Ui) {
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
                        self.drawers.right = Some(RightDrawer::Project);
                        self.drawers.active = RailSide::Right;
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
            ui.add_enabled_ui(self.session.connected && !self.session.edit_busy && self.session.history_state.as_ref().is_some_and(|state| state.can_undo), |ui| {
                if design::icon_button(ui,design::Icon::Undo,"Undo last committed edit (Ctrl+Z)").clicked() { self.session.undo(); }
            });
            ui.add_enabled_ui(self.session.connected && !self.session.edit_busy && self.session.history_state.as_ref().is_some_and(|state| state.can_redo), |ui| {
                if design::icon_button(ui,design::Icon::Redo,"Redo (Ctrl+Y or Ctrl+Shift+Z)").clicked() { self.session.redo(); }
            });
            if self.session.edit_busy || self.session.imports.iter().any(|card| card.stage == ImportStage::Committing) {
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
                        .is_some_and(|p| p.visual_duration_frames() > 0),
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
                        self.fullscreen = false;
                        self.drawers = Drawers::default();
                        self.session.timeline = Timeline::default();
                        ui.close();
                    }
                    ui.label(
                        RichText::new(
                            "Space: play / pause\nLeft / Right: step one frame\nShift + Left / Right: seek 5 seconds\nF11: full screen\nDrop files to import",
                        )
                        .small()
                        .color(design::MUTED),
                    );
                });
            });
        });
    }

    fn editor(&mut self, ui: &mut egui::Ui, bounds: egui::Rect) {
        let compact = bounds.width() < 1160.0;
        let (left_drawer, right_drawer) = self.drawers.visible(compact);
        let visual_rows = self
            .session
            .project
            .as_ref()
            .filter(|p| !p.visual_tracks.is_empty() || p.uses_composition())
            .map_or(0, |p| p.visual_tracks.len() + 3);
        let layout = EditorLayout::new(bounds, self.drawers).with_visual_rows(visual_rows);
        pane(ui, "left-rail", layout.left_rail, 4, |ui| {
            if design::rail_button(
                ui,
                design::Icon::Media,
                "My media",
                left_drawer == Some(LeftDrawer::Media),
            )
            .clicked()
            {
                self.drawers.toggle_left(LeftDrawer::Media, compact);
            }
            if design::rail_button(
                ui,
                design::Icon::Library,
                "Library",
                left_drawer == Some(LeftDrawer::Library),
            )
            .clicked()
            {
                self.drawers.toggle_left(LeftDrawer::Library, compact);
            }
        });
        pane(ui, "right-rail", layout.right_rail, 4, |ui| {
            if design::rail_button(
                ui,
                design::Icon::Clip,
                "Clip",
                right_drawer == Some(RightDrawer::Clip),
            )
            .clicked()
            {
                self.drawers.toggle_right(RightDrawer::Clip, compact);
            }
            if design::rail_button(
                ui,
                design::Icon::Project,
                "Project",
                right_drawer == Some(RightDrawer::Project),
            )
            .clicked()
            {
                self.drawers.toggle_right(RightDrawer::Project, compact);
            }
        });
        if let Some(rect) = layout.left {
            pane(ui, "resource-drawer", rect, 12, |ui| {
                if drawer_header(
                    ui,
                    if left_drawer == Some(LeftDrawer::Library) {
                        "Content library"
                    } else {
                        "My media"
                    },
                ) {
                    self.drawers.left = None;
                }
                match left_drawer {
                    Some(LeftDrawer::Library) => {
                        if self.session.library.show(
                            ui,
                            self.session.epoch,
                            self.session.connected && !self.session.edit_busy,
                        ) && let Some(preview) = &self.session.preview
                            && let Err(error) = preview.pause()
                        {
                            self.session
                                .fail(format!("Pause timeline for audition: {error}"));
                        }
                    }
                    _ => self.session.media_panel(ui),
                }
            });
        }
        if let Some(rect) = layout.right {
            pane(ui, "property-drawer", rect, 12, |ui| {
                if drawer_header(
                    ui,
                    if right_drawer == Some(RightDrawer::Clip) {
                        "Clip"
                    } else {
                        "Project"
                    },
                ) {
                    self.drawers.right = None;
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match right_drawer {
                        Some(RightDrawer::Clip) => self.session.inspector(ui),
                        Some(RightDrawer::Project) => {
                            self.session.project_panel(ui);
                            ui.add_space(12.0);
                            if ui.button("History & project tools").clicked() {
                                self.tools_open = true;
                            }
                        }
                        None => {}
                    });
            });
        }
        pane(ui, "preview", layout.preview, 12, |ui| {
            self.session.preview_panel(ui, &mut self.fullscreen)
        });
        pane(ui, "timeline", layout.timeline, 8, |ui| {
            self.session.timeline_panel(ui)
        });
        if let Some(payload) = egui::DragAndDrop::payload::<timeline::DragPayload>(ui.ctx())
            && let Some(pointer) = ui.input(|input| input.pointer.hover_pos())
            && let Some(project) = &self.session.project
        {
            let media_id = match payload.source {
                timeline::DragSource::VisualClip { clip, .. } => project
                    .visual_clips
                    .iter()
                    .find(|c| c.id == clip)
                    .map(|c| c.media_id)
                    .or_else(|| {
                        project
                            .clips
                            .iter()
                            .find(|c| c.id == clip)
                            .map(|c| c.media_id)
                    }),
                timeline::DragSource::Media(id) => Some(id),
                timeline::DragSource::Clip(id) => project
                    .clips
                    .iter()
                    .find(|clip| clip.id == id)
                    .map(|clip| clip.media_id),
                timeline::DragSource::AudioClip { clip, .. } => project
                    .audio_clips
                    .iter()
                    .find(|item| item.id == clip)
                    .map(|item| item.media_id),
            };
            if let Some(media) = project
                .media
                .iter()
                .find(|media| Some(media.id) == media_id)
            {
                let painter = ui.ctx().layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::new("clip-drag-preview"),
                ));
                let rect = egui::Rect::from_min_size(
                    pointer + Vec2::new(16.0, 20.0),
                    Vec2::new(172.0, 52.0),
                );
                painter.rect_filled(rect, 5.0, design::PANEL);
                painter.rect_stroke(
                    rect,
                    5.0,
                    Stroke::new(1.0, design::ACCENT),
                    egui::StrokeKind::Inside,
                );
                painter.with_clip_rect(rect.shrink(8.0)).text(
                    rect.left_top() + Vec2::splat(10.0),
                    egui::Align2::LEFT_TOP,
                    media.display_name(),
                    egui::FontId::proportional(13.0),
                    design::TEXT,
                );
                painter.text(
                    rect.left_bottom() + Vec2::new(10.0, -10.0),
                    egui::Align2::LEFT_BOTTOM,
                    if matches!(payload.source, timeline::DragSource::AudioClip { .. })
                        || media.metadata.video.is_none()
                    {
                        "Drop on Audio 1"
                    } else {
                        "Drag to a clip boundary"
                    },
                    egui::FontId::proportional(11.0),
                    design::MUTED,
                );
            }
        }
        ui.advance_cursor_after_rect(bounds);
    }
}

fn drawer_header(ui: &mut egui::Ui, title: &str) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong().size(17.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close = design::icon_button(ui, design::Icon::Close, "Close sidebar").clicked();
        });
    });
    ui.separator();
    close
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
        if !ui.input(|input| input.focused)
            || self.export_open
            || self.tools_open
            || self.fullscreen
            || self.session.media_rename.is_some()
            || self.session.picker.is_some()
        {
            egui::DragAndDrop::clear_payload(ui.ctx());
            self.session.timeline.cancel_gesture();
        }
        let started = Instant::now();
        let ctx = ui.ctx().clone();
        self.session.viewport_size = ctx.content_rect().size();
        self.session.pixels_per_point = ctx.pixels_per_point();
        self.session.native_fullscreen = ctx.input(|input| input.viewport().fullscreen);
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
                self.session.import_media(paths);
            }
        }
        if self.session.media_rename.is_some()
            && ctx.input(|input| input.key_pressed(egui::Key::Escape))
        {
            self.session.media_rename = None;
        }
        if !ctx.egui_wants_keyboard_input() && self.session.media_rename.is_none() {
            if !self.export_open
                && !self.tools_open
                && self.session.media_rename.is_none()
                && !self.fullscreen
                && !self.session.edit_busy
                && self.session.connected
            {
                if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z)) {
                    if ctx.input(|i| i.modifiers.shift) {
                        self.session.redo();
                    } else {
                        self.session.undo();
                    }
                } else if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Y)) {
                    self.session.redo();
                } else if let Some(text) = ctx.input(|i| {
                    i.events.iter().find_map(|event| {
                        if let egui::Event::Paste(text) = event {
                            Some(text.clone())
                        } else {
                            None
                        }
                    })
                }) {
                    self.session.paste_clip(text.as_bytes());
                } else if ctx.input(|i| {
                    i.events
                        .iter()
                        .any(|event| matches!(event, egui::Event::Copy))
                }) && let Some(id) = self.session.selected
                {
                    self.session.copy_clip(&ctx, id);
                }
            }
            if !self.export_open
                && !self.tools_open
                && self.session.media_rename.is_none()
                && !self.fullscreen
                && !self.session.edit_busy
                && let Some(clip_id) = self.session.selected
            {
                if ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
                    if self
                        .session
                        .project
                        .as_ref()
                        .is_some_and(|project| project.visual_clips.iter().any(|c| c.id == clip_id))
                    {
                        self.session
                            .edit(Edit::DeleteVisualClip { clip_id }, "Delete visual clip");
                    } else if self.session.project.as_ref().is_some_and(|project| {
                        project.audio_clips.iter().any(|clip| clip.id == clip_id)
                    }) {
                        self.session
                            .edit(Edit::DeleteAudioClip { clip_id }, "Delete audio");
                    } else {
                        self.session.edit(
                            Edit::DeleteClip {
                                clip_id,
                                ripple: !self
                                    .session
                                    .project
                                    .as_ref()
                                    .is_some_and(Project::uses_composition),
                            },
                            "Delete clip",
                        );
                    }
                } else if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::D)) {
                    if self
                        .session
                        .project
                        .as_ref()
                        .is_some_and(|p| p.visual_clips.iter().any(|c| c.id == clip_id))
                    {
                        self.session.fail("Visual duplication is not available yet; add the source to the layer again.".into());
                    } else if self.session.project.as_ref().is_some_and(|project| {
                        project.audio_clips.iter().any(|clip| clip.id == clip_id)
                    }) {
                        self.session.fail(
                            "Duplication for this audio/overlay selection is not available; add the source again.".into(),
                        );
                    } else {
                        self.session.edit(
                            Edit::DuplicateClip {
                                clip_id,
                                id: Uuid::new_v4(),
                            },
                            "Duplicate clip",
                        );
                    }
                } else if ctx.input(|i| !i.modifiers.command && i.key_pressed(egui::Key::S)) {
                    if self
                        .session
                        .project
                        .as_ref()
                        .is_some_and(|p| p.visual_clips.iter().any(|c| c.id == clip_id))
                    {
                        self.session.fail(
                            "Overlay splitting is not available yet; use its range controls."
                                .into(),
                        );
                    } else if self.session.project.as_ref().is_some_and(|project| {
                        project.audio_clips.iter().any(|clip| clip.id == clip_id)
                    }) {
                        self.session.fail(
                            "Audio split is not implemented; use source trim controls.".into(),
                        );
                    } else {
                        self.session.edit(
                            Edit::SplitClip {
                                clip_id,
                                right_clip_id: Uuid::new_v4(),
                                at_frame: self.session.playhead,
                            },
                            "Split clip",
                        );
                    }
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
                self.session.toggle_play();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                if ctx.input(|i| i.modifiers.shift) {
                    self.session.jump_seconds(-5);
                } else {
                    self.session.step(-1);
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                if ctx.input(|i| i.modifiers.shift) {
                    self.session.jump_seconds(5);
                } else {
                    self.session.step(1);
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                if self.fullscreen {
                    self.fullscreen = false;
                } else if self.export_open {
                    self.export_open = false;
                } else if self.tools_open {
                    self.tools_open = false;
                } else {
                    match self.drawers.active {
                        RailSide::Left => self.drawers.left = None,
                        RailSide::Right => self.drawers.right = None,
                    }
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F11))
                && self
                    .session
                    .project
                    .as_ref()
                    .is_some_and(|project| project.visual_duration_frames() > 0)
            {
                self.fullscreen = !self.fullscreen;
            }
        }
        let mut bounds = ui.available_rect_before_wrap();
        if self.fullscreen {
            pane(ui, "fullscreen-preview", bounds, 12, |ui| {
                self.session.preview_panel(ui, &mut self.fullscreen)
            });
            ui.advance_cursor_after_rect(bounds);
        } else {
            let header = egui::Rect::from_min_size(bounds.min, Vec2::new(bounds.width(), 58.0));
            pane(ui, "header", header, 8, |ui| self.header(ui));
            bounds.min.y = header.bottom();
            if let Some(error) = self.session.error.clone() {
                let rect = egui::Rect::from_min_size(bounds.min, Vec2::new(bounds.width(), 44.0));
                pane(ui, "error", rect, 4, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Dismiss").clicked() {
                            self.session.error = None;
                        }
                        ui.add(
                            egui::Label::new(RichText::new(&error).color(design::WARNING))
                                .truncate(),
                        )
                        .on_hover_text(&error);
                    });
                });
                bounds.min.y = rect.bottom();
            }
            self.editor(ui, bounds);
        }
        if self.fullscreen_applied != self.fullscreen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
            self.fullscreen_applied = self.fullscreen;
            ctx.request_repaint();
        }
        if self.session.project.is_none() && !self.session.project_input.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.export_open && !self.fullscreen {
            egui::Window::new("Export video")
                .open(&mut self.export_open)
                .collapsible(false)
                .resizable(false)
                .default_width(640.0)
                .max_height((ctx.content_rect().height() - 80.0).max(360.0))
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(&ctx, |ui| self.session.render_panel(ui));
        }
        if !self.fullscreen {
            self.session.rename_panel(&ctx);
        }
        if self.tools_open && !self.fullscreen {
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
            let ready = if self.capture_state == CaptureState::Importing {
                self.session.connected
                    && self
                        .session
                        .imports
                        .iter()
                        .any(|card| !card.stage.is_terminal())
            } else if self.capture_state == CaptureState::Empty
                || self.capture_state == CaptureState::Project
                    && self
                        .session
                        .project
                        .as_ref()
                        .is_some_and(|project| project.visual_duration_frames() == 0)
            {
                if self.session.project_input.is_empty() {
                    self.session.project.is_none()
                } else {
                    self.session.connected
                        && self
                            .session
                            .project
                            .as_ref()
                            .is_some_and(|project| project.visual_duration_frames() == 0)
                }
            } else if let Some(project) = &self.session.project {
                let revision = project.revision;
                if self.capture_state == CaptureState::Media {
                    self.session.selected_media = project
                        .media
                        .iter()
                        .find(|media| media.metadata.video.is_some() && !still_png(media))
                        .map(|media| media.id);
                }
                let target = self
                    .capture_frame
                    .min(timeline::total_frames(project).saturating_sub(1));
                let clip = if self.capture_state == CaptureState::Audio {
                    project.audio_clips.first().map(|clip| clip.id)
                } else if self.capture_state == CaptureState::Visual {
                    project
                        .visual_clips
                        .iter()
                        .rev()
                        .find(|clip| {
                            clip.at_frame <= target && target < clip.at_frame + clip.duration_frames
                        })
                        .map(|clip| clip.id)
                } else {
                    project
                        .clips
                        .iter()
                        .find(|c| c.at_frame <= target && target < c.at_frame + c.duration_frames)
                        .map(|c| c.id)
                };
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
                    && (self.capture_state != CaptureState::Fullscreen
                        || self.session.native_fullscreen == Some(true))
                    && (self.capture_state != CaptureState::Library
                        || self.session.library.thumbnail_count() > 0)
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

pub(crate) fn media_card_drag(
    response: &egui::Response,
    editable: bool,
    payload: timeline::DragPayload,
) {
    // Verified PNGs, video and audio share the source gesture. Target rows own
    // compatibility checks, so PNGs remain rejected by Video 1 and Audio 1.
    if editable && response.drag_started_by(egui::PointerButton::Primary) {
        response.dnd_set_drag_payload(payload);
    }
}

fn media_footer_height(ui: &egui::Ui, buttons: usize, show_details: bool) -> f32 {
    let spacing = ui.spacing().item_spacing.y;
    let button_height = ui.spacing().interact_size.y.max(36.0);
    buttons as f32 * (button_height + spacing)
        + if show_details {
            ui.text_style_height(&egui::TextStyle::Small) + spacing
        } else {
            0.0
        }
        + spacing
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

    #[test]
    fn drawer_toggles_restore_context_across_compact_layouts() {
        let mut drawers = Drawers::default();
        assert_eq!(drawers.visible(true), (Some(LeftDrawer::Media), None));
        drawers.toggle_right(RightDrawer::Clip, true);
        assert_eq!(drawers.visible(true), (None, Some(RightDrawer::Clip)));
        assert_eq!(
            drawers.visible(false),
            (Some(LeftDrawer::Media), Some(RightDrawer::Clip))
        );
        drawers.toggle_right(RightDrawer::Project, true);
        assert_eq!(drawers.visible(true), (None, Some(RightDrawer::Project)));
        drawers.toggle_right(RightDrawer::Project, true);
        assert_eq!(drawers.visible(true), (None, None));
        drawers.toggle_left(LeftDrawer::Media, true);
        assert_eq!(drawers.visible(true), (Some(LeftDrawer::Media), None));
        drawers.toggle_left(LeftDrawer::Media, true);
        assert_eq!(drawers.visible(true), (None, None));
        drawers.toggle_left(LeftDrawer::Media, true);
        assert_eq!(drawers.visible(true), (Some(LeftDrawer::Media), None));
    }

    #[test]
    fn scrollable_media_keeps_both_footer_actions_inside_short_drawers() {
        for height in [180.0, 300.0, 500.0] {
            let ctx = egui::Context::default();
            design::configure(&ctx);
            let mut rects = Vec::new();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(260.0, height),
                    )),
                    ..Default::default()
                },
                |ui| {
                    ui.scope(|ui| {
                        let bounds = ui.available_rect_before_wrap();
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .max_height(
                                (ui.available_height() - media_footer_height(ui, 2, true)).max(0.0),
                            )
                            .show(ui, |ui| {
                                for _ in 0..30 {
                                    ui.allocate_space(Vec2::new(100.0, 60.0));
                                }
                            });
                        ui.label(RichText::new("Source 60 fps - timeline 60 fps").small());
                        for label in ["Add to timeline", "Add video to layer"] {
                            let response = ui.add(
                                egui::Button::new(label)
                                    .min_size(Vec2::new(ui.available_width(), 36.0)),
                            );
                            rects.push((bounds, response.rect));
                        }
                    });
                },
            );
            assert!(!output.shapes.is_empty());
            output.textures_delta.clear();
            assert_eq!(rects.len(), 2);
            for (bounds, button) in rects {
                assert!(
                    bounds.contains_rect(button),
                    "{height}: {button:?} in {bounds:?}"
                );
            }
        }
    }

    #[test]
    fn real_media_panel_drags_png_video_and_audio_only_when_editable() -> lumeryn_core::Result<()> {
        use clap::Parser;
        use lumeryn_core::{
            media::{Audio, Metadata, Video},
            model::Media,
        };
        for (codec, connected, busy, button) in [
            ("png", true, false, egui::PointerButton::Primary),
            ("ffv1", true, false, egui::PointerButton::Primary),
            ("audio", true, false, egui::PointerButton::Primary),
            ("png", false, false, egui::PointerButton::Primary),
            ("png", true, true, egui::PointerButton::Primary),
            ("png", true, false, egui::PointerButton::Secondary),
        ] {
            let ctx = egui::Context::default();
            design::configure(&ctx);
            let options = Options::parse_from(["studio"]);
            let cc = eframe::CreationContext::_new_kittest(ctx.clone());
            let mut session = Session::new(&cc, &options);
            session.assets = Some(Assets::empty());
            let mut project = Project::new("Drag fixture".into(), 320, 180, "30".parse()?)?;
            let project_id = project.id;
            let media_id = Uuid::new_v4();
            project.media.push(Media {
                id: media_id,
                path: format!("source.{codec}").into(),
                alias: None,
                metadata: Metadata {
                    size_bytes: 1,
                    blake3: Some("a".repeat(64)),
                    video: (codec != "audio").then(|| Video {
                        codec: codec.into(),
                        width: 320,
                        height: 180,
                        frame_rate: project.frame_rate,
                        frames: if codec == "png" { 1 } else { 90 },
                        start_time_micros: 0,
                        sample_aspect_ratio: "1:1".into(),
                        color_transfer: None,
                        constant_rate: true,
                    }),
                    audio: (codec == "audio").then(|| Audio {
                        codec: "pcm_s16le".into(),
                        sample_rate: 48_000,
                        channels: 2,
                        start_time_micros: 0,
                        canonical_samples_48k: Some(48_000),
                    }),
                },
            });
            session.project = Some(project);
            session.connected = connected;
            session.edit_busy = busy;
            let mut pass = |time, events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(260.0, 640.0),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ui| session.media_panel(ui),
                )
            };
            let mut output = pass(0.0, vec![]);
            let caption = match codec {
                "png" => "Image",
                "audio" => "Audio",
                _ => "Video",
            };
            let center = output
                .shapes
                .iter()
                .find_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape
                        && text.galley.job.text == caption
                    {
                        Some(text.pos + text.galley.size() * 0.5)
                    } else {
                        None
                    }
                })
                .expect("real media card painted its stream caption");
            output.textures_delta.clear();
            let mut output = pass(
                0.02,
                vec![
                    egui::Event::PointerMoved(center),
                    egui::Event::PointerButton {
                        pos: center,
                        button,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            output.textures_delta.clear();
            let mut output = pass(
                0.04,
                vec![egui::Event::PointerMoved(center + Vec2::new(30.0, 0.0))],
            );
            output.textures_delta.clear();
            let payload = egui::DragAndDrop::payload::<timeline::DragPayload>(&ctx);
            assert!(session.send(Command::Shutdown));
            if let Some(preview) = &session.preview {
                preview.shutdown();
            }
            if connected && !busy && button == egui::PointerButton::Primary {
                let payload = payload.expect("actual verified media card starts a primary drag");
                assert_eq!(payload.project, project_id);
                assert_eq!(payload.revision, 0);
                assert_eq!(payload.epoch, session.epoch);
                assert!(
                    matches!(payload.source, timeline::DragSource::Media(id) if id == media_id)
                );
            } else {
                assert!(
                    payload.is_none(),
                    "{codec}: connected={connected} busy={busy} button={button:?}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn native_rail_layout_keeps_preview_and_timeline_reachable() {
        for size in [
            Vec2::new(1440.0, 842.0),
            Vec2::new(1280.0, 742.0),
            Vec2::new(980.0, 582.0),
        ] {
            for active in [RailSide::Left, RailSide::Right] {
                let bounds = egui::Rect::from_min_size(egui::pos2(0.0, 58.0), size);
                let layout = EditorLayout::new(
                    bounds,
                    Drawers {
                        active,
                        ..Default::default()
                    },
                );
                for rect in [
                    Some(layout.left_rail),
                    Some(layout.right_rail),
                    layout.left,
                    layout.right,
                    Some(layout.preview),
                    Some(layout.timeline),
                ]
                .into_iter()
                .flatten()
                {
                    assert!(bounds.contains_rect(rect));
                }
                assert!(layout.preview.width() >= 560.0);
                assert!(layout.preview.height() >= 300.0);
                assert!(layout.timeline.height() >= 220.0);
                assert_eq!(layout.preview.bottom(), layout.timeline.top());
                if size.x < 1160.0 {
                    assert!(layout.left.is_none() || layout.right.is_none());
                }
            }
        }
    }

    #[test]
    fn five_second_seeks_use_rational_rate_and_clamp() -> lumeryn_core::Result<()> {
        let rate = lumeryn_core::time::FrameRate::new(30_000, 1001)?;
        assert_eq!(seek_seconds(200, rate, -5, 1000), 50);
        assert_eq!(seek_seconds(200, rate, 5, 1000), 350);
        assert_eq!(seek_seconds(10, rate, -5, 1000), 0);
        assert_eq!(seek_seconds(990, rate, 5, 1000), 999);
        assert_eq!(seek_seconds(990, rate, 5, 0), 0);
        Ok(())
    }
}
