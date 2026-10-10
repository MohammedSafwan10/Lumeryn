use crate::PreviewConfig;
use lumeryn_core::{
    Error, Project, Result,
    audio_plan::AudioPlan,
    media::{content_hash, local_file},
    time::FrameRate,
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub(crate) struct Segment {
    pub path: PathBuf,
    pub digest: String,
    pub at: u64,
    pub source_in: u64,
    pub frames: u64,
    pub audio: bool,
}

#[derive(Clone)]
pub(crate) struct Plan {
    pub segments: Vec<Segment>,
    pub rate: FrameRate,
    pub total: u64,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub audio_plan: Option<AudioPlan>,
}

impl Plan {
    pub async fn build(project: Project, database: &Path, config: PreviewConfig) -> Result<Self> {
        project.validate()?;
        let parent = database
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut clips = project.clips.iter().collect::<Vec<_>>();
        clips.sort_by_key(|c| c.at_frame);
        if clips.is_empty() || clips.len() > 64 {
            return Err(Error::Unsupported(
                "preview requires 1..64 contiguous video clips".into(),
            ));
        }
        let mut total = 0;
        let mut segments = Vec::with_capacity(clips.len());
        let mut verified = HashSet::new();
        for clip in clips {
            if clip.at_frame != total {
                return Err(Error::Unsupported(
                    "preview gaps/overlaps require a compositor".into(),
                ));
            }
            let media = project
                .media
                .iter()
                .find(|m| m.id == clip.media_id)
                .ok_or_else(|| Error::NotFound("preview clip media".into()))?;
            let video = media
                .metadata
                .video
                .as_ref()
                .ok_or_else(|| Error::Unsupported("video stream required".into()))?;
            if video.start_time_micros != 0
                || media
                    .metadata
                    .audio
                    .as_ref()
                    .is_some_and(|a| a.start_time_micros != 0)
            {
                return Err(Error::Unsupported(
                    "preview requires zero-origin video/audio, matching baseline export".into(),
                ));
            }
            if video.sample_aspect_ratio != "1:1"
                || video
                    .color_transfer
                    .as_deref()
                    .is_some_and(|t| t == "smpte2084" || t == "arib-std-b67")
            {
                return Err(Error::Unsupported(
                    "preview requires square-pixel SDR media, matching baseline export".into(),
                ));
            }
            let path = local_file(&parent.join(&media.path))?;
            let digest = media.metadata.blake3.as_ref().ok_or_else(|| {
                Error::Unsupported("legacy media requires verified reimport".into())
            })?;
            if std::fs::metadata(&path)?.len() != media.metadata.size_bytes
                || (verified.insert((path.clone(), digest.clone()))
                    && &content_hash(&path).await? != digest)
            {
                return Err(Error::Invalid(
                    "preview source changed; reimport media".into(),
                ));
            }
            segments.push(Segment {
                path,
                digest: digest.clone(),
                at: clip.at_frame,
                source_in: clip.source_in_frame,
                frames: clip.duration_frames,
                audio: media.metadata.audio.is_some(),
            });
            total += clip.duration_frames;
        }
        let (width, height) = preview_size(
            project.width,
            project.height,
            config.max_width,
            config.max_height,
        );
        let audio_plan = if project.audio_clips.is_empty() {
            None
        } else {
            Some(AudioPlan::from_project(&project, database).await?)
        };
        Ok(Self {
            segments,
            total,
            revision: project.revision,
            rate: project.frame_rate,
            width,
            height,
            audio_plan,
        })
    }
}

fn preview_size(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    if width <= max_width && height <= max_height {
        return (width, height);
    }
    let (w, h) =
        if u64::from(width) * u64::from(max_height) > u64::from(height) * u64::from(max_width) {
            (
                max_width,
                (u64::from(height) * u64::from(max_width) / u64::from(width)) as u32,
            )
        } else {
            (
                (u64::from(width) * u64::from(max_height) / u64::from(height)) as u32,
                max_height,
            )
        };
    (w.max(2) & !1, h.max(2) & !1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_sizes_are_bounded_even_and_aspect_preserving() {
        assert_eq!(preview_size(3840, 2160, 1280, 720), (1280, 720));
        assert_eq!(preview_size(1080, 1920, 1280, 720), (404, 720));
        assert_eq!(preview_size(320, 180, 1280, 720), (320, 180));
    }
}
