use crate::{
    Error, MediaTools, Project, Result,
    audio_plan::AudioPlan,
    media::{content_hash, local_file},
};
use serde::Serialize;
mod encoder;
mod options;
pub use options::{Acceleration, RenderOptions, RenderQuality, RenderResolution, VideoCodec};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
use tempfile::Builder;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderStage {
    Validating,
    EncoderProbe,
    Encoding,
    Verification,
    Publication,
}

#[derive(Debug, Serialize)]
pub struct RenderResult {
    pub path: PathBuf,
    pub project_revision: u64,
    pub frames: u64,
    pub frame_rate: String,
    pub width: u32,
    pub height: u32,
    pub preset: &'static str,
    pub verified_full_decode: bool,
    pub options: RenderOptions,
    pub encoder: String,
    pub hardware: bool,
    pub fallback_reason: Option<String>,
    pub duration_seconds: f64,
    pub upscaled: bool,
}

/// Evaluate a narrow supported graph in Rust before translating to a media backend.
pub async fn export(
    project: &Project,
    database: &Path,
    output: &Path,
    tools: &MediaTools,
) -> Result<RenderResult> {
    export_with_options(project, database, output, tools, &RenderOptions::default()).await
}

pub async fn export_with_options(
    project: &Project,
    database: &Path,
    output: &Path,
    tools: &MediaTools,
    options: &RenderOptions,
) -> Result<RenderResult> {
    export_with_progress(project, database, output, tools, options, |_| {
        std::future::ready(())
    })
    .await
}

