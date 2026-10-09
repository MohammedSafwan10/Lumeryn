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

async fn read_limited(reader: impl AsyncRead + Unpin, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes).await?;
    if bytes.len() as u64 > limit {
        return Err(Error::Media("worker output exceeded limit".into()));
    }
    Ok(bytes)
}

impl MediaTools {
    pub async fn run(&self, executable: &Path, args: &[OsString]) -> Result<Vec<u8>> {
        let started = std::time::Instant::now();
        tracing::debug!(tool = ?executable.file_name(), "starting bounded media worker");
        let mut child = Command::new(executable)
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

    pub async fn probe(&self, path: &Path) -> Result<Metadata> {
        let path = local_file(path)?;
        let digest = content_hash(&path).await?;
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
        let video = if let Some(s) = probe.streams.iter().find(|s| s.codec_type == "video") {
            let frame_rate = s
                .avg_frame_rate
                .as_deref()
                .ok_or_else(|| Error::Media("missing frame rate".into()))?
                .parse::<FrameRate>()?;
            let count_args = [
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
            let counted: Probe =
                serde_json::from_slice(&self.run(&self.ffprobe, &count_args).await?)?;
            let counted = counted
                .streams
                .first()
                .ok_or_else(|| Error::Media("video stream disappeared".into()))?;
            let frames = counted
                .nb_read_frames
                .as_deref()
                .ok_or_else(|| Error::Unsupported("cannot count video frames".into()))?
                .parse::<u64>()
                .map_err(|_| Error::Media("invalid frame count".into()))?;
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
            let constant_rate = candidate
                && self
                    .constant_rate(&path, frame_rate, time_base, frames)
                    .await?;
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
        let audio = probe
            .streams
            .iter()
            .find(|s| s.codec_type == "audio")
            .map(|s| {
                Ok::<_, Error>(Audio {
                    codec: s.codec_name.clone().unwrap_or_default(),
                    sample_rate: s
                        .sample_rate
                        .as_deref()
                        .ok_or_else(|| Error::Media("missing audio sample rate".into()))?
                        .parse()
                        .map_err(|_| Error::Media("invalid audio sample rate".into()))?,
                    channels: s.channels.unwrap_or(0),
                    start_time_micros: stream_start(s)?,
                })
            })
            .transpose()?;
        if video.is_none() && audio.is_none() {
            return Err(Error::Unsupported("no video or audio stream".into()));
        }
        if content_hash(&path).await? != digest {
            return Err(Error::Invalid("media changed while probing".into()));
        }
        Ok(Metadata {
            size_bytes: std::fs::metadata(path)?.len(),
            blake3: Some(digest),
            video,
            audio,
        })
    }

    async fn constant_rate(
        &self,
        path: &Path,
        rate: FrameRate,
        time_base: &str,
        expected_frames: u64,
    ) -> Result<bool> {
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
        if timestamps.len() as u64 != expected_frames || timestamps.is_empty() {
            return Ok(false);
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
            return Ok(false);
        }
        Ok(timestamps.iter().enumerate().all(|(index, timestamp)| {
            let observed =
                (*timestamp - base) * i128::from(numerator) * i128::from(rate.numerator());
            let ideal = index as i128 * i128::from(rate.denominator()) * i128::from(denominator);
            (observed - ideal).abs() <= i128::from(numerator) * i128::from(rate.numerator())
        }))
    }
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
    #[test]
    fn timestamps_are_integer() -> Result<()> {
        assert_eq!(decimal_micros("1.001000")?, 1_001_000);
        assert_eq!(decimal_micros("-0.500000")?, -500_000);
        assert!(decimal_micros("NaN").is_err());
        Ok(())
    }
}
