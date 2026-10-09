use crate::{
    AudioStatus,
    decode::{WorkerPipe, base_args},
    plan::Plan,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use lumeryn_core::{Error, MediaTools, Result, media::content_hash, time::FrameRate};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
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
}

pub(crate) struct Output {
    shared: Arc<Shared>,
    _stream: cpal::Stream,
    task: tokio::task::JoinHandle<()>,
    start: u64,
}
impl Output {
    pub fn start(
        plan: Plan,
        start: u64,
        tools: MediaTools,
        cancelled: watch::Receiver<u64>,
    ) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| Error::Unsupported("no native audio output device".into()))?;
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
            decode(plan, start, tools, decode_shared, cancelled).await;
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
    pub fn finished(&self) -> bool {
        self.shared.finished.load(Ordering::Acquire)
            && self.shared.buffer.lock().is_ok_and(|b| {
                b.samples.is_empty()
                    && heard_samples(&b, self.shared.rate, Instant::now()) == b.submitted
            })
    }
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
                let available = buffer.samples.len() / channels;
                let frames = available.min(wanted);
                for sample in output.iter_mut().take(frames * channels) {
                    if let Some(value) = buffer.samples.pop_front() {
                        *sample = T::from_sample(value);
                    }
                }
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

async fn decode(
    plan: Plan,
    start: u64,
    tools: MediaTools,
    shared: Arc<Shared>,
    mut cancelled: watch::Receiver<u64>,
) {
    let work = async {
        for segment in plan.segments.iter().filter(|s| s.at + s.frames > start) {
            if content_hash(&segment.path).await? != segment.digest {
                return Err(Error::Invalid(
                    "audio preview source changed; reimport media".into(),
                ));
            }
            let offset = start.saturating_sub(segment.at);
            let duration = plan.rate.seconds(segment.frames - offset);
            let mut args = base_args();
            if segment.audio {
                args.extend(["-threads".into(), "2".into(), "-protocol_whitelist".into(), "file".into(), "-i".into(), segment.path.as_os_str().to_owned(), "-map".into(), "0:a:0".into(), "-vn".into(), "-af".into(),
                    format!("atrim=start={}:duration={duration},asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,apad,atrim=duration={duration}", plan.rate.seconds(segment.source_in + offset)).into()]);
            } else {
                args.extend([
                    "-f".into(),
                    "lavfi".into(),
                    "-i".into(),
                    "anullsrc=r=48000:cl=stereo".into(),
                    "-t".into(),
                    duration.into(),
                ]);
            }
            args.extend([
                "-ar".into(),
                shared.rate.to_string().into(),
                "-ac".into(),
                shared.channels.to_string().into(),
                "-f".into(),
                "f32le".into(),
                "pipe:1".into(),
            ]);
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
            if content_hash(&segment.path).await? != segment.digest {
                return Err(Error::Invalid(
                    "audio preview source changed during decoding".into(),
                ));
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
    fn audio_master_rational_mapping_and_dac_latency() -> Result<()> {
        let rate = FrameRate::new(30_000, 1001)?;
        assert_eq!(samples_to_frames(48_048, 48_000, rate), 30);
        assert_eq!(samples_to_frames(48_047, 48_000, rate), 29);
        let now = Instant::now();
        let buffer = Buffer {
            samples: VecDeque::new(),
            submitted: 480,
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
