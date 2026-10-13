//! Bounded, seek-independent animation in exact local project-frame time.
use crate::{Error, Result, model::MAX_FRAMES, time::FrameRate};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_KEYFRAMES: usize = 128;
pub const MAX_ANIMATION_CHANNELS: usize = 6;

/// Integer serialized values avoid JSON floating-point ambiguity. Position is
/// milli-pixels, scale/opacity millionths, rotation milli-degrees clockwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnimationProperty {
    X,
    Y,
    ScaleX,
    ScaleY,
    Rotation,
    Opacity,
}

impl AnimationProperty {
    fn bounds(self) -> (i64, i64) {
        match self {
            Self::X | Self::Y => (-8_192_000, 8_192_000),
            Self::ScaleX | Self::ScaleY => (1_000, 8_000_000),
            Self::Rotation => (-360_000, 360_000),
            Self::Opacity => (0, 1_000_000),
        }
    }
    fn divisor(self) -> f64 {
        match self {
            Self::X | Self::Y | Self::Rotation => 1_000.0,
            _ => 1_000_000.0,
        }
    }
}

/// A key's interpolation applies to the segment beginning at that key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    Hold,
    Linear,
    SmoothStep,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Keyframe {
    pub at_frame: u64,
    pub value: i64,
    pub interpolation: Interpolation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnimationChannel {
    pub property: AnimationProperty,
    pub keys: Vec<Keyframe>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Animation {
    pub channels: Vec<AnimationChannel>,
}

/// Exact time is deliberately not persisted; it is constructed from output
/// sample index/rates or a native seek, and never accumulated between frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RationalFrame {
    numerator: u128,
    denominator: u128,
}

impl RationalFrame {
    pub fn as_ratio(self) -> (u128, u128) {
        (self.numerator, self.denominator)
    }
    pub fn new(numerator: u128, denominator: u128) -> Result<Self> {
        if denominator == 0 {
            return Err(Error::Invalid("animation time denominator is zero".into()));
        }
        let (mut a, mut b) = (numerator, denominator);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Ok(Self {
            numerator: numerator / a,
            denominator: denominator / a,
        })
    }
    pub fn from_output_frame(
        frame: u64,
        project_rate: FrameRate,
        output_rate: FrameRate,
    ) -> Result<Self> {
        if frame > MAX_FRAMES {
            return Err(Error::Invalid(
                "animation output index exceeds bound".into(),
            ));
        }
        Self::new(
            u128::from(frame)
                * u128::from(output_rate.denominator())
                * u128::from(project_rate.numerator()),
            u128::from(output_rate.numerator()) * u128::from(project_rate.denominator()),
        )
    }
    /// Clip intervals are [start,end). Before/after samples produce no object.
    pub fn in_clip(self, start: u64, duration: u64) -> Result<Option<Self>> {
        let end = start
            .checked_add(duration)
            .filter(|end| duration > 0 && *end <= MAX_FRAMES)
            .ok_or_else(|| Error::Invalid("invalid animation clip interval".into()))?;
        let start = u128::from(start)
            .checked_mul(self.denominator)
            .ok_or_else(|| Error::Invalid("animation time overflow".into()))?;
        let end = u128::from(end)
            .checked_mul(self.denominator)
            .ok_or_else(|| Error::Invalid("animation time overflow".into()))?;
        if self.numerator < start || self.numerator >= end {
            return Ok(None);
        }
        Self::new(self.numerator - start, self.denominator).map(Some)
    }
}

impl Animation {
    /// Keys may reach duration to define the mathematical end value although
    /// the clip itself has no sample at that half-open endpoint.
    pub fn validate(&self, duration: u64) -> Result<()> {
        if duration == 0 || duration > MAX_FRAMES || self.channels.len() > MAX_ANIMATION_CHANNELS {
            return Err(Error::Invalid(
                "invalid animation duration or channel count".into(),
            ));
        }
        for (index, channel) in self.channels.iter().enumerate() {
            if self.channels[..index]
                .iter()
                .any(|other| other.property == channel.property)
            {
                return Err(Error::Invalid("duplicate animation property".into()));
            }
            let (min, max) = channel.property.bounds();
            if channel.keys.is_empty()
                || channel.keys.len() > MAX_KEYFRAMES
                || channel
                    .keys
                    .iter()
                    .any(|key| key.at_frame > duration || !(min..=max).contains(&key.value))
                || channel
                    .keys
                    .windows(2)
                    .any(|keys| keys[0].at_frame >= keys[1].at_frame)
            {
                return Err(Error::Invalid(
                    "animation keys must be ordered, unique, bounded and inside the clip".into(),
                ));
            }
        }
        Ok(())
    }
    /// Omitted channels preserve the static object's properties. Values use
    /// floating point only after exact segment selection; bit identity across
    /// CPU architectures is not promised.
    pub fn evaluate(
        &self,
        duration: u64,
        local: RationalFrame,
    ) -> Result<Vec<(AnimationProperty, f64)>> {
        self.validate(duration)?;
        let end = u128::from(duration)
            .checked_mul(local.denominator)
            .ok_or_else(|| Error::Invalid("animation time overflow".into()))?;
        if local.numerator >= end {
            return Err(Error::Invalid(
                "animation sample is outside the clip".into(),
            ));
        }
        self.channels
            .iter()
            .map(|channel| {
                channel
                    .evaluate(local)
                    .map(|value| (channel.property, value))
            })
            .collect()
    }
}

impl AnimationChannel {
    fn evaluate(&self, time: RationalFrame) -> Result<f64> {
        let first = self
            .keys
            .first()
            .ok_or_else(|| Error::Invalid("empty animation channel".into()))?;
        let mut previous = first;
        for key in &self.keys {
            let at = u128::from(key.at_frame)
                .checked_mul(time.denominator)
                .ok_or_else(|| Error::Invalid("animation time overflow".into()))?;
            if time.numerator < at {
                if key.at_frame == first.at_frame {
                    return Ok(first.value as f64 / self.property.divisor());
                }
                let begin = u128::from(previous.at_frame)
                    .checked_mul(time.denominator)
                    .ok_or_else(|| Error::Invalid("animation time overflow".into()))?;
                let progress = (time.numerator - begin) as f64 / (at - begin) as f64;
                let eased = match previous.interpolation {
                    Interpolation::Hold => 0.0,
                    Interpolation::Linear => progress,
                    Interpolation::SmoothStep => progress * progress * (3.0 - 2.0 * progress),
                };
                return Ok(
                    (previous.value as f64 + (key.value - previous.value) as f64 * eased)
                        / self.property.divisor(),
                );
            }
            previous = key;
        }
        Ok(previous.value as f64 / self.property.divisor())
    }
}

/// Original declarative starter specifications. These are offgraph blueprints,
/// not a renderer capability; the scene adapter must resolve audited fonts and
/// verified media before one atomic project mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Starter {
    CleanTitle,
    LowerThird,
    Callout,
    ChapterCard,
    PictureInPicture,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StarterContent {
    Text {
        text: String,
        font_size: u32,
        rgba: [u8; 4],
    },
    Rectangle {
        rgba: [u8; 4],
    },
    /// The caller supplies this verified source when adapting to the graph.
    MediaSlot {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StarterObject {
    pub id: uuid::Uuid,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub content: StarterContent,
    pub animation: Animation,
}

/// Caller-owned IDs ensure retry expansion is deterministic and cannot allocate
/// fresh objects on idempotent replay. Returned ordinary objects stay editable.
pub fn expand_starter(
    starter: Starter,
    width: u32,
    height: u32,
    duration: u64,
    title: &str,
    ids: &[uuid::Uuid],
) -> Result<Vec<StarterObject>> {
    if !(320..=8192).contains(&width)
        || !(180..=8192).contains(&height)
        || !(2..=MAX_FRAMES).contains(&duration)
        || title.is_empty()
        || title.len() > 4096
        || title.chars().any(|ch| ch.is_control())
        || ids.len() != 2
        || ids.iter().any(uuid::Uuid::is_nil)
        || ids[0] == ids[1]
    {
        return Err(Error::Invalid(
            "starter needs bounded canvas/duration/text and two distinct non-nil object IDs".into(),
        ));
    }
    let margin = width / 20;
    let entrance = duration.min(16) / 2;
    let fade = Animation {
        channels: vec![AnimationChannel {
            property: AnimationProperty::Opacity,
            keys: vec![
                Keyframe {
                    at_frame: 0,
                    value: 0,
                    interpolation: Interpolation::SmoothStep,
                },
                Keyframe {
                    at_frame: entrance,
                    value: 1_000_000,
                    interpolation: Interpolation::Hold,
                },
            ],
        }],
    };
    let (x, y, w, h, panel, text_color) = match starter {
        Starter::CleanTitle => (
            margin,
            height * 2 / 5,
            width - margin * 2,
            height / 5,
            [0, 0, 0, 0],
            [240, 244, 250, 255],
        ),
        Starter::LowerThird => (
            margin,
            height * 3 / 4,
            width * 3 / 5,
            height / 6,
            [17, 28, 38, 235],
            [240, 244, 250, 255],
        ),
        Starter::Callout => (
            width / 2,
            height / 4,
            width * 2 / 5,
            height / 4,
            [102, 242, 199, 255],
            [8, 24, 30, 255],
        ),
        Starter::ChapterCard => (
            margin,
            height / 3,
            width - margin * 2,
            height / 3,
            [17, 28, 38, 255],
            [102, 242, 199, 255],
        ),
        Starter::PictureInPicture => (
            width * 13 / 20,
            height * 3 / 5,
            width * 3 / 10,
            height * 3 / 10,
            [102, 242, 199, 255],
            [240, 244, 250, 255],
        ),
    };
    let inset = (h / 12).max(2);
    let objects = vec![
        StarterObject {
            id: ids[0],
            x,
            y,
            width: w,
            height: h,
            content: StarterContent::Rectangle { rgba: panel },
            animation: fade.clone(),
        },
        StarterObject {
            id: ids[1],
            x: x + inset,
            y: y + inset,
            width: w - inset * 2,
            height: h - inset * 2,
            content: if starter == Starter::PictureInPicture {
                StarterContent::MediaSlot {}
            } else {
                StarterContent::Text {
                    text: title.into(),
                    font_size: (h / 3).max(12),
                    rgba: text_color,
                }
            },
            animation: fade,
        },
    ];
    for object in &objects {
        object.animation.validate(duration)?;
    }
    Ok(objects)
}
