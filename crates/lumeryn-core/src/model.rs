use crate::{Error, Result, media::Metadata, time::FrameRate};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 5;
pub const MAX_SCENES: usize = 8;
pub const MAX_FRAMES: u64 = 1_000_000_000;
pub const MAX_VISUAL_TRACKS: usize = 2;
pub const MAX_COMPOSITION_INPUTS: usize = 3;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_track_id: Option<Uuid>,
    pub media: Vec<Media>,
    pub clips: Vec<Clip>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio_clips: Vec<AudioClip>,
    /// Bottom-to-top ordering above the existing primary video track.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub visual_tracks: Vec<VisualTrack>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub visual_clips: Vec<VisualClip>,
    /// Shared visual properties for primary and overlay clips; keys are stable clip IDs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub visual_settings: BTreeMap<Uuid, VisualSettings>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenes: Vec<SceneAsset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scene_placements: Vec<ScenePlacement>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub animations: BTreeMap<Uuid, crate::animation::Animation>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub time_effects: BTreeMap<Uuid, crate::time_effects::TimeEffect>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<TransitionEdge>,
}

/// Fixed existing cut; video-only transition with unchanged hard-cut linked audio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionEdge {
    pub id: Uuid,
    pub outgoing_clip_id: Uuid,
    pub incoming_clip_id: Uuid,
    pub kind: crate::time_effects::TransitionKind,
    pub duration_frames: u64,
}

/// Editable semantic scene; fonts are identities, never project-supplied paths.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneAsset {
    pub id: Uuid,
    pub name: String,
    pub document: crate::scene_text::SceneDocument,
}

/// Silent full-canvas hold on an ordered visual track; no fake media identifier.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenePlacement {
    pub id: Uuid,
    pub scene_id: Uuid,
    pub track_id: Uuid,
    pub at_frame: u64,
    pub duration_frames: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualSettings {
    pub transform: crate::visual_transform::VisualTransform,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects: Option<crate::visual_effects::VisualEffects>,
}

impl VisualSettings {
    pub fn is_identity(&self) -> bool {
        self.transform.is_identity() && self.effects.is_none_or(|effects| effects.is_identity())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualTrack {
    pub id: Uuid,
    pub name: String,
}

/// Source frames belong to moving video only; a still hold has no invented FPS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VisualSource {
    Video { source_in_frame: u64 },
    Image {},
}

/// A contain-fit rectangle in project canvas pixels, independent of preview resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualPlacement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualClip {
    pub id: Uuid,
    pub media_id: Uuid,
    pub track_id: Uuid,
    pub at_frame: u64,
    pub duration_frames: u64,
    pub source: VisualSource,
    /// None contains the source in the complete canvas.
    pub placement: Option<VisualPlacement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Media {
    pub id: Uuid,
    /// Relative to the database's parent when the asset is inside it.
    pub path: PathBuf,
    /// Project-local display text. None retains the legacy filename-derived label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub metadata: Metadata,
}

impl Media {
    /// A display label only; source resolution and cache identity always use path/metadata.
    pub fn display_name(&self) -> String {
        self.alias.clone().unwrap_or_else(|| {
            self.path.file_stem().map_or_else(
                || "Media".into(),
                |name| name.to_string_lossy().replace('_', " "),
            )
        })
    }
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

/// Independent audio uses canonical 48 kHz source samples and rational project frames.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioClip {
    pub id: Uuid,
    pub media_id: Uuid,
    pub track_id: Uuid,
    pub at_frame: u64,
    pub source_in_sample: u64,
    pub duration_frames: u64,
    pub gain_millidecibels: i32,
    pub muted: bool,
}

/// Commands contain no executable text. IDs may be supplied for batch references.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    CreateScene {
        id: Uuid,
        name: String,
        document: crate::scene_text::SceneDocument,
    },
    SetScene {
        scene_id: Uuid,
        document: crate::scene_text::SceneDocument,
    },
    AddScenePlacement {
        id: Uuid,
        scene_id: Uuid,
        track_id: Uuid,
        at_frame: u64,
        duration_frames: u64,
    },
    SetSceneRange {
        clip_id: Uuid,
        at_frame: u64,
        duration_frames: u64,
    },
    MoveScenePlacement {
        clip_id: Uuid,
        track_id: Uuid,
        at_frame: u64,
    },
    DeleteScenePlacement {
        clip_id: Uuid,
    },
    DeleteScene {
        scene_id: Uuid,
    },
    AddVisualTrack {
        id: Uuid,
        name: String,
    },
    ReorderVisualTrack {
        track_id: Uuid,
        before_track_id: Option<Uuid>,
    },
    DeleteVisualTrack {
        track_id: Uuid,
    },
    AddVisualClip {
        id: Uuid,
        media_id: Uuid,
        track_id: Uuid,
        at_frame: u64,
        duration_frames: u64,
        source: VisualSource,
        placement: Option<VisualPlacement>,
    },
    MoveVisualClip {
        clip_id: Uuid,
        track_id: Uuid,
        at_frame: u64,
    },
    SetVisualRange {
        clip_id: Uuid,
        at_frame: u64,
        duration_frames: u64,
        source: VisualSource,
    },
    SetVisualPlacement {
        clip_id: Uuid,
        #[serde(deserialize_with = "deserialize_required_placement")]
        #[schemars(required, schema_with = "nullable_placement_schema")]
        placement: Option<VisualPlacement>,
    },
    SetTransition {
        id: Uuid,
        outgoing_clip_id: Uuid,
        incoming_clip_id: Uuid,
        kind: crate::time_effects::TransitionKind,
        duration_frames: u64,
    },
    DeleteTransition {
        transition_id: Uuid,
    },
    SetTimeEffect {
        clip_id: Uuid,
        effect: crate::time_effects::TimeEffect,
    },
    SetVisualAnimation {
        clip_id: Uuid,
        animation: crate::animation::Animation,
    },
    SetVisualEffects {
        clip_id: Uuid,
        effects: crate::visual_effects::VisualEffects,
    },
    SetVisualTransform {
        clip_id: Uuid,
        transform: crate::visual_transform::VisualTransform,
    },
    DeleteVisualClip {
        clip_id: Uuid,
    },
    /// Free primary-track placement; this never ripples or closes gaps.
    MoveClip {
        clip_id: Uuid,
        at_frame: u64,
    },
    AddAudioClip {
        id: Uuid,
        media_id: Uuid,
        at_frame: u64,
        source_in_sample: u64,
        duration_frames: u64,
        gain_millidecibels: i32,
        muted: bool,
    },
    MoveAudioClip {
        clip_id: Uuid,
        at_frame: u64,
    },
    TrimAudioClip {
        clip_id: Uuid,
        source_in_sample: u64,
        duration_frames: u64,
    },
    /// Update position and source range together, validating only their final combination.
    SetAudioRange {
        clip_id: Uuid,
        at_frame: u64,
        source_in_sample: u64,
        duration_frames: u64,
    },
    SetAudioProperties {
        clip_id: Uuid,
        gain_millidecibels: i32,
        muted: bool,
    },
    DeleteAudioClip {
        clip_id: Uuid,
    },
    Import {
        id: Uuid,
        path: PathBuf,
    },
    /// Set project-local display text; explicit null restores the filename-derived label.
    RenameMedia {
        media_id: Uuid,
        #[serde(deserialize_with = "deserialize_required_alias")]
        #[schemars(required, schema_with = "nullable_alias_schema")]
        alias: Option<String>,
    },
    /// Remove only an unused project reference, without deleting source or cache files.
    RemoveMedia {
        media_id: Uuid,
    },
    AddClip {
        id: Uuid,
        media_id: Uuid,
        at_frame: u64,
        source_in_frame: u64,
        duration_frames: u64,
    },
    /// Insert before a clip, or append with None, and ripple the contiguous sequence.
    InsertClip {
        id: Uuid,
        media_id: Uuid,
        before_clip_id: Option<Uuid>,
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
    /// Choose an exact timeline rate before placing any video or audio clips.
    SetFrameRate {
        frame_rate: FrameRate,
    },
    PasteClip {
        id: Uuid,
        at_frame: u64,
        clipboard: crate::clipboard::Clipboard,
    },
    /// Restore the preceding logical snapshot through the authoritative Store history.
    Undo {},
    /// Restore the next logical snapshot through the authoritative Store history.
    Redo {},
}

