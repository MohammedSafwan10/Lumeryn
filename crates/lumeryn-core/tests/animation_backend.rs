use lumeryn_core::{
    animation::{Animation, AnimationChannel, AnimationProperty, Interpolation, Keyframe},
    animation_backend::{self, PositionAxis},
    time::FrameRate,
};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
fn animation(property: AnimationProperty, interpolation: Interpolation) -> Animation {
    Animation {
        channels: vec![AnimationChannel {
            property,
            keys: vec![
                Keyframe {
                    at_frame: 0,
                    value: 0,
                    interpolation,
                },
                Keyframe {
                    at_frame: 6,
                    value: if property == AnimationProperty::Opacity {
                        1_000_000
                    } else {
                        12_000
                    },
                    interpolation: Interpolation::Hold,
                },
            ],
        }],
    }
}
#[test]
fn exact_requested_frame_count_and_numeric_piecewise_contract() -> TestResult {
    let rate = FrameRate::new(30, 1)?;
    assert_eq!(
        animation_backend::output_frames(6, rate, FrameRate::new(60, 1)?)?,
        12
    );
    assert_eq!(
        animation_backend::output_frames(6, rate, FrameRate::new(25, 1)?)?,
        5
    );
    assert_eq!(
        animation_backend::output_frames(6, FrameRate::new(30_000, 1001)?, FrameRate::new(60, 1)?)?,
        13
    );
    for interpolation in [
        Interpolation::Hold,
        Interpolation::Linear,
        Interpolation::SmoothStep,
    ] {
        let expression = animation_backend::position(
            &animation(AnimationProperty::X, interpolation),
            6,
            rate,
            3,
            PositionAxis {
                property: AnimationProperty::X,
                canvas_axis: 640,
                project_axis: 320,
                offset: 10,
                fallback: 99,
            },
        )?;
        assert!(expression.contains("t*30/1-3"));
        assert!(expression.contains("*640/320+10"));
        assert!(!expression.contains("nan"));
    }
    let alpha = animation_backend::filters(
        &animation(AnimationProperty::Opacity, Interpolation::Linear),
        6,
        rate,
    )?;
    assert!(alpha.contains("alpha(X,Y)*clip("));
    assert!(alpha.contains("T*30/1"));
    assert!(animation_backend::output_frames(0, rate, rate).is_err());
    let scale = Animation {
        channels: vec![AnimationChannel {
            property: AnimationProperty::ScaleX,
            keys: vec![Keyframe {
                at_frame: 0,
                value: 500_000,
                interpolation: Interpolation::Hold,
            }],
        }],
    };
    assert!(animation_backend::filters(&scale, 6, rate)?.contains("premultiply=inplace=1"));
    assert!(
        animation_backend::filters_at_size(&scale, 6, rate, 3840, 2160)?.contains("eval=frame")
    );
    let grow = Animation {
        channels: vec![AnimationChannel {
            property: AnimationProperty::ScaleX,
            keys: vec![Keyframe {
                at_frame: 0,
                value: 2_000_000,
                interpolation: Interpolation::Hold,
            }],
        }],
    };
    assert!(animation_backend::filters_at_size(&grow, 6, rate, 3840, 2160).is_err());
    assert!(animation_backend::filters_at_size(&grow, 6, rate, 1920, 1080).is_ok());
    assert!(
        animation_backend::validate_supported(
            &animation(AnimationProperty::ScaleX, Interpolation::Linear),
            6
        )
        .is_err()
    );
    Ok(())
}