pub async fn export_with_progress<F, Fut>(
    project: &Project,
    database: &Path,
    output: &Path,
    tools: &MediaTools,
    options: &RenderOptions,
    progress: F,
) -> Result<RenderResult>
where
    F: Fn(RenderStage) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    progress(RenderStage::Validating).await;
    project.validate()?;
    options.validate()?;
    let (width, height) = options.dimensions(project);
    if width > 3840 || height > 3840 {
        return Err(Error::Unsupported(
            "export beyond UHD dimensions is staged; select an implemented resolution preset"
                .into(),
        ));
    }
    let frame_rate = options.frame_rate(project)?;
    // Evaluate the project canvas directly at final resolution. Never downsample a UHD
    // source to the editor canvas before expanding it again for export.
    let (canvas_width, canvas_height) = fitted_canvas(project.width, project.height, width, height);
    if output.exists() {
        return Err(Error::Exists(output.display().to_string()));
    }
    if output.extension().and_then(|s| s.to_str()) != Some("mp4") {
        return Err(Error::Unsupported("baseline export requires .mp4".into()));
    }
    let mut clips = project.clips.iter().collect::<Vec<_>>();
    clips.sort_by_key(|c| c.at_frame);
    if clips.is_empty() || clips.len() > 64 {
        return Err(Error::Unsupported(
            "export requires 1..64 contiguous clips".into(),
        ));
    }
    let mut frames = 0_u64;
    let mut args = [
        "-hide_banner",
        "-v",
        "error",
        "-nostdin",
        "-y",
        "-filter_complex_threads",
        "2",
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    let mut filters = Vec::new();
    let mut source_digests = Vec::new();
    // Keep the established video-only evaluator stable. Independent audio activates
    // the shared canonical mix also consumed by native playback.
    let audio_plan = if project.audio_clips.is_empty() {
        None
    } else {
        Some(AudioPlan::from_project(project, database).await?)
    };
    for (index, clip) in clips.iter().enumerate() {
        if clip.at_frame != frames {
            return Err(Error::Unsupported(
                "export cannot render gaps or overlapping clips yet".into(),
            ));
        }
        frames += clip.duration_frames;
        let media = project
            .media
            .iter()
            .find(|m| m.id == clip.media_id)
            .ok_or_else(|| Error::NotFound("clip media".into()))?;
        if media
            .metadata
            .video
            .as_ref()
            .is_some_and(|v| v.start_time_micros != 0)
            || media
                .metadata
                .audio
                .as_ref()
                .is_some_and(|a| a.start_time_micros != 0)
        {
            return Err(Error::Unsupported("baseline export requires zero-origin video/audio; timestamp offset preservation is planned".into()));
        }
        let video = media
            .metadata
            .video
            .as_ref()
            .ok_or_else(|| Error::Unsupported("video stream required".into()))?;
        if video.sample_aspect_ratio != "1:1" {
            return Err(Error::Unsupported(
                "baseline export requires known square-pixel sources".into(),
            ));
        }
        if video
            .color_transfer
            .as_deref()
            .is_some_and(|t| t == "smpte2084" || t == "arib-std-b67")
        {
            return Err(Error::Unsupported(
                "HDR export requires a color-managed backend".into(),
            ));
        }
        let parent = database
            .parent()
            .ok_or_else(|| Error::Invalid("database has no parent".into()))?;
        let path = local_file(&parent.join(&media.path))?;
        if std::fs::metadata(&path)?.len() != media.metadata.size_bytes {
            return Err(Error::Invalid("source size changed; reimport media".into()));
        }
        let digest = media.metadata.blake3.as_ref().ok_or_else(|| {
            Error::Unsupported(
                "legacy media has no content digest; import a verified source".into(),
            )
        })?;
        if &content_hash(&path).await? != digest {
            return Err(Error::Invalid(
                "source content changed; reimport media".into(),
            ));
        }
        source_digests.push((path.clone(), digest));
        args.extend([
            OsString::from("-threads"),
            OsString::from("2"),
            OsString::from("-protocol_whitelist"),
            OsString::from("file"),
            OsString::from("-i"),
            path.into_os_string(),
        ]);
        let end = clip.source_in_frame + clip.duration_frames;
        filters.push(format!("[{index}:v:0]trim=start_frame={}:end_frame={end},setpts=PTS-STARTPTS,scale={canvas_width}:{canvas_height}:force_original_aspect_ratio=decrease:flags=lanczos,pad={canvas_width}:{canvas_height}:(ow-iw)/2:(oh-ih)/2,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2,setsar=1[v{index}]", clip.source_in_frame));
        let duration = project.frame_rate.seconds(clip.duration_frames);
        if audio_plan.is_some() {
            continue;
        }
        if media.metadata.audio.is_some() {
            filters.push(format!("[{index}:a:0]atrim=start={}:duration={duration},asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,apad,atrim=duration={duration}[a{index}]", project.frame_rate.seconds(clip.source_in_frame)));
        } else {
            filters.push(format!("anullsrc=r=48000:cl=stereo,atrim=duration={duration},asetpts=PTS-STARTPTS[a{index}]"));
        }
    }
    if let Some(plan) = &audio_plan {
        let inputs = (0..clips.len())
            .map(|index| format!("[v{index}]"))
            .collect::<String>();
        filters.push(format!("{inputs}concat=n={}:v=1:a=0[vcat]", clips.len()));
        args.extend(plan.input_args());
        filters.push(plan.filter_program(clips.len(), 0)?.filters);
    } else {
        let inputs = (0..clips.len())
            .map(|index| format!("[v{index}][a{index}]"))
            .collect::<String>();
        filters.push(format!(
            "{inputs}concat=n={}:v=1:a=1[vcat][acat]",
            clips.len()
        ));
    }
    let rendered_frames = options::output_frames(frames, project.frame_rate, frame_rate)?;
    let duration = frame_rate.seconds(rendered_frames);
    // Convert presentation rate once after evaluating original source-frame cuts. Pad at most
    // the quantized final frame; never decode proxies or apply monitor-only audio settings.
    let mut video_filter = String::new();
    video_filter.push_str(&format!("settb=expr={}/{},setpts=N,fps=fps={frame_rate}:start_time=0:round=up:eof_action=pass,tpad=stop_mode=clone:stop_duration=1,trim=end_frame={rendered_frames},setpts=N/({frame_rate}*TB)", project.frame_rate.denominator(), project.frame_rate.numerator()));
    filters.push(format!("[vcat]{video_filter}[vout]"));
    let audio_label = if audio_plan.is_some() {
        "lumeryn_audio_output"
    } else {
        "acat"
    };
    filters.push(format!(
        "[{audio_label}]apad,atrim=duration={duration},asetpts=PTS-STARTPTS[aout]"
    ));
    progress(RenderStage::EncoderProbe).await;
    let mut selected =
        encoder::select(tools, options, width, height, &frame_rate.to_string()).await?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = Builder::new()
        .prefix(".lumeryn-export-")
        .suffix(".mp4")
        .tempfile_in(parent)?;
    args.extend([
        OsString::from("-filter_complex"),
        filters.join(";").into(),
        "-map".into(),
        "[vout]".into(),
        "-map".into(),
        "[aout]".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-r".into(),
        frame_rate.to_string().into(),
        "-fps_mode".into(),
        "cfr".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "192k".into(),
        "-threads".into(),
        "2".into(),
        "-movflags".into(),
        "+faststart".into(),
    ]);
    let encode = |selected: &encoder::Encoder| {
        let mut arguments = args.clone();
        arguments.extend(encoder::arguments(selected.name, options));
        arguments.push(temporary.path().as_os_str().to_owned());
        arguments
    };
    progress(RenderStage::Encoding).await;
    if let Err(error) = tools.run(&tools.ffmpeg, &encode(&selected)).await {
        if !selected.hardware || options.acceleration != Acceleration::Auto {
            return Err(error);
        }
        let mut cpu_options = options.clone();
        cpu_options.acceleration = Acceleration::Cpu;
        let cpu =
            encoder::select(tools, &cpu_options, width, height, &frame_rate.to_string()).await?;
        selected = encoder::Encoder {
            fallback: Some(format!(
                "Hardware encode {} failed; retried CPU {}: {error}",
                selected.name, cpu.name
            )),
            ..cpu
        };
        tools.run(&tools.ffmpeg, &encode(&selected)).await?;
    }
    progress(RenderStage::Verification).await;
    for (path, digest) in source_digests {
        if &content_hash(&path).await? != digest {
            return Err(Error::Invalid(
                "source changed during export; output rejected".into(),
            ));
        }
    }
    if let Some(plan) = &audio_plan {
        plan.verify_sources().await?;
    }
    let metadata = tools.probe(temporary.path()).await?;
    let video = metadata
        .video
        .ok_or_else(|| Error::Media("export has no video".into()))?;
    if video.frames != rendered_frames
        || video.frame_rate != frame_rate
        || video.width != width
        || video.height != height
        || video.codec
            != match options.codec {
                VideoCodec::Mpeg4 => "mpeg4",
                VideoCodec::H264 => "h264",
                VideoCodec::H265 => "hevc",
            }
        || video.start_time_micros != 0
        || metadata.audio.as_ref().is_none_or(|audio| {
            audio.start_time_micros != 0 || audio.channels != 2 || audio.sample_rate != 48_000
        })
    {
        return Err(Error::Media(
            "export failed frame count, frame rate, dimensions or audio verification".into(),
        ));
    }
    let args = [
        "-v",
        "error",
        "-nostdin",
        "-xerror",
        "-protocol_whitelist",
        "file",
        "-i",
    ]
    .into_iter()
    .map(OsString::from)
    .chain([
        temporary.path().as_os_str().to_owned(),
        "-f".into(),
        "null".into(),
        "-".into(),
    ])
    .collect::<Vec<_>>();
    tools.run(&tools.ffmpeg, &args).await?;
    progress(RenderStage::Publication).await;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(output).map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            Error::Exists(output.display().to_string())
        } else {
            Error::Io(error.error)
        }
    })?;
    Ok(RenderResult {
        path: std::fs::canonicalize(output)?,
        project_revision: project.revision,
        frames: rendered_frames,
        frame_rate: frame_rate.to_string(),
        width,
        height,
        preset: match options.codec {
            VideoCodec::Mpeg4 => "baseline-mpeg4-aac",
            VideoCodec::H264 => "h264-aac",
            VideoCodec::H265 => "h265-aac",
        },
        verified_full_decode: true,
        options: options.clone(),
        encoder: selected.name.into(),
        hardware: selected.hardware,
        fallback_reason: selected.fallback,
        duration_seconds: rendered_frames as f64 * f64::from(frame_rate.denominator())
            / f64::from(frame_rate.numerator()),
        upscaled: clips.iter().any(|clip| {
            project
                .media
                .iter()
                .find(|media| media.id == clip.media_id)
                .and_then(|media| media.metadata.video.as_ref())
                .is_some_and(|video| {
                    (f64::from(canvas_width) / f64::from(video.width))
                        .min(f64::from(canvas_height) / f64::from(video.height))
                        > 1.0
                })
        }),
    })
}

fn fitted_canvas(project_width: u32, project_height: u32, width: u32, height: u32) -> (u32, u32) {
    let (fit_width, fit_height) = if u64::from(width) * u64::from(project_height)
        <= u64::from(height) * u64::from(project_width)
    {
        (
            width,
            (u64::from(width) * u64::from(project_height) / u64::from(project_width)) as u32,
        )
    } else {
        (
            (u64::from(height) * u64::from(project_width) / u64::from(project_height)) as u32,
            height,
        )
    };
    (fit_width.max(2) / 2 * 2, fit_height.max(2) / 2 * 2)
}
