use crate::{Error, Result, media::Metadata, time::FrameRate};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf};
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_FRAMES: u64 = 1_000_000_000;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub video_track_id: Uuid,
    pub media: Vec<Media>,
    pub clips: Vec<Clip>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Media {
    pub id: Uuid,
    /// Relative to the database's parent when the asset is inside it.
    pub path: PathBuf,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    pub id: Uuid,
    pub media_id: Uuid,
    pub track_id: Uuid,
    pub at_frame: u64,
    pub source_in_frame: u64,
    pub duration_frames: u64,
}

/// Commands contain no executable text. IDs may be supplied for batch references.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Import {
        id: Uuid,
        path: PathBuf,
    },
    AddClip {
        id: Uuid,
        media_id: Uuid,
        at_frame: u64,
        source_in_frame: u64,
        duration_frames: u64,
    },
    TrimClip {
        clip_id: Uuid,
        source_in_frame: u64,
        duration_frames: u64,
    },
    /// Split at an absolute interior timeline frame; the original ID remains on the left.
    SplitClip {
        clip_id: Uuid,
        right_clip_id: Uuid,
        at_frame: u64,
    },
    /// Insert a copy immediately after its source and ripple the contiguous sequence.
    DuplicateClip {
        clip_id: Uuid,
        id: Uuid,
    },
    DeleteClip {
        clip_id: Uuid,
        #[serde(default = "default_ripple")]
        ripple: bool,
    },
    /// Move within a contiguous sequence. None appends; all clip source ranges are retained.
    ReorderClip {
        clip_id: Uuid,
        before_clip_id: Option<Uuid>,
    },
    SetCanvas {
        width: u32,
        height: u32,
    },
}

fn default_ripple() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    pub operations: Vec<Edit>,
}

