use crate::{
    AudioStatus,
    decode::{WorkerPipe, base_args},
    plan::Plan,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use lumeryn_core::{Error, MediaTools, Result, media::content_hash, time::FrameRate};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    time::Instant,
};
use tokio::{
    io::AsyncReadExt,
    sync::{Notify, watch},
};

struct Buffer {
    samples: VecDeque<f32>,
    submitted: u64,
    decoded: u64,
    timing: Option<(u64, u64, Instant)>,
    error: Option<String>,
}
struct Shared {
    buffer: Mutex<Buffer>,
    playing: AtomicBool,
    underruns: AtomicU64,
    rate: u32,
    channels: u16,
    capacity: usize,
    finished: AtomicBool,
    consumed: Notify,
    volume: Arc<AtomicU32>,
}

pub(crate) struct Output {
    shared: Arc<Shared>,
    _stream: cpal::Stream,
    task: tokio::task::JoinHandle<()>,
    start: u64,
}

enum Source {
    Timeline(Plan, u64),
    Audition(PathBuf, String),
}

struct Input {
    args: Vec<std::ffi::OsString>,
    sources: Vec<(PathBuf, String)>,
}
impl Source {
    fn inputs(self, sample_rate: u32, channels: u16) -> Result<Vec<Input>> {
        let device_args = || {
            [
                "-ar".into(),
                sample_rate.to_string().into(),
                "-ac".into(),
                channels.to_string().into(),
                "-f".into(),
                "f32le".into(),
                "pipe:1".into(),
            ]
        };
        match self {
            Self::Timeline(plan, start) => {
                if let Some(audio_plan) = plan.audio_plan {
                    Ok(vec![Input {
                        args: audio_plan.pcm_args_for_device(start, sample_rate, channels)?,
                        sources: audio_plan.output_sources(),
                    }])
                } else {
                    Ok(plan.segments.iter()
                        .filter(|segment| segment.at + segment.frames > start)
                        .map(|segment| {
                            let offset = start.saturating_sub(segment.at);
                            let duration = plan.rate.seconds(segment.frames - offset);
                            let mut args = base_args();
                            if segment.audio {
                                args.extend(["-threads".into(), "2".into(), "-protocol_whitelist".into(), "file".into(), "-i".into(), segment.path.as_os_str().to_owned(), "-map".into(), "0:a:0".into(), "-vn".into(), "-af".into(),
                                    format!("atrim=start={}:duration={duration},asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,apad,atrim=duration={duration}", plan.rate.seconds(segment.source_in + offset)).into()]);
                            } else {
                                args.extend(["-f".into(), "lavfi".into(), "-i".into(), "anullsrc=r=48000:cl=stereo".into(), "-t".into(), duration.into()]);
                            }
                            args.extend(device_args());
                            Input { args, sources: vec![(segment.path.clone(), segment.digest.clone())] }
                        }).collect())
                }
            }
            Self::Audition(path, digest) => {
                let mut args = base_args();
                args.extend(["-threads".into(), "2".into(), "-protocol_whitelist".into(), "file".into(), "-i".into(), path.as_os_str().to_owned(), "-map".into(), "0:a:0".into(), "-vn".into(), "-af".into(),
                    "atrim=start=0:duration=30,asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo".into()]);
                args.extend(device_args());
                Ok(vec![Input {
                    args,
                    sources: vec![(path, digest)],
                }])
            }
        }
    }
}
impl Output {
    pub fn start(
        plan: Plan,
        start: u64,
        tools: MediaTools,
        cancelled: watch::Receiver<u64>,
        volume: Arc<AtomicU32>,
    ) -> Result<Self> {
        Self::start_source(
            Source::Timeline(plan, start),
            start,
            tools,
            cancelled,
            volume,
        )
    }
    pub fn audition(
        path: PathBuf,
        digest: String,
        tools: MediaTools,
        cancelled: watch::Receiver<u64>,
        volume: Arc<AtomicU32>,
    ) -> Result<Self> {
        Self::start_source(Source::Audition(path, digest), 0, tools, cancelled, volume)
    }
    fn start_source(
        source: Source,
        start: u64,
        tools: MediaTools,
        cancelled: watch::Receiver<u64>,
        volume: Arc<AtomicU32>,
    ) -> Result<Self> {
        let host = cpal::default_host();
        let device = output_device(host.default_output_device())?;
        let supported = device
            .default_output_config()
            .map_err(|e| Error::Media(e.to_string()))?;
        let config: cpal::StreamConfig = supported.into();
        let rate = config.sample_rate;
        let channels = config.channels;
        if rate == 0 || channels == 0 || channels > 8 {
            return Err(Error::Unsupported(
                "audio device rate/channel layout is unsupported".into(),
            ));
        }
        let capacity = rate as usize * channels as usize / 4;
        let shared = Arc::new(Shared {
            buffer: Mutex::new(Buffer {
                samples: VecDeque::with_capacity(capacity),
                submitted: 0,
                decoded: 0,
                timing: None,
                error: None,
            }),
            playing: AtomicBool::new(false),
            underruns: AtomicU64::new(0),
            rate,
            channels,
            capacity,
            finished: AtomicBool::new(false),
            consumed: Notify::new(),
            volume,
        });
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(&device, config, Arc::clone(&shared)),
            cpal::SampleFormat::I16 => build::<i16>(&device, config, Arc::clone(&shared)),
            cpal::SampleFormat::U16 => build::<u16>(&device, config, Arc::clone(&shared)),
            format => {
                return Err(Error::Unsupported(format!(
                    "audio device sample format {format}"
                )));
            }
        }?;
        let decode_shared = Arc::clone(&shared);
        let task = tokio::spawn(async move {
            decode(source, tools, decode_shared, cancelled).await;
        });
        Ok(Self {
            shared,
            _stream: stream,
            task,
            start,
        })
    }
    pub fn set_playing(&self, playing: bool) {
        if !playing && let Ok(mut buffer) = self.shared.buffer.lock() {
            // Resuming rebases the clock to already submitted samples; no elapsed wall time is counted.
            buffer.timing = None;
        }
        self.shared.playing.store(playing, Ordering::Release);
        let result = if playing {
            self._stream.play()
        } else {
            self._stream.pause()
        };
        if let Err(error) = result
            && let Ok(mut buffer) = self.shared.buffer.lock()
        {
            buffer.error = Some(error.to_string());
        }
    }
    pub fn position_frame(&self, rate: FrameRate) -> u64 {
        let samples = self.shared.buffer.lock().map_or(0, |buffer| {
            heard_samples(&buffer, self.shared.rate, Instant::now())
        });
        self.start + samples_to_frames(samples, self.shared.rate, rate)
    }
    pub fn status(&self, playing: bool) -> AudioStatus {
        if playing {
            AudioStatus::Playing {
                sample_rate: self.shared.rate,
                channels: self.shared.channels,
                underruns: self.shared.underruns.load(Ordering::Relaxed),
            }
        } else {
            AudioStatus::Ready {
                sample_rate: self.shared.rate,
                channels: self.shared.channels,
            }
        }
    }
    pub fn error(&self) -> Option<String> {
        self.shared.buffer.lock().ok().and_then(|b| b.error.clone())
    }
    pub fn metrics(&self) -> (u64, usize, usize, u64) {
        self.shared
            .buffer
            .lock()
            .map_or((0, 0, self.shared.capacity, 0), |buffer| {
                let millis = (u128::from(heard_samples(&buffer, self.shared.rate, Instant::now()))
                    * 1000
                    / u128::from(self.shared.rate)) as u64;
                (
                    millis,
                    buffer.samples.len(),
                    self.shared.capacity,
                    buffer.decoded,
                )
            })
    }
    pub fn finished(&self) -> bool {
        self.shared.finished.load(Ordering::Acquire)
            && self.shared.buffer.lock().is_ok_and(|b| {
                b.samples.is_empty()
                    && heard_samples(&b, self.shared.rate, Instant::now()) == b.submitted
            })
    }
}

