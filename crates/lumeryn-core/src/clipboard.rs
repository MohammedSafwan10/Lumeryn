//! Versioned same-project clip interchange shared by native Studio and the CLI.
//! It carries imported source identity and exact ranges, never paths or executable text.
use crate::{Edit, Error, Project, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const VERSION: u32 = 3;
pub const MAX_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Clipboard {
    pub version: u32,
    pub project_id: Uuid,
    pub media_id: Uuid,
    pub source_blake3: String,
    pub frame_rate: crate::time::FrameRate,
    pub source_in_frame: u64,
    pub duration_frames: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visual_settings: Option<crate::model::VisualSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<crate::animation::Animation>,
}

impl Clipboard {
    pub fn copy(project: &Project, clip_id: Uuid) -> Result<Self> {
        project.validate()?;
        if project.time_effects.contains_key(&clip_id) {
            return Err(crate::Error::Unsupported("clipboard of a retimed clip requires an interchange update; reset its time effect first".into()));
        }
        let clip = project
            .clips
            .iter()
            .find(|clip| clip.id == clip_id)
            .ok_or_else(|| Error::NotFound(format!("clip {clip_id}")))?;
        let source = project
            .media
            .iter()
            .find(|media| media.id == clip.media_id)
            .ok_or_else(|| Error::NotFound(format!("media {}", clip.media_id)))?;
        let digest = source.metadata.blake3.as_ref().ok_or_else(|| {
            Error::Unsupported(
                "clipboard requires a verified imported source digest; reimport legacy media"
                    .into(),
            )
        })?;
        validate_digest(digest)?;
        let clipboard = Self {
            version: VERSION,
            project_id: project.id,
            media_id: clip.media_id,
            source_blake3: digest.clone(),
            frame_rate: project.frame_rate,
            source_in_frame: clip.source_in_frame,
            duration_frames: clip.duration_frames,
            visual_settings: project.visual_settings.get(&clip_id).cloned(),
            animation: project.animations.get(&clip_id).cloned(),
        };
        if serde_json::to_vec(&clipboard)?.len() > MAX_BYTES {
            return Err(Error::Invalid(
                "animated clipboard exceeds 16 KiB; simplify keyframes".into(),
            ));
        }
        Ok(clipboard)
    }

    /// Strictly parse a bounded payload object, rather than a CLI response envelope.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Invalid("clipboard exceeds 16 KiB".into()));
        }
        let clipboard: Self = serde_json::from_slice(bytes)?;
        clipboard.validate_format()?;
        Ok(clipboard)
    }

    fn validate_format(&self) -> Result<()> {
        if serde_json::to_vec(self)?.len() > MAX_BYTES {
            return Err(Error::Invalid(
                "clipboard exceeds 16 KiB; simplify animation keys".into(),
            ));
        }
        if !(1..=VERSION).contains(&self.version) {
            return Err(Error::Unsupported(format!(
                "clipboard version {}",
                self.version
            )));
        }
        if let Some(animation) = &self.animation {
            if self.version < 3 || animation.channels.is_empty() {
                return Err(Error::Invalid(
                    "nonempty clipboard animation requires version 3".into(),
                ));
            }
            animation.validate(self.duration_frames)?;
        }
        if self.version == 1 && self.visual_settings.is_some() {
            return Err(Error::Invalid(
                "clipboard visual properties require version 2".into(),
            ));
        }
        if let Some(settings) = &self.visual_settings {
            if settings.is_identity()
                || settings
                    .effects
                    .is_some_and(|effects| effects.is_identity())
            {
                return Err(Error::Invalid(
                    "clipboard identity visual settings must be omitted".into(),
                ));
            }
            if let Some(effects) = settings.effects {
                effects.validate(self.duration_frames)?;
            }
        }
        validate_digest(&self.source_blake3)?;
        if self.duration_frames == 0 {
            return Err(Error::Invalid("clipboard duration must be positive".into()));
        }
        Ok(())
    }
}

