use super::{Acceleration, RenderOptions, RenderQuality, VideoCodec};
use crate::{Error, MediaTools, Result};
use std::{ffi::OsString, time::Duration};

pub(super) struct Encoder {
    pub name: &'static str,
    pub hardware: bool,
    pub fallback: Option<String>,
}
pub(super) fn arguments(name: &str, options: &RenderOptions) -> Vec<OsString> {
    let mut args = vec!["-c:v".into(), name.into()];
    let quality = match options.quality {
        RenderQuality::Draft => "28",
        RenderQuality::Balanced => "23",
        RenderQuality::High => "18",
    };
    if name == "mpeg4" {
        if let Some(rate) = options.bitrate_kbps {
            args.extend(["-b:v".into(), format!("{rate}k").into()]);
        } else {
            args.extend([
                "-q:v".into(),
                match options.quality {
                    RenderQuality::Draft => "6",
                    RenderQuality::Balanced => "4",
                    RenderQuality::High => "2",
                }
                .into(),
            ]);
        }
    } else if name.starts_with("libx") {
        args.extend([
            "-preset".into(),
            match options.quality {
                RenderQuality::Draft => "veryfast",
                RenderQuality::Balanced => "fast",
                RenderQuality::High => "medium",
            }
            .into(),
        ]);
        if let Some(rate) = options.bitrate_kbps {
            args.extend(["-b:v".into(), format!("{rate}k").into()]);
        } else {
            args.extend(["-crf".into(), quality.into()]);
        }
        if name == "libx265" {
            args.extend([
                "-x265-params".into(),
                "pools=2:frame-threads=2:log-level=error".into(),
            ]);
        }
    } else if name.ends_with("_nvenc") {
        args.extend([
            "-preset".into(),
            match options.quality {
                RenderQuality::Draft => "p2",
                RenderQuality::Balanced => "p4",
                RenderQuality::High => "p5",
            }
            .into(),
        ]);
        if let Some(rate) = options.bitrate_kbps {
            args.extend(["-b:v".into(), format!("{rate}k").into()]);
        } else {
            args.extend([
                "-rc".into(),
                "vbr".into(),
                "-cq".into(),
                quality.into(),
                "-b:v".into(),
                "0".into(),
            ]);
        }
    } else if name.ends_with("_qsv") {
        if let Some(rate) = options.bitrate_kbps {
            args.extend(["-b:v".into(), format!("{rate}k").into()]);
        } else {
            args.extend(["-global_quality".into(), quality.into()]);
        }
    } else {
        let rate = options.bitrate_kbps.unwrap_or(match options.quality {
            RenderQuality::Draft => 12_000,
            RenderQuality::Balanced => 25_000,
            RenderQuality::High => 45_000,
        });
        args.extend(["-b:v".into(), format!("{rate}k").into()]);
    }
    if options.codec == VideoCodec::H265 {
        args.extend(["-tag:v".into(), "hvc1".into()]);
    }
    args
}

async fn probe(
    tools: &MediaTools,
    name: &str,
    options: &RenderOptions,
    width: u32,
    height: u32,
    fps: &str,
) -> Result<()> {
    let mut tools = tools.clone();
    tools.timeout = tools.timeout.min(Duration::from_secs(8));
    let mut args = [
        "-hide_banner",
        "-v",
        "error",
        "-nostdin",
        "-f",
        "lavfi",
        "-i",
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    args.push(format!("color=c=black:s={width}x{height}:r={fps}").into());
    args.extend([
        "-frames:v".into(),
        "1".into(),
        "-an".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-threads".into(),
        "2".into(),
    ]);
    args.extend(arguments(name, options));
    args.extend(["-f".into(), "null".into(), "-".into()]);
    tools.run(&tools.ffmpeg, &args).await?;
    Ok(())
}

pub(super) async fn select(
    tools: &MediaTools,
    options: &RenderOptions,
    width: u32,
    height: u32,
    fps: &str,
) -> Result<Encoder> {
    let software = match options.codec {
        VideoCodec::Mpeg4 => "mpeg4",
        VideoCodec::H264 => "libx264",
        VideoCodec::H265 => "libx265",
    };
    if options.codec == VideoCodec::Mpeg4 {
        return Ok(Encoder {
            name: software,
            hardware: false,
            fallback: None,
        });
    }
    let listing = tools
        .run(&tools.ffmpeg, &["-hide_banner".into(), "-encoders".into()])
        .await?;
    let listing = String::from_utf8_lossy(&listing);
    let available = |name: &str| {
        listing
            .lines()
            .any(|line| line.split_whitespace().nth(1) == Some(name))
    };
    let mut failures = Vec::new();
    if options.acceleration == Acceleration::Auto {
        let candidates = match options.codec {
            VideoCodec::H264 => ["h264_nvenc", "h264_qsv", "h264_amf"],
            VideoCodec::H265 => ["hevc_nvenc", "hevc_qsv", "hevc_amf"],
            VideoCodec::Mpeg4 => {
                return Ok(Encoder {
                    name: software,
                    hardware: false,
                    fallback: None,
                });
            }
        };
        for name in candidates {
            if !available(name) {
                continue;
            }
            match probe(tools, name, options, width, height, fps).await {
                Ok(()) => {
                    return Ok(Encoder {
                        name,
                        hardware: true,
                        fallback: None,
                    });
                }
                Err(error) => failures.push(format!("{name}: {error}")),
            }
        }
    }
    if !available(software) {
        return Err(Error::Unsupported(format!(
            "requested codec has no usable encoder in this FFmpeg build; {software} absent; {}",
            failures.join("; ")
        )));
    }
    probe(tools, software, options, width, height, fps)
        .await
        .map_err(|e| {
            Error::Unsupported(format!("CPU encoder {software} initialization failed: {e}"))
        })?;
    let fallback = (options.acceleration == Acceleration::Auto).then(|| {
        if failures.is_empty() {
            "No supported hardware encoder present; selected CPU encoder".into()
        } else {
            format!(
                "Hardware probes failed; selected CPU: {}",
                failures.join("; ")
            )
        }
    });
    Ok(Encoder {
        name: software,
        hardware: false,
        fallback,
    })
}