fn output_device(device: Option<cpal::Device>) -> Result<cpal::Device> {
    device.ok_or_else(|| Error::Unsupported("no native audio output device".into()))
}
impl Drop for Output {
    fn drop(&mut self) {
        self.shared.playing.store(false, Ordering::Release);
        self.task.abort();
    }
}

fn samples_to_frames(samples: u64, sample_rate: u32, rate: FrameRate) -> u64 {
    ((u128::from(samples) * u128::from(rate.numerator()))
        / (u128::from(sample_rate) * u128::from(rate.denominator()))) as u64
}
fn heard_samples(buffer: &Buffer, sample_rate: u32, now: Instant) -> u64 {
    buffer
        .timing
        .map_or(buffer.submitted, |(first, count, at)| {
            first
                + ((now.saturating_duration_since(at).as_nanos() * u128::from(sample_rate)
                    / 1_000_000_000) as u64)
                    .min(count)
        })
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    shared: Arc<Shared>,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let errors = Arc::clone(&shared);
    device
        .build_output_stream(
            config,
            move |output: &mut [T], info| {
                // Never block, allocate, decode, or perform I/O on the device callback.
                output.fill(T::from_sample(0.0));
                if !shared.playing.load(Ordering::Acquire) {
                    return;
                }
                let Ok(mut buffer) = shared.buffer.try_lock() else {
                    shared.underruns.fetch_add(1, Ordering::Relaxed);
                    return;
                };
                let channels = usize::from(shared.channels);
                let wanted = output.len() / channels;
                let gain = f32::from_bits(shared.volume.load(Ordering::Relaxed));
                let frames = write_samples(&mut buffer, output, channels, gain);
                let timestamps = info.timestamp();
                let latency = timestamps.playback.duration_since(timestamps.callback);
                if frames > 0 {
                    buffer.timing =
                        Some((buffer.submitted, frames as u64, Instant::now() + latency));
                }
                buffer.submitted += frames as u64;
                if frames > 0 {
                    shared.consumed.notify_one();
                }
                if frames < wanted {
                    shared.underruns.fetch_add(1, Ordering::Relaxed);
                }
            },
            move |error| {
                if let Ok(mut buffer) = errors.buffer.try_lock() {
                    buffer.error = Some(error.to_string());
                }
            },
            None,
        )
        .map_err(|e| Error::Media(e.to_string()))
}

