//! Checked, headless picture scheduling shared by preview and final rendering.
use crate::{
    Error, Project, Result,
    media::{content_hash, local_file},
    model::{MAX_FRAMES, VisualSource},
    time::FrameRate,
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

/// Versioned reference semantics: encoded SDR straight RGBA, ordinary source-over.
/// This deliberately makes no linear-light or comprehensive colour-management claim.
pub const REFERENCE_POLICY: &str = "ffmpeg-encoded-sdr-rgba-v1";
pub const MAX_COMPOSITION_INPUTS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PictureSource {
    Video { source_in_frame: u64 },
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PictureRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct Picture {
    pub path: PathBuf,
    pub digest: String,
    pub source: PictureSource,
    pub at_frame: u64,
    pub duration_frames: u64,
    pub layer: usize,
    pub rect: PictureRect,
}

#[derive(Debug, Clone)]
pub struct VisualPlan {
    pub pictures: Vec<Picture>,
    pub rate: FrameRate,
    pub total_frames: u64,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
}

impl VisualPlan {
    /// Separate pure admission from local-source verification so headless scheduling
    /// and rational boundary behavior can be tested without opening media.
    pub fn new(
        rate: FrameRate,
        revision: u64,
        width: u32,
        height: u32,
        mut pictures: Vec<Picture>,
    ) -> Result<Self> {
        if pictures.is_empty()
            || pictures.len() > MAX_COMPOSITION_INPUTS
            || !(2..=8192).contains(&width)
            || !(2..=8192).contains(&height)
        {
            return Err(Error::Unsupported(
                "composition requires 1..3 picture inputs and a 2..8192 project canvas".into(),
            ));
        }
        let mut boundaries = Vec::new();
        for picture in &pictures {
            let end = picture
                .at_frame
                .checked_add(picture.duration_frames)
                .ok_or_else(|| Error::Invalid("picture endpoint overflow".into()))?;
            if picture.duration_frames == 0
                || end > MAX_FRAMES
                || picture.rect.width == 0
                || picture.rect.height == 0
                || picture.rect.width > 8192
                || picture.rect.height > 8192
                || !(-8192..=8192).contains(&picture.rect.x)
                || !(-8192..=8192).contains(&picture.rect.y)
            {
                return Err(Error::Invalid("invalid picture span or rectangle".into()));
            }
            if let PictureSource::Video { source_in_frame } = picture.source {
                source_in_frame
                    .checked_add(picture.duration_frames)
                    .filter(|end| *end <= MAX_FRAMES)
                    .ok_or_else(|| Error::Invalid("picture source endpoint overflow".into()))?;
            }
            boundaries.extend([picture.at_frame, end]);
        }
        for frame in &boundaries {
            let active = pictures
                .iter()
                .filter(|picture| {
                    picture.at_frame <= *frame
                        && *frame < picture.at_frame + picture.duration_frames
                })
                .collect::<Vec<_>>();
            if active
                .iter()
                .filter(|picture| matches!(picture.source, PictureSource::Video { .. }))
                .count()
                > 2
                || active
                    .iter()
                    .filter(|picture| picture.source == PictureSource::Image)
                    .count()
                    > 1
            {
                return Err(Error::Unsupported(
                    "composition admits at most two moving pictures and one PNG at a time".into(),
                ));
            }
            let mut layers = HashSet::new();
            if active.iter().any(|picture| !layers.insert(picture.layer)) {
                return Err(Error::Invalid(
                    "pictures overlap on one visual track".into(),
                ));
            }
        }
        pictures.sort_by_key(|picture| (picture.layer, picture.at_frame));
        let total_frames = boundaries
            .into_iter()
            .max()
            .ok_or_else(|| Error::Invalid("composition has no duration".into()))?;
        Ok(Self {
            pictures,
            rate,
            total_frames,
            revision,
            width,
            height,
        })
    }

    pub async fn from_project(project: &Project, database: &Path) -> Result<Self> {
        project.validate()?;
        let parent = database
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut pictures = Vec::new();
        let mut verified = HashSet::new();
        let full = PictureRect {
            x: 0,
            y: 0,
            width: project.width,
            height: project.height,
        };
        let mut placements = project
            .clips
            .iter()
            .map(|clip| {
                (
                    clip.media_id,
                    PictureSource::Video {
                        source_in_frame: clip.source_in_frame,
                    },
                    clip.at_frame,
                    clip.duration_frames,
                    0,
                    full,
                )
            })
            .collect::<Vec<_>>();
        for clip in &project.visual_clips {
            let layer = project
                .visual_tracks
                .iter()
                .position(|track| track.id == clip.track_id)
                .ok_or_else(|| Error::NotFound("visual track".into()))?
                + 1;
            let source = match clip.source {
                VisualSource::Video { source_in_frame } => PictureSource::Video { source_in_frame },
                VisualSource::Image {} => PictureSource::Image,
            };
            let rect = clip.placement.as_ref().map_or(full, |rect| PictureRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            });
            placements.push((
                clip.media_id,
                source,
                clip.at_frame,
                clip.duration_frames,
                layer,
                rect,
            ));
        }
        if placements.len() > MAX_COMPOSITION_INPUTS {
            return Err(Error::Unsupported("composition requires at most three total picture placements; sequential contiguous projects retain their established fast path".into()));
        }
        for (media_id, source, at_frame, duration_frames, layer, rect) in placements {
            let media = project
                .media
                .iter()
                .find(|media| media.id == media_id)
                .ok_or_else(|| Error::NotFound("picture media".into()))?;
            let video = media.metadata.video.as_ref().ok_or_else(|| {
                Error::Unsupported("picture source requires a video or PNG stream".into())
            })?;
            if video.width > 3840
                || video.height > 3840
                || video.sample_aspect_ratio != "1:1"
                || video.start_time_micros != 0
                || video
                    .color_transfer
                    .as_deref()
                    .is_some_and(|value| matches!(value, "smpte2084" | "arib-std-b67"))
            {
                return Err(Error::Unsupported("picture composition requires zero-origin, square-pixel SDR sources no larger than UHD".into()));
            }
            match source {
                PictureSource::Image if video.codec != "png" => return Err(Error::Unsupported("image holds currently require PNG".into())),
                PictureSource::Video { source_in_frame } if video.codec == "png" || !video.constant_rate || video.frame_rate != project.frame_rate || source_in_frame.checked_add(duration_frames).is_none_or(|end| end > video.frames) => return Err(Error::Unsupported("moving picture requires matching verified CFR and an available source range".into())),
                _ => {}
            }
            let path = local_file(&parent.join(&media.path))?;
            let digest = media.metadata.blake3.as_ref().ok_or_else(|| {
                Error::Unsupported("picture source requires verified reimport".into())
            })?;
            if std::fs::metadata(&path)?.len() != media.metadata.size_bytes
                || (verified.insert((path.clone(), digest.clone()))
                    && content_hash(&path).await? != *digest)
            {
                return Err(Error::Invalid(
                    "picture source changed; reimport media".into(),
                ));
            }
            pictures.push(Picture {
                path,
                digest: digest.clone(),
                source,
                at_frame,
                duration_frames,
                layer,
                rect,
            });
        }
        Self::new(
            project.frame_rate,
            project.revision,
            project.width,
            project.height,
            pictures,
        )
    }

    /// Active pictures at an exact output sample, in stable bottom-to-top order.
    pub fn active_at(
        &self,
        output_frame: u64,
        output_rate: FrameRate,
    ) -> Result<Vec<(&Picture, Option<u64>)>> {
        Self::new(
            self.rate,
            self.revision,
            self.width,
            self.height,
            self.pictures.clone(),
        )?;
        let numerator = u128::from(output_frame)
            .checked_mul(u128::from(output_rate.denominator()))
            .and_then(|value| value.checked_mul(u128::from(self.rate.numerator())))
            .ok_or_else(|| Error::Invalid("picture time overflow".into()))?;
        let denominator = u128::from(output_rate.numerator()) * u128::from(self.rate.denominator());
        let mut active = Vec::new();
        for picture in &self.pictures {
            let end = picture
                .at_frame
                .checked_add(picture.duration_frames)
                .ok_or_else(|| Error::Invalid("picture endpoint overflow".into()))?;
            if numerator >= u128::from(picture.at_frame) * denominator
                && numerator < u128::from(end) * denominator
            {
                let source = match picture.source {
                    PictureSource::Image => None,
                    PictureSource::Video { source_in_frame } => Some(
                        source_in_frame
                            .checked_add(
                                u64::try_from(numerator / denominator)
                                    .map_err(|_| Error::Invalid("picture time overflow".into()))?
                                    - picture.at_frame,
                            )
                            .ok_or_else(|| Error::Invalid("picture time overflow".into()))?,
                    ),
                };
                active.push((picture, source));
            }
        }
        active.sort_by_key(|(picture, _)| (picture.layer, picture.at_frame));
        Ok(active)
    }

    pub async fn verify_sources(&self) -> Result<()> {
        for picture in &self.pictures {
            if content_hash(&picture.path).await? != picture.digest {
                return Err(Error::Invalid(
                    "picture source changed during rendering".into(),
                ));
            }
        }
        Ok(())
    }
}

/// Keep the mature sequential path whenever no composition semantics are required.
pub fn requires_composition(project: &Project) -> bool {
    if !project.visual_clips.is_empty() {
        return true;
    }
    let mut clips = project.clips.iter().collect::<Vec<_>>();
    clips.sort_by_key(|clip| clip.at_frame);
    let mut end = 0;
    clips.into_iter().any(|clip| {
        let gap = clip.at_frame != end;
        end = clip.at_frame.saturating_add(clip.duration_frames);
        gap
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn picture(
        source: PictureSource,
        layer: usize,
        at_frame: u64,
        duration_frames: u64,
    ) -> Picture {
        Picture {
            path: "fixture".into(),
            digest: "proof".into(),
            source,
            at_frame,
            duration_frames,
            layer,
            rect: PictureRect {
                x: 0,
                y: 0,
                width: 128,
                height: 72,
            },
        }
    }
    #[test]
    fn rational_half_open_boundaries_and_order() -> Result<()> {
        let rate = FrameRate::new(30_000, 1001)?;
        let plan = VisualPlan::new(
            rate,
            4,
            128,
            72,
            vec![
                picture(PictureSource::Image, 2, 1, 2),
                picture(PictureSource::Video { source_in_frame: 9 }, 0, 0, 2),
            ],
        )?;
        let twice = FrameRate::new(60_000, 1001)?;
        assert_eq!(plan.active_at(1, twice)?.len(), 1);
        let active = plan.active_at(2, twice)?;
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].1, Some(10));
        assert_eq!(active[1].1, None);
        assert_eq!(plan.active_at(4, twice)?.len(), 1);
        assert!(plan.active_at(6, twice)?.is_empty());
        let mut mutated = plan.clone();
        mutated.pictures.reverse();
        let ordered = mutated.active_at(2, twice)?;
        assert_eq!(ordered[0].0.layer, 0);
        assert_eq!(ordered[1].0.layer, 2);
        Ok(())
    }
    #[test]
    fn rejects_overflow_overlap_and_unbounded_input_count() -> Result<()> {
        let rate = FrameRate::new(30, 1)?;
        assert!(
            VisualPlan::new(
                rate,
                0,
                128,
                72,
                vec![picture(PictureSource::Image, 0, u64::MAX, 1)]
            )
            .is_err()
        );
        assert!(
            VisualPlan::new(
                rate,
                0,
                128,
                72,
                vec![
                    picture(PictureSource::Image, 0, 0, 2),
                    picture(PictureSource::Video { source_in_frame: 0 }, 0, 1, 2)
                ]
            )
            .is_err()
        );
        assert!(
            VisualPlan::new(
                rate,
                0,
                128,
                72,
                (0..4)
                    .map(|layer| picture(PictureSource::Video { source_in_frame: 0 }, layer, 0, 2))
                    .collect()
            )
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn mutated_public_plan_rejects_overflow_without_panicking() -> Result<()> {
        let rate = FrameRate::new(30, 1)?;
        let mut plan = VisualPlan::new(
            rate,
            0,
            128,
            72,
            vec![picture(PictureSource::Image, 0, 0, 1)],
        )?;
        plan.pictures[0].at_frame = u64::MAX;
        assert!(matches!(plan.active_at(0, rate), Err(Error::Invalid(_))));
        Ok(())
    }
}
