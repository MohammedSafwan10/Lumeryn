//! Exact bounded time mapping; verified metadata is supplied by the graph compiler.
use crate::{Error, Result, animation::RationalFrame, model::MAX_FRAMES};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackSpeed {
    Half,
    #[default]
    Normal,
    Double,
}
impl PlaybackSpeed {
    pub fn ratio(self) -> (u64, u64) {
        match self {
            Self::Half => (1, 2),
            Self::Normal => (1, 1),
            Self::Double => (2, 1),
        }
    }
    pub fn tempo(self) -> &'static str {
        match self {
            Self::Half => "0.5",
            Self::Normal => "1",
            Self::Double => "2",
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpeedAudio {
    #[default]
    PreservePitch,
    Mute,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimeEffect {
    #[default]
    Normal,
    Speed {
        speed: PlaybackSpeed,
        audio: SpeedAudio,
    },
    /// Exact frame hold, contributing silence rather than repeating a sample.
    Freeze { source_frame: u64 },
}
fn overflow() -> Error {
    Error::Invalid("time-effect arithmetic overflow".into())
}
impl TimeEffect {
    pub fn speed(self) -> PlaybackSpeed {
        match self {
            Self::Speed { speed, .. } => speed,
            _ => PlaybackSpeed::Normal,
        }
    }
    pub fn is_silent(self) -> bool {
        matches!(
            self,
            Self::Freeze { .. }
                | Self::Speed {
                    audio: SpeedAudio::Mute,
                    ..
                }
        )
    }
    pub fn validate(self, source_in: u64, duration: u64, source_frames: u64) -> Result<()> {
        if duration == 0
            || duration > MAX_FRAMES
            || source_in > MAX_FRAMES
            || source_frames == 0
            || source_frames > MAX_FRAMES
        {
            return Err(Error::Invalid("invalid bounded time-effect range".into()));
        }
        if let Self::Freeze { source_frame } = self {
            if source_frame >= source_frames {
                return Err(Error::Invalid(
                    "freeze frame is outside verified source".into(),
                ));
            }
            return Ok(());
        }
        let (n, d) = self.speed().ratio();
        if source_in >= source_frames
            || u128::from(duration) * u128::from(n)
                > u128::from(source_frames.saturating_sub(source_in)) * u128::from(d)
        {
            return Err(Error::Invalid(
                "speed range exhausts verified source; trim explicitly".into(),
            ));
        }
        Ok(())
    }
    pub fn source_at(
        self,
        local: RationalFrame,
        source_in: u64,
        duration: u64,
        source_frames: u64,
    ) -> Result<Option<RationalFrame>> {
        self.validate(source_in, duration, source_frames)?;
        let Some(local) = local.in_clip(0, duration)? else {
            return Ok(None);
        };
        if let Self::Freeze { source_frame } = self {
            return RationalFrame::new(u128::from(source_frame), 1).map(Some);
        }
        let (a, b) = local.as_ratio();
        let (n, d) = self.speed().ratio();
        let denominator = b.checked_mul(u128::from(d)).ok_or_else(overflow)?;
        let numerator = a
            .checked_mul(u128::from(n))
            .and_then(|v| {
                u128::from(source_in)
                    .checked_mul(denominator)
                    .and_then(|base| base.checked_add(v))
            })
            .ok_or_else(overflow)?;
        RationalFrame::new(numerator, denominator).map(Some)
    }
    /// Preserves source range; placement/ripple is a separate atomic graph edit.
    pub fn duration_for_source(self, source_duration: u64) -> Result<u64> {
        if matches!(self, Self::Freeze { .. }) {
            return Err(Error::Unsupported(
                "freeze duration must be selected explicitly".into(),
            ));
        }
        let (n, d) = self.speed().ratio();
        let scaled = source_duration.checked_mul(d).ok_or_else(overflow)?;
        if source_duration == 0 || scaled % n != 0 || scaled / n > MAX_FRAMES {
            return Err(Error::Invalid(
                "source range has no exact bounded timeline duration at this speed".into(),
            ));
        }
        Ok(scaled / n)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    CrossDissolve,
    WipeLeft,
    WipeRight,
    WipeUp,
    WipeDown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FixedCutTransition {
    pub kind: TransitionKind,
    pub duration_frames: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionHandles {
    pub before_cut: u64,
    pub after_cut: u64,
}
impl FixedCutTransition {
    pub fn handles(self) -> Result<TransitionHandles> {
        if !(2..=300).contains(&self.duration_frames) {
            return Err(Error::Invalid(
                "transition duration must be 2..300 project frames".into(),
            ));
        }
        Ok(TransitionHandles {
            before_cut: self.duration_frames / 2,
            after_cut: self.duration_frames - self.duration_frames / 2,
        })
    }
    /// Caller proves adjacency, normal speed, verified identity and CFR metadata.
    pub fn validate_sources(
        self,
        cut: u64,
        outgoing_source_out: u64,
        outgoing_frames: u64,
        incoming_source_in: u64,
        incoming_frames: u64,
    ) -> Result<TransitionHandles> {
        let h = self.handles()?;
        if cut < h.before_cut
            || cut
                .checked_add(h.after_cut)
                .is_none_or(|end| end > MAX_FRAMES)
        {
            return Err(Error::Invalid(
                "transition interval is outside project bounds".into(),
            ));
        }
        if outgoing_frames > MAX_FRAMES
            || incoming_frames > MAX_FRAMES
            || outgoing_source_out < h.before_cut
            || outgoing_source_out
                .checked_add(h.after_cut)
                .is_none_or(|end| end > outgoing_frames)
            || incoming_source_in < h.before_cut
            || incoming_source_in
                .checked_add(h.after_cut)
                .is_none_or(|end| end > incoming_frames)
        {
            return Err(Error::Invalid(
                "transition lacks verified source handles; trim adjoining clips explicitly".into(),
            ));
        }
        Ok(h)
    }
    /// Half-open output-rate samples, without rounding to integer project frames.
    pub fn progress(self, time: RationalFrame, cut: u64) -> Result<Option<f64>> {
        let h = self.handles()?;
        let start = cut
            .checked_sub(h.before_cut)
            .ok_or_else(|| Error::Invalid("transition starts before project".into()))?;
        let Some(local) = time.in_clip(start, self.duration_frames)? else {
            return Ok(None);
        };
        let (n, d) = local.as_ratio();
        Ok(Some(n as f64 / d as f64 / self.duration_frames as f64))
    }
    /// Return the two exact source positions through the overlap. Incoming
    /// before-handle and outgoing after-handle are explicit verified media.
    pub fn sources_at(
        self,
        time: RationalFrame,
        cut: u64,
        outgoing_source_out: u64,
        outgoing_frames: u64,
        incoming_source_in: u64,
        incoming_frames: u64,
    ) -> Result<Option<(RationalFrame, RationalFrame)>> {
        let handles = self.validate_sources(
            cut,
            outgoing_source_out,
            outgoing_frames,
            incoming_source_in,
            incoming_frames,
        )?;
        let Some(local) = time.in_clip(cut - handles.before_cut, self.duration_frames)? else {
            return Ok(None);
        };
        let (numerator, denominator) = local.as_ratio();
        let source = |source_cut: u64| {
            let base = u128::from(source_cut - handles.before_cut)
                .checked_mul(denominator)
                .and_then(|base| base.checked_add(numerator))
                .ok_or_else(overflow)?;
            RationalFrame::new(base, denominator)
        };
        Ok(Some((
            source(outgoing_source_out)?,
            source(incoming_source_in)?,
        )))
    }
    pub fn incoming_weight(
        self,
        progress: f64,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<f64> {
        if !progress.is_finite()
            || !(0.0..=1.0).contains(&progress)
            || width == 0
            || height == 0
            || x >= width
            || y >= height
        {
            return Err(Error::Invalid("invalid transition pixel sample".into()));
        }
        let horizontal = (f64::from(x) + 0.5) / f64::from(width);
        let vertical = (f64::from(y) + 0.5) / f64::from(height);
        let selected = match self.kind {
            TransitionKind::CrossDissolve => return Ok(progress),
            TransitionKind::WipeLeft => horizontal >= 1.0 - progress,
            TransitionKind::WipeRight => horizontal < progress,
            TransitionKind::WipeUp => vertical >= 1.0 - progress,
            TransitionKind::WipeDown => vertical < progress,
        };
        Ok(if selected { 1.0 } else { 0.0 })
    }
}
/// Straight RGBA interpolated via premultiplied alpha in the stored encoded-SDR
/// policy; no claim of linear-light/HDR blending.
pub fn blend_rgba(outgoing: [u8; 4], incoming: [u8; 4], weight: f64) -> Result<[u8; 4]> {
    if !weight.is_finite() || !(0.0..=1.0).contains(&weight) {
        return Err(Error::Invalid("invalid dissolve weight".into()));
    }
    let oa = f64::from(outgoing[3]) / 255.0 * (1.0 - weight);
    let ia = f64::from(incoming[3]) / 255.0 * weight;
    let alpha = oa + ia;
    if alpha == 0.0 {
        return Ok([0; 4]);
    }
    let mut result = [0; 4];
    for (target, (o, i)) in result[..3]
        .iter_mut()
        .zip(outgoing[..3].iter().zip(&incoming[..3]))
    {
        *target = ((f64::from(*o) * oa + f64::from(*i) * ia) / alpha).round() as u8;
    }
    result[3] = (alpha * 255.0).round() as u8;
    Ok(result)
}
