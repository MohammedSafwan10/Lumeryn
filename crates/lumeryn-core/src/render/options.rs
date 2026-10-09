use crate::{Error, Project, Result, model::MAX_FRAMES, time::FrameRate};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RenderResolution {
    #[default]
    Project,
    Hd720,
    Fhd1080,
    Uhd2160,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    #[default]
    Mpeg4,
    H264,
    H265,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RenderQuality {
    Draft,
    Balanced,
    #[default]
    High,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Acceleration {
    #[default]
    Cpu,
    Auto,
}

macro_rules! parse_choice {
    ($kind:ty, $($name:literal => $variant:path),+ $(,)?) => {
        impl std::str::FromStr for $kind {
            type Err = Error;
            fn from_str(value: &str) -> Result<Self> {
                match value { $($name => Ok($variant),)+ _ => Err(Error::Invalid(format!("invalid {}: {value}", stringify!($kind)))) }
            }
        }
    };
}
parse_choice!(RenderResolution, "project" => Self::Project, "hd720" => Self::Hd720, "fhd1080" => Self::Fhd1080, "uhd2160" => Self::Uhd2160);
parse_choice!(VideoCodec, "mpeg4" => Self::Mpeg4, "h264" => Self::H264, "h265" => Self::H265);
parse_choice!(RenderQuality, "draft" => Self::Draft, "balanced" => Self::Balanced, "high" => Self::High);
parse_choice!(Acceleration, "cpu" => Self::Cpu, "auto" => Self::Auto);

/// Final-source export settings. Preview quality and monitor volume are never inputs here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RenderOptions {
    pub resolution: RenderResolution,
    pub fps: Option<u32>,
    pub codec: VideoCodec,
    pub quality: RenderQuality,
    /// Target kilobits per second; absent selects the encoder's documented quality mode.
    pub bitrate_kbps: Option<u32>,
    pub acceleration: Acceleration,
}
impl RenderOptions {
    pub fn validate(&self) -> Result<()> {
        if self
            .fps
            .is_some_and(|fps| !matches!(fps, 24 | 25 | 30 | 60))
        {
            return Err(Error::Invalid(
                "output FPS must be 24, 25, 30 or 60; omit to preserve project FPS".into(),
            ));
        }
        if self
            .bitrate_kbps
            .is_some_and(|rate| !(100..=200_000).contains(&rate))
        {
            return Err(Error::Invalid(
                "video bitrate must be 100..200000 kbps".into(),
            ));
        }
        Ok(())
    }
    pub fn dimensions(&self, project: &Project) -> (u32, u32) {
        let (long, short) = match self.resolution {
            RenderResolution::Project => return (project.width, project.height),
            RenderResolution::Hd720 => (1280, 720),
            RenderResolution::Fhd1080 => (1920, 1080),
            RenderResolution::Uhd2160 => (3840, 2160),
        };
        match project.width.cmp(&project.height) {
            std::cmp::Ordering::Less => (short, long),
            std::cmp::Ordering::Equal => (short, short),
            std::cmp::Ordering::Greater => (long, short),
        }
    }
    pub fn frame_rate(&self, project: &Project) -> Result<FrameRate> {
        self.validate()?;
        self.fps
            .map_or(Ok(project.frame_rate), |fps| FrameRate::new(fps, 1))
    }
}
pub(super) fn output_frames(frames: u64, source: FrameRate, output: FrameRate) -> Result<u64> {
    let numerator =
        u128::from(frames) * u128::from(source.denominator()) * u128::from(output.numerator());
    let denominator = u128::from(source.numerator()) * u128::from(output.denominator());
    let frames = numerator.div_ceil(denominator);
    if frames == 0 || frames > u128::from(MAX_FRAMES) {
        return Err(Error::Invalid(
            "output duration exceeds supported frame range".into(),
        ));
    }
    Ok(frames as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn options_validate_and_orient_exact_uhd() -> Result<()> {
        let mut project = Project::new("Export".into(), 1920, 1080, FrameRate::new(30, 1)?)?;
        let options = RenderOptions {
            resolution: RenderResolution::Uhd2160,
            ..Default::default()
        };
        assert_eq!(options.dimensions(&project), (3840, 2160));
        project.width = 1080;
        project.height = 1920;
        assert_eq!(options.dimensions(&project), (2160, 3840));
        for fps in [0, 23, 120] {
            assert!(
                RenderOptions {
                    fps: Some(fps),
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            RenderOptions {
                bitrate_kbps: Some(0),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert_eq!(
            serde_json::from_str::<RenderOptions>("{}")?,
            RenderOptions::default()
        );
        Ok(())
    }
    #[test]
    fn rational_duration_uses_ceiling_once_and_preserves_default() -> Result<()> {
        let source = FrameRate::new(30_000, 1001)?;
        assert_eq!(output_frames(30, source, source)?, 30);
        assert_eq!(output_frames(30, source, FrameRate::new(24, 1)?)?, 25);
        assert_eq!(output_frames(30, source, FrameRate::new(60, 1)?)?, 61);
        Ok(())
    }
}
