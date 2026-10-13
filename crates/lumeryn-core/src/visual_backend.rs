//! One generated FFmpeg picture reference for headless export and native preview.
use crate::{
    Error, Result,
    visual_plan::{Picture, PictureRect, PictureSource, VisualPlan},
};
use std::{ffi::OsString, io::Write, sync::Arc};

#[derive(Debug, Clone)]
pub struct VisualProgram {
    pub input_args: Vec<OsString>,
    pub filters: String,
    pub output_label: &'static str,
    pub input_count: usize,
    /// Retain until the consuming FFmpeg process exits, including fallback retries.
    pub resources: Option<Arc<tempfile::TempDir>>,
}

pub fn lower(plan: &VisualPlan, width: u32, height: u32) -> Result<VisualProgram> {
    lower_at_rate(plan, width, height, plan.rate)
}

pub fn lower_at_rate(
    plan: &VisualPlan,
    width: u32,
    height: u32,
    output_rate: crate::time::FrameRate,
) -> Result<VisualProgram> {
    if let Some(transition) = &plan.transition {
        return crate::transition_backend::lower(transition, width, height, output_rate);
    }
    // Public plans are useful to offgraph tests; admit them again at the process boundary.
    let plan = VisualPlan::new_with_scenes(
        plan.rate,
        plan.revision,
        plan.width,
        plan.height,
        plan.pictures.clone(),
        plan.scenes.clone(),
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
    let output_frames =
        crate::animation_backend::output_frames(plan.total_frames, plan.rate, output_rate)?;
    let (mut a, mut b) = (plan.rate.numerator(), output_rate.numerator());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    let speed_clock =
        if plan.pictures.iter().any(|picture| {
            picture.time_effect.speed() != crate::time_effects::PlaybackSpeed::Normal
        }) {
            2
        } else {
            1
        };
    let clock =
        u64::from(plan.rate.numerator() / a) * u64::from(output_rate.numerator()) * speed_clock;
    if clock > i32::MAX as u64 {
        return Err(Error::Unsupported(
            "combined animation clock exceeds FFmpeg rational timebase bound".into(),
        ));
    }

    let (canvas_width, canvas_height) = fitted_canvas(plan.width, plan.height, width, height);
    let offset_x = (width - canvas_width) / 2;
    let offset_y = (height - canvas_height) / 2;
    let resources = if plan.scenes.is_empty() {
        None
    } else {
        Some(Arc::new(
            tempfile::Builder::new()
                .prefix("lumeryn-scene-")
                .tempdir()?,
        ))
    };
    let mut prepared = Vec::new();
    if let Some(directory) = &resources {
        let (font, bytes) = crate::scene_text::audited_font();
        let mut cache = crate::scene_cache::StaticSceneCache::new(
            font,
            bytes,
            crate::scene_cache::DEFAULT_CACHE_BYTES,
            crate::scene_cache::DEFAULT_CACHE_ENTRIES,
        )
        .map_err(|error| Error::Unsupported(error.to_string()))?;
        for (index, scene) in plan.scenes.iter().enumerate() {
            // Strict aspect layout is intentional: reject rounded letterboxing dimensions
            // rather than upscale a proxy or change logical wrapping.
            let raster = cache
                .render_cached(&scene.document, canvas_width, canvas_height)
                .map_err(|error| Error::Unsupported(error.to_string()))?;
            let path = directory.path().join(format!("scene-{index}.rgba"));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            file.write_all(&raster.pixels)?;
            drop(file);
            prepared.push(Picture {
                time_effect: Default::default(),
                animation: scene.animation.clone(),
                path,
                digest: String::new(),
                source: PictureSource::Image,
                at_frame: scene.at_frame,
                duration_frames: scene.duration_frames,
                layer: scene.layer,
                rect: PictureRect {
                    x: 0,
                    y: 0,
                    width: plan.width,
                    height: plan.height,
                },
                source_width: canvas_width,
                source_height: canvas_height,
                transform: Default::default(),
                effects: Default::default(),
            });
        }
    }
    let mut inputs = plan
        .pictures
        .iter()
        .map(|picture| (picture, false))
        .chain(prepared.iter().map(|picture| (picture, true)))
        .collect::<Vec<_>>();
    inputs.sort_by_key(|(picture, _)| (picture.layer, picture.at_frame));
    let mut input_args = Vec::new();
    let mut filters = vec![format!(
        "color=c=black:s={width}x{height}:r={},trim=end_frame={},settb=expr={}/{},setpts=N,format=rgba[lumeryn_picture_base]",
        output_rate,
        output_frames,
        output_rate.denominator(),
        output_rate.numerator()
    )];
    let mut previous = "lumeryn_picture_base".to_owned();
    for (index, (picture, raw_scene)) in inputs.iter().enumerate() {
        input_args.extend([
            "-threads".into(),
            "2".into(),
            "-protocol_whitelist".into(),
            "file".into(),
        ]);
        if *raw_scene {
            input_args.extend([
                "-stream_loop".into(),
                "-1".into(),
                "-f".into(),
                "rawvideo".into(),
                "-pixel_format".into(),
                "rgba".into(),
                "-video_size".into(),
                format!("{canvas_width}x{canvas_height}").into(),
                "-framerate".into(),
                plan.rate.to_string().into(),
            ]);
        } else if picture.source == PictureSource::Image {
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
        let (source_in, end, hold) = match (picture.source, picture.time_effect) {
            (
                PictureSource::Video { .. },
                crate::time_effects::TimeEffect::Freeze { source_frame },
            ) => (
                source_frame,
                source_frame
                    .checked_add(1)
                    .ok_or_else(|| Error::Invalid("freeze endpoint overflow".into()))?,
                format!(",tpad=stop_mode=clone:stop={}", picture.duration_frames),
            ),
            (source, effect) => {
                let source_in = match source {
                    PictureSource::Video { source_in_frame } => source_in_frame,
                    PictureSource::Image => 0,
                };
                let (n, d) = effect.speed().ratio();
                let consumed = picture
                    .duration_frames
                    .checked_mul(n)
                    .ok_or_else(|| Error::Invalid("speed range overflow".into()))?
                    .div_ceil(d);
                (
                    source_in,
                    source_in
                        .checked_add(consumed)
                        .ok_or_else(|| Error::Invalid("source endpoint overflow".into()))?,
                    String::new(),
                )
            }
        };
        let (speed_n, speed_d) = picture.time_effect.speed().ratio();
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
        let transform_filters = if *raw_scene {
            "format=rgba".into()
        } else if picture.transform.is_identity() {
            format!(
                "format=rgba,scale={rectangle_width}:{rectangle_height}:force_original_aspect_ratio=decrease:flags=lanczos"
            )
        } else {
            picture.transform.lower_filters(
                picture.source_width,
                picture.source_height,
                rectangle_width as u32,
                rectangle_height as u32,
            )?
        };
        let effects = picture
            .effects
            .lower_filters(plan.rate, picture.duration_frames)?
            .map_or_else(String::new, |filters| format!(",{filters}"));
        let animation_filters = crate::animation_backend::filters(
            &picture.animation,
            picture.duration_frames,
            plan.rate,
        )?;
        let animation_filters = if animation_filters.is_empty() {
            String::new()
        } else {
            format!(",{animation_filters}")
        };
        let offset = picture
            .at_frame
            .checked_mul(u64::from(plan.rate.denominator()))
            .and_then(|value| value.checked_mul(clock / u64::from(plan.rate.numerator())))
            .filter(|offset| *offset <= (1_u64 << 53))
            .ok_or_else(|| {
                Error::Unsupported("animation timestamp exceeds exact expression range".into())
            })?;
        let timing = if output_rate == plan.rate && speed_n == speed_d {
            format!(
                "settb=expr={}/{},setpts=N",
                plan.rate.denominator(),
                plan.rate.numerator()
            )
        } else {
            // Align pictures to global output samples before local animation evaluation.
            // Prefix holds are invisible until admission's half-open interval begins.
            let source_tick =
                u64::from(plan.rate.denominator()) * clock / u64::from(plan.rate.numerator());
            format!(
                "settb=expr=1/{clock},setpts=N*{source_tick}*{speed_d}/{speed_n}+{offset},fps=fps={output_rate}:start_time=0:round=up:eof_action=pass,settb=expr=1/{clock},setpts=PTS-{offset}"
            )
        };
        filters.push(format!("[{index}:v:0]trim=start_frame={source_in}:end_frame={end}{hold},{timing},{transform_filters}{effects}{animation_filters},settb=expr=1/{clock},setpts=PTS+{offset},setsar=1[lumeryn_picture_{index}]"));
        let next = format!("lumeryn_picture_mix_{index}");
        let visible_end = picture.at_frame + picture.duration_frames;
        let x = crate::animation_backend::position(
            &picture.animation,
            picture.duration_frames,
            plan.rate,
            picture.at_frame,
            crate::animation_backend::PositionAxis {
                property: crate::animation::AnimationProperty::X,
                canvas_axis: canvas_width,
                project_axis: plan.width,
                offset: offset_x,
                fallback: x,
            },
        )?;
        let y = crate::animation_backend::position(
            &picture.animation,
            picture.duration_frames,
            plan.rate,
            picture.at_frame,
            crate::animation_backend::PositionAxis {
                property: crate::animation::AnimationProperty::Y,
                canvas_axis: canvas_height,
                project_axis: plan.height,
                offset: offset_y,
                fallback: y,
            },
        )?;
        // Compare integer-valued rational sample products at every requested output sample.
        let enable = if output_rate == plan.rate {
            format!("gte(n,{})*lt(n,{visible_end})", picture.at_frame)
        } else {
            format!(
                "gte(n*{}*{},{}*{}*{})*lt(n*{}*{},{visible_end}*{}*{})",
                output_rate.denominator(),
                plan.rate.numerator(),
                picture.at_frame,
                output_rate.numerator(),
                plan.rate.denominator(),
                output_rate.denominator(),
                plan.rate.numerator(),
                output_rate.numerator(),
                plan.rate.denominator()
            )
        };
        filters.push(format!("[{previous}][lumeryn_picture_{index}]overlay=x='{x}+({rectangle_width}-overlay_w)/2':y='{y}+({rectangle_height}-overlay_h)/2':format=rgb:alpha=straight:eof_action=pass:repeatlast=0:enable='{enable}'[{next}]"));
        previous = next;
    }
    filters.push(format!(
        "[{previous}]trim=end_frame={},setsar=1[lumeryn_visual_output]",
        output_frames
    ));
    Ok(VisualProgram {
        input_args,
        filters: filters.join(";"),
        output_label: "lumeryn_visual_output",
        input_count: inputs.len(),
        resources,
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
                time_effect: Default::default(),
                animation: Default::default(),
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
                source_width: 128,
                source_height: 72,
                transform: Default::default(),
                effects: Default::default(),
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