#[tokio::test]
#[ignore = "root-coordinated native dynamic-scale reconfiguration and pixel acceptance"]
async fn native_scale_quantized_geometry_alpha_and_60_frame_reconfiguration() -> TestResult {
    use lumeryn_core::MediaTools;
    use std::{
        ffi::OsString,
        path::PathBuf,
        time::{Duration, Instant},
    };
    let tools = MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG").map_or_else(|| "ffmpeg".into(), PathBuf::from),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map_or_else(|| "ffprobe".into(), PathBuf::from),
        timeout: Duration::from_secs(60),
    };
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("source.rgba");
    let source = (0..8)
        .flat_map(|y| {
            (0..8).flat_map(move |x| {
                if (2..6).contains(&x) && (2..6).contains(&y) {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 0]
                }
            })
        })
        .collect::<Vec<u8>>();
    std::fs::write(&path, &source)?;
    let make = |x, y| Animation {
        channels: vec![
            AnimationChannel {
                property: AnimationProperty::ScaleX,
                keys: vec![Keyframe {
                    at_frame: 0,
                    value: x,
                    interpolation: Interpolation::Hold,
                }],
            },
            AnimationChannel {
                property: AnimationProperty::ScaleY,
                keys: vec![Keyframe {
                    at_frame: 0,
                    value: y,
                    interpolation: Interpolation::Hold,
                }],
            },
        ],
    };
    for (sx, sy) in [
        (500_000, 500_000),
        (1_000_000, 1_000_000),
        (2_000_000, 2_000_000),
        (500_000, 2_000_000),
    ] {
        let filters =
            animation_backend::filters_at_size(&make(sx, sy), 6, FrameRate::new(30, 1)?, 8, 8)?;
        let mut args = [
            "-v",
            "error",
            "-nostdin",
            "-filter_threads",
            "1",
            "-stream_loop",
            "-1",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            "8x8",
            "-framerate",
            "60",
            "-i",
        ]
        .map(OsString::from)
        .to_vec();
        args.push(path.as_os_str().to_owned());
        let filters = if filters.is_empty() {
            "null".to_owned()
        } else {
            filters
        };
        args.extend([
            "-vf".into(),
            filters.into(),
            "-frames:v".into(),
            "1".into(),
            "-threads".into(),
            "2".into(),
            "-pix_fmt".into(),
            "rgba".into(),
            "-f".into(),
            "rawvideo".into(),
            "pipe:1".into(),
        ]);
        let actual = tools.run(&tools.ffmpeg, &args).await?;
        assert_eq!(actual.len(), source.len());
        if sx == 1_000_000 && sy == 1_000_000 {
            assert_eq!(actual, source);
        } else {
            for pixel in actual.chunks_exact(4) {
                if pixel[3] > 0 {
                    assert!(
                        pixel[0] >= 250 && pixel[1] <= 2 && pixel[2] <= 2,
                        "native alpha fringe {pixel:?}"
                    );
                } else {
                    assert_eq!(&pixel[..3], &[0, 0, 0]);
                }
            }
            assert!(actual[(3 * 8 + 3) * 4 + 3] > 180);
            if sx == 500_000 {
                assert_eq!(actual[(3 * 8) * 4 + 3], 0);
                assert_eq!(actual[(3 * 8 + 7) * 4 + 3], 0);
            }
            if sy == 500_000 {
                assert_eq!(actual[3 * 4 + 3], 0);
                assert_eq!(actual[(7 * 8 + 3) * 4 + 3], 0);
            }
        }
        println!("native_scale_golden {sx}/{sy} {}", blake3::hash(&actual));
        assert_eq!(tools.run(&tools.ffmpeg, &args).await?, actual);
    }
    let dynamic = Animation {
        channels: vec![AnimationChannel {
            property: AnimationProperty::ScaleX,
            keys: vec![
                Keyframe {
                    at_frame: 0,
                    value: 500_000,
                    interpolation: Interpolation::Linear,
                },
                Keyframe {
                    at_frame: 60,
                    value: 2_000_000,
                    interpolation: Interpolation::Hold,
                },
            ],
        }],
    };
    let filters =
        animation_backend::filters_at_size(&dynamic, 60, FrameRate::new(60, 1)?, 1920, 1080)?;
    let args = [
        "-v",
        "error",
        "-nostdin",
        "-filter_threads",
        "1",
        "-f",
        "lavfi",
        "-i",
        "color=red:s=1920x1080:r=60,format=rgba",
        "-vf",
        &filters,
        "-frames:v",
        "60",
        "-threads",
        "2",
        "-f",
        "framemd5",
        "pipe:1",
    ]
    .map(OsString::from)
    .to_vec();
    let started = Instant::now();
    let output = tools.run(&tools.ffmpeg, &args).await?;
    println!(
        "native_scale_1080p_60_frames_ms {}",
        started.elapsed().as_secs_f64() * 1000.
    );
    let output = String::from_utf8(output)?;
    let frames = output
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .collect::<Vec<_>>();
    assert_eq!(frames.len(), 60);
    assert!(frames.iter().all(|line| line.contains("8294400")));
    assert_ne!(
        frames[0].split(',').next_back(),
        frames[10].split(',').next_back()
    );
    Ok(())
}

