//! Numeric-only FFmpeg expressions matching bounded clip-local keyframe semantics.
use crate::{
    Error, Result,
    animation::{Animation, AnimationChannel, AnimationProperty, Interpolation},
    model::MAX_FRAMES,
    time::FrameRate,
};

/// Reject unsupported channels before launching any rendering process.
pub fn validate_supported(animation: &Animation, duration: u64) -> Result<()> {
    animation.validate(duration)?;
    Ok(())
}

pub fn output_frames(frames: u64, source: FrameRate, output: FrameRate) -> Result<u64> {
    let numerator =
        u128::from(frames) * u128::from(source.denominator()) * u128::from(output.numerator());
    let denominator = u128::from(source.numerator()) * u128::from(output.denominator());
    u64::try_from(numerator.div_ceil(denominator))
        .ok()
        .filter(|frames| *frames > 0 && *frames <= MAX_FRAMES)
        .ok_or_else(|| Error::Invalid("animation output duration exceeds supported bounds".into()))
}

#[derive(Clone, Copy)]
enum Clock {
    Filter,
    FilterUpper,
    Overlay { at: u64 },
}
fn expression(channel: &AnimationChannel, rate: FrameRate, clock: Clock) -> Result<String> {
    let divisor = match channel.property {
        AnimationProperty::X | AnimationProperty::Y | AnimationProperty::Rotation => 1000,
        _ => 1_000_000,
    };
    let frame = match clock {
        Clock::Filter => format!("(t*{}/{})", rate.numerator(), rate.denominator()),
        Clock::FilterUpper => format!("(T*{}/{})", rate.numerator(), rate.denominator()),
        Clock::Overlay { at } => {
            format!("(t*{}/{}-{at})", rate.numerator(), rate.denominator())
        }
    };
    let first = channel
        .keys
        .first()
        .ok_or_else(|| Error::Invalid("empty animation channel".into()))?;
    let last = channel
        .keys
        .last()
        .ok_or_else(|| Error::Invalid("empty animation channel".into()))?;
    let mut tail = format!("({}/{divisor})", last.value);
    for pair in channel.keys.windows(2).rev() {
        let left = &pair[0];
        let right = &pair[1];
        let progress = format!(
            "(({frame}-{})/{})",
            left.at_frame,
            right.at_frame - left.at_frame
        );
        let eased = match left.interpolation {
            Interpolation::Hold => "0".into(),
            Interpolation::Linear => progress,
            Interpolation::SmoothStep => format!("({progress}*{progress}*(3-2*{progress}))"),
        };
        let segment = format!(
            "(({}+({})*{eased})/{divisor})",
            left.value,
            right.value - left.value
        );
        tail = format!("if(lt({frame},{}),{segment},{tail})", right.at_frame);
    }
    Ok(format!(
        "if(lt({frame},{}),({}/{divisor}),{tail})",
        first.at_frame, first.value
    ))
}

/// X/Y are absolute logical positions, scaled into the output canvas. Missing channels use static coordinates.
pub struct PositionAxis {
    pub property: AnimationProperty,
    pub canvas_axis: u32,
    pub project_axis: u32,
    pub offset: u32,
    pub fallback: i64,
}
pub fn position(
    animation: &Animation,
    duration: u64,
    rate: FrameRate,
    at: u64,
    axis: PositionAxis,
) -> Result<String> {
    let PositionAxis {
        property,
        canvas_axis,
        project_axis,
        offset,
        fallback,
    } = axis;
    validate_supported(animation, duration)?;
    if !matches!(property, AnimationProperty::X | AnimationProperty::Y) || project_axis == 0 {
        return Err(Error::Invalid("animation position axis".into()));
    }
    animation
        .channels
        .iter()
        .find(|channel| channel.property == property)
        .map_or_else(
            || Ok(fallback.to_string()),
            |channel| {
                Ok(format!(
                    "({}*{canvas_axis}/{project_axis}+{offset})",
                    expression(channel, rate, Clock::Overlay { at })?
                ))
            },
        )
}