fn write_samples<T: cpal::Sample + cpal::FromSample<f32>>(
    buffer: &mut Buffer,
    output: &mut [T],
    channels: usize,
    gain: f32,
) -> usize {
    let frames = (buffer.samples.len() / channels).min(output.len() / channels);
    for sample in output.iter_mut().take(frames * channels) {
        if let Some(value) = buffer.samples.pop_front() {
            let gained = if value.is_finite() {
                (value * gain).clamp(-1.0, 1.0)
            } else {
                0.0
            };
            *sample = T::from_sample(gained);
        }
    }
    frames
}

async fn decode(
    source: Source,
    tools: MediaTools,
    shared: Arc<Shared>,
    mut cancelled: watch::Receiver<u64>,
) {
    let work = async {
        for input in source.inputs(shared.rate, shared.channels)? {
            for (path, digest) in &input.sources {
                if &content_hash(path).await? != digest {
                    return Err(Error::Invalid(
                        "audio preview source changed; reimport media".into(),
                    ));
                }
            }
            let args = input.args;
            let mut pipe = WorkerPipe::start(&tools, &args)?;
            let mut bytes = [0_u8; 4096];
            let mut remainder = Vec::with_capacity(3);
            loop {
                let count = tokio::time::timeout(tools.timeout, pipe.stdout.read(&mut bytes))
                    .await
                    .map_err(|_| Error::Timeout)??;
                if count == 0 {
                    break;
                }
                let mut combined = std::mem::take(&mut remainder);
                combined.extend_from_slice(&bytes[..count]);
                let complete = combined.len() & !3;
                let samples = combined[..complete]
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect::<Vec<_>>();
                remainder.extend_from_slice(&combined[complete..]);
                let mut at = 0;
                while at < samples.len() {
                    let consumed = shared.consumed.notified();
                    tokio::pin!(consumed);
                    consumed.as_mut().enable();
                    {
                        let mut buffer = shared
                            .buffer
                            .lock()
                            .map_err(|_| Error::Media("audio queue poisoned".into()))?;
                        let count =
                            (shared.capacity - buffer.samples.len()).min(samples.len() - at);
                        buffer
                            .samples
                            .extend(samples[at..at + count].iter().copied());
                        buffer.decoded += count as u64;
                        at += count;
                    }
                    if at < samples.len() {
                        consumed.await;
                    }
                }
            }
            if !remainder.is_empty() {
                return Err(Error::Media("incomplete PCM sample".into()));
            }
            tokio::time::timeout(tools.timeout, pipe.finish())
                .await
                .map_err(|_| Error::Timeout)??;
            for (path, digest) in &input.sources {
                if &content_hash(path).await? != digest {
                    return Err(Error::Invalid(
                        "audio preview source changed during decoding".into(),
                    ));
                }
            }
        }
        Ok::<_, Error>(())
    };
    tokio::select! {
        biased;
        _ = cancelled.changed() => {},
        result = work => { match result { Ok(()) => shared.finished.store(true, Ordering::Release), Err(error) => { if let Ok(mut buffer) = shared.buffer.lock() { buffer.error = Some(error.to_string()); } } } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn missing_device_is_explicit() {
        assert!(
            matches!(output_device(None),Err(Error::Unsupported(message)) if message=="no native audio output device")
        );
    }

    #[tokio::test]
    #[ignore = "requires FFmpeg; bounded shared mix, final device conversion and cancellation without a device"]
    async fn timeline_shared_mix_stream_matches_canonical_device_pcm() -> Result<()> {
        use lumeryn_core::audio_plan::{
            AudioPlan, AudioSegment, AudioSource, CANONICAL_DECODE_FILTER,
        };
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("music.wav");
        let tools = MediaTools::default();
        tools
            .run(
                &tools.ffmpeg,
                &[
                    "-v".into(),
                    "error".into(),
                    "-nostdin".into(),
                    "-n".into(),
                    "-f".into(),
                    "lavfi".into(),
                    "-i".into(),
                    "sine=frequency=880:sample_rate=44100:duration=2".into(),
                    "-c:a".into(),
                    "pcm_f32le".into(),
                    path.as_os_str().to_owned(),
                ],
            )
            .await?;
        let digest = content_hash(&path).await?;
        let proof = tools
            .run(
                &tools.ffmpeg,
                &[
                    "-v".into(),
                    "error".into(),
                    "-nostdin".into(),
                    "-i".into(),
                    path.as_os_str().to_owned(),
                    "-map".into(),
                    "0:a:0".into(),
                    "-af".into(),
                    CANONICAL_DECODE_FILTER.into(),
                    "-f".into(),
                    "f32le".into(),
                    "pipe:1".into(),
                ],
            )
            .await?;
        assert_eq!(proof.len(), 96_000 * 8);
        let rate = FrameRate::new(30, 1)?;
        let audio_plan = AudioPlan::new(
            rate,
            60,
            vec![AudioSource {
                path: path.clone(),
                digest: digest.clone(),
                canonical_samples: Some(proof.len() as u64 / 8),
            }],
            vec![AudioSegment {
                source: 0,
                at_frame: 15,
                source_in_sample: 2_000,
                duration_frames: 30,
                gain_millidecibels: -6_000,
                muted: false,
            }],
        )?;
        let expected = tools
            .run(
                &tools.ffmpeg,
                &audio_plan.pcm_args_for_device(20, 44_100, 1)?,
            )
            .await?;
        let plan = Plan {
            segments: Vec::new(),
            rate,
            total: 60,
            revision: 1,
            width: 128,
            height: 72,
            audio_plan: Some(audio_plan),
            visual_plan: None,
        };
        let shared = Arc::new(Shared {
            buffer: Mutex::new(Buffer {
                samples: VecDeque::with_capacity(11_025),
                submitted: 0,
                decoded: 0,
                timing: None,
                error: None,
            }),
            playing: AtomicBool::new(false),
            underruns: AtomicU64::new(0),
            rate: 44_100,
            channels: 1,
            capacity: 11_025,
            finished: AtomicBool::new(false),
            consumed: Notify::new(),
            volume: Arc::new(AtomicU32::new(1.0_f32.to_bits())),
        });
        let (cancel, receiver) = watch::channel(0);
        let task = tokio::spawn(decode(
            Source::Timeline(plan.clone(), 20),
            tools.clone(),
            Arc::clone(&shared),
            receiver,
        ));
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                {
                    let buffer = shared
                        .buffer
                        .lock()
                        .map_err(|_| Error::Media("test queue poisoned".into()))?;
                    if let Some(error) = &buffer.error {
                        return Err(Error::Media(error.clone()));
                    }
                    if buffer.samples.len() == shared.capacity {
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok::<_, Error>(())
        })
        .await
        .map_err(|_| Error::Timeout)??;
        assert!(!shared.finished.load(Ordering::Acquire));
        cancel.send_replace(1);
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|error| Error::Media(error.to_string()))?;
        {
            let mut buffer = shared
                .buffer
                .lock()
                .map_err(|_| Error::Media("test queue poisoned".into()))?;
            buffer.samples.clear();
            buffer.decoded = 0;
        }
        let (_cancel, receiver) = watch::channel(0);
        let task = tokio::spawn(decode(
            Source::Timeline(plan, 20),
            tools,
            Arc::clone(&shared),
            receiver,
        ));
        let mut actual = Vec::<f32>::new();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                {
                    let mut buffer = shared
                        .buffer
                        .lock()
                        .map_err(|_| Error::Media("test queue poisoned".into()))?;
                    if let Some(error) = &buffer.error {
                        return Err(Error::Media(error.clone()));
                    }
                    assert!(buffer.samples.len() <= shared.capacity);
                    actual.extend(buffer.samples.drain(..));
                }
                shared.consumed.notify_one();
                if shared.finished.load(Ordering::Acquire) {
                    let mut buffer = shared
                        .buffer
                        .lock()
                        .map_err(|_| Error::Media("test queue poisoned".into()))?;
                    actual.extend(buffer.samples.drain(..));
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            Ok::<_, Error>(())
        })
        .await
        .map_err(|_| Error::Timeout)??;
        task.await
            .map_err(|error| Error::Media(error.to_string()))?;
        let actual = actual
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        assert_eq!(content_hash(&path).await?, digest);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires installed FFmpeg; actual bounded standalone PCM decoding, no audio device"]
    async fn audition_decode_is_thirty_second_bounded_and_cancellable() -> Result<()> {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.lumeryn-scratch/media");
        std::fs::create_dir_all(&root)?;
        let dir = tempfile::tempdir_in(root)?;
        let path = dir.path().join("tone.wav");
        let tools = MediaTools {
            ffmpeg: std::env::var_os("LUMERYN_FFMPEG")
                .map_or_else(|| "ffmpeg".into(), PathBuf::from),
            ..Default::default()
        };
        let result = tokio::process::Command::new(&tools.ffmpeg)
            .args([
                "-hide_banner",
                "-v",
                "error",
                "-nostdin",
                "-n",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=8000",
                "-t",
                "40",
                "-c:a",
                "pcm_s16le",
            ])
            .arg(&path)
            .output()
            .await?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let digest = content_hash(&path).await?;
        let shared = Arc::new(Shared {
            buffer: Mutex::new(Buffer {
                samples: VecDeque::with_capacity(2000),
                submitted: 0,
                decoded: 0,
                timing: None,
                error: None,
            }),
            playing: AtomicBool::new(false),
            underruns: AtomicU64::new(0),
            rate: 8000,
            channels: 1,
            capacity: 2000,
            finished: AtomicBool::new(false),
            consumed: Notify::new(),
            volume: Arc::new(AtomicU32::new(1.0_f32.to_bits())),
        });
        let (cancel, receiver) = watch::channel(0);
        let task = tokio::spawn(decode(
            Source::Audition(path.clone(), digest.clone()),
            tools.clone(),
            Arc::clone(&shared),
            receiver,
        ));
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_eq!(
            shared
                .buffer
                .lock()
                .map_err(|_| Error::Media("test queue poisoned".into()))?
                .samples
                .len(),
            2000
        );
        assert!(!shared.finished.load(Ordering::Acquire));
        cancel.send_replace(1);
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|error| Error::Media(error.to_string()))?;
        {
            let mut buffer = shared
                .buffer
                .lock()
                .map_err(|_| Error::Media("test queue poisoned".into()))?;
            buffer.samples.clear();
            buffer.decoded = 0;
        }
        let (_cancel, receiver) = watch::channel(0);
        let task = tokio::spawn(decode(
            Source::Audition(path.clone(), digest.clone()),
            tools,
            Arc::clone(&shared),
            receiver,
        ));
        tokio::time::timeout(Duration::from_secs(10), async {
            while !shared.finished.load(Ordering::Acquire) {
                {
                    let mut buffer = shared
                        .buffer
                        .lock()
                        .map_err(|_| Error::Media("test queue poisoned".into()))?;
                    assert!(buffer.samples.len() <= 2000);
                    assert!(buffer.samples.iter().all(|sample| sample.is_finite()));
                    buffer.samples.clear();
                }
                shared.consumed.notify_one();
                tokio::time::sleep(Duration::from_millis(1)).await;
                if let Some(error) = shared
                    .buffer
                    .lock()
                    .map_err(|_| Error::Media("test queue poisoned".into()))?
                    .error
                    .clone()
                {
                    return Err(Error::Media(error));
                }
            }
            Ok(())
        })
        .await
        .map_err(|_| Error::Timeout)??;
        task.await
            .map_err(|error| Error::Media(error.to_string()))?;
        assert_eq!(
            shared
                .buffer
                .lock()
                .map_err(|_| Error::Media("test queue poisoned".into()))?
                .decoded,
            30 * 8000
        );
        assert_eq!(content_hash(&path).await?, digest);
        Ok(())
    }
    fn pcm_buffer() -> Buffer {
        Buffer {
            samples: [0.75, -0.5, 0.25, -1.0].into(),
            submitted: 480,
            decoded: 4,
            timing: None,
            error: None,
        }
    }
    #[test]
    fn monitor_gain_changes_output_only_and_preserves_consumed_sample_count() {
        let mut full = pcm_buffer();
        let mut half = pcm_buffer();
        let mut muted = pcm_buffer();
        let mut full_pcm = [0_f32; 4];
        let mut half_pcm = [0_f32; 4];
        let mut muted_pcm = [1_f32; 4];
        assert_eq!(write_samples(&mut full, &mut full_pcm, 2, 1.0), 2);
        assert_eq!(write_samples(&mut half, &mut half_pcm, 2, 0.5), 2);
        assert_eq!(write_samples(&mut muted, &mut muted_pcm, 2, 0.0), 2);
        assert_eq!(full_pcm, [0.75, -0.5, 0.25, -1.0]);
        assert_eq!(half_pcm, [0.375, -0.25, 0.125, -0.5]);
        assert_eq!(muted_pcm, [0.0; 4]);
        for buffer in [&full, &half, &muted] {
            assert!(buffer.samples.is_empty());
            assert_eq!(buffer.submitted, 480);
            assert_eq!(heard_samples(buffer, 48_000, Instant::now()), 480);
        }
    }
    #[test]
    fn monitor_gain_converts_integer_pcm_and_contains_nonfinite_samples() {
        use cpal::Sample;
        let mut signed = pcm_buffer();
        let mut unsigned = pcm_buffer();
        let mut signed_pcm = [0_i16; 4];
        let mut unsigned_pcm = [0_u16; 4];
        write_samples(&mut signed, &mut signed_pcm, 2, 0.5);
        write_samples(&mut unsigned, &mut unsigned_pcm, 2, 0.5);
        assert_eq!(
            signed_pcm,
            [0.375, -0.25, 0.125, -0.5].map(i16::from_sample)
        );
        assert_eq!(
            unsigned_pcm,
            [0.375, -0.25, 0.125, -0.5].map(u16::from_sample)
        );
        let mut malformed = Buffer {
            samples: [f32::NAN, f32::INFINITY, 5.0, -5.0].into(),
            submitted: 0,
            decoded: 4,
            timing: None,
            error: None,
        };
        let mut safe_pcm = [0_f32; 4];
        write_samples(&mut malformed, &mut safe_pcm, 2, 1.0);
        assert_eq!(safe_pcm, [0.0, 0.0, 1.0, -1.0]);
    }
    #[test]
    fn audio_master_rational_mapping_and_dac_latency() -> Result<()> {
        let rate = FrameRate::new(30_000, 1001)?;
        assert_eq!(samples_to_frames(48_048, 48_000, rate), 30);
        assert_eq!(samples_to_frames(48_047, 48_000, rate), 29);
        let now = Instant::now();
        let buffer = Buffer {
            samples: VecDeque::new(),
            submitted: 480,
            decoded: 0,
            timing: Some((0, 480, now + Duration::from_millis(10))),
            error: None,
        };
        assert_eq!(heard_samples(&buffer, 48_000, now), 0);
        assert_eq!(
            heard_samples(&buffer, 48_000, now + Duration::from_millis(15)),
            240
        );
        assert_eq!(
            heard_samples(&buffer, 48_000, now + Duration::from_millis(30)),
            480
        );
        Ok(())
    }
}