// A custom deserializer without a default rejects a missing field while retaining null reset.
fn deserialize_required_alias<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

// Schemars `required` alone strips Option nullability; keep both presence and nullable value.
fn nullable_alias_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    <Option<String>>::json_schema(generator)
}

fn deserialize_required_placement<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<VisualPlacement>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<VisualPlacement>::deserialize(deserializer)
}

fn nullable_placement_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    <Option<VisualPlacement>>::json_schema(generator)
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
            audio_track_id: Some(Uuid::new_v4()),
            media: Vec::new(),
            clips: Vec::new(),
            audio_clips: Vec::new(),
            visual_tracks: Vec::new(),
            visual_clips: Vec::new(),
            visual_settings: BTreeMap::new(),
            scenes: Vec::new(),
            scene_placements: Vec::new(),
            animations: BTreeMap::new(),
            time_effects: BTreeMap::new(),
            transitions: Vec::new(),
        };
        project.validate()?;
        Ok(project)
    }

    pub fn validate(&self) -> Result<()> {
        if !(1..=SCHEMA_VERSION).contains(&self.schema_version) {
            return Err(Error::Unsupported(format!(
                "project schema {}",
                self.schema_version
            )));
        }
        if (self.schema_version == 1
            && (self.audio_track_id.is_some() || !self.audio_clips.is_empty()))
            || (self.schema_version >= 2 && self.audio_track_id.is_none())
        {
            return Err(Error::Invalid(
                "audio track does not match project schema".into(),
            ));
        }
        if self.schema_version < 3
            && (!self.visual_tracks.is_empty()
                || !self.visual_clips.is_empty()
                || !self.scene_placements.is_empty())
        {
            return Err(Error::Invalid(
                "visual composition requires schema 3".into(),
            ));
        }
        if self.schema_version < 4 && !self.visual_settings.is_empty() {
            return Err(Error::Invalid("visual transforms require schema 4".into()));
        }
        if self.schema_version < 5 && (!self.scenes.is_empty() || !self.scene_placements.is_empty())
        {
            return Err(Error::Invalid("editable scenes require schema 5".into()));
        }
        if self.schema_version < 5 && !self.animations.is_empty() {
            return Err(Error::Invalid("animation requires schema 5".into()));
        }
        if self.schema_version < 5 && !self.time_effects.is_empty() {
            return Err(Error::Invalid("time effects require schema5".into()));
        }
        self.validate_transitions()?;
        for (clip_id, effect) in &self.time_effects {
            if *effect == crate::time_effects::TimeEffect::Normal {
                return Err(Error::Invalid("normal time effect must be omitted".into()));
            }
            let media_id = self
                .clips
                .iter()
                .find(|clip| clip.id == *clip_id)
                .map(|clip| clip.media_id)
                .or_else(|| {
                    self.visual_clips
                        .iter()
                        .find(|clip| {
                            clip.id == *clip_id && matches!(clip.source, VisualSource::Video { .. })
                        })
                        .map(|clip| clip.media_id)
                })
                .ok_or_else(|| Error::NotFound(format!("time effect moving clip {clip_id}")))?;
            if self
                .media
                .iter()
                .find(|media| media.id == media_id)
                .and_then(|media| media.metadata.video.as_ref())
                .is_none_or(|video| video.codec == "png")
            {
                return Err(Error::Unsupported(
                    "time effects require moving video".into(),
                ));
            }
        }
        for (clip_id, animation) in &self.animations {
            validate_render_animation(animation)?;
            let duration = self.visual_duration(*clip_id)?;
            if animation.channels.is_empty() {
                return Err(Error::Invalid(
                    "empty animation must be omitted; reset through SetVisualAnimation".into(),
                ));
            }
            animation.validate(duration).map_err(|error| {
                Error::Invalid(format!(
                    "clip {clip_id}: {error}; shorten/reset keys before trimming"
                ))
            })?;
        }
        if self.scenes.len() > MAX_SCENES {
            return Err(Error::Unsupported(
                "at most eight editable scenes are supported".into(),
            ));
        }
        for scene in &self.scenes {
            if scene.name.trim().is_empty()
                || scene.name.len() > 256
                || scene.name.chars().any(char::is_control)
            {
                return Err(Error::Invalid(
                    "scene name must be nonblank, bounded and free of controls".into(),
                ));
            }
            scene
                .document
                .validate()
                .map_err(|error| Error::Invalid(error.to_string()))?;
            if scene.document.width != self.width || scene.document.height != self.height {
                return Err(Error::Invalid("scene canvas must match project canvas; remove scenes or recreate them for the new canvas".into()));
            }
        }
        crate::scene_text::validate_audited_documents(
            self.scenes.iter().map(|scene| &scene.document),
        )
        .map_err(|error| Error::Invalid(error.to_string()))?;
        for (clip_id, settings) in &self.visual_settings {
            let (media_id, duration_frames) = self
                .clips
                .iter()
                .find(|clip| clip.id == *clip_id)
                .map(|clip| (clip.media_id, clip.duration_frames))
                .or_else(|| {
                    self.visual_clips
                        .iter()
                        .find(|clip| clip.id == *clip_id)
                        .map(|clip| (clip.media_id, clip.duration_frames))
                })
                .ok_or_else(|| Error::NotFound(format!("visual settings clip {clip_id}")))?;
            let source = self
                .media
                .iter()
                .find(|media| media.id == media_id)
                .and_then(|media| media.metadata.video.as_ref())
                .ok_or_else(|| {
                    Error::Invalid("visual transform requires a verified visual source".into())
                })?;
            settings.transform.validate(source.width, source.height)?;
            if let Some(effects) = settings.effects {
                if self.schema_version < 5 || effects.is_identity() {
                    return Err(Error::Invalid("effects require schema5 and identity effects must be omitted; reset through SetVisualEffects".into()));
                }
                effects.validate(duration_frames).map_err(|error| {
                    Error::Invalid(format!(
                        "clip {clip_id}: {error}; shorten/reset fades before trimming or splitting"
                    ))
                })?;
            }
            if settings.is_identity() {
                return Err(Error::Invalid(
                    "identity visual settings must be omitted; reset through visual settings edits"
                        .into(),
                ));
            }
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
            || self.audio_clips.len() > crate::audio_plan::MAX_AUDIO_INPUTS
            || self.revision > i64::MAX as u64
        {
            return Err(Error::Invalid(
                "invalid project name, even dimensions, revision or object count".into(),
            ));
        }
        let mut ids = HashSet::from([self.id, self.video_track_id]);
        if self.id == self.video_track_id
            || self.audio_track_id.is_some_and(|id| !ids.insert(id))
            || self.media.iter().any(|m| !ids.insert(m.id))
            || self.clips.iter().any(|c| !ids.insert(c.id))
            || self.transitions.iter().any(|edge| !ids.insert(edge.id))
            || self.audio_clips.iter().any(|c| !ids.insert(c.id))
            || self.visual_tracks.iter().any(|track| !ids.insert(track.id))
            || self.visual_clips.iter().any(|clip| !ids.insert(clip.id))
            || self.scenes.iter().any(|scene| !ids.insert(scene.id))
            || self
                .scenes
                .iter()
                .flat_map(|scene| &scene.document.nodes)
                .any(|node| !ids.insert(node.id))
            || self
                .scene_placements
                .iter()
                .any(|clip| !ids.insert(clip.id))
            || (self.schema_version >= 3 && ids.contains(&Uuid::nil()))
        {
            return Err(Error::Invalid("object IDs must be unique".into()));
        }
        for media in &self.media {
            if media.alias.as_ref().is_some_and(|alias| {
                alias.trim().is_empty() || alias.len() > 256 || alias.chars().any(char::is_control)
            }) {
                return Err(Error::Invalid(
                    "media alias must be nonblank, at most 256 UTF-8 bytes and free of control characters"
                        .into(),
                ));
            }
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
                a.sample_rate == 0
                    || a.sample_rate > 384_000
                    || a.channels == 0
                    || a.channels > 64
                    || a.canonical_samples_48k
                        .is_some_and(|samples| samples == 0 || samples > i64::MAX as u64)
            }) || (media.metadata.video.is_none() && media.metadata.audio.is_none())
            {
                return Err(Error::Invalid("invalid media metadata".into()));
            }
        }
        for clip in &self.clips {
            self.validate_clip(clip)?;
        }
        for clip in &self.audio_clips {
            self.validate_audio_clip(clip)?;
        }
        if self.schema_version >= 3 {
            self.validate_visual_composition()?;
        }
        Ok(())
    }

    pub fn visual_duration_frames(&self) -> u64 {
        self.clips
            .iter()
            .map(|clip| clip.at_frame.saturating_add(clip.duration_frames))
            .chain(
                self.visual_clips
                    .iter()
                    .map(|clip| clip.at_frame.saturating_add(clip.duration_frames)),
            )
            .chain(
                self.scene_placements
                    .iter()
                    .map(|clip| clip.at_frame.saturating_add(clip.duration_frames)),
            )
            .max()
            .unwrap_or(0)
    }

    /// The bounded reference compositor is required for overlays or primary-track gaps.
    pub fn uses_composition(&self) -> bool {
        if !self.animations.is_empty()
            || !self.time_effects.is_empty()
            || !self.transitions.is_empty()
        {
            return true;
        }
        if !self.visual_clips.is_empty()
            || !self.visual_settings.is_empty()
            || !self.scene_placements.is_empty()
        {
            return true;
        }
        let mut clips = self.clips.iter().collect::<Vec<_>>();
        clips.sort_by_key(|clip| clip.at_frame);
        let mut expected = 0;
        for clip in clips {
            if clip.at_frame != expected {
                return true;
            }
            expected = clip.at_frame.saturating_add(clip.duration_frames);
        }
        false
    }

    fn validate_visual_composition(&self) -> Result<()> {
        if self.visual_tracks.len() > MAX_VISUAL_TRACKS {
            return Err(Error::Unsupported(
                "composition supports at most two additional visual tracks".into(),
            ));
        }
        for track in &self.visual_tracks {
            if track.name.trim().is_empty()
                || track.name.len() > 256
                || track.name.chars().any(char::is_control)
            {
                return Err(Error::Invalid("visual track name must be nonblank, at most 256 UTF-8 bytes and free of control characters".into()));
            }
        }
        let mut spans = self
            .clips
            .iter()
            .map(|clip| {
                (
                    clip.track_id,
                    clip.at_frame,
                    clip.at_frame + clip.duration_frames,
                    false,
                )
            })
            .collect::<Vec<_>>();
        for clip in &self.visual_clips {
            self.visual_track_index(clip.track_id)?;
            let end = clip
                .at_frame
                .checked_add(clip.duration_frames)
                .filter(|end| clip.duration_frames > 0 && *end <= MAX_FRAMES)
                .ok_or_else(|| Error::Invalid("invalid visual clip timeline range".into()))?;
            if clip.placement.as_ref().is_some_and(|placement| {
                !(-8192..=8192).contains(&placement.x)
                    || !(-8192..=8192).contains(&placement.y)
                    || !(1..=3840).contains(&placement.width)
                    || !(1..=3840).contains(&placement.height)
            }) {
                return Err(Error::Invalid("visual placement requires x/y within +/-8192 and width/height within 1..3840 canvas pixels".into()));
            }
            let media = self
                .media
                .iter()
                .find(|media| media.id == clip.media_id)
                .ok_or_else(|| Error::NotFound(format!("media {}", clip.media_id)))?;
            let video = media.metadata.video.as_ref().ok_or_else(|| {
                Error::Unsupported("visual placement requires verified video or PNG".into())
            })?;
            if media.metadata.blake3.is_none()
                || video.start_time_micros != 0
                || video.sample_aspect_ratio != "1:1"
            {
                return Err(Error::Unsupported("visual placement requires hashed zero-origin square-pixel source; reimport media".into()));
            }
            match &clip.source {
                VisualSource::Video { source_in_frame } => {
                    if video.codec == "png"
                        || !video.constant_rate
                        || video.frame_rate != self.frame_rate
                    {
                        return Err(Error::Unsupported("moving visual placement requires matching CFR video; use image holds for PNG".into()));
                    }
                    self.time_effects
                        .get(&clip.id)
                        .copied()
                        .unwrap_or_default()
                        .validate(*source_in_frame, clip.duration_frames, video.frames)?;
                }
                VisualSource::Image {} => {
                    if video.codec != "png" || video.frames != 1 || media.metadata.audio.is_some() {
                        return Err(Error::Unsupported(
                            "image hold requires a verified single-frame silent PNG".into(),
                        ));
                    }
                }
            }
            spans.push((
                clip.track_id,
                clip.at_frame,
                end,
                matches!(clip.source, VisualSource::Image {}),
            ));
        }
        for clip in &self.scene_placements {
            self.visual_track_index(clip.track_id)?;
            if !self.scenes.iter().any(|scene| scene.id == clip.scene_id) {
                return Err(Error::NotFound(format!("scene {}", clip.scene_id)));
            }
            let end = clip
                .at_frame
                .checked_add(clip.duration_frames)
                .filter(|end| clip.duration_frames > 0 && *end <= MAX_FRAMES)
                .ok_or_else(|| Error::Invalid("invalid scene placement range".into()))?;
            spans.push((clip.track_id, clip.at_frame, end, true));
        }
        spans.sort_by_key(|(track, start, _, _)| (*track, *start));
        if spans
            .windows(2)
            .any(|pair| pair[0].0 == pair[1].0 && pair[0].2 > pair[1].1)
        {
            return Err(Error::Invalid(
                "visual clips must not overlap on the same track; move to another track".into(),
            ));
        }
        if self.uses_composition() {
            if spans.len() > MAX_COMPOSITION_INPUTS {
                return Err(Error::Unsupported(
                    "initial composition supports at most three total primary/overlay placements"
                        .into(),
                ));
            }
            for media_id in self
                .clips
                .iter()
                .map(|clip| clip.media_id)
                .chain(self.visual_clips.iter().map(|clip| clip.media_id))
            {
                let media = self
                    .media
                    .iter()
                    .find(|media| media.id == media_id)
                    .ok_or_else(|| Error::NotFound(format!("media {media_id}")))?;
                let video = media
                    .metadata
                    .video
                    .as_ref()
                    .ok_or_else(|| Error::Invalid("missing visual video metadata".into()))?;
                if video.width > 3840 || video.height > 3840 || media.metadata.blake3.is_none() {
                    return Err(Error::Unsupported(
                        "composition requires hashed sources with each dimension at most 3840"
                            .into(),
                    ));
                }
                if video.start_time_micros != 0
                    || video.sample_aspect_ratio != "1:1"
                    || video
                        .color_transfer
                        .as_deref()
                        .is_some_and(|transfer| matches!(transfer, "smpte2084" | "arib-std-b67"))
                    || media
                        .metadata
                        .audio
                        .as_ref()
                        .is_some_and(|audio| audio.start_time_micros != 0)
                {
                    return Err(Error::Unsupported(
                        "composition requires zero-origin square-pixel SDR sources".into(),
                    ));
                }
            }
            // Half-open intervals: endpoints are processed before starts at the same frame.
            let mut events = spans
                .iter()
                .flat_map(|(_, start, end, image)| {
                    [(*start, 1_i32, *image), (*end, -1_i32, *image)]
                })
                .collect::<Vec<_>>();
            events.sort_by_key(|(frame, delta, _)| (*frame, *delta));
            let mut videos = 0;
            let mut images = 0;
            for (_, delta, image) in events {
                if image {
                    images += delta;
                } else {
                    videos += delta;
                }
                if videos > 2 || images > 1 {
                    return Err(Error::Unsupported("composition supports at most two moving sources and one PNG or scene hold simultaneously".into()));
                }
            }
        }
        Ok(())
    }

    fn validate_transitions(&self) -> Result<()> {
        if self.transitions.is_empty() {
            return Ok(());
        }
        if self.schema_version < 5 {
            return Err(Error::Invalid("transitions require schema5".into()));
        }
        if self.transitions.len() != 1
            || self.clips.len() != 2
            || !self.visual_clips.is_empty()
            || !self.scene_placements.is_empty()
            || !self.visual_settings.is_empty()
            || !self.animations.is_empty()
            || !self.time_effects.is_empty()
        {
            return Err(Error::Unsupported("fixed-cut transitions currently require exactly two adjacent primary video clips without overlays, transforms, effects, scenes, animation or time effects; remove the transition before combining those features".into()));
        }
        let edge = &self.transitions[0];
        let outgoing = self
            .clips
            .iter()
            .find(|clip| clip.id == edge.outgoing_clip_id)
            .ok_or_else(|| Error::NotFound("transition outgoing clip".into()))?;
        let incoming = self
            .clips
            .iter()
            .find(|clip| clip.id == edge.incoming_clip_id)
            .ok_or_else(|| Error::NotFound("transition incoming clip".into()))?;
        let cut = outgoing
            .at_frame
            .checked_add(outgoing.duration_frames)
            .ok_or_else(|| Error::Invalid("transition cut overflow".into()))?;
        if outgoing.id == incoming.id || outgoing.at_frame != 0 || cut != incoming.at_frame {
            return Err(Error::Invalid(
                "transition requires adjacent distinct primary clips beginning at frame0".into(),
            ));
        }
        let frames = |id| {
            let media = self
                .media
                .iter()
                .find(|media| media.id == id)
                .ok_or_else(|| Error::NotFound("transition media".into()))?;
            if media.metadata.blake3.is_none() {
                return Err(Error::Unsupported(
                    "transition requires verified source digest; reimport media".into(),
                ));
            }
            media
                .metadata
                .video
                .as_ref()
                .ok_or_else(|| Error::NotFound("transition video source".into()))
        };
        let a = frames(outgoing.media_id)?;
        let b = frames(incoming.media_id)?;
        if a.codec == "png"
            || b.codec == "png"
            || !a.constant_rate
            || !b.constant_rate
            || a.frame_rate != self.frame_rate
            || b.frame_rate != self.frame_rate
            || a.start_time_micros != 0
            || b.start_time_micros != 0
            || a.sample_aspect_ratio != "1:1"
            || b.sample_aspect_ratio != "1:1"
        {
            return Err(Error::Unsupported(
                "transitions require verified zero-origin square-pixel matching CFR moving video"
                    .into(),
            ));
        }
        let source_out = outgoing
            .source_in_frame
            .checked_add(outgoing.duration_frames)
            .ok_or_else(|| Error::Invalid("transition source endpoint overflow".into()))?;
        let h = crate::time_effects::FixedCutTransition {
            kind: edge.kind,
            duration_frames: edge.duration_frames,
        }
        .validate_sources(
            cut,
            source_out,
            a.frames,
            incoming.source_in_frame,
            b.frames,
        )?;
        if outgoing.duration_frames < h.before_cut || incoming.duration_frames < h.after_cut {
            return Err(Error::Invalid(
                "transition must fit inside both adjoining clip spans".into(),
            ));
        }
        Ok(())
    }

    fn validate_audio_clip(&self, clip: &AudioClip) -> Result<()> {
        let media = self
            .media
            .iter()
            .find(|media| media.id == clip.media_id)
            .ok_or_else(|| Error::NotFound(format!("media {}", clip.media_id)))?;
        let audio = media.metadata.audio.as_ref().ok_or_else(|| {
            Error::Unsupported("independent placement requires an audio source".into())
        })?;
        let samples = audio
            .canonical_samples_48k
            .filter(|samples| *samples > 0 && *samples <= i64::MAX as u64)
            .ok_or_else(|| {
                Error::Invalid(
                    "independent audio requires verified canonical 48 kHz sample count".into(),
                )
            })?;
        if media.metadata.blake3.is_none()
            || audio.start_time_micros != 0
            || !(1..=2).contains(&audio.channels)
        {
            return Err(Error::Unsupported(
                "independent audio requires hashed mono/stereo zero-origin source".into(),
            ));
        }
        let end = clip
            .at_frame
            .checked_add(clip.duration_frames)
            .ok_or_else(|| Error::Invalid("audio timeline overflow".into()))?;
        if clip.duration_frames == 0
            || end > self.visual_duration_frames()
            || end > MAX_FRAMES
            || Some(clip.track_id) != self.audio_track_id
            || !(-60_000..=12_000).contains(&clip.gain_millidecibels)
        {
            return Err(Error::Invalid(
                "invalid audio track/gain/range; shorten audio or extend visual timeline".into(),
            ));
        }
        let length = crate::audio_plan::sample_boundary(self.frame_rate, end)?
            - crate::audio_plan::sample_boundary(self.frame_rate, clip.at_frame)?;
        if length == 0
            || clip
                .source_in_sample
                .checked_add(length)
                .is_none_or(|end| end > samples)
        {
            return Err(Error::Invalid(
                "audio range exceeds verified canonical source samples".into(),
            ));
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
        self.time_effects
            .get(&clip.id)
            .copied()
            .unwrap_or_default()
            .validate(clip.source_in_frame, clip.duration_frames, video.frames)?;
        let timeline_end = clip
            .at_frame
            .checked_add(clip.duration_frames)
            .ok_or_else(|| Error::Invalid("timeline overflow".into()))?;
        if clip.duration_frames == 0
            || timeline_end > MAX_FRAMES
            || clip.track_id != self.video_track_id
        {
            return Err(Error::Invalid(
                "clip range exceeds source/timeline or track is invalid".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn apply(&mut self, operation: &Edit, imported: Option<Media>) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(Error::Unsupported(
                "run project migrate before editing legacy project schema".into(),
            ));
        }
        match operation {
            Edit::CreateScene { id, name, document } => self.scenes.push(SceneAsset {
                id: *id,
                name: name.clone(),
                document: document.clone(),
            }),
            Edit::SetScene { scene_id, document } => {
                let scene = self
                    .scenes
                    .iter_mut()
                    .find(|scene| scene.id == *scene_id)
                    .ok_or_else(|| Error::NotFound(format!("scene {scene_id}")))?;
                scene.document = document.clone();
            }
            Edit::AddScenePlacement {
                id,
                scene_id,
                track_id,
                at_frame,
                duration_frames,
            } => self.scene_placements.push(ScenePlacement {
                id: *id,
                scene_id: *scene_id,
                track_id: *track_id,
                at_frame: *at_frame,
                duration_frames: *duration_frames,
            }),
            Edit::SetSceneRange {
                clip_id,
                at_frame,
                duration_frames,
            } => {
                let clip = self.scene_placement_mut(*clip_id)?;
                clip.at_frame = *at_frame;
                clip.duration_frames = *duration_frames;
            }
            Edit::MoveScenePlacement {
                clip_id,
                track_id,
                at_frame,
            } => {
                let clip = self.scene_placement_mut(*clip_id)?;
                clip.track_id = *track_id;
                clip.at_frame = *at_frame;
            }
            Edit::DeleteScenePlacement { clip_id } => {
                let index = self
                    .scene_placements
                    .iter()
                    .position(|clip| clip.id == *clip_id)
                    .ok_or_else(|| Error::NotFound(format!("scene placement {clip_id}")))?;
                self.scene_placements.remove(index);
                self.animations.remove(clip_id);
            }
            Edit::DeleteScene { scene_id } => {
                if self
                    .scene_placements
                    .iter()
                    .any(|clip| clip.scene_id == *scene_id)
                {
                    return Err(Error::Invalid(
                        "remove scene placements before deleting their scene".into(),
                    ));
                }
                let index = self
                    .scenes
                    .iter()
                    .position(|scene| scene.id == *scene_id)
                    .ok_or_else(|| Error::NotFound(format!("scene {scene_id}")))?;
                self.scenes.remove(index);
            }
            Edit::SetTransition {
                id,
                outgoing_clip_id,
                incoming_clip_id,
                kind,
                duration_frames,
            } => {
                let edge = TransitionEdge {
                    id: *id,
                    outgoing_clip_id: *outgoing_clip_id,
                    incoming_clip_id: *incoming_clip_id,
                    kind: *kind,
                    duration_frames: *duration_frames,
                };
                if let Some(existing) = self.transitions.iter_mut().find(|edge| edge.id == *id) {
                    *existing = edge;
                } else {
                    self.transitions.push(edge);
                }
            }
            Edit::DeleteTransition { transition_id } => {
                let index = self
                    .transitions
                    .iter()
                    .position(|edge| edge.id == *transition_id)
                    .ok_or_else(|| Error::NotFound(format!("transition {transition_id}")))?;
                self.transitions.remove(index);
            }
            Edit::SetTimeEffect { clip_id, effect } => {
                if !self.clips.iter().any(|clip| clip.id == *clip_id)
                    && !self.visual_clips.iter().any(|clip| {
                        clip.id == *clip_id && matches!(clip.source, VisualSource::Video { .. })
                    })
                {
                    return Err(Error::NotFound(format!(
                        "time effect moving clip {clip_id}"
                    )));
                }
                if *effect == crate::time_effects::TimeEffect::Normal {
                    self.time_effects.remove(clip_id);
                } else {
                    self.time_effects.insert(*clip_id, *effect);
                }
            }
            Edit::SetVisualAnimation { clip_id, animation } => {
                validate_render_animation(animation)?;
                animation.validate(self.visual_duration(*clip_id)?)?;
                if animation.channels.is_empty() {
                    self.animations.remove(clip_id);
                } else {
                    self.animations.insert(*clip_id, animation.clone());
                }
            }
            Edit::SetVisualTransform { clip_id, transform } => {
                if !self.clips.iter().any(|clip| clip.id == *clip_id)
                    && !self.visual_clips.iter().any(|clip| clip.id == *clip_id)
                {
                    return Err(Error::NotFound(format!("visual clip {clip_id}")));
                }
                let settings = self.visual_settings.entry(*clip_id).or_default();
                settings.transform = *transform;
                if settings.is_identity() {
                    self.visual_settings.remove(clip_id);
                }
            }
            Edit::SetVisualEffects { clip_id, effects } => {
                if !self.clips.iter().any(|clip| clip.id == *clip_id)
                    && !self.visual_clips.iter().any(|clip| clip.id == *clip_id)
                {
                    return Err(Error::NotFound(format!(
                        "media visual clip {clip_id}; scene effects are unsupported"
                    )));
                }
                let settings = self.visual_settings.entry(*clip_id).or_default();
                settings.effects = (!effects.is_identity()).then_some(*effects);
                if settings.is_identity() {
                    self.visual_settings.remove(clip_id);
                }
            }
            Edit::AddVisualTrack { id, name } => {
                self.visual_tracks.push(VisualTrack {
                    id: *id,
                    name: name.clone(),
                });
            }
            Edit::ReorderVisualTrack {
                track_id,
                before_track_id,
            } => {
                let index = self.visual_track_index(*track_id)?;
                if let Some(before) = before_track_id {
                    self.visual_track_index(*before)?;
                    if before == track_id {
                        return Err(Error::Invalid(
                            "cannot reorder a visual track before itself".into(),
                        ));
                    }
                }
                let track = self.visual_tracks.remove(index);
                let target = match before_track_id {
                    Some(before) => self.visual_track_index(*before)?,
                    None => self.visual_tracks.len(),
                };
                self.visual_tracks.insert(target, track);
            }
            Edit::DeleteVisualTrack { track_id } => {
                let index = self.visual_track_index(*track_id)?;
                if self
                    .visual_clips
                    .iter()
                    .any(|clip| clip.track_id == *track_id)
                    || self
                        .scene_placements
                        .iter()
                        .any(|clip| clip.track_id == *track_id)
                {
                    return Err(Error::Invalid(
                        "remove or move visual clips before deleting their track".into(),
                    ));
                }
                self.visual_tracks.remove(index);
            }
            Edit::AddVisualClip {
                id,
                media_id,
                track_id,
                at_frame,
                duration_frames,
                source,
                placement,
            } => {
                self.visual_clips.push(VisualClip {
                    id: *id,
                    media_id: *media_id,
                    track_id: *track_id,
                    at_frame: *at_frame,
                    duration_frames: *duration_frames,
                    source: source.clone(),
                    placement: placement.clone(),
                });
            }
            Edit::MoveVisualClip {
                clip_id,
                track_id,
                at_frame,
            } => {
                let clip = self.visual_clip_mut(*clip_id)?;
                clip.track_id = *track_id;
                clip.at_frame = *at_frame;
            }
            Edit::SetVisualRange {
                clip_id,
                at_frame,
                duration_frames,
                source,
            } => {
                let clip = self.visual_clip_mut(*clip_id)?;
                clip.at_frame = *at_frame;
                clip.duration_frames = *duration_frames;
                clip.source = source.clone();
            }
            Edit::SetVisualPlacement { clip_id, placement } => {
                self.visual_clip_mut(*clip_id)?.placement = placement.clone();
            }
            Edit::DeleteVisualClip { clip_id } => {
                let index = self
                    .visual_clips
                    .iter()
                    .position(|clip| clip.id == *clip_id)
                    .ok_or_else(|| Error::NotFound(format!("visual clip {clip_id}")))?;
                self.visual_clips.remove(index);
                self.visual_settings.remove(clip_id);
                self.animations.remove(clip_id);
                self.time_effects.remove(clip_id);
            }
            Edit::MoveClip { clip_id, at_frame } => {
                let index = self.clip_index(*clip_id)?;
                self.clips[index].at_frame = *at_frame;
            }
            Edit::AddAudioClip {
                id,
                media_id,
                at_frame,
                source_in_sample,
                duration_frames,
                gain_millidecibels,
                muted,
            } => {
                self.audio_clips.push(AudioClip {
                    id: *id,
                    media_id: *media_id,
                    track_id: self
                        .audio_track_id
                        .ok_or_else(|| Error::Invalid("missing audio track".into()))?,
                    at_frame: *at_frame,
                    source_in_sample: *source_in_sample,
                    duration_frames: *duration_frames,
                    gain_millidecibels: *gain_millidecibels,
                    muted: *muted,
                });
            }
            Edit::MoveAudioClip { clip_id, at_frame } => {
                self.audio_clip_mut(*clip_id)?.at_frame = *at_frame
            }
            Edit::TrimAudioClip {
                clip_id,
                source_in_sample,
                duration_frames,
            } => {
                let clip = self.audio_clip_mut(*clip_id)?;
                clip.source_in_sample = *source_in_sample;
                clip.duration_frames = *duration_frames;
            }
            Edit::SetAudioProperties {
                clip_id,
                gain_millidecibels,
                muted,
            } => {
                let clip = self.audio_clip_mut(*clip_id)?;
                clip.gain_millidecibels = *gain_millidecibels;
                clip.muted = *muted;
            }
            Edit::SetAudioRange {
                clip_id,
                at_frame,
                source_in_sample,
                duration_frames,
            } => {
                let clip = self.audio_clip_mut(*clip_id)?;
                clip.at_frame = *at_frame;
                clip.source_in_sample = *source_in_sample;
                clip.duration_frames = *duration_frames;
            }
            Edit::DeleteAudioClip { clip_id } => {
                let index = self
                    .audio_clips
                    .iter()
                    .position(|clip| clip.id == *clip_id)
                    .ok_or_else(|| Error::NotFound(format!("audio clip {clip_id}")))?;
                self.audio_clips.remove(index);
            }
            Edit::Import { .. } => self
                .media
                .push(imported.ok_or_else(|| Error::Invalid("import must be prepared".into()))?),
            Edit::RenameMedia { media_id, alias } => {
                let media = self
                    .media
                    .iter_mut()
                    .find(|media| media.id == *media_id)
                    .ok_or_else(|| Error::NotFound(format!("media {media_id}")))?;
                media.alias = alias.clone();
            }
            Edit::RemoveMedia { media_id } => {
                let index = self
                    .media
                    .iter()
                    .position(|media| media.id == *media_id)
                    .ok_or_else(|| Error::NotFound(format!("media {media_id}")))?;
                let usage_count = self
                    .clips
                    .iter()
                    .filter(|clip| clip.media_id == *media_id)
                    .count();
                let usage_count = usage_count
                    + self
                        .audio_clips
                        .iter()
                        .filter(|clip| clip.media_id == *media_id)
                        .count();
                let usage_count = usage_count
                    + self
                        .visual_clips
                        .iter()
                        .filter(|clip| clip.media_id == *media_id)
                        .count();
                if usage_count > 0 {
                    return Err(Error::Invalid(format!(
                        "media {media_id} is referenced by {usage_count} clip(s); remove its clips first"
                    )));
                }
                self.media.remove(index);
            }
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
            Edit::InsertClip {
                id,
                media_id,
                before_clip_id,
                source_in_frame,
                duration_frames,
            } => {
                self.require_contiguous()?;
                let target = match before_clip_id {
                    Some(before) => self.clip_index(*before)?,
                    None => self.clips.len(),
                };
                let clip = Clip {
                    id: *id,
                    media_id: *media_id,
                    track_id: self.video_track_id,
                    at_frame: 0,
                    source_in_frame: *source_in_frame,
                    duration_frames: *duration_frames,
                };
                self.validate_clip(&clip)?;
                self.clips.insert(target, clip);
                self.reposition_contiguous()?;
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
                if self.time_effects.contains_key(clip_id) {
                    return Err(Error::Unsupported("split time-remapped clips after resetting the time effect; automatic source rebasing is unsupported".into()));
                }
                if self.animations.contains_key(clip_id) {
                    return Err(Error::Unsupported("splitting animated clips requires curve rebasing; reset animation before splitting".into()));
                }
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
                if let Some(settings) = self.visual_settings.get(clip_id).cloned() {
                    self.visual_settings.insert(*right_clip_id, settings);
                }
            }
            Edit::DuplicateClip { clip_id, id } => {
                self.require_contiguous()?;
                let index = self.clip_index(*clip_id)?;
                if let Some(effect) = self.time_effects.get(clip_id).copied() {
                    self.time_effects.insert(*id, effect);
                }
                let mut duplicate = self.clips[index].clone();
                duplicate.id = *id;
                self.clips.insert(index + 1, duplicate);
                if let Some(settings) = self.visual_settings.get(clip_id).cloned() {
                    self.visual_settings.insert(*id, settings);
                }
                if let Some(animation) = self.animations.get(clip_id).cloned() {
                    self.animations.insert(*id, animation);
                }
                self.reposition_contiguous()?;
            }
            Edit::DeleteClip { clip_id, ripple } => {
                if *ripple {
                    self.require_contiguous()?;
                }
                let index = self.clip_index(*clip_id)?;
                self.clips.remove(index);
                self.visual_settings.remove(clip_id);
                self.animations.remove(clip_id);
                self.time_effects.remove(clip_id);
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
            Edit::SetFrameRate { frame_rate } => {
                if self.frame_rate != *frame_rate
                    && (!self.clips.is_empty()
                        || !self.audio_clips.is_empty()
                        || !self.visual_clips.is_empty()
                        || !self.scene_placements.is_empty())
                {
                    return Err(Error::Unsupported(
                        "timeline frame rate is locked while video or audio clips are placed; choose it before editing".into(),
                    ));
                }
                self.frame_rate = *frame_rate;
            }
            Edit::PasteClip {
                id,
                at_frame,
                clipboard,
            } => {
                let edit = crate::clipboard::paste_edit(self, clipboard, *id, *at_frame)?;
                self.apply(&edit, None)?;
                if let Some(settings) = &clipboard.visual_settings {
                    self.visual_settings.insert(*id, settings.clone());
                }
                if let Some(animation) = &clipboard.animation {
                    self.animations.insert(*id, animation.clone());
                }
            }
            Edit::Undo {} | Edit::Redo {} => {
                return Err(Error::Unsupported(
                    "undo/redo requires authoritative persisted history context".into(),
                ));
            }
        }
        self.validate()
    }

    pub fn visual_duration(&self, id: Uuid) -> Result<u64> {
        self.clips
            .iter()
            .find(|clip| clip.id == id)
            .map(|clip| clip.duration_frames)
            .or_else(|| {
                self.visual_clips
                    .iter()
                    .find(|clip| clip.id == id)
                    .map(|clip| clip.duration_frames)
            })
            .or_else(|| {
                self.scene_placements
                    .iter()
                    .find(|clip| clip.id == id)
                    .map(|clip| clip.duration_frames)
            })
            .ok_or_else(|| Error::NotFound(format!("visual animation clip {id}")))
    }

    fn scene_placement_mut(&mut self, id: Uuid) -> Result<&mut ScenePlacement> {
        self.scene_placements
            .iter_mut()
            .find(|clip| clip.id == id)
            .ok_or_else(|| Error::NotFound(format!("scene placement {id}")))
    }

    fn clip_index(&self, id: Uuid) -> Result<usize> {
        self.clips
            .iter()
            .position(|clip| clip.id == id)
            .ok_or_else(|| Error::NotFound(format!("clip {id}")))
    }

    fn visual_track_index(&self, id: Uuid) -> Result<usize> {
        self.visual_tracks
            .iter()
            .position(|track| track.id == id)
            .ok_or_else(|| Error::NotFound(format!("visual track {id}")))
    }

    fn visual_clip_mut(&mut self, id: Uuid) -> Result<&mut VisualClip> {
        self.visual_clips
            .iter_mut()
            .find(|clip| clip.id == id)
            .ok_or_else(|| Error::NotFound(format!("visual clip {id}")))
    }

    fn audio_clip_mut(&mut self, id: Uuid) -> Result<&mut AudioClip> {
        self.audio_clips
            .iter_mut()
            .find(|clip| clip.id == id)
            .ok_or_else(|| Error::NotFound(format!("audio clip {id}")))
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

fn validate_render_animation(animation: &crate::animation::Animation) -> Result<()> {
    if animation.channels.iter().any(|channel| {
        matches!(
            channel.property,
            crate::animation::AnimationProperty::ScaleX
                | crate::animation::AnimationProperty::ScaleY
        )
    }) {
        return Err(Error::Unsupported("animated scale requires bounded scale rendering proof; use a static destination rectangle".into()));
    }
    Ok(())
}
