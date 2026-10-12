//! One generated FFmpeg picture reference for headless export and native preview.
use crate::{
    Error, Result,
    visual_plan::{PictureSource, VisualPlan},
};
use std::ffi::OsString;

#[derive(Debug, Clone)]
pub struct VisualProgram {
    pub input_args: Vec<OsString>,
    pub filters: String,
    pub output_label: &'static str,
    pub input_count: usize,
}

pub fn lower(plan: &VisualPlan, width: u32, height: u32) -> Result<VisualProgram> {
    // Public plans are useful to offgraph tests; admit them again at the process boundary.
    let plan = VisualPlan::new(
        plan.rate,
        plan.revision,
        plan.width,
        plan.height,
        plan.pictures.clone(),
    )?;
    if !(2..=3840).contains(&width)
        || !(2..=3840).contains(&height)
        || !width.is_multiple_of(2)
        || !height.is_multiple_of(2)
    {
        return Err(Error::Unsupported(
            "picture output requires even dimensions from 2 through 3840".into(),
        ));
    }
    let (canvas_width, canvas_height) = fitted_canvas(plan.width, plan.height, width, height);
    let offset_x = (width - canvas_width) / 2;
    let offset_y = (height - canvas_height) / 2;
    let mut input_args = Vec::new();
    let mut filters = vec![format!(
        "color=c=black:s={width}x{height}:r={},trim=end_frame={},settb=expr={}/{},setpts=N,format=rgba[lumeryn_picture_base]",
        plan.rate,
        plan.total_frames,
        plan.rate.denominator(),
        plan.rate.numerator()
    )];
    let mut previous = "lumeryn_picture_base".to_owned();
    for (index, picture) in plan.pictures.iter().enumerate() {
        input_args.extend([
            "-threads".into(),
            "2".into(),
            "-protocol_whitelist".into(),
            "file".into(),
        ]);
        if picture.source == PictureSource::Image {
            input_args.extend([
                "-f".into(),
                "image2".into(),
                "-pattern_type".into(),
                "none".into(),
                "-loop".into(),
                "1".into(),
                "-framerate".into(),
                plan.rate.to_string().into(),
            ]);
        }
        input_args.extend(["-i".into(), picture.path.as_os_str().to_owned()]);
        let source_in = match picture.source {
            PictureSource::Video { source_in_frame } => source_in_frame,
            PictureSource::Image => 0,
        };
        let end = source_in
            .checked_add(picture.duration_frames)
            .ok_or_else(|| Error::Invalid("picture source endpoint overflow".into()))?;
        let rectangle_width = (u64::from(picture.rect.width) * u64::from(canvas_width)
            / u64::from(plan.width))
        .max(1);
        let rectangle_height = (u64::from(picture.rect.height) * u64::from(canvas_height)
            / u64::from(plan.height))
        .max(1);
        if rectangle_width > 3840 || rectangle_height > 3840 {
            return Err(Error::Unsupported("scaled picture rectangle exceeds the bounded UHD intermediate surface; reduce its project-space size or output resolution".into()));
        }
        let x = i64::from(picture.rect.x) * i64::from(canvas_width) / i64::from(plan.width)
            + i64::from(offset_x);
        let y = i64::from(picture.rect.y) * i64::from(canvas_height) / i64::from(plan.height)
            + i64::from(offset_y);
        filters.push(format!("[{index}:v:0]trim=start_frame={source_in}:end_frame={end},settb=expr={}/{},setpts=N+{},format=rgba,scale={rectangle_width}:{rectangle_height}:force_original_aspect_ratio=decrease:flags=lanczos,setsar=1[lumeryn_picture_{index}]", plan.rate.denominator(), plan.rate.numerator(), picture.at_frame));
        let next = format!("lumeryn_picture_mix_{index}");
        let visible_end = picture.at_frame + picture.duration_frames;
        // n belongs to the main constant-rate canvas, not input timestamps. Exact
        // integer half-open bounds avoid secondary EOF persistence and decimal drift.
        filters.push(format!("[{previous}][lumeryn_picture_{index}]overlay=x='{x}+({rectangle_width}-overlay_w)/2':y='{y}+({rectangle_height}-overlay_h)/2':format=rgb:alpha=straight:eof_action=pass:repeatlast=0:enable='gte(n,{})*lt(n,{visible_end})'[{next}]", picture.at_frame));
        previous = next;
    }
    filters.push(format!(
        "[{previous}]trim=end_frame={},setsar=1[lumeryn_visual_output]",
        plan.total_frames
    ));
    Ok(VisualProgram {
        input_args,
        filters: filters.join(";"),
        output_label: "lumeryn_visual_output",
        input_count: plan.pictures.len(),
    })
}

fn fitted_canvas(project_width: u32, project_height: u32, width: u32, height: u32) -> (u32, u32) {
    if u64::from(width) * u64::from(project_height) <= u64::from(height) * u64::from(project_width)
    {
        (
            width,
            (u64::from(width) * u64::from(project_height) / u64::from(project_width)) as u32,
        )
    } else {
        (
            (u64::from(height) * u64::from(project_width) / u64::from(project_height)) as u32,
            height,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        time::FrameRate,
        visual_plan::{Picture, PictureRect},
    };
    #[test]
    fn paths_never_enter_filter_syntax_and_bounds_are_half_open() -> Result<()> {
        let path = std::path::PathBuf::from("not'a;filter[0].png");
        let plan = VisualPlan::new(
            FrameRate::new(30, 1)?,
            0,
            128,
            72,
            vec![Picture {
                path: path.clone(),
                digest: "fixture".into(),
                source: PictureSource::Image,
                at_frame: 3,
                duration_frames: 7,
                layer: 0,
                rect: PictureRect {
                    x: 4,
                    y: 6,
                    width: 64,
                    height: 36,
                },
            }],
        )?;
        let program = lower(&plan, 256, 144)?;
        assert!(
            program
                .input_args
                .iter()
                .any(|value| value == path.as_os_str())
        );
        assert!(!program.filters.contains("not'a"));
        assert!(program.filters.contains("gte(n,3)*lt(n,10)"));
        assert!(program.filters.contains("scale=128:72"));
        assert!(lower(&plan, 255, 144).is_err());
        let mut oversized = plan.clone();
        oversized.width = 2;
        oversized.height = 2;
        oversized.pictures[0].rect.width = 3840;
        oversized.pictures[0].rect.height = 3840;
        assert!(matches!(
            lower(&oversized, 3840, 2160),
            Err(Error::Unsupported(_))
        ));
        Ok(())
    }
}