pub fn paste_edit(
    project: &Project,
    clipboard: &Clipboard,
    id: Uuid,
    at_frame: u64,
) -> Result<Edit> {
    project.validate()?;
    clipboard.validate_format()?;
    if project.id != clipboard.project_id {
        return Err(Error::Unsupported(
            "clipboard paste requires its original project; cross-project asset transfer is staged"
                .into(),
        ));
    }
    if project.frame_rate != clipboard.frame_rate {
        return Err(Error::Unsupported(
            "clipboard/project frame rates differ".into(),
        ));
    }
    let source = project
        .media
        .iter()
        .find(|media| media.id == clipboard.media_id)
        .ok_or_else(|| {
            Error::NotFound(format!(
                "clipboard media {} is not imported",
                clipboard.media_id
            ))
        })?;
    if source.metadata.blake3.as_ref() != Some(&clipboard.source_blake3) {
        return Err(Error::Invalid(
            "clipboard source identity differs from the currently imported source".into(),
        ));
    }
    let edit = Edit::AddClip {
        id,
        media_id: clipboard.media_id,
        at_frame,
        source_in_frame: clipboard.source_in_frame,
        duration_frames: clipboard.duration_frames,
    };
    // Reuse authoritative graph validation for ID collisions, source/timeline bounds,
    // track/rate compatibility and object limits; caller's project remains unchanged.
    let mut candidate = project.clone();
    candidate.apply(&edit, None)?;
    if let Some(settings) = &clipboard.visual_settings {
        candidate.visual_settings.insert(id, settings.clone());
        candidate.validate()?;
    }
    if let Some(animation) = &clipboard.animation {
        candidate.animations.insert(id, animation.clone());
        candidate.validate()?;
    }
    Ok(edit)
}

fn validate_digest(digest: &str) -> Result<()> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(Error::Invalid(
            "clipboard requires a 64-character lowercase BLAKE3 digest".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        media::{Metadata, Video},
        model::{Clip, Media},
        time::FrameRate,
    };
    fn project() -> Result<Project> {
        let mut project = Project::new(
            "Clipboard test".into(),
            640,
            360,
            FrameRate::new(30_000, 1001)?,
        )?;
        let media_id = Uuid::new_v4();
        project.media.push(Media {
            id: media_id,
            path: "source.mp4".into(),
            alias: None,
            metadata: Metadata {
                size_bytes: 1,
                blake3: Some("a".repeat(64)),
                audio: None,
                video: Some(Video {
                    codec: "mpeg4".into(),
                    width: 640,
                    height: 360,
                    frame_rate: project.frame_rate,
                    frames: 120,
                    constant_rate: true,
                    start_time_micros: 0,
                    sample_aspect_ratio: "1:1".into(),
                    color_transfer: None,
                }),
            },
        });
        project.clips.push(Clip {
            id: Uuid::new_v4(),
            media_id,
            track_id: project.video_track_id,
            at_frame: 0,
            source_in_frame: 17,
            duration_frames: 31,
        });
        Ok(project)
    }
    #[test]
    fn exact_copy_round_trip_and_new_identity_reuse_authoritative_validation() -> Result<()> {
        let project = project()?;
        let clipboard = Clipboard::copy(&project, project.clips[0].id)?;
        assert_eq!(
            Clipboard::from_slice(&serde_json::to_vec(&clipboard)?)?,
            clipboard
        );
        let id = Uuid::new_v4();
        let edit = paste_edit(&project, &clipboard, id, 31)?;
        assert!(
            matches!(edit, Edit::AddClip { id: actual, source_in_frame: 17, duration_frames: 31, at_frame: 31, .. } if actual == id)
        );
        assert_eq!(project.clips.len(), 1);
        assert_eq!(project.revision, 0);
        assert!(paste_edit(&project, &clipboard, project.clips[0].id, 31).is_err());
        assert!(paste_edit(&project, &clipboard, Uuid::new_v4(), u64::MAX).is_err());
        Ok(())
    }
    #[test]
    fn wrong_project_source_version_rate_range_and_payload_are_rejected() -> Result<()> {
        let mut project = project()?;
        let clipboard = Clipboard::copy(&project, project.clips[0].id)?;
        for changed in [
            Clipboard {
                project_id: Uuid::new_v4(),
                ..clipboard.clone()
            },
            Clipboard {
                source_blake3: "b".repeat(64),
                ..clipboard.clone()
            },
            Clipboard {
                version: 3,
                ..clipboard.clone()
            },
            Clipboard {
                frame_rate: FrameRate::new(30, 1)?,
                ..clipboard.clone()
            },
            Clipboard {
                source_in_frame: 110,
                ..clipboard.clone()
            },
            Clipboard {
                duration_frames: 0,
                ..clipboard.clone()
            },
        ] {
            assert!(paste_edit(&project, &changed, Uuid::new_v4(), 31).is_err());
        }
        let mut unknown = serde_json::to_value(&clipboard)?;
        unknown["path"] = "remote://source".into();
        assert!(Clipboard::from_slice(&serde_json::to_vec(&unknown)?).is_err());
        assert!(Clipboard::from_slice(&vec![b' '; MAX_BYTES + 1]).is_err());
        project.media[0].metadata.blake3 = None;
        assert!(Clipboard::copy(&project, project.clips[0].id).is_err());
        assert!(paste_edit(&project, &clipboard, Uuid::new_v4(), 31).is_err());
        Ok(())
    }
}