impl Project {
    pub fn new(name: String, width: u32, height: u32, frame_rate: FrameRate) -> Result<Self> {
        let project = Self {
            schema_version: SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name,
            revision: 0,
            width,
            height,
            frame_rate,
            video_track_id: Uuid::new_v4(),
            media: Vec::new(),
            clips: Vec::new(),
        };
        project.validate()?;
        Ok(project)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(Error::Unsupported(format!(
                "project schema {}",
                self.schema_version
            )));
        }
        if self.name.trim().is_empty()
            || self.name.len() > 256
            || self.width < 2
            || self.height < 2
            || self.width > 8192
            || self.height > 8192
            || !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
            || self.media.len() > 10_000
            || self.clips.len() > 10_000
            || self.revision > i64::MAX as u64
        {
            return Err(Error::Invalid(
                "invalid project name, even dimensions, revision or object count".into(),
            ));
        }
        let mut ids = HashSet::from([self.id, self.video_track_id]);
        if self.id == self.video_track_id
            || self.media.iter().any(|m| !ids.insert(m.id))
            || self.clips.iter().any(|c| !ids.insert(c.id))
        {
            return Err(Error::Invalid("object IDs must be unique".into()));
        }
        for media in &self.media {
            if media.metadata.blake3.as_ref().is_some_and(|hash| {
                hash.len() != 64
                    || !hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            }) {
                return Err(Error::Invalid("invalid BLAKE3 digest".into()));
            }
            if media.path.as_os_str().is_empty()
                || (!media.path.is_absolute()
                    && media
                        .path
                        .components()
                        .any(|c| matches!(c, std::path::Component::ParentDir)))
            {
                return Err(Error::Invalid(
                    "invalid media reference or relative path traversal".into(),
                ));
            }
            if media.metadata.video.as_ref().is_some_and(|v| {
                v.width == 0
                    || v.height == 0
                    || v.width > 8192
                    || v.height > 8192
                    || v.frames == 0
                    || v.frames > MAX_FRAMES
            }) || media.metadata.audio.as_ref().is_some_and(|a| {
                a.sample_rate == 0 || a.sample_rate > 384_000 || a.channels == 0 || a.channels > 64
            }) || (media.metadata.video.is_none() && media.metadata.audio.is_none())
            {
                return Err(Error::Invalid("invalid media metadata".into()));
            }
        }
        for clip in &self.clips {
            self.validate_clip(clip)?;
        }
        Ok(())
    }

    fn validate_clip(&self, clip: &Clip) -> Result<()> {
        let media = self
            .media
            .iter()
            .find(|m| m.id == clip.media_id)
            .ok_or_else(|| Error::NotFound(format!("media {}", clip.media_id)))?;
        let video = media
            .metadata
            .video
            .as_ref()
            .ok_or_else(|| Error::Unsupported("audio-only timeline placement".into()))?;
        if video.frame_rate != self.frame_rate || !video.constant_rate {
            return Err(Error::Unsupported(
                "clip placement requires matching constant frame rate".into(),
            ));
        }
        let end = clip
            .source_in_frame
            .checked_add(clip.duration_frames)
            .ok_or_else(|| Error::Invalid("source range overflow".into()))?;
        let timeline_end = clip
            .at_frame
            .checked_add(clip.duration_frames)
            .ok_or_else(|| Error::Invalid("timeline overflow".into()))?;
        if clip.duration_frames == 0
            || timeline_end > MAX_FRAMES
            || end > video.frames
            || clip.track_id != self.video_track_id
        {
            return Err(Error::Invalid(
                "clip range exceeds source/timeline or track is invalid".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn apply(&mut self, operation: &Edit, imported: Option<Media>) -> Result<()> {
        match operation {
            Edit::Import { .. } => self
                .media
                .push(imported.ok_or_else(|| Error::Invalid("import must be prepared".into()))?),
            Edit::AddClip {
                id,
                media_id,
                at_frame,
                source_in_frame,
                duration_frames,
            } => {
                self.clips.push(Clip {
                    id: *id,
                    media_id: *media_id,
                    track_id: self.video_track_id,
                    at_frame: *at_frame,
                    source_in_frame: *source_in_frame,
                    duration_frames: *duration_frames,
                });
            }
            Edit::TrimClip {
                clip_id,
                source_in_frame,
                duration_frames,
            } => {
                let clip = self
                    .clips
                    .iter_mut()
                    .find(|c| c.id == *clip_id)
                    .ok_or_else(|| Error::NotFound(format!("clip {clip_id}")))?;
                clip.source_in_frame = *source_in_frame;
                clip.duration_frames = *duration_frames;
            }
            Edit::SplitClip {
                clip_id,
                right_clip_id,
                at_frame,
            } => {
                let index = self.clip_index(*clip_id)?;
                let original = &self.clips[index];
                let offset = at_frame
                    .checked_sub(original.at_frame)
                    .filter(|offset| *offset > 0 && *offset < original.duration_frames)
                    .ok_or_else(|| {
                        Error::Invalid(
                            "split must be strictly inside the clip's timeline range".into(),
                        )
                    })?;
                let right = Clip {
                    id: *right_clip_id,
                    media_id: original.media_id,
                    track_id: original.track_id,
                    at_frame: *at_frame,
                    source_in_frame: original
                        .source_in_frame
                        .checked_add(offset)
                        .ok_or_else(|| Error::Invalid("split source overflow".into()))?,
                    duration_frames: original.duration_frames - offset,
                };
                self.clips[index].duration_frames = offset;
                self.clips.insert(index + 1, right);
            }
            Edit::DuplicateClip { clip_id, id } => {
                self.require_contiguous()?;
                let index = self.clip_index(*clip_id)?;
                let mut duplicate = self.clips[index].clone();
                duplicate.id = *id;
                self.clips.insert(index + 1, duplicate);
                self.reposition_contiguous()?;
            }
            Edit::DeleteClip { clip_id, ripple } => {
                if *ripple {
                    self.require_contiguous()?;
                }
                let index = self.clip_index(*clip_id)?;
                self.clips.remove(index);
                if *ripple {
                    self.reposition_contiguous()?;
                }
            }
            Edit::ReorderClip {
                clip_id,
                before_clip_id,
            } => {
                self.require_contiguous()?;
                let index = self.clip_index(*clip_id)?;
                if let Some(before) = before_clip_id {
                    self.clip_index(*before)?;
                    if *before == *clip_id {
                        return Err(Error::Invalid("cannot reorder a clip before itself".into()));
                    }
                }
                let clip = self.clips.remove(index);
                let target = match before_clip_id {
                    Some(before) => self.clip_index(*before)?,
                    None => self.clips.len(),
                };
                self.clips.insert(target, clip);
                self.reposition_contiguous()?;
            }
            Edit::SetCanvas { width, height } => {
                self.width = *width;
                self.height = *height;
            }
        }
        self.validate()
    }

    fn clip_index(&self, id: Uuid) -> Result<usize> {
        self.clips
            .iter()
            .position(|clip| clip.id == id)
            .ok_or_else(|| Error::NotFound(format!("clip {id}")))
    }

    /// These operations never silently close gaps or resolve overlapping clips.
    fn require_contiguous(&mut self) -> Result<()> {
        self.clips.sort_by_key(|clip| clip.at_frame);
        let mut expected = 0_u64;
        for clip in &self.clips {
            if clip.at_frame != expected {
                return Err(Error::Unsupported(
                    "operation requires a contiguous sequence starting at frame zero".into(),
                ));
            }
            expected = expected
                .checked_add(clip.duration_frames)
                .filter(|end| *end <= MAX_FRAMES)
                .ok_or_else(|| Error::Invalid("timeline overflow".into()))?;
        }
        Ok(())
    }

    fn reposition_contiguous(&mut self) -> Result<()> {
        let mut at_frame = 0_u64;
        for clip in &mut self.clips {
            clip.at_frame = at_frame;
            at_frame = at_frame
                .checked_add(clip.duration_frames)
                .filter(|end| *end <= MAX_FRAMES)
                .ok_or_else(|| Error::Invalid("timeline overflow".into()))?;
        }
        Ok(())
    }
}
