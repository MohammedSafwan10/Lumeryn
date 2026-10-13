//! Bounded, numeric-only source transforms shared by preview and export.
use crate::{Error, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FitMode {
    #[default]
    Contain,
    Cover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceCrop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisualTransform {
    pub crop: Option<SourceCrop>,
    pub fit: FitMode,
    /// Clockwise rotation of the fitted destination surface, in thousandths of a degree.
    pub rotation_millidegrees: i32,
    pub pivot_x_basis_points: u16,
    pub pivot_y_basis_points: u16,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    pub opacity_basis_points: u16,
}

impl Default for VisualTransform {
    fn default() -> Self {
        Self {
            crop: None,
            fit: FitMode::Contain,
            rotation_millidegrees: 0,
            pivot_x_basis_points: 5000,
            pivot_y_basis_points: 5000,
            flip_horizontal: false,
            flip_vertical: false,
            opacity_basis_points: 10000,
        }
    }
}

impl VisualTransform {
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(&self, source_width: u32, source_height: u32) -> Result<()> {
        bounded_size(source_width, source_height)?;
        if !(-360000..=360000).contains(&self.rotation_millidegrees)
            || self.pivot_x_basis_points > 10000
            || self.pivot_y_basis_points > 10000
            || self.opacity_basis_points > 10000
        {
            return Err(Error::Invalid(
                "transform rotation, pivot or opacity is outside its fixed-point bounds".into(),
            ));
        }
        if let Some(crop) = self.crop
            && (crop.width == 0
                || crop.height == 0
                || crop
                    .x
                    .checked_add(crop.width)
                    .is_none_or(|end| end > source_width)
                || crop
                    .y
                    .checked_add(crop.height)
                    .is_none_or(|end| end > source_height))
        {
            return Err(Error::Invalid(
                "source crop must be nonempty and inside verified source dimensions".into(),
            ));
        }
        Ok(())
    }

    /// Produces only generated filter syntax, never caller strings or file names.
    /// Output is a fixed target-sized straight-alpha RGBA surface.
    pub fn lower_filters(
        &self,
        source_width: u32,
        source_height: u32,
        target_width: u32,
        target_height: u32,
    ) -> Result<String> {
        self.validate(source_width, source_height)?;
        bounded_size(target_width, target_height)?;
        let mut filters = vec!["format=rgba".to_owned()];
        if let Some(crop) = self.crop {
            filters.push(format!(
                "crop={}:{}:{}:{}:exact=1",
                crop.width, crop.height, crop.x, crop.y
            ));
        }
        if self.flip_horizontal {
            filters.push("hflip".into());
        }
        if self.flip_vertical {
            filters.push("vflip".into());
        }
        // Premultiplied interpolation avoids RGB from fully transparent source pixels
        // contaminating visible glyph/image edges during scaling and arbitrary rotation.
        filters.extend(["format=gbrap".into(), "premultiply=inplace=1".into()]);
        match self.fit {
            FitMode::Contain => filters.push(format!("scale={target_width}:{target_height}:force_original_aspect_ratio=decrease:flags=bilinear,pad={target_width}:{target_height}:(ow-iw)/2:(oh-ih)/2:color=black@0")),
            FitMode::Cover => {
                let crop = self.crop.unwrap_or(SourceCrop { x:0,y:0,width:source_width,height:source_height });
                // Crop before enlargement: a very thin source never allocates a huge
                // cover intermediate. Integer center rounding is explicitly stable.
                let (width,height) = cover_crop(crop.width,crop.height,target_width,target_height);
                filters.push(format!("crop={width}:{height}:{}:{}:exact=1,scale={target_width}:{target_height}:flags=bilinear",(crop.width-width)/2,(crop.height-height)/2));
            }
        }
        let angle = self.rotation_millidegrees.rem_euclid(360000);
        if angle != 0 {
            let centered = self.pivot_x_basis_points == 5000 && self.pivot_y_basis_points == 5000;
            if centered && angle % 90000 == 0 {
                let turn = match angle {
                    90000 => "transpose=clock",
                    180000 => "hflip,vflip",
                    _ => "transpose=cclock",
                };
                filters.push(format!("{turn},pad=max(iw\\,{target_width}):max(ih\\,{target_height}):(ow-iw)/2:(oh-ih)/2:color=black@0,crop={target_width}:{target_height}:(iw-ow)/2:(ih-oh)/2:exact=1"));
            } else if centered {
                let radians = f64::from(angle) * std::f64::consts::PI / 180000.0;
                filters.push(format!(
                    "format=rgba,rotate={radians:.15}:ow=iw:oh=ih:c=black@0:bilinear=1,format=gbrap"
                ));
            } else {
                if target_width > 720 || target_height > 720 {
                    return Err(Error::Unsupported("non-center rotation is limited to 720 pixels per destination dimension by the measured CPU reference; use a centered pivot or a smaller placement/output".into()));
                }
                filters.push(affine_filter(
                    angle,
                    self.pivot_x_basis_points,
                    self.pivot_y_basis_points,
                ));
            }
        }
        filters.extend([
            "unpremultiply=inplace=1".into(),
            "format=rgba".into(),
            "setsar=1".into(),
        ]);
        if self.opacity_basis_points != 10000 {
            filters.push(format!(
                "colorchannelmixer=aa={:.4}",
                f64::from(self.opacity_basis_points) / 10000.0
            ));
        }
        Ok(filters.join(","))
    }
}

fn bounded_size(width: u32, height: u32) -> Result<()> {
    if width == 0 || height == 0 || width > 3840 || height > 3840 {
        return Err(Error::Unsupported(
            "transform source/target surfaces must be 1..3840 pixels per dimension".into(),
        ));
    }
    Ok(())
}

fn cover_crop(width: u32, height: u32, target_width: u32, target_height: u32) -> (u32, u32) {
    if u64::from(width) * u64::from(target_height) > u64::from(height) * u64::from(target_width) {
        (
            ((u64::from(height) * u64::from(target_width) / u64::from(target_height)).max(1))
                as u32,
            height,
        )
    } else {
        (
            width,
            ((u64::from(width) * u64::from(target_height) / u64::from(target_width)).max(1)) as u32,
        )
    }
}

fn affine_filter(angle: i32, pivot_x: u16, pivot_y: u16) -> String {
    let radians = f64::from(angle) * std::f64::consts::PI / 180000.0;
    let (sin, cos) = radians.sin_cos();
    let px = format!("(W-1)*{pivot_x}/10000");
    let py = format!("(H-1)*{pivot_y}/10000");
    let sx = format!("({cos:.15}*(X-({px}))+{sin:.15}*(Y-({py}))+({px}))");
    let sy = format!("(-{sin:.15}*(X-({px}))+{cos:.15}*(Y-({py}))+({py}))");
    let valid = format!("between({sx},0,W-1)*between({sy},0,H-1)");
    let channel =
        |name: &str, function: &str| format!("{name}='if({valid},{function}({sx},{sy}),0)'");
    format!(
        "geq={}:{}:{}:{}:interpolation=bilinear",
        channel("r", "r"),
        channel("g", "g"),
        channel("b", "b"),
        channel("a", "alpha")
    )
}
