//! Versioned encoded-SDR colour and exact clip-local fade contracts.
use crate::{Error, Result, audio_plan::sample_boundary, model::MAX_FRAMES, time::FrameRate};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FadeEnvelope {
    pub in_frames: u64,
    pub out_frames: u64,
}

impl FadeEnvelope {
    pub fn validate(self, duration_frames: u64) -> Result<()> {
        if duration_frames == 0
            || duration_frames > MAX_FRAMES
            || self
                .in_frames
                .checked_add(self.out_frames)
                .is_none_or(|sum| sum > duration_frames)
        {
            return Err(Error::Invalid(
                "fades must fit without overlap inside a nonempty bounded clip".into(),
            ));
        }
        Ok(())
    }

    /// Canonical 48 kHz samples use absolute rational boundaries, never rounded
    /// clip-local lengths. The clip interval is half open; end/outside is silent.
    pub fn audio_gain(
        self,
        rate: FrameRate,
        at_frame: u64,
        duration_frames: u64,
        absolute_sample: u64,
    ) -> Result<f32> {
        self.validate(duration_frames)?;
        let end_frame = at_frame
            .checked_add(duration_frames)
            .filter(|end| *end <= MAX_FRAMES)
            .ok_or_else(|| Error::Invalid("fade clip endpoint overflow".into()))?;
        let start = sample_boundary(rate, at_frame)?;
        let end = sample_boundary(rate, end_frame)?;
        if absolute_sample < start || absolute_sample >= end {
            return Ok(0.0);
        }
        let mut gain = 1.0_f64;
        if self.in_frames > 0 {
            let fade_end = sample_boundary(rate, at_frame + self.in_frames)?;
            if fade_end == start {
                return Err(Error::Unsupported(
                    "fade is shorter than one canonical sample".into(),
                ));
            }
            gain *= ((absolute_sample - start) as f64 / (fade_end - start) as f64).min(1.0);
        }
        if self.out_frames > 0 {
            let fade_start = sample_boundary(rate, end_frame - self.out_frames)?;
            if fade_start == end {
                return Err(Error::Unsupported(
                    "fade is shorter than one canonical sample".into(),
                ));
            }
            gain *= ((end - absolute_sample) as f64 / (end - fade_start) as f64).min(1.0);
        }
        Ok(gain as f32)
    }

