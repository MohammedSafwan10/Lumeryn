//! Disposable, bounded presentation cache. Asset decoding never runs on the UI thread.
use lumeryn_core::{
    Error, MediaTools, Project, Result,
    media::{content_hash, local_file},
    model::Media,
};
use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    process::Command,
    sync::{mpsc, watch},
};
use uuid::Uuid;

const WIDTH: usize = 192;
const HEIGHT: usize = 108;
const MAX_ASSETS: usize = 128;
const MAX_PEAKS: usize = 1024;
const SAMPLE_RATE: u32 = 8000;

/// Absolute mono peak amplitude, derived from actual source PCM (not a synthetic decoration).
#[derive(Debug)]
pub struct Waveform {
    pub peaks: Vec<f32>,
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub max_peak: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    path: PathBuf,
    digest: Option<String>,
    size: u64,
}
struct Entry {
    key: Key,
    media: Media,
    state: State,
}
enum State {
    Queued,
    Loading,
    Ready {
        thumbnail: Option<egui::TextureHandle>,
        waveform: Option<Waveform>,
        error: Option<String>,
    },
}
struct Job {
    id: Uuid,
    key: Key,
    media: Media,
    epoch: u64,
}
struct Decoded {
    id: Uuid,
    epoch: u64,
    key: Key,
    image: Option<Vec<u8>>,
    waveform: Option<Waveform>,
    error: Option<String>,
}

pub struct Assets {
    entries: HashMap<Uuid, Entry>,
    jobs: mpsc::Sender<Job>,
    results: mpsc::Receiver<Decoded>,
    cancel: watch::Sender<u64>,
    epoch: u64,
}

impl Assets {
    /// Empty cache for headless timeline layout measurements; starts no worker or media process.
    #[cfg(test)]
    pub fn empty() -> Self {
        let (jobs, receiver) = mpsc::channel(1);
        let (sender, results) = mpsc::channel(1);
        let (cancel, _) = watch::channel(0);
        drop(receiver);
        drop(sender);
        Self {
            entries: HashMap::new(),
            jobs,
            results,
            cancel,
            epoch: 0,
        }
    }
    pub fn new(ctx: &egui::Context, tools: MediaTools) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (jobs, mut receiver) = mpsc::channel::<Job>(8);
        let (sender, results) = mpsc::channel(8);
        let (cancel, mut cancelled) = watch::channel(0);
        let repaint = ctx.clone();
        std::thread::Builder::new()
            .name("lumeryn-assets".into())
            .spawn(move || {
                runtime.block_on(async move {
            loop {
                let job = tokio::select! {
                    biased;
                    change = cancelled.changed() => { if change.is_err() { break; } continue; },
                    job = receiver.recv() => { let Some(job) = job else { break; }; job }
                };
                if job.epoch != *cancelled.borrow_and_update() { repaint.request_repaint(); continue; }
                let result = tokio::select! {
                    biased;
                    _ = cancelled.changed() => continue,
                    result = decode(job, &tools) => result
                };
                let sent = tokio::select! {
                    biased;
                    _ = cancelled.changed() => continue,
                    result = sender.send(result) => result
                };
                if sent.is_err() { break; }
                repaint.request_repaint();
            }
        })
            })?;
        Ok(Self {
            entries: HashMap::new(),
            jobs,
            results,
            cancel,
            epoch: 0,
        })
    }

    /// O(128) comparison only; canonicalization, hashes and decoding happen in the worker.
    pub fn sync(&mut self, project: &Project, database: &Path) {
        let parent = database
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let desired = project
            .media
            .iter()
            .take(MAX_ASSETS)
            .map(|media| {
                let key = Key {
                    path: parent.join(&media.path),
                    digest: media.metadata.blake3.clone(),
                    size: media.metadata.size_bytes,
                };
                (media.id, key, media)
            })
            .collect::<Vec<_>>();
        let changed = desired.len() != self.entries.len()
            || desired
                .iter()
                .any(|(id, key, _)| self.entries.get(id).is_none_or(|e| e.key != *key));
        if changed {
            self.epoch = self.epoch.wrapping_add(1);
            self.cancel.send_replace(self.epoch);
            self.entries.retain(|id, entry| {
                desired
                    .iter()
                    .any(|(wanted, key, _)| id == wanted && entry.key == *key)
            });
            for entry in self.entries.values_mut() {
                if matches!(entry.state, State::Loading) {
                    entry.state = State::Queued;
                }
            }
            for (id, key, media) in desired {
                self.entries.entry(id).or_insert_with(|| Entry {
                    key,
                    media: media.clone(),
                    state: State::Queued,
                });
            }
        }
        self.schedule();
    }

    fn schedule(&mut self) {
        for (id, entry) in &mut self.entries {
            if !matches!(entry.state, State::Queued) {
                continue;
            }
            let job = Job {
                id: *id,
                key: entry.key.clone(),
                media: entry.media.clone(),
                epoch: self.epoch,
            };
            match self.jobs.try_send(job) {
                Ok(()) => entry.state = State::Loading,
                Err(mpsc::error::TrySendError::Full(_)) => break,
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    entry.state = State::Ready {
                        thumbnail: None,
                        waveform: None,
                        error: Some("Asset decoder stopped".into()),
                    };
                }
            }
        }
    }

    /// Called on the Studio thread: bounded texture uploads onto egui's existing renderer device.
    pub fn poll(&mut self, ctx: &egui::Context) {
        for _ in 0..4 {
            let Ok(decoded) = self.results.try_recv() else {
                break;
            };
            if decoded.epoch != self.epoch {
                continue;
            }
            let Some(entry) = self
                .entries
                .get_mut(&decoded.id)
                .filter(|entry| entry.key == decoded.key)
            else {
                continue;
            };
            let thumbnail = decoded.image.map(|bytes| {
                ctx.load_texture(
                    format!("asset-{}", decoded.id),
                    egui::ColorImage::from_rgba_unmultiplied([WIDTH, HEIGHT], &bytes),
                    egui::TextureOptions::LINEAR,
                )
            });
            entry.state = State::Ready {
                thumbnail,
                waveform: decoded.waveform,
                error: decoded.error,
            };
        }
        self.schedule();
        if !self.results.is_empty() {
            ctx.request_repaint();
        }
    }
    pub fn thumbnail(&self, id: Uuid) -> Option<&egui::TextureHandle> {
        match &self.entries.get(&id)?.state {
            State::Ready { thumbnail, .. } => thumbnail.as_ref(),
            _ => None,
        }
    }
    pub fn waveform(&self, id: Uuid) -> Option<&Waveform> {
        match &self.entries.get(&id)?.state {
            State::Ready { waveform, .. } => waveform.as_ref(),
            _ => None,
        }
    }
    pub fn error(&self, id: Uuid) -> Option<&str> {
        match self.entries.get(&id) {
            Some(Entry {
                state: State::Ready { error, .. },
                ..
            }) => error.as_deref(),
            Some(_) => None,
            None => Some("Presentation cache is limited to the first 128 media assets"),
        }
    }
    pub fn pending(&self, id: Uuid) -> bool {
        self.entries
            .get(&id)
            .is_some_and(|entry| !matches!(entry.state, State::Ready { .. }))
    }
}