#[tokio::test]
#[ignore = "root-coordinated actual premultiplied fixed-surface scale pixel acceptance"]
async fn fixed_surface_scale_pixels_alpha_and_seek_match_independent_reference() -> TestResult {
    use lumeryn_core::{MediaTools, animation::RationalFrame};
    use std::{ffi::OsString, path::PathBuf, time::Duration};
    let tools = MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG").map_or_else(|| "ffmpeg".into(), PathBuf::from),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map_or_else(|| "ffprobe".into(), PathBuf::from),
        timeout: Duration::from_secs(60),
    };
    let directory = tempfile::tempdir()?;
    let input = directory.path().join("alpha.rgba");
    let mut source = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            source.extend_from_slice(if (2..6).contains(&x) && (2..6).contains(&y) {
                &[255, 0, 0, 255]
            } else {
                &[0, 0, 255, 0]
            });
        }
    }
    std::fs::write(&input, &source)?;
    let scale = |x, y, dynamic| Animation {
        channels: vec![
            AnimationChannel {
                property: AnimationProperty::ScaleX,
                keys: if dynamic {
                    vec![
                        Keyframe {
                            at_frame: 0,
                            value: 500_000,
                            interpolation: Interpolation::Linear,
                        },
                        Keyframe {
                            at_frame: 6,
                            value: 2_000_000,
                            interpolation: Interpolation::Hold,
                        },
                    ]
                } else {
                    vec![Keyframe {
                        at_frame: 0,
                        value: x,
                        interpolation: Interpolation::Hold,
                    }]
                },
            },
            AnimationChannel {
                property: AnimationProperty::ScaleY,
                keys: vec![Keyframe {
                    at_frame: 0,
                    value: y,
                    interpolation: Interpolation::Hold,
                }],
            },
        ],
    };
    let cases = [
        scale(500_000, 500_000, false),
        scale(1_000_000, 1_000_000, false),
        scale(2_000_000, 2_000_000, false),
        scale(500_000, 2_000_000, false),
        scale(500_000, 1_000_000, true),
    ];
    for (animation, timeline_fps, output_fps, at) in cases
        .into_iter()
        .map(|animation| (animation, 30, 60, 0))
        .chain([(scale(500_000, 1_000_000, true), 24, 25, 1)])
    {
        let filters = animation_backend::filters(&animation, 6, FrameRate::new(timeline_fps, 1)?)?;
        let clock = timeline_fps * output_fps;
        let timing = format!(
            "settb=expr=1/{clock},setpts=N*{timeline_fps}-{}",
            at * output_fps
        );
        let mut args = [
            "-v",
            "error",
            "-nostdin",
            "-filter_threads",
            "1",
            "-stream_loop",
            "-1",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            "8x8",
            "-framerate",
            "OUTPUT_RATE",
            "-i",
        ]
        .map(OsString::from)
        .to_vec();
        let rate_index = args
            .iter()
            .position(|value| value == "OUTPUT_RATE")
            .ok_or("missing rate")?;
        args[rate_index] = output_fps.to_string().into();
        args.push(input.as_os_str().to_owned());
        let filters = if filters.is_empty() {
            "null".to_owned()
        } else {
            filters
        };
        args.extend([
            "-vf".into(),
            format!("trim=end_frame=12,{timing},{filters},format=rgba").into(),
            "-frames:v".into(),
            "12".into(),
            "-threads".into(),
            "2".into(),
            "-pix_fmt".into(),
            "rgba".into(),
            "-f".into(),
            "rawvideo".into(),
            "pipe:1".into(),
        ]);
        let decoded = tools.run(&tools.ffmpeg, &args).await?;
        assert_eq!(decoded.len(), 8 * 8 * 4 * 12);
        for (frame_index, frame) in decoded.chunks_exact(8 * 8 * 4).enumerate() {
            let local = RationalFrame::from_output_frame(
                frame_index as u64,
                FrameRate::new(timeline_fps, 1)?,
                FrameRate::new(output_fps, 1)?,
            )?
            .in_clip(at.into(), 6)?;
            let Some(local) = local else {
                continue;
            };
            let evaluated = animation.evaluate(6, local)?;
            let sx = evaluated[0].1;
            let sy = evaluated[1].1;
            for y in 0..8 {
                for x in 0..8 {
                    let source_x = 3.5 + (x as f64 - 3.5) / sx;
                    let source_y = 3.5 + (y as f64 - 3.5) / sy;
                    let alpha =
                        if !(0.0..=7.0).contains(&source_x) || !(0.0..=7.0).contains(&source_y) {
                            0.0
                        } else {
                            let x0 = source_x.floor() as usize;
                            let y0 = source_y.floor() as usize;
                            let tx = source_x - x0 as f64;
                            let ty = source_y - y0 as f64;
                            let mut alpha = 0.;
                            for (py, wy) in [(y0, 1. - ty), ((y0 + 1).min(7), ty)] {
                                for (px, wx) in [(x0, 1. - tx), ((x0 + 1).min(7), tx)] {
                                    alpha += f64::from(source[(py * 8 + px) * 4 + 3]) * wx * wy;
                                }
                            }
                            alpha
                        };
                    let pixel = &frame[(y * 8 + x) * 4..][..4];
                    assert!(
                        (f64::from(pixel[3]) - alpha).abs() <= 2.0,
                        "alpha frame{frame_index}({x},{y}) scale{sx},{sy}:{} expected{alpha}",
                        pixel[3]
                    );
                    if !filters.contains("geq=") {
                        assert_eq!(frame, &source);
                    } else if pixel[3] > 0 {
                        assert!(
                            pixel[0] >= 250 && pixel[1] <= 2 && pixel[2] <= 2,
                            "premultiplied fringe {pixel:?}"
                        );
                    } else {
                        assert_eq!(&pixel[..3], &[0, 0, 0]);
                    }
                }
            }
        }
        // Identical request order cannot change the immutable reference's output.
        assert_eq!(tools.run(&tools.ffmpeg, &args).await?, decoded);
        // Backward inspection seeks to an actual earlier output sample, not accumulated animation state.
        let mut seek_args = args.clone();
        let vf = seek_args
            .iter()
            .position(|value| value == "-vf")
            .ok_or("missing vf")?
            + 1;
        seek_args[vf] = format!(
            "trim=end_frame=12,{timing},{filters},trim=start_frame=3:end_frame=4,format=rgba"
        )
        .into();
        let frame_limit = seek_args
            .iter()
            .position(|value| value == "-frames:v")
            .ok_or("missing frame limit")?
            + 1;
        seek_args[frame_limit] = "1".into();
        assert_eq!(
            tools.run(&tools.ffmpeg, &seek_args).await?,
            decoded[3 * 8 * 8 * 4..4 * 8 * 8 * 4]
        );
    }
    Ok(())
}