    /// Apply after canonical decode/trim with local sample zero at clip start.
    /// Integer sample counts match audio_gain, including absolute fractional-rate
    /// placement boundaries; no approximate seconds conversion is used here.
    pub fn lower_audio_filters(
        self,
        rate: FrameRate,
        at_frame: u64,
        duration_frames: u64,
    ) -> Result<Option<String>> {
        self.validate(duration_frames)?;
        let end_frame = at_frame
            .checked_add(duration_frames)
            .filter(|end| *end <= MAX_FRAMES)
            .ok_or_else(|| Error::Invalid("audio fade endpoint overflow".into()))?;
        let start = sample_boundary(rate, at_frame)?;
        let end = sample_boundary(rate, end_frame)?;
        let mut filters = Vec::new();
        if self.in_frames > 0 {
            let samples = sample_boundary(rate, at_frame + self.in_frames)? - start;
            if samples == 0 {
                return Err(Error::Unsupported(
                    "fade is shorter than one canonical sample".into(),
                ));
            }
            filters.push(format!("afade=t=in:ss=0:ns={samples}:curve=tri"));
        }
        if self.out_frames > 0 {
            let fade_start = sample_boundary(rate, end_frame - self.out_frames)?;
            let samples = end - fade_start;
            if samples == 0 {
                return Err(Error::Unsupported(
                    "fade is shorter than one canonical sample".into(),
                ));
            }
            filters.push(format!(
                "afade=t=out:ss={}:ns={samples}:curve=tri",
                fade_start - start
            ));
        }
        Ok((!filters.is_empty()).then(|| filters.join(",")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualEffects {
    pub version: u32,
    pub exposure_millistops: i32,
    pub contrast_basis_points: u16,
    pub saturation_basis_points: u16,
    /// Artistic red/blue balance, not calibrated Kelvin white balance.
    pub temperature_basis_points: i32,
    pub fades: FadeEnvelope,
}

impl Default for VisualEffects {
    fn default() -> Self {
        Self {
            version: 1,
            exposure_millistops: 0,
            contrast_basis_points: 10000,
            saturation_basis_points: 10000,
            temperature_basis_points: 0,
            fades: FadeEnvelope::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColourPreset {
    Neutral,
    WarmStory,
    CoolClarity,
    SoftMono,
}

impl VisualEffects {
    pub fn is_identity(self) -> bool {
        self == Self::default()
    }
    pub fn validate(self, duration_frames: u64) -> Result<()> {
        if self.version != 1
            || !(-2000..=2000).contains(&self.exposure_millistops)
            || self.contrast_basis_points > 20000
            || self.saturation_basis_points > 20000
            || !(-10000..=10000).contains(&self.temperature_basis_points)
        {
            return Err(Error::Invalid(
                "unsupported colour version or fixed-point parameter outside bounds".into(),
            ));
        }
        self.fades.validate(duration_frames)
    }

    /// Original numeric recipes; intensity interpolates around neutral. No third
    /// party LUT or proprietary preset assets are copied. Reset is Default.
    pub fn preset(preset: ColourPreset, intensity_basis_points: u16) -> Result<Self> {
        if intensity_basis_points > 10000 {
            return Err(Error::Invalid("preset intensity must be 0..10000".into()));
        }
        let (exposure, contrast, saturation, temperature) = match preset {
            ColourPreset::Neutral => (0, 10000, 10000, 0),
            ColourPreset::WarmStory => (100, 10800, 11200, 1800),
            ColourPreset::CoolClarity => (0, 11500, 10500, -1600),
            ColourPreset::SoftMono => (0, 9000, 0, 0),
        };
        let blend = |target: i32, baseline: i32| {
            baseline + (target - baseline) * i32::from(intensity_basis_points) / 10000
        };
        Ok(Self {
            exposure_millistops: blend(exposure, 0),
            contrast_basis_points: blend(contrast, 10000) as u16,
            saturation_basis_points: blend(saturation, 10000) as u16,
            temperature_basis_points: blend(temperature, 0),
            ..Default::default()
        })
    }

    /// Input must have clip-local PTS, straight alpha, after spatial transform and
    /// static opacity, before timeline offset/overlay. Rational seconds at the FFmpeg
    /// boundary have microsecond precision; endpoints are not output-frame rounded.
    pub fn lower_filters(self, rate: FrameRate, duration_frames: u64) -> Result<Option<String>> {
        self.validate(duration_frames)?;
        if self.is_identity() {
            return Ok(None);
        }
        let exposure = 2_f64.powf(f64::from(self.exposure_millistops) / 1000.0);
        let contrast = f64::from(self.contrast_basis_points) / 10000.0;
        let saturation = f64::from(self.saturation_basis_points) / 10000.0;
        let temperature = f64::from(self.temperature_basis_points) / 50000.0;
        let mut filters = vec!["format=rgba".to_owned()];
        if self.exposure_millistops != 0 || self.contrast_basis_points != 10000 {
            let curve = format!("clip((val*{exposure:.8}-127.5)*{contrast:.8}+127.5,0,255)");
            filters.push(format!("lutrgb=r='{curve}':g='{curve}':b='{curve}'"));
        }
        if self.saturation_basis_points != 10000 {
            let weights = [0.2126, 0.7152, 0.0722];
            let names = [["rr", "rg", "rb"], ["gr", "gg", "gb"], ["br", "bg", "bb"]];
            let mut matrix = Vec::new();
            for (row, row_names) in names.into_iter().enumerate() {
                for (column, weight) in weights.into_iter().enumerate() {
                    let coefficient =
                        (1.0 - saturation) * weight + if row == column { saturation } else { 0.0 };
                    matrix.push(format!("{}={coefficient:.8}", row_names[column]));
                }
            }
            filters.push(format!("colorchannelmixer={}", matrix.join(":")));
        }
        // A separate bounded LUT avoids exceeding colorchannelmixer's ±2 matrix
        // coefficient range at maximum saturation + positive temperature.
        if self.temperature_basis_points != 0 {
            filters.push(format!(
                "lutrgb=r='clip(val*{:.8},0,255)':b='clip(val*{:.8},0,255)'",
                1.0 + temperature,
                1.0 - temperature
            ));
        }
        if self.fades.in_frames > 0 {
            filters.push(format!(
                "fade=t=in:st=0:d={}:alpha=1",
                rate.seconds(self.fades.in_frames)
            ));
        }
        if self.fades.out_frames > 0 {
            filters.push(format!(
                "fade=t=out:st={}:d={}:alpha=1",
                rate.seconds(duration_frames - self.fades.out_frames),
                rate.seconds(self.fades.out_frames)
            ));
        }
        Ok(Some(filters.join(",")))
    }
}