async fn verified(key: &Key) -> Result<PathBuf> {
    let path = local_file(&key.path)?;
    let digest = key.digest.as_ref().ok_or_else(|| {
        Error::Unsupported("Reimport legacy media to verify source identity".into())
    })?;
    if tokio::fs::metadata(&path).await?.len() != key.size || &content_hash(&path).await? != digest
    {
        return Err(Error::Invalid("Source changed; reimport media".into()));
    }
    Ok(path)
}

async fn decode(job: Job, tools: &MediaTools) -> Decoded {
    let mut output = Decoded {
        id: job.id,
        epoch: job.epoch,
        key: job.key.clone(),
        image: None,
        waveform: None,
        error: None,
    };
    let work = async {
        let path = verified(&job.key).await?;
        let mut errors = Vec::new();
        if job.media.metadata.video.is_some() {
            match thumbnail(&path, &job.media, tools).await {
                Ok(image) => output.image = Some(image),
                Err(error) => errors.push(format!("Thumbnail: {error}")),
            }
        }
        if job.media.metadata.audio.is_some() {
            let decoded = if job
                .media
                .metadata
                .audio
                .as_ref()
                .is_some_and(|audio| audio.start_time_micros != 0)
            {
                Err(Error::Unsupported(
                    "Waveform timeline mapping currently requires zero-origin source audio".into(),
                ))
            } else {
                waveform(&path, tools).await
            };
            match decoded {
                Ok(waveform) => output.waveform = Some(waveform),
                Err(error) => errors.push(format!("Waveform: {error}")),
            }
        }
        verified(&job.key).await?;
        if !errors.is_empty() {
            output.error = Some(errors.join("; "));
        }
        Ok::<_, Error>(())
    };
    match tokio::time::timeout(tools.timeout.min(Duration::from_secs(30)), work).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            output.image = None;
            output.waveform = None;
            output.error = Some(error.to_string());
        }
        Err(_) => {
            output.image = None;
            output.waveform = None;
            output.error = Some("Asset extraction timed out (30s maximum)".into());
        }
    }
    output
}

