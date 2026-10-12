//! Offline catalogue I/O and integrity work never runs on the GUI thread.
use egui::{Context, RichText};
use lumeryn_core::library::{AssetEntry, AssetKind, Catalog, default_root};
use lumeryn_preview::{AuditionPlayer, AuditionState};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::mpsc,
};

enum Request {
    Import { id: String, epoch: u64 },
    Thumbnail { id: String, index: usize },
    Audition { id: String, generation: u64 },
}
enum Reply {
    Loaded(Vec<AssetEntry>),
    Imported {
        path: PathBuf,
        epoch: u64,
    },
    Error(String),
    Thumbnail {
        index: usize,
        image: egui::ColorImage,
    },
    ThumbnailError {
        index: usize,
        error: String,
    },
    Audition {
        path: PathBuf,
        generation: u64,
    },
}
pub struct Library {
    requests: mpsc::SyncSender<Request>,
    replies: mpsc::Receiver<Reply>,
    entries: Vec<AssetEntry>,
    query: String,
    matches: Vec<usize>,
    pub status: String,
    importing: bool,
    kind: Option<AssetKind>,
    thumbnails: HashMap<usize, egui::TextureHandle>,
    thumbnail_order: VecDeque<usize>,
    requested: HashSet<usize>,
    failed: HashSet<usize>,
    audition: Result<AuditionPlayer, String>,
    audition_generation: u64,
}
impl Library {
    pub fn new(ctx: Context, root: Option<PathBuf>, tools: lumeryn_core::MediaTools) -> Self {
        let audition = AuditionPlayer::new(tools)
            .and_then(|player| {
                player.set_volume(0.8)?;
                Ok(player)
            })
            .map_err(|error| error.to_string());
        let (requests, jobs) = mpsc::sync_channel::<Request>(4);
        let (results, replies) = mpsc::sync_channel::<Reply>(4);
        let spawned = std::thread::Builder::new()
            .name("lumeryn-library".into())
            .spawn(move || {
                let catalog = root.map_or_else(default_root, Ok).and_then(Catalog::load);
                let catalog = match catalog {
                    Ok(catalog) => catalog,
                    Err(error) => {
                        ctx.request_repaint();
                        if let Err(error) = results.send(Reply::Error(error.to_string())) {
                            eprintln!("Library notification: {error}");
                        }
                        ctx.request_repaint();
                        return;
                    }
                };
                ctx.request_repaint();
                if results
                    .send(Reply::Loaded(catalog.entries().to_vec()))
                    .is_err()
                {
                    return;
                }
                ctx.request_repaint();
                while let Ok(request) = jobs.recv() {
                    let reply = match request {
                        Request::Thumbnail { id, index } => match thumbnail(&catalog, &id) {
                            Ok(image) => Reply::Thumbnail { index, image },
                            Err(error) => Reply::ThumbnailError { index, error },
                        },
                        Request::Audition { id, generation } => match catalog.get(&id).and_then(|entry| {
                            if entry.kind != AssetKind::Audio { return Err(lumeryn_core::Error::Unsupported("Audio audition requires an audio asset".into())); }
                            catalog.verified_path(&id)
                        }) {
                            Ok(path) => Reply::Audition { path, generation },
                            Err(error) => Reply::Error(error.to_string()),
                        },
                        Request::Import { id, epoch } => match catalog.get(&id) {
                        Ok(entry) if matches!(entry.kind, AssetKind::Audio | AssetKind::Image) => {
                            match catalog.verified_path(&id) {
                                Ok(path) => Reply::Imported { path, epoch },
                                Err(error) => Reply::Error(error.to_string()),
                            }
                        }
                        Ok(_) => Reply::Error(
                            "Fonts and SVG placement need the upcoming typography/vector renderer"
                                .into(),
                        ),
                        Err(error) => Reply::Error(error.to_string()),
                        },
                    };
                    ctx.request_repaint();
                    if results.send(reply).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            });
        Self {
            requests,
            replies,
            entries: Vec::new(),
            query: String::new(),
            matches: Vec::new(),
            status: spawned.map_or_else(
                |e| format!("Library worker: {e}"),
                |_| "Loading local library…".into(),
            ),
            importing: false,
            kind: Some(AssetKind::Audio),
            thumbnails: HashMap::new(),
            thumbnail_order: VecDeque::new(),
            requested: HashSet::new(),
            failed: HashSet::new(),
            audition,
            audition_generation: 0,
        }
    }
    pub fn poll(&mut self, ctx: &Context) -> Vec<(PathBuf, u64)> {
        let mut imports = Vec::new();
        for _ in 0..4 {
            let Ok(reply) = self.replies.try_recv() else {
                break;
            };
            match reply {
                Reply::Loaded(entries) => {
                    self.entries = entries;
                    self.filter();
                    self.status = format!("{} local assets", self.entries.len());
                }
                Reply::Imported { path, epoch } => {
                    self.importing = false;
                    imports.push((path, epoch));
                }
                Reply::Error(error) => {
                    self.importing = false;
                    self.status = error;
                }
                Reply::Thumbnail { index, image } => {
                    self.requested.remove(&index);
                    if self.thumbnails.len() >= 64
                        && let Some(old) = self.thumbnail_order.pop_front()
                    {
                        self.thumbnails.remove(&old);
                    }
                    self.thumbnail_order.push_back(index);
                    self.thumbnails.insert(
                        index,
                        ctx.load_texture(
                            format!("library-{index}"),
                            image,
                            egui::TextureOptions::LINEAR,
                        ),
                    );
                }
                Reply::ThumbnailError { index, error } => {
                    self.requested.remove(&index);
                    self.failed.insert(index);
                    self.status = error;
                }
                Reply::Audition { path, generation } if generation == self.audition_generation => {
                    match &self.audition {
                        Ok(player) => {
                            if let Err(error) = player.play(path, None) {
                                self.status = error.to_string();
                            }
                        }
                        Err(error) => self.status = error.clone(),
                    }
                }
                Reply::Audition { .. } => {}
            }
        }
        if !self.requested.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        if let Ok(player) = &self.audition {
            let status = player.status();
            if matches!(
                status.state,
                AuditionState::Playing | AuditionState::Loading
            ) {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
        }
        imports
    }
    fn filter(&mut self) {
        let query = self.query.to_lowercase();
        self.matches = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                (self.kind.is_none() || self.kind == Some(entry.kind))
                    && (entry.name.to_lowercase().contains(&query)
                        || entry.category.to_lowercase().contains(&query)
                        || entry
                            .tags
                            .iter()
                            .any(|tag| tag.to_lowercase().contains(&query)))
            })
            .map(|(index, _)| index)
            .collect();
    }
    pub fn show(&mut self, ui: &mut egui::Ui, epoch: u64, can_import: bool) -> bool {
        let mut audition_requested = false;
        let mut stop_requested = false;
        ui.label(RichText::new(&self.status).color(crate::design::MUTED));
        if ui
            .add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("Search sounds, music, images…")
                    .char_limit(256)
                    .desired_width(f32::INFINITY),
            )
            .changed()
        {
            self.filter();
        }
        let previous = self.kind;
        egui::ComboBox::from_id_salt("library-kind")
            .selected_text(match self.kind {
                None => "All assets",
                Some(AssetKind::Audio) => "Sounds & music",
                Some(AssetKind::Image) => "Images",
                Some(AssetKind::Font) => "Fonts",
                Some(AssetKind::Svg) => "Vectors",
            })
            .show_ui(ui, |ui| {
                for (kind, label) in [
                    (None, "All assets"),
                    (Some(AssetKind::Audio), "Sounds & music"),
                    (Some(AssetKind::Image), "Images"),
                    (Some(AssetKind::Font), "Fonts"),
                ] {
                    ui.selectable_value(&mut self.kind, kind, label);
                }
            });
        if self.kind != previous {
            self.filter();
        }
        if let Ok(player) = &self.audition {
            let status = player.status();
            if matches!(
                status.state,
                AuditionState::Playing | AuditionState::Loading
            ) {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Audition {:.1}s / 30s",
                        status.position_millis as f64 / 1000.0
                    ));
                    stop_requested = ui.button("Stop").clicked();
                });
            } else if let Some(error) = status.error {
                ui.label(egui::RichText::new(error).small());
            }
        }
        if stop_requested {
            self.stop_audition();
        }
        ui.label(RichText::new("Import to My media, then drag sounds onto Audio 1. Create a layer in My media to place an imported PNG.").small().color(crate::design::MUTED));
        ui.add_space(8.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, 140.0, self.matches.len(), |ui, rows| {
                for row in rows {
                    let entry = &self.entries[self.matches[row]];
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), 140.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.push_id(&entry.id, |ui| {
                                ui.horizontal(|ui| {
                                    if entry.kind == AssetKind::Image {
                                        if let Some(texture) =
                                            self.thumbnails.get(&self.matches[row])
                                        {
                                            ui.add(
                                                egui::Image::new(texture)
                                                    .max_size(egui::vec2(56.0, 42.0)),
                                            );
                                        } else {
                                            ui.allocate_space(egui::vec2(56.0, 42.0));
                                            let index = self.matches[row];
                                            if !self.requested.contains(&index)
                                                && !self.failed.contains(&index)
                                                && self.requested.len() < 4
                                                && self
                                                    .requests
                                                    .try_send(Request::Thumbnail {
                                                        id: entry.id.clone(),
                                                        index,
                                                    })
                                                    .is_ok()
                                            {
                                                self.requested.insert(index);
                                            }
                                        }
                                    }
                                    ui.add(
                                        egui::Label::new(RichText::new(&entry.name).strong())
                                            .truncate(),
                                    )
                                    .on_hover_text(format!(
                                        "{}\n{}\n{}",
                                        entry.id, entry.attribution, entry.license_id
                                    ));
                                });
                                let detail = entry.duration_ms.map_or_else(
                                    || category_label(&entry.category).into(),
                                    |ms| {
                                        format!(
                                            "{} · {:.1}s",
                                            category_label(&entry.category),
                                            ms as f64 / 1000.0
                                        )
                                    },
                                );
                                ui.label(RichText::new(detail).small().color(crate::design::MUTED));
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&entry.license_id).small());
                                    if entry.kind == AssetKind::Audio
                                        && crate::design::icon_button(
                                            ui,
                                            crate::design::Icon::Play,
                                            "Audition audio (up to 30 seconds)",
                                        )
                                        .clicked()
                                    {
                                        let generation = self.audition_generation.saturating_add(1);
                                        match self.requests.try_send(Request::Audition {
                                            id: entry.id.clone(),
                                            generation,
                                        }) {
                                            Ok(()) => {
                                                self.audition_generation = generation;
                                                audition_requested = true;
                                            }
                                            Err(error) => {
                                                self.status = format!("Audition request: {error}")
                                            }
                                        }
                                    }
                                    let supported =
                                        matches!(entry.kind, AssetKind::Audio | AssetKind::Image);
                                    if ui
                                        .add_enabled(
                                            can_import && !self.importing && supported,
                                            egui::Button::new("Import"),
                                        )
                                        .clicked()
                                    {
                                        match self.requests.try_send(Request::Import {
                                            id: entry.id.clone(),
                                            epoch,
                                        }) {
                                            Ok(()) => self.importing = true,
                                            Err(error) => {
                                                self.status = format!("Library request: {error}")
                                            }
                                        }
                                    }
                                    ui.hyperlink_to("Source", &entry.source_url);
                                });
                                ui.separator();
                            });
                        },
                    );
                }
            });
        audition_requested
    }
    pub fn count(&self) -> usize {
        self.entries.len()
    }
    pub fn focus_images(&mut self) {
        self.kind = Some(AssetKind::Image);
        self.filter();
    }
    pub fn thumbnail_count(&self) -> usize {
        self.thumbnails.len()
    }
    pub fn stop_audition(&mut self) {
        self.audition_generation = self.audition_generation.saturating_add(1);
        if let Ok(player) = &self.audition
            && let Err(error) = player.stop()
        {
            self.status = error.to_string();
        }
    }
}

fn thumbnail(catalog: &Catalog, id: &str) -> Result<egui::ColorImage, String> {
    let entry = catalog.get(id).map_err(|error| error.to_string())?;
    if entry.kind != AssetKind::Image {
        return Err("Thumbnail requires an image".into());
    }
    let path = catalog
        .verified_path(id)
        .map_err(|error| error.to_string())?;
    let mut reader = image::ImageReader::open(path).map_err(|error| error.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|error| error.to_string())?
        .thumbnail(128, 80)
        .to_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

fn category_label(category: &str) -> &str {
    match category {
        "cue" => "Sound effect",
        "jingle" => "Music jingle",
        "music-loop" => "Music loop",
        "background-element" => "Illustration",
        "font" => "Font",
        _ => category,
    }
}
