use crate::{Error, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Exact project frame rate. Construction and deserialization validate bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "RateInput")]
pub struct FrameRate {
    numerator: u32,
    denominator: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RateInput {
    numerator: u32,
    denominator: u32,
}

impl TryFrom<RateInput> for FrameRate {
    type Error = Error;
    fn try_from(value: RateInput) -> Result<Self> {
        Self::new(value.numerator, value.denominator)
    }
}

impl FrameRate {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self> {
        if numerator == 0
            || denominator == 0
            || numerator > 1_000_000
            || denominator > 1_000_000
            || u64::from(numerator) > 1000 * u64::from(denominator)
        {
            return Err(Error::Invalid(
                "frame rate must be positive and <=1000 fps".into(),
            ));
        }
        let mut a = numerator;
        let mut b = denominator;
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Ok(Self {
            numerator: numerator / a,
            denominator: denominator / a,
        })
    }

    pub fn numerator(self) -> u32 {
        self.numerator
    }
    pub fn denominator(self) -> u32 {
        self.denominator
    }

    /// Decimal seconds only at FFmpeg's boundary; authoritative time stays rational.
    pub fn seconds(self, frames: u64) -> String {
        let nanos = u128::from(frames) * u128::from(self.denominator) * 1_000_000_000
            / u128::from(self.numerator);
        format!("{}.{:09}", nanos / 1_000_000_000, nanos % 1_000_000_000)
    }
}

impl std::fmt::Display for FrameRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

impl FromStr for FrameRate {
    type Err = Error;
    fn from_str(input: &str) -> Result<Self> {
        let (n, d) = input.split_once('/').unwrap_or((input, "1"));
        Self::new(
            n.parse()
                .map_err(|_| Error::Invalid("invalid frame-rate numerator".into()))?,
            d.parse()
                .map_err(|_| Error::Invalid("invalid frame-rate denominator".into()))?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_fractional_rates() -> Result<()> {
        let rate: FrameRate = "30000/1001".parse()?;
        assert_eq!(rate.seconds(30_000), "1001.000000000");
        assert_eq!("60000/2002".parse::<FrameRate>()?, rate);
        assert!("0/1".parse::<FrameRate>().is_err());
        assert!("24/0".parse::<FrameRate>().is_err());
        assert!(serde_json::from_str::<FrameRate>(r#"{"numerator":0,"denominator":1}"#).is_err());
        Ok(())
    }
}
