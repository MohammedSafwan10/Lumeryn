use crate::{
    Error, MediaTools, Project, Result,
    media::{content_hash, local_file},
};
use serde::Serialize;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
use tempfile::Builder;

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
}

/// Evaluate a narrow supported graph in Rust before translating to a media backend.
pub async fn export(
    project: &Project,
    database: &Path,
    output: &Path,
    tools: &MediaTools,
) -> Result<RenderResult> {
    project.validate()?;
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
        filters.push(format!("[{index}:v:0]trim=start_frame={}:end_frame={end},setpts=PTS-STARTPTS,scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,setsar=1[v{index}]",
            clip.source_in_frame, project.width, project.height, project.width, project.height));
        let duration = project.frame_rate.seconds(clip.duration_frames);
        if media.metadata.audio.is_some() {
            filters.push(format!("[{index}:a:0]atrim=start={}:duration={duration},asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,apad,atrim=duration={duration}[a{index}]", project.frame_rate.seconds(clip.source_in_frame)));
        } else {
            filters.push(format!("anullsrc=r=48000:cl=stereo,atrim=duration={duration},asetpts=PTS-STARTPTS[a{index}]"));
        }
    }
    let inputs = (0..clips.len())
        .map(|index| format!("[v{index}][a{index}]"))
        .collect::<String>();
    filters.push(format!(
        "{inputs}concat=n={}:v=1:a=1[vout][aout]",
        clips.len()
    ));
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
        "-c:v".into(),
        "mpeg4".into(),
        "-q:v".into(),
        "2".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-r".into(),
        project.frame_rate.to_string().into(),
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
        temporary.path().as_os_str().to_owned(),
    ]);
    tools.run(&tools.ffmpeg, &args).await?;
    for (path, digest) in source_digests {
        if &content_hash(&path).await? != digest {
            return Err(Error::Invalid(
                "source changed during export; output rejected".into(),
            ));
        }
    }
    let metadata = tools.probe(temporary.path()).await?;
    let video = metadata
        .video
        .ok_or_else(|| Error::Media("export has no video".into()))?;
    if video.frames != frames
        || video.frame_rate != project.frame_rate
        || video.width != project.width
        || video.height != project.height
        || metadata.audio.is_none()
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
        frames,
        frame_rate: project.frame_rate.to_string(),
        width: project.width,
        height: project.height,
        preset: "baseline-mpeg4-aac",
        verified_full_decode: true,
    })
}