fn common_args(path: &Path) -> Vec<OsString> {
    let mut args = [
        "-hide_banner",
        "-v",
        "error",
        "-nostdin",
        "-threads",
        "2",
        "-filter_threads",
        "2",
        "-protocol_whitelist",
        "file",
        "-i",
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    args.push(path.as_os_str().to_owned());
    args
}
async fn thumbnail(path: &Path, media: &Media, tools: &MediaTools) -> Result<Vec<u8>> {
    let video = media
        .metadata
        .video
        .as_ref()
        .ok_or_else(|| Error::Unsupported("No video stream".into()))?;
    if video.sample_aspect_ratio != "1:1"
        || video
            .color_transfer
            .as_deref()
            .is_some_and(|t| matches!(t, "smpte2084" | "arib-std-b67"))
    {
        return Err(Error::Unsupported(
            "Thumbnail normalization currently requires square-pixel SDR".into(),
        ));
    }
    let mut args = common_args(path);
    args.extend(["-map".into(), "0:v:0".into(), "-an".into(), "-vf".into(), format!("trim=start_frame={},scale={WIDTH}:{HEIGHT}:force_original_aspect_ratio=decrease,pad={WIDTH}:{HEIGHT}:(ow-iw)/2:(oh-ih)/2,setsar=1,format=rgba", video.frames.saturating_sub(1).min(video.frames / 10)).into(), "-frames:v".into(), "1".into(), "-threads".into(), "2".into(), "-f".into(), "rawvideo".into(), "pipe:1".into()]);
    let bytes = tools.run(&tools.ffmpeg, &args).await?;
    if bytes.len() != WIDTH * HEIGHT * 4 {
        return Err(Error::Media("Incomplete thumbnail frame".into()));
    }
    Ok(bytes)
}

#[derive(Default)]
struct Envelope {
    peaks: Vec<f32>,
    span: u64,
    partial: u64,
    peak: f32,
    samples: u64,
}
impl Envelope {
    fn push(&mut self, value: f32) -> Result<()> {
        if !value.is_finite() {
            return Err(Error::Media("Nonfinite decoded audio sample".into()));
        }
        if self.span == 0 {
            self.span = 256;
        }
        self.samples += 1;
        self.partial += 1;
        self.peak = self.peak.max(value.abs().min(1.0));
        if self.partial == self.span {
            self.peaks.push(self.peak);
            self.partial = 0;
            self.peak = 0.0;
            if self.peaks.len() == MAX_PEAKS {
                self.peaks = self
                    .peaks
                    .chunks_exact(2)
                    .map(|pair| pair[0].max(pair[1]))
                    .collect();
                self.span *= 2;
            }
        }
        Ok(())
    }
    fn finish(mut self) -> Waveform {
        if self.partial > 0 {
            self.peaks.push(self.peak);
        }
        let max_peak = self.peaks.iter().copied().fold(0.0_f32, f32::max);
        Waveform {
            max_peak,
            peaks: self.peaks,
            duration_seconds: self.samples as f64 / f64::from(SAMPLE_RATE),
            sample_rate: SAMPLE_RATE,
        }
    }
}

async fn waveform(path: &Path, tools: &MediaTools) -> Result<Waveform> {
    let mut args = common_args(path);
    args.extend([
        "-map".into(),
        "0:a:0".into(),
        "-vn".into(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        SAMPLE_RATE.to_string().into(),
        "-f".into(),
        "f32le".into(),
        "pipe:1".into(),
    ]);
    let mut child = Command::new(&tools.ffmpeg)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Media("Waveform pipe missing".into()))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::Media("Waveform diagnostics missing".into()))?;
    let diagnostics = async {
        let mut retained = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let count = stderr.read(&mut buffer).await?;
            if count == 0 {
                break;
            }
            let keep = count.min((64 * 1024_usize).saturating_sub(retained.len()));
            retained.extend_from_slice(&buffer[..keep]);
        }
        Ok::<_, Error>(retained)
    };
    let samples = async {
        let mut envelope = Envelope::default();
        let mut buffer = [0_u8; 4096];
        let mut remainder = Vec::with_capacity(3);
        loop {
            let count = stdout.read(&mut buffer).await?;
            if count == 0 {
                break;
            }
            let mut bytes = std::mem::take(&mut remainder);
            bytes.extend_from_slice(&buffer[..count]);
            let complete = bytes.len() & !3;
            for sample in bytes[..complete].chunks_exact(4) {
                envelope.push(f32::from_le_bytes([
                    sample[0], sample[1], sample[2], sample[3],
                ]))?;
            }
            remainder.extend_from_slice(&bytes[complete..]);
        }
        if !remainder.is_empty() {
            return Err(Error::Media("Incomplete waveform PCM sample".into()));
        }
        Ok::<_, Error>(envelope.finish())
    };
    let (waveform, diagnostics, status) = tokio::try_join!(samples, diagnostics, async {
        child.wait().await.map_err(Error::from)
    })?;
    if !status.success() {
        return Err(Error::Media(
            String::from_utf8_lossy(&diagnostics).into_owned(),
        ));
    }
    if waveform.peaks.is_empty() {
        return Err(Error::Media("No audio samples decoded".into()));
    }
    Ok(waveform)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    #[test]
    fn waveform_compaction_is_bounded_and_preserves_real_peak_duration() -> Result<()> {
        let mut envelope = Envelope::default();
        for sample in 0..1_000_003 {
            envelope.push(if sample == 12 { -0.75 } else { 0.0 })?;
        }
        let waveform = envelope.finish();
        assert!(waveform.peaks.len() <= MAX_PEAKS);
        assert_eq!(waveform.peaks.first(), Some(&0.75));
        assert_eq!(waveform.duration_seconds, 1_000_003.0 / 8000.0);
        Ok(())
    }
    #[test]
    fn malformed_audio_sample_is_explicit() {
        assert!(Envelope::default().push(f32::NAN).is_err());
    }

    #[test]
    #[ignore = "requires installed FFmpeg/FFprobe; generated source only"]
    fn real_async_thumbnail_waveform_and_changed_source_rejection()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let tools = MediaTools {
            ffmpeg: std::env::var_os("LUMERYN_FFMPEG")
                .map(PathBuf::from)
                .unwrap_or_else(|| "ffmpeg".into()),
            ffprobe: std::env::var_os("LUMERYN_FFPROBE")
                .map(PathBuf::from)
                .unwrap_or_else(|| "ffprobe".into()),
            timeout: Duration::from_secs(10),
        };
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.lumeryn-scratch/media");
        std::fs::create_dir_all(&root)?;
        let directory = root.join(format!("assets-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory)?;
        let source = directory.join("source.mkv");
        let generated = std::process::Command::new(&tools.ffmpeg)
            .args([
                "-hide_banner",
                "-v",
                "error",
                "-nostdin",
                "-n",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=160x90:rate=30:duration=1",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000:duration=1",
                "-c:v",
                "ffv1",
                "-pix_fmt",
                "bgr0",
                "-c:a",
                "pcm_s16le",
            ])
            .arg(&source)
            .output()?;
        assert!(
            generated.status.success(),
            "{}",
            String::from_utf8_lossy(&generated.stderr)
        );
        let runtime = tokio::runtime::Runtime::new()?;
        let metadata = runtime.block_on(tools.probe(&source))?;
        let mut project = Project::new(
            "Asset cache fixture".into(),
            160,
            90,
            lumeryn_core::time::FrameRate::new(30, 1)?,
        )?;
        let id = Uuid::new_v4();
        project.media.push(Media {
            alias: None,
            id,
            path: "source.mkv".into(),
            metadata,
        });
        let ctx = egui::Context::default();
        let mut assets = Assets::new(&ctx, tools.clone())?;
        let started = Instant::now();
        assets.sync(&project, &directory.join("project.lumeryn"));
        let deadline = Instant::now() + Duration::from_secs(8);
        while assets.pending(id) {
            assets.poll(&ctx);
            assert!(Instant::now() < deadline, "asset extraction timeout");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(assets.error(id).is_none(), "{:?}", assets.error(id));
        let texture = assets.thumbnail(id).ok_or("missing real thumbnail")?;
        assert_eq!(texture.size(), [WIDTH, HEIGHT]);
        let waveform = assets.waveform(id).ok_or("missing real waveform")?;
        assert!((waveform.duration_seconds - 1.0).abs() < 0.001);
        assert!(
            waveform
                .peaks
                .iter()
                .all(|peak| (0.10..0.14).contains(peak))
        );
        println!(
            "actual asset extraction {:.2}ms: {} peaks, {:.6}s",
            started.elapsed().as_secs_f64() * 1000.0,
            waveform.peaks.len(),
            waveform.duration_seconds
        );
        // New job must reject a same-size replacement despite identical imported metadata.
        let mut changed = std::fs::read(&source)?;
        let last = changed.last_mut().ok_or("empty generated fixture")?;
        *last ^= 1;
        std::fs::write(&source, changed)?;
        let mut fresh = Assets::new(&ctx, tools)?;
        fresh.sync(&project, &directory.join("project.lumeryn"));
        let deadline = Instant::now() + Duration::from_secs(8);
        while fresh.pending(id) {
            fresh.poll(&ctx);
            assert!(Instant::now() < deadline, "hash validation timeout");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(
            fresh
                .error(id)
                .is_some_and(|error| error.contains("Source changed"))
        );
        assert!(fresh.thumbnail(id).is_none());
        drop(assets);
        drop(fresh);
        std::fs::remove_file(source)?;
        std::fs::remove_dir(directory)?;
        Ok(())
    }
}