/// CPU inverse-map conformance reference. Product rendering uses `filters_at_size`.
pub fn filters(animation: &Animation, duration: u64, rate: FrameRate) -> Result<String> {
    validate_supported(animation, duration)?;
    let mut result = Vec::new();
    let scales = [AnimationProperty::ScaleX, AnimationProperty::ScaleY].map(|property| {
        animation
            .channels
            .iter()
            .find(|channel| channel.property == property)
    });
    let dynamic_scale = scales
        .iter()
        .flatten()
        .any(|channel| channel.keys.iter().any(|key| key.value != 1_000_000));
    let rotation = animation
        .channels
        .iter()
        .find(|channel| channel.property == AnimationProperty::Rotation);
    let dynamic_rotation =
        rotation.is_some_and(|channel| channel.keys.iter().any(|key| key.value != 0));
    if dynamic_scale || dynamic_rotation {
        result.extend(["format=gbrap".into(), "premultiply=inplace=1".into()]);
        if dynamic_scale {
            let axis = |channel: Option<&AnimationChannel>| -> Result<String> {
                channel.map_or_else(
                    || Ok("1".into()),
                    |channel| expression(channel, rate, Clock::FilterUpper),
                )
            };
            let sx = format!("((W-1)/2+(X-(W-1)/2)/({}))", axis(scales[0])?);
            let sy = format!("((H-1)/2+(Y-(H-1)/2)/({}))", axis(scales[1])?);
            let valid = format!("between({sx},0,W-1)*between({sy},0,H-1)");
            let channel = |name: &str, function: &str| {
                format!("{name}='if({valid},{function}({sx},{sy}),0)'")
            };
            result.push(format!(
                "geq={}:{}:{}:{}:interpolation=bilinear",
                channel("r", "r"),
                channel("g", "g"),
                channel("b", "b"),
                channel("a", "alpha")
            ));
        }
        if let Some(channel) = rotation.filter(|_| dynamic_rotation) {
            result.push(format!(
                "format=rgba,rotate=angle='({})*PI/180':ow=iw:oh=ih:c=none:bilinear=1,format=gbrap",
                expression(channel, rate, Clock::Filter)?
            ));
        }
        result.extend(["unpremultiply=inplace=1".into(), "format=rgba".into()]);
    }
    if let Some(channel) = animation
        .channels
        .iter()
        .find(|channel| channel.property == AnimationProperty::Opacity)
    {
        result.push(format!(
            "format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*clip({},0,1)'",
            expression(channel, rate, Clock::FilterUpper)?
        ));
    }
    Ok(result.join(","))
}

/// Native fixed-surface scale: round requested physical size, centre with integer
/// pad/crop offsets, then clip to the original placement bounds. No oversized surfaces.
pub fn filters_at_size(
    animation: &Animation,
    duration: u64,
    rate: FrameRate,
    width: u32,
    height: u32,
) -> Result<String> {
    validate_supported(animation, duration)?;
    if !(1..=3840).contains(&width) || !(1..=3840).contains(&height) {
        return Err(Error::Unsupported(
            "animation surface must be 1..3840 per dimension".into(),
        ));
    }
    let axes = [
        (AnimationProperty::ScaleX, width),
        (AnimationProperty::ScaleY, height),
    ];
    let mut sizes = Vec::new();
    let mut needs_scale = false;
    for (property, dimension) in axes {
        let channel = animation
            .channels
            .iter()
            .find(|channel| channel.property == property);
        if let Some(channel) = channel {
            let maximum = channel
                .keys
                .iter()
                .map(|key| key.value)
                .max()
                .ok_or_else(|| Error::Invalid("empty scale channel".into()))?;
            // Monotone supported easing cannot overshoot its endpoint extrema.
            let maximum_pixels = (u64::from(dimension) * maximum as u64 + 500_000) / 1_000_000;
            if maximum_pixels > 3840 {
                return Err(Error::Unsupported("animated scale intermediate exceeds 3840 pixels; reduce placement size or scale".into()));
            }
            needs_scale |= channel.keys.iter().any(|key| key.value != 1_000_000);
            sizes.push(format!(
                "max(1,round({dimension}*({})))",
                expression(channel, rate, Clock::Filter)?
            ));
        } else {
            sizes.push(dimension.to_string());
        }
    }
    let mut result = Vec::new();
    if needs_scale {
        result.push(format!("format=gbrap,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=black@0,premultiply=inplace=1,scale=w='{}':h='{}':eval=frame:flags=bilinear,format=gbrap,pad=w='max(iw,{width})':h='max(ih,{height})':x='(ow-iw)/2':y='(oh-ih)/2':color=black@0:eval=frame,crop={width}:{height}:(iw-ow)/2:(ih-oh)/2:exact=1,format=gbrap,unpremultiply=inplace=1,format=rgba",sizes[0],sizes[1]));
    }
    let remaining = Animation {
        channels: animation
            .channels
            .iter()
            .filter(|channel| {
                !matches!(
                    channel.property,
                    AnimationProperty::ScaleX | AnimationProperty::ScaleY
                )
            })
            .cloned()
            .collect(),
    };
    let rest = filters(&remaining, duration, rate)?;
    if !rest.is_empty() {
        result.push(rest);
    }
    Ok(result.join(","))
}