#[tokio::test]
#[ignore = "root-coordinated FFmpeg output-FPS animation pixel acceptance"]
async fn target_60fps_motion_has_distinct_half_project_frame_samples() -> TestResult {
    use lumeryn_core::{
        MediaTools,
        media::content_hash,
        visual_backend,
        visual_plan::{Picture, PictureRect, PictureSource, VisualPlan},
    };
    use std::{ffi::OsString, path::PathBuf, time::Duration};
    let tools = MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG").map_or_else(|| "ffmpeg".into(), PathBuf::from),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map_or_else(|| "ffprobe".into(), PathBuf::from),
        timeout: Duration::from_secs(60),
    };
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("red.mkv");
    let args = [
        "-v",
        "error",
        "-nostdin",
        "-n",
        "-f",
        "lavfi",
        "-i",
        "color=red:s=8x8:r=30:d=0.2",
        "-frames:v",
        "6",
        "-threads",
        "2",
        "-c:v",
        "ffv1",
    ]
    .map(OsString::from)
    .into_iter()
    .chain([source.as_os_str().to_owned()])
    .collect::<Vec<_>>();
    tools.run(&tools.ffmpeg, &args).await?;
    for (timeline_fps, export_fps, at) in [(30, 60, 0), (24, 25, 1)] {
        for interpolation in [
            Interpolation::Hold,
            Interpolation::Linear,
            Interpolation::SmoothStep,
        ] {
            let motion = animation(AnimationProperty::X, interpolation);
            let plan = VisualPlan::new(
                FrameRate::new(timeline_fps, 1)?,
                0,
                64,
                32,
                vec![Picture {
                    path: source.clone(),
                    digest: content_hash(&source).await?,
                    source: PictureSource::Video { source_in_frame: 0 },
                    at_frame: at,
                    duration_frames: 6,
                    layer: 0,
                    rect: PictureRect {
                        x: 0,
                        y: 8,
                        width: 8,
                        height: 8,
                    },
                    source_width: 8,
                    source_height: 8,
                    transform: Default::default(),
                    effects: Default::default(),
                    animation: motion.clone(),
                    time_effect: Default::default(),
                }],
            )?;
            let output_rate = FrameRate::new(export_fps, 1)?;
            let count =
                animation_backend::output_frames(plan.total_frames, plan.rate, output_rate)?;
            let program = visual_backend::lower_at_rate(&plan, 64, 32, output_rate)?;
            let mut args = ["-v", "error", "-nostdin", "-filter_complex_threads", "1"]
                .map(OsString::from)
                .to_vec();
            args.extend(program.input_args);
            args.extend([
                "-filter_complex".into(),
                program.filters.into(),
                "-map".into(),
                format!("[{}]", program.output_label).into(),
                "-an".into(),
                "-frames:v".into(),
                count.to_string().into(),
                "-threads".into(),
                "2".into(),
                "-pix_fmt".into(),
                "rgba".into(),
                "-f".into(),
                "rawvideo".into(),
                "pipe:1".into(),
            ]);
            let decoded = tools.run(&tools.ffmpeg, &args).await?;
            assert_eq!(decoded.len(), 64 * 32 * 4 * count as usize);
            for (index, frame) in decoded.chunks_exact(64 * 32 * 4).enumerate() {
                let local = lumeryn_core::animation::RationalFrame::from_output_frame(
                    index as u64,
                    plan.rate,
                    output_rate,
                )?
                .in_clip(at, 6)?;
                let Some(local) = local else {
                    assert!(frame.chunks_exact(4).all(|pixel| pixel[0] < 20));
                    continue;
                };
                let evaluated = motion.evaluate(6, local)?;
                let x = evaluated[0].1 as usize;
                let pixels = frame.chunks_exact(4).collect::<Vec<_>>();
                assert!(pixels[10 * 64 + x][0] > 200, "red at sample {index},x{x}");
                if x > 0 {
                    assert!(
                        pixels[10 * 64 + x - 1][0] < 20,
                        "black before sample {index},x{x}"
                    );
                }
            }
        }
    }
    let opacity = animation(AnimationProperty::Opacity, Interpolation::Linear);
    let alpha_plan = VisualPlan::new(
        FrameRate::new(30, 1)?,
        0,
        8,
        8,
        vec![Picture {
            path: source.clone(),
            digest: content_hash(&source).await?,
            source: PictureSource::Video { source_in_frame: 0 },
            at_frame: 0,
            duration_frames: 6,
            layer: 0,
            rect: PictureRect {
                x: 0,
                y: 0,
                width: 8,
                height: 8,
            },
            source_width: 8,
            source_height: 8,
            transform: Default::default(),
            effects: Default::default(),
            animation: opacity,
            time_effect: Default::default(),
        }],
    )?;
    let program = visual_backend::lower_at_rate(&alpha_plan, 8, 8, FrameRate::new(60, 1)?)?;
    let mut args = ["-v", "error", "-nostdin", "-filter_complex_threads", "1"]
        .map(OsString::from)
        .to_vec();
    args.extend(program.input_args);
    args.extend([
        "-filter_complex".into(),
        program.filters.into(),
        "-map".into(),
        format!("[{}]", program.output_label).into(),
        "-an".into(),
        "-frames:v".into(),
        "12".into(),
        "-threads".into(),
        "2".into(),
        "-pix_fmt".into(),
        "rgba".into(),
        "-f".into(),
        "rawvideo".into(),
        "pipe:1".into(),
    ]);
    let decoded = tools.run(&tools.ffmpeg, &args).await?;
    assert_eq!(decoded.len(), 8 * 8 * 4 * 12);
    for (index, frame) in decoded.chunks_exact(8 * 8 * 4).enumerate() {
        let expected = 253.0 * index as f64 / 12.0;
        assert!(
            (f64::from(frame[0]) - expected).abs() <= 3.0,
            "opacity sample {index}:{} vs {expected}",
            frame[0]
        );
    }
    Ok(())
}
