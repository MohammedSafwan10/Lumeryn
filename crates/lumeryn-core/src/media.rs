use crate::{Error, Result, time::FrameRate};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

/// Construct a background media/tool worker without allocating a Windows console.
///
/// GUI parents have no console for a console-subsystem FFmpeg/ffprobe to inherit.
/// CREATE_NO_WINDOW prevents Windows from allocating one; it does not hide Studio
/// or change the worker's redirected pipes, arguments, environment or lifecycle.
/// See https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags.
pub fn worker_command(executable: &Path) -> std::process::Command {
    let command = std::process::Command::new(executable);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut command = command;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW; not DETACHED_PROCESS/NEW_CONSOLE.
        command
    }
    #[cfg(not(windows))]
    command
}

/// Tokio adapter for the same worker policy; callers retain their pipe/cancel/timeout policy.
pub fn async_worker_command(executable: &Path) -> Command {
    Command::from(worker_command(executable))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub size_bytes: u64,
    #[serde(default)]
    pub blake3: Option<String>,
    pub video: Option<Video>,
    pub audio: Option<Audio>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Video {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub frames: u64,
    pub start_time_micros: i64,
    pub sample_aspect_ratio: String,
    pub color_transfer: Option<String>,
    /// Verified by frame timestamp inspection, not merely avg_frame_rate.
    pub constant_rate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Audio {
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u32,
    pub start_time_micros: i64,
    /// Full decoded length under the shared stereo 48 kHz resampling policy.
    /// Absent on legacy imports until a placement worker verifies the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_samples_48k: Option<u64>,
}

/// Advisory stream headers. These are never a source proof or editable media.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuickMetadata {
    pub size_bytes: u64,
    pub video: Option<QuickVideo>,
    pub audio: Option<QuickAudio>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuickVideo {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub frame_rate: Option<FrameRate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuickAudio {
    pub codec: String,
    pub sample_rate: Option<u32>,
    pub channels: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStage {
    Inspecting,
    Hashing,
    ValidatingVideo,
    ValidatingAudio,
    VerifyingSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProbeProgress {
    pub stage: ProbeStage,
    /// Cumulative time at stage entry; completion is the caller's terminal result.
    pub elapsed_ms: u64,
    pub quick: Option<QuickMetadata>,
}

/// A proof produced only by the media validator, never deserialized from a client.
#[derive(Debug, Clone)]
pub struct VerifiedMetadata {
    metadata: Metadata,
    source_path: PathBuf,
}

impl VerifiedMetadata {
    pub fn source_path(&self) -> &Path {
        &self.source_path
    }
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn into_metadata(self) -> Metadata {
        self.metadata
    }
}

#[derive(Debug, Clone)]
pub struct MediaTools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub timeout: Duration,
}

impl Default for MediaTools {
    fn default() -> Self {
        Self {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            timeout: Duration::from_secs(300),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Probe {
    streams: Vec<Stream>,
}

#[derive(Debug, Deserialize)]
struct Stream {
    codec_type: String,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    nb_read_frames: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    time_base: Option<String>,
    start_time: Option<String>,
    sample_aspect_ratio: Option<String>,
    color_transfer: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AudioFrames {
    frames: Vec<AudioFrame>,
}

#[derive(Debug, Deserialize)]
struct AudioFrame {
    best_effort_timestamp_time: Option<String>,
}

fn canonical_sample_count(bytes: u64) -> Result<u64> {
    // f32le output contains two interleaved four-byte values per sample frame.
    if bytes == 0 || !bytes.is_multiple_of(8) {
        return Err(Error::Media(
            "canonical audio is empty or has a partial sample frame".into(),
        ));
    }
    Ok(bytes / 8)
}

async fn count_canonical_samples(mut reader: impl AsyncRead + Unpin) -> Result<u64> {
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let bytes = reader.read(&mut buffer).await?;
        if bytes == 0 {
            break;
        }
        total = total
            .checked_add(bytes as u64)
            .ok_or_else(|| Error::Media("canonical audio byte count overflow".into()))?;
    }
    canonical_sample_count(total)
}

async fn read_limited(reader: impl AsyncRead + Unpin, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes).await?;
    if bytes.len() as u64 > limit {
        return Err(Error::Media("worker output exceeded limit".into()));
    }
    Ok(bytes)
}

impl MediaTools {
    /// Prove a source's complete canonical PCM length without retaining its samples.
    /// Call outside the project writer lock; this can decode an entire long asset.
    pub async fn canonical_audio_samples(&self, path: &Path, expected_digest: &str) -> Result<u64> {
        let path = local_file(path)?;
        if content_hash(&path).await? != expected_digest {
            return Err(Error::Invalid(
                "audio source changed before sample verification".into(),
            ));
        }
        let probe_args = [
            "-v",
            "error",
            "-threads",
            "2",
            "-protocol_whitelist",
            "file",
            "-select_streams",
            "a:0",
            "-show_streams",
            "-of",
            "json",
        ]
        .into_iter()
        .map(OsString::from)
        .chain([path.as_os_str().to_owned()])
        .collect::<Vec<_>>();
        let probe: Probe = serde_json::from_slice(&self.run(&self.ffprobe, &probe_args).await?)?;
        let stream = probe
            .streams
            .first()
            .ok_or_else(|| Error::Unsupported("source has no audio stream".into()))?;
        if !matches!(stream.channels, Some(1 | 2)) {
            return Err(Error::Unsupported(
                "independent audio requires mono or stereo sources".into(),
            ));
        }
        if self.audio_start(&path, stream).await? != 0 {
            return Err(Error::Unsupported(
                "independent audio requires a zero-origin stream".into(),
            ));
        }
        let args = [
            "-hide_banner",
            "-v",
            "error",
            "-nostdin",
            "-xerror",
            "-threads",
            "2",
            "-protocol_whitelist",
            "file",
            "-i",
        ]
        .into_iter()
        .map(OsString::from)
        .chain([path.as_os_str().to_owned()])
        .chain(
            [
                "-map",
                "0:a:0",
                "-vn",
                "-sn",
                "-dn",
                "-af",
                crate::audio_plan::CANONICAL_DECODE_FILTER,
                "-ar",
                "48000",
                "-ac",
                "2",
                "-c:a",
                "pcm_f32le",
                "-f",
                "f32le",
                "pipe:1",
            ]
            .into_iter()
            .map(OsString::from),
        )
        .collect::<Vec<_>>();
        let started = std::time::Instant::now();
        let mut child = async_worker_command(&self.ffmpeg)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| Error::Media(format!("cannot start audio verification worker: {e}")))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Media("stdout unavailable".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Media("stderr unavailable".into()))?;
        let samples = tokio::time::timeout(self.timeout, async {
            let (samples, stderr, status) = tokio::try_join!(
                count_canonical_samples(stdout),
                read_limited(stderr, 1024 * 1024),
                async { child.wait().await.map_err(Error::Io) }
            )?;
            if !status.success() {
                return Err(Error::Media(format!(
                    "audio verification {status}: {}",
                    String::from_utf8_lossy(&stderr)
                )));
            }
            Ok(samples)
        })
        .await
        .map_err(|_| Error::Timeout)??;
        if content_hash(&path).await? != expected_digest {
            return Err(Error::Invalid(
                "audio source changed during sample verification".into(),
            ));
        }
        tracing::debug!(
            canonical_samples = samples,
            elapsed_ms = started.elapsed().as_millis(),
            "canonical audio length verified"
        );
        Ok(samples)
    }

    async fn audio_start(&self, path: &Path, stream: &Stream) -> Result<i64> {
        if stream.start_time.is_some() {
            return stream_start(stream);
        }
        // WAV omits stream.start_time. Inspect bounded decoded-frame PTS instead of
        // assuming zero for arbitrary demuxers or using rounded container duration.
        let args = [
            "-v",
            "error",
            "-threads",
            "2",
            "-protocol_whitelist",
            "file",
            "-select_streams",
            "a:0",
            "-read_intervals",
            "%+#8",
            "-show_frames",
            "-show_entries",
            "frame=best_effort_timestamp_time",
            "-of",
            "json",
        ]
        .into_iter()
        .map(OsString::from)
        .chain([path.as_os_str().to_owned()])
        .collect::<Vec<_>>();
        let frames: AudioFrames = serde_json::from_slice(&self.run(&self.ffprobe, &args).await?)?;
        let first = frames
            .frames
            .first()
            .and_then(|frame| frame.best_effort_timestamp_time.as_deref())
            .ok_or_else(|| Error::Unsupported("unknown decoded audio start time".into()))?;
        i64::try_from(decimal_micros(first)?)
            .map_err(|_| Error::Media("audio stream start overflow".into()))
    }

    pub async fn run(&self, executable: &Path, args: &[OsString]) -> Result<Vec<u8>> {
        let started = std::time::Instant::now();
        tracing::debug!(tool = ?executable.file_name(), "starting bounded media worker");
        let mut child = async_worker_command(executable)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| Error::Media(format!("cannot start {}: {e}", executable.display())))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Media("stdout unavailable".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Media("stderr unavailable".into()))?;
        let future = async {
            let (stdout, stderr, status) = tokio::try_join!(
                read_limited(stdout, 16 * 1024 * 1024),
                read_limited(stderr, 1024 * 1024),
                async { child.wait().await.map_err(Error::Io) }
            )?;
            if !status.success() {
                return Err(Error::Media(format!(
                    "{}: {}",
                    status,
                    String::from_utf8_lossy(&stderr)
                )));
            }
            Ok(stdout)
        };
        let result = tokio::time::timeout(self.timeout, future)
            .await
            .map_err(|_| Error::Timeout)?;
        tracing::debug!(
            elapsed_ms = started.elapsed().as_millis(),
            succeeded = result.is_ok(),
            "media worker finished"
        );
        result
    }

    pub async fn versions(&self) -> Result<serde_json::Value> {
        let args = [OsString::from("-version")];
        let (ffmpeg, ffprobe) = tokio::try_join!(
            self.run(&self.ffmpeg, &args),
            self.run(&self.ffprobe, &args)
        )?;
        let mut capabilities = serde_json::json!({"ffmpeg": String::from_utf8_lossy(&ffmpeg), "ffprobe": String::from_utf8_lossy(&ffprobe),"hardware_runtime_verified":false});
        for kind in ["encoders", "decoders", "hwaccels"] {
            let args = [
                OsString::from("-hide_banner"),
                OsString::from(format!("-{kind}")),
            ];
            let output = self.run(&self.ffmpeg, &args).await?;
            let text = String::from_utf8_lossy(&output);
            let names = text
                .lines()
                .filter_map(|line| {
                    let mut fields = line.split_whitespace();
                    let first = fields.next()?;
                    if kind == "hwaccels" {
                        return (first != "Hardware" && fields.next().is_none())
                            .then(|| first.to_owned());
                    }
                    (first.len() == 6 && first.bytes().all(|b| b == b'.' || b.is_ascii_uppercase()))
                        .then(|| fields.next().map(str::to_owned))
                        .flatten()
                })
                .filter(|name| name != "=")
                .collect::<Vec<_>>();
            capabilities["compiled_components"][kind] = serde_json::json!(names);
        }
        Ok(capabilities)
    }

    async fn inspect_streams(&self, path: &Path) -> Result<Probe> {
        let args = [
            "-v",
            "error",
            "-threads",
            "2",
            "-protocol_whitelist",
            "file",
            "-show_streams",
            "-of",
            "json",
        ]
        .into_iter()
        .map(OsString::from)
        .chain([path.as_os_str().to_owned()])
        .collect::<Vec<_>>();
        let probe: Probe = serde_json::from_slice(&self.run(&self.ffprobe, &args).await?)?;
        if probe.streams.len() > 16
            || probe
                .streams
                .iter()
                .any(|s| s.width.unwrap_or(0) > 8192 || s.height.unwrap_or(0) > 8192)
        {
            return Err(Error::Unsupported(
                "media stream count/dimensions exceed baseline limits".into(),
            ));
        }
        Ok(probe)
    }

    /// Read headers only. Frame count, CFR and source identity remain unverified.
    pub async fn inspect(&self, path: &Path) -> Result<QuickMetadata> {
        let path = local_file(path)?;
        let streams = self.inspect_streams(&path).await?;
        quick_metadata(&path, &streams)
    }

    pub async fn probe(&self, path: &Path) -> Result<Metadata> {
        Ok(self
            .probe_with_progress(path, |_| {})
            .await?
            .into_metadata())
    }

    pub async fn probe_with_progress(
        &self,
        path: &Path,
        progress: impl FnMut(ProbeProgress),
    ) -> Result<VerifiedMetadata> {
        self.probe_with_cached_progress(path, &[], progress).await
    }

    /// Reuse only metadata previously verified by this service and tool policy.
    /// Every reuse still reads the complete source twice to prove its identity.
    /// Dropping this future cancels its bounded kill-on-drop external worker.
    pub async fn probe_with_cached_progress(
        &self,
        path: &Path,
        cache: &[VerifiedMetadata],
        mut progress: impl FnMut(ProbeProgress),
    ) -> Result<VerifiedMetadata> {
        let started = std::time::Instant::now();
        let path = local_file(path)?;
        emit_progress(&mut progress, ProbeStage::Inspecting, started, None);
        let probe = self.inspect_streams(&path).await?;
        let quick = quick_metadata(&path, &probe)?;
        emit_progress(&mut progress, ProbeStage::Hashing, started, Some(&quick));
        let digest = content_hash(&path).await?;
        if let Some(cached) = cache.iter().find(|cached| {
            cached.metadata.size_bytes == quick.size_bytes
                && cached.metadata.blake3.as_deref() == Some(digest.as_str())
        }) {
            emit_progress(
                &mut progress,
                ProbeStage::VerifyingSource,
                started,
                Some(&quick),
            );
            if content_hash(&path).await? != digest {
                return Err(Error::Invalid("media changed while probing".into()));
            }
            tracing::debug!(
                elapsed_ms = started.elapsed().as_millis(),
                "verified import cache reused"
            );
            return Ok(VerifiedMetadata {
                metadata: cached.metadata.clone(),
                source_path: path,
            });
        }
        // Quick headers precede the identity proof and cannot become authoritative.
        // Re-read them inside the pre/post hash window to catch replacement races.
        let probe = self.inspect_streams(&path).await?;
        let video = if let Some(s) = probe.streams.iter().find(|s| s.codec_type == "video") {
            emit_progress(
                &mut progress,
                ProbeStage::ValidatingVideo,
                started,
                Some(&quick),
            );
            let frame_rate = s
                .avg_frame_rate
                .as_deref()
                .ok_or_else(|| Error::Media("missing frame rate".into()))?
                .parse::<FrameRate>()?;
            // Equal average and nominal rates are only a candidate; inspect every timestamp.
            let candidate = s
                .r_frame_rate
                .as_deref()
                .map(str::parse::<FrameRate>)
                .transpose()?
                == Some(frame_rate);
            let time_base = s
                .time_base
                .as_deref()
                .ok_or_else(|| Error::Media("missing video timebase".into()))?;
            // The every-frame PTS pass already decodes and enumerates every frame.
            // Count that same bounded output instead of decoding the source twice.
            // Non-candidates retain the count-only path and are never marked CFR.
            let (frames, constant_rate) = if candidate {
                let timing = self.frame_timing(&path, frame_rate, time_base).await?;
                if timing.0 == 0 {
                    // Empty timestamp output cannot prove even an empty stream.
                    // Retain the previous independent count/error behavior.
                    (self.count_frames(&path).await?, false)
                } else {
                    timing
                }
            } else {
                (self.count_frames(&path).await?, false)
            };
            Some(Video {
                codec: s.codec_name.clone().unwrap_or_default(),
                width: s.width.unwrap_or(0),
                height: s.height.unwrap_or(0),
                frame_rate,
                frames,
                constant_rate,
                start_time_micros: stream_start(s)?,
                sample_aspect_ratio: s
                    .sample_aspect_ratio
                    .clone()
                    .unwrap_or_else(|| "unknown".into()),
                color_transfer: s.color_transfer.clone(),
            })
        } else {
            None
        };
        let mut audio = if let Some(s) = probe.streams.iter().find(|s| s.codec_type == "audio") {
            Some(Audio {
                codec: s.codec_name.clone().unwrap_or_default(),
                sample_rate: s
                    .sample_rate
                    .as_deref()
                    .ok_or_else(|| Error::Media("missing audio sample rate".into()))?
                    .parse()
                    .map_err(|_| Error::Media("invalid audio sample rate".into()))?,
                channels: s.channels.unwrap_or(0),
                start_time_micros: self.audio_start(&path, s).await?,
                canonical_samples_48k: None,
            })
        } else {
            None
        };
        // Music-only imports need a proven length for deterministic placement defaults.
        // Video imports retain their cheap audio metadata path; placement proves them
        // lazily if independent audio is requested.
        if video.is_none()
            && let Some(audio) = &mut audio
            && matches!(audio.channels, 1 | 2)
            && audio.start_time_micros == 0
        {
            emit_progress(
                &mut progress,
                ProbeStage::ValidatingAudio,
                started,
                Some(&quick),
            );
            audio.canonical_samples_48k = Some(self.canonical_audio_samples(&path, &digest).await?);
        }
        if video.is_none() && audio.is_none() {
            return Err(Error::Unsupported("no video or audio stream".into()));
        }
        emit_progress(
            &mut progress,
            ProbeStage::VerifyingSource,
            started,
            Some(&quick),
        );
        if content_hash(&path).await? != digest {
            return Err(Error::Invalid("media changed while probing".into()));
        }
        Ok(VerifiedMetadata {
            source_path: path.clone(),
            metadata: Metadata {
                size_bytes: std::fs::metadata(path)?.len(),
                blake3: Some(digest),
                video,
                audio,
            },
        })
    }

    async fn count_frames(&self, path: &Path) -> Result<u64> {
        let args = [
            "-v",
            "error",
            "-threads",
            "2",
            "-protocol_whitelist",
            "file",
            "-select_streams",
            "v:0",
            "-count_frames",
            "-show_streams",
            "-of",
            "json",
        ]
        .into_iter()
        .map(OsString::from)
        .chain([path.as_os_str().to_owned()])
        .collect::<Vec<_>>();
        let counted: Probe = serde_json::from_slice(&self.run(&self.ffprobe, &args).await?)?;
        counted
            .streams
            .first()
            .and_then(|stream| stream.nb_read_frames.as_deref())
            .ok_or_else(|| Error::Unsupported("cannot count video frames".into()))?
            .parse()
            .map_err(|_| Error::Media("invalid frame count".into()))
    }

    async fn frame_timing(
        &self,
        path: &Path,
        rate: FrameRate,
        time_base: &str,
    ) -> Result<(u64, bool)> {
        let args = [
            "-v",
            "error",
            "-threads",
            "2",
            "-protocol_whitelist",
            "file",
            "-select_streams",
            "v:0",
            "-show_frames",
            "-show_entries",
            "frame=best_effort_timestamp",
            "-of",
            "csv=p=0",
        ]
        .into_iter()
        .map(OsString::from)
        .chain([path.as_os_str().to_owned()])
        .collect::<Vec<_>>();
        let bytes = self.run(&self.ffprobe, &args).await?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| Error::Media("invalid timestamp text".into()))?;
        let timestamps = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                line.split(',')
                    .next()
                    .unwrap_or_default()
                    .parse::<i64>()
                    .map(i128::from)
                    .map_err(|_| Error::Media("missing/invalid frame PTS".into()))
            })
            .collect::<Result<Vec<_>>>()?;
        let frames = timestamps.len() as u64;
        if timestamps.is_empty() {
            return Ok((frames, false));
        }
        let (numerator, denominator) = time_base
            .split_once('/')
            .ok_or_else(|| Error::Media("invalid stream timebase".into()))?;
        let numerator = numerator
            .parse::<u32>()
            .map_err(|_| Error::Media("invalid timebase numerator".into()))?;
        let denominator = denominator
            .parse::<u32>()
            .map_err(|_| Error::Media("invalid timebase denominator".into()))?;
        if numerator == 0 || denominator == 0 {
            return Err(Error::Media("zero stream timebase".into()));
        }
        // Compare integer timestamps with one container tick tolerance for quantized CFR.
        let base = timestamps[0];
        if timestamps.windows(2).any(|pair| pair[0] >= pair[1])
            || u64::from(numerator) * u64::from(rate.numerator())
                >= u64::from(denominator) * u64::from(rate.denominator())
        {
            return Ok((frames, false));
        }
        Ok((
            frames,
            timestamps.iter().enumerate().all(|(index, timestamp)| {
                let observed =
                    (*timestamp - base) * i128::from(numerator) * i128::from(rate.numerator());
                let ideal =
                    index as i128 * i128::from(rate.denominator()) * i128::from(denominator);
                (observed - ideal).abs() <= i128::from(numerator) * i128::from(rate.numerator())
            }),
        ))
    }
}

fn emit_progress(
    callback: &mut impl FnMut(ProbeProgress),
    stage: ProbeStage,
    started: std::time::Instant,
    quick: Option<&QuickMetadata>,
) {
    callback(ProbeProgress {
        stage,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        quick: quick.cloned(),
    });
}

fn quick_metadata(path: &Path, probe: &Probe) -> Result<QuickMetadata> {
    Ok(QuickMetadata {
        size_bytes: std::fs::metadata(path)?.len(),
        video: probe
            .streams
            .iter()
            .find(|stream| stream.codec_type == "video")
            .map(|stream| QuickVideo {
                codec: stream.codec_name.clone().unwrap_or_default(),
                width: stream.width.unwrap_or(0),
                height: stream.height.unwrap_or(0),
                frame_rate: stream
                    .avg_frame_rate
                    .as_deref()
                    .and_then(|rate| rate.parse().ok()),
            }),
        audio: probe
            .streams
            .iter()
            .find(|stream| stream.codec_type == "audio")
            .map(|stream| QuickAudio {
                codec: stream.codec_name.clone().unwrap_or_default(),
                sample_rate: stream
                    .sample_rate
                    .as_deref()
                    .and_then(|rate| rate.parse().ok()),
                channels: stream.channels.unwrap_or(0),
            }),
    })
}

fn stream_start(stream: &Stream) -> Result<i64> {
    let start = stream
        .start_time
        .as_deref()
        .ok_or_else(|| Error::Unsupported("unknown stream start time".into()))?;
    i64::try_from(decimal_micros(start)?).map_err(|_| Error::Media("stream start overflow".into()))
}

fn decimal_micros(text: &str) -> Result<i128> {
    let (whole, decimal) = text.split_once('.').unwrap_or((text, ""));
    let whole = whole
        .parse::<i64>()
        .map_err(|_| Error::Media("invalid frame timestamp".into()))?;
    if !decimal.bytes().all(|c| c.is_ascii_digit()) || decimal.len() > 6 {
        return Err(Error::Media("unsupported frame timestamp precision".into()));
    }
    let fraction = format!("{decimal:0<6}")
        .parse::<i128>()
        .map_err(|_| Error::Media("invalid timestamp fraction".into()))?;
    let sign = if text.starts_with('-') { -1 } else { 1 };
    Ok(i128::from(whole) * 1_000_000 + sign * fraction)
}

pub fn local_file(path: &Path) -> Result<PathBuf> {
    let path = std::fs::canonicalize(path)?;
    if !path.is_file() {
        return Err(Error::Invalid("media must be a regular local file".into()));
    }
    Ok(path)
}

/// Bounded-memory hashing; no whole-file allocation or mmap of mutable footage.
pub async fn content_hash(path: &Path) -> Result<String> {
    let mut input = tokio::fs::File::open(path).await?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; 256 * 1024];
    loop {
        let bytes = input.read(&mut buffer).await?;
        if bytes == 0 {
            break;
        }
        hasher.update(&buffer[..bytes]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleWindow() -> *mut std::ffi::c_void;
    }

    #[cfg(windows)]
    fn has_console() -> bool {
        // SAFETY: this query takes no arguments and returns a borrowed HWND that is only
        // compared with null, never dereferenced or closed. Win32 is confined to this test.
        !unsafe { GetConsoleWindow() }.is_null()
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn worker_from_no_console_parent_preserves_pipes_without_console() -> Result<()> {
        const ROLE: &str = "LUMERYN_MEDIA_CONSOLE_TEST_ROLE";
        const NAME: &str =
            "media::tests::worker_from_no_console_parent_preserves_pipes_without_console";
        let executable = std::env::current_exe()?;
        match std::env::var(ROLE).as_deref() {
            Ok("probe") => {
                assert!(!has_console(), "media worker must not allocate a console");
                println!("media-worker-stdout-without-console");
                eprintln!("media-worker-stderr-without-console");
                Ok(())
            }
            role => {
                let mut command = if role == Ok("parent") {
                    assert!(
                        !has_console(),
                        "test parent must have no console, like Studio"
                    );
                    async_worker_command(&executable)
                } else {
                    // The outer console test harness starts a genuine no-console parent.
                    // Without the factory's CREATE_NO_WINDOW, its console child allocates one.
                    let mut detached = Command::new(&executable);
                    detached.creation_flags(0x0000_0008); // DETACHED_PROCESS; test parent only.
                    detached
                };
                let inner_role = if role == Ok("parent") {
                    "probe"
                } else {
                    "parent"
                };
                let output = tokio::time::timeout(
                    Duration::from_secs(10),
                    command
                        .args(["--exact", NAME, "--nocapture"])
                        .env(ROLE, inner_role)
                        .stdin(Stdio::null())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .kill_on_drop(true)
                        .output(),
                )
                .await
                .map_err(|error| {
                    Error::Media(format!("no-console worker test timed out: {error}"))
                })??;
                assert!(
                    output.status.success(),
                    "worker test failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                if inner_role == "probe" {
                    assert!(
                        String::from_utf8_lossy(&output.stdout)
                            .contains("media-worker-stdout-without-console")
                    );
                    assert!(
                        String::from_utf8_lossy(&output.stderr)
                            .contains("media-worker-stderr-without-console")
                    );
                }
                Ok(())
            }
        }
    }
    #[test]
    fn timestamps_are_integer() -> Result<()> {
        assert_eq!(decimal_micros("1.001000")?, 1_001_000);
        assert_eq!(decimal_micros("-0.500000")?, -500_000);
        assert!(decimal_micros("NaN").is_err());
        Ok(())
    }

    #[tokio::test]
    async fn canonical_counter_rejects_partial_and_empty_frames() -> Result<()> {
        assert_eq!(count_canonical_samples(&[0_u8; 24][..]).await?, 3);
        assert!(count_canonical_samples(&[0_u8; 7][..]).await.is_err());
        assert!(count_canonical_samples(&[][..]).await.is_err());
        assert_eq!(canonical_sample_count(u64::MAX - 7)?, (u64::MAX - 7) / 8);
        assert!(canonical_sample_count(u64::MAX).is_err());
        Ok(())
    }

    #[test]
    fn old_audio_json_preserves_omitted_sample_proof() -> Result<()> {
        let old = serde_json::json!({"codec":"pcm_s16le","sample_rate":48000,
            "channels":2,"start_time_micros":0});
        let mut audio: Audio = serde_json::from_value(old.clone())?;
        assert_eq!(audio.canonical_samples_48k, None);
        assert_eq!(serde_json::to_value(&audio)?, old);
        audio.canonical_samples_48k = Some(48_000);
        assert_eq!(
            serde_json::to_value(audio)?["canonical_samples_48k"],
            48_000
        );
        Ok(())
    }
}
