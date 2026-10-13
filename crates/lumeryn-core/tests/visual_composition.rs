//! Synthetic decoded-pixel acceptance for the actual shared picture compiler.
use lumeryn_core::{
    MediaTools, Project,
    media::content_hash,
    time::FrameRate,
    visual_backend,
    visual_plan::{Picture, PictureRect, PictureSource, VisualPlan},
};
use lumeryn_core::{
    model::{Clip, Media, VisualClip, VisualPlacement, VisualSource, VisualTrack},
    render::{self, RenderOptions, RenderResolution},
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
fn tools() -> MediaTools {
    MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG").map_or_else(|| "ffmpeg".into(), PathBuf::from),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map_or_else(|| "ffprobe".into(), PathBuf::from),
        timeout: Duration::from_secs(60),
    }
}
async fn generate(path: &Path, color: &str, png: bool) -> TestResult {
    let mut args = ["-v", "error", "-nostdin", "-n", "-f", "lavfi", "-i"]
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    args.push(
        format!(
            "color=c={color}:s=128x72:r=30:d=1{}",
            if png {
                ",format=rgba,colorchannelmixer=aa=0.5"
            } else {
                ""
            }
        )
        .into(),
    );
    args.extend([
        "-frames:v".into(),
        if png { "1" } else { "30" }.into(),
        "-threads".into(),
        "2".into(),
    ]);
    if !png {
        args.extend(["-c:v".into(), "ffv1".into()]);
    }
    args.push(path.as_os_str().to_owned());
    tools().run(&tools().ffmpeg, &args).await?;
    Ok(())
}
async fn picture(
    path: &Path,
    source: PictureSource,
    layer: usize,
    at_frame: u64,
    duration_frames: u64,
    rect: PictureRect,
) -> TestResult<Picture> {
    Ok(Picture {
        path: path.into(),
        digest: content_hash(path).await?,
        source,
        at_frame,
        duration_frames,
        layer,
        rect,
        source_width: 128,
        source_height: 72,
        transform: Default::default(),
        effects: Default::default(),
        animation: Default::default(),
        time_effect: Default::default(),
    })
}
fn pixel(bytes: &[u8], frame: usize, x: usize, y: usize) -> &[u8] {
    let offset = (frame * 128 * 72 + y * 128 + x) * 4;
    &bytes[offset..offset + 4]
}

#[tokio::test]
#[ignore = "requires configured FFmpeg/ffprobe"]
async fn shared_reference_pixels_cover_layers_gaps_alpha_and_half_open_holds() -> TestResult {
    let directory = tempfile::tempdir()?;
    let red = directory.path().join("red.mkv");
    let blue = directory.path().join("blue.mkv");
    let generated_green = directory.path().join("green.png");
    let green = directory.path().join("green'%03d;.png");
    generate(&red, "red", false).await?;
    generate(&blue, "blue", false).await?;
    generate(&generated_green, "0x00ff00", true).await?;
    std::fs::rename(generated_green, &green)?;
    let full = PictureRect {
        x: 0,
        y: 0,
        width: 128,
        height: 72,
    };
    let plan = VisualPlan::new(
        FrameRate::new(30, 1)?,
        7,
        128,
        72,
        vec![
            picture(
                &red,
                PictureSource::Video { source_in_frame: 0 },
                0,
                0,
                5,
                full,
            )
            .await?,
            picture(
                &blue,
                PictureSource::Video { source_in_frame: 2 },
                1,
                3,
                4,
                PictureRect {
                    x: 32,
                    y: 18,
                    width: 64,
                    height: 36,
                },
            )
            .await?,
            picture(&green, PictureSource::Image, 2, 8, 2, full).await?,
        ],
    )?;
    let program = visual_backend::lower(&plan, 128, 72)?;
    let mut args = ["-v", "error", "-nostdin", "-filter_complex_threads", "2"]
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    args.extend(program.input_args);
    args.extend([
        "-filter_complex".into(),
        format!(
            "{};[{}]format=rgba[pixels]",
            program.filters, program.output_label
        )
        .into(),
        "-map".into(),
        "[pixels]".into(),
        "-an".into(),
        "-threads".into(),
        "2".into(),
        "-fps_mode".into(),
        "passthrough".into(),
        "-f".into(),
        "rawvideo".into(),
        "pipe:1".into(),
    ]);
    let bytes = tools().run(&tools().ffmpeg, &args).await?;
    assert_eq!(bytes.len(), 10 * 128 * 72 * 4);
    for frame in 0..3 {
        assert!(
            pixel(&bytes, frame, 64, 36)[0] > 240,
            "red at {frame}: {:?}",
            pixel(&bytes, frame, 64, 36)
        );
    }
    for frame in 3..7 {
        assert!(
            pixel(&bytes, frame, 64, 36)[2] > 240,
            "blue at {frame}: {:?}",
            pixel(&bytes, frame, 64, 36)
        );
    }
    assert!(pixel(&bytes, 4, 2, 2)[0] > 240);
    assert_eq!(pixel(&bytes, 5, 2, 2), [0, 0, 0, 255]);
    assert_eq!(pixel(&bytes, 7, 64, 36), [0, 0, 0, 255]);
    for frame in 8..10 {
        let value = pixel(&bytes, frame, 64, 36);
        assert!(
            (120..=135).contains(&value[1]),
            "alpha at {frame}: {value:?}"
        );
        assert_eq!(value[3], 255);
    }
    plan.verify_sources().await?;
    // Mutating bytes invalidates the same source proof used in preview/export.
    std::fs::write(&green, b"changed fixture")?;
    assert!(plan.verify_sources().await.is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "requires configured FFmpeg/ffprobe; exports genuine UHD landscape and portrait"]
async fn shared_graph_exports_actual_uhd_all_four_rates_without_omitting_layers() -> TestResult {
    let directory = tempfile::tempdir()?;
    let red = directory.path().join("red.mkv");
    let blue = directory.path().join("blue.mkv");
    let green = directory.path().join("green.png");
    generate(&red, "red", false).await?;
    generate(&blue, "blue", false).await?;
    generate(&green, "0x00ff00", true).await?;
    for portrait in [false, true] {
        let (width, height) = if portrait { (72, 128) } else { (128, 72) };
        let mut project = Project::new(
            "Actual picture export".into(),
            width,
            height,
            FrameRate::new(30, 1)?,
        )?;
        for path in [&red, &blue, &green] {
            let mut metadata = tools().probe(path).await?;
            metadata.blake3 = Some(content_hash(path).await?);
            project.media.push(Media {
                id: Uuid::new_v4(),
                path: path.clone(),
                alias: None,
                metadata,
            });
        }
        project.clips.push(Clip {
            id: Uuid::new_v4(),
            media_id: project.media[0].id,
            track_id: project.video_track_id,
            at_frame: 0,
            source_in_frame: 0,
            duration_frames: 5,
        });
        let video_track = Uuid::new_v4();
        let image_track = Uuid::new_v4();
        project.visual_tracks = vec![
            VisualTrack {
                id: video_track,
                name: "Picture in picture".into(),
            },
            VisualTrack {
                id: image_track,
                name: "Still".into(),
            },
        ];
        project.visual_clips = vec![
            VisualClip {
                id: Uuid::new_v4(),
                media_id: project.media[1].id,
                track_id: video_track,
                at_frame: 3,
                duration_frames: 4,
                source: VisualSource::Video { source_in_frame: 2 },
                placement: Some(VisualPlacement {
                    x: 18,
                    y: 18,
                    width: width / 2,
                    height: height / 2,
                }),
            },
            VisualClip {
                id: Uuid::new_v4(),
                media_id: project.media[2].id,
                track_id: image_track,
                at_frame: 8,
                duration_frames: 2,
                source: VisualSource::Image {},
                placement: None,
            },
        ];
        project.validate()?;
        let database = directory.path().join("project.lumeryn");
        for fps in [24, 25, 30, 60] {
            let output = directory.path().join(format!("{portrait}-{fps}.mp4"));
            let options = RenderOptions {
                resolution: RenderResolution::Uhd2160,
                fps: Some(fps),
                ..Default::default()
            };
            let result =
                render::export_with_options(&project, &database, &output, &tools(), &options)
                    .await?;
            assert_eq!(
                (result.width, result.height),
                if portrait { (2160, 3840) } else { (3840, 2160) }
            );
            assert_eq!(result.frame_rate, format!("{fps}/1"));
            assert_eq!(result.frames, (10 * u64::from(fps)).div_ceil(30));
            assert!(result.verified_full_decode);
            let args = ["-v", "error", "-nostdin", "-i"]
                .into_iter()
                .map(OsString::from)
                .chain([
                    output.as_os_str().to_owned(),
                    "-vf".into(),
                    "scale=128:72,format=rgba".into(),
                    "-an".into(),
                    "-threads".into(),
                    "2".into(),
                    "-f".into(),
                    "rawvideo".into(),
                    "pipe:1".into(),
                ])
                .collect::<Vec<_>>();
            let pixels = tools().run(&tools().ffmpeg, &args).await?;
            assert_eq!(pixels.len(), result.frames as usize * 128 * 72 * 4);
            assert!(pixel(&pixels, 0, 64, 36)[0] > 220);
            let overlay_frame = (4 * fps / 30) as usize;
            let (overlay_x, overlay_y) = if portrait { (64, 28) } else { (50, 36) };
            assert!(
                pixel(&pixels, overlay_frame, overlay_x, overlay_y)[2] > 220,
                "missing moving overlay at {portrait}/{fps}"
            );
            let still_frame = (9 * fps / 30) as usize;
            let still = pixel(&pixels, still_frame, 64, 36);
            assert!(
                (90..=165).contains(&still[1]) && still[0] < 25 && still[2] < 25,
                "missing translucent PNG at {portrait}/{fps}: {still:?}"
            );
            if fps == 30 {
                let gap = pixel(&pixels, 7, 64, 36);
                assert!(gap[..3].iter().all(|value| *value < 5));
            }
            assert!(matches!(
                render::export_with_options(&project, &database, &output, &tools(), &options).await,
                Err(lumeryn_core::Error::Exists(_))
            ));
        }
        VisualPlan::from_project(&project, &database)
            .await?
            .verify_sources()
            .await?;
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires configured FFmpeg/ffprobe; changing-frame source offsets and large-canvas UHD"]
async fn source_frame_offsets_and_large_project_canvas_are_rendered_exactly() -> TestResult {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("numbered-gray.mkv");
    let args = [
        "-v",
        "error",
        "-nostdin",
        "-n",
        "-f",
        "lavfi",
        "-i",
        "nullsrc=s=128x72:r=30:d=1,format=gray,geq=lum='N*7+10'",
        "-c:v",
        "ffv1",
        "-threads",
        "2",
    ]
    .into_iter()
    .map(OsString::from)
    .chain([source.as_os_str().to_owned()])
    .collect::<Vec<_>>();
    tools().run(&tools().ffmpeg, &args).await?;
    let plan = VisualPlan::new(
        FrameRate::new(30, 1)?,
        0,
        128,
        72,
        vec![
            picture(
                &source,
                PictureSource::Video { source_in_frame: 7 },
                0,
                3,
                4,
                PictureRect {
                    x: 0,
                    y: 0,
                    width: 128,
                    height: 72,
                },
            )
            .await?,
        ],
    )?;
    let program = visual_backend::lower(&plan, 128, 72)?;
    let mut args = ["-v", "error", "-nostdin", "-filter_complex_threads", "2"]
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    args.extend(program.input_args);
    args.extend([
        "-filter_complex".into(),
        format!(
            "{};[{}]format=rgba[pixels]",
            program.filters, program.output_label
        )
        .into(),
        "-map".into(),
        "[pixels]".into(),
        "-an".into(),
        "-threads".into(),
        "2".into(),
        "-fps_mode".into(),
        "passthrough".into(),
        "-f".into(),
        "rawvideo".into(),
        "pipe:1".into(),
    ]);
    let composite = tools().run(&tools().ffmpeg, &args).await?;
    let args = ["-v", "error", "-nostdin", "-i"]
        .into_iter()
        .map(OsString::from)
        .chain([
            source.as_os_str().to_owned(),
            "-vf".into(),
            "trim=start_frame=7:end_frame=11,format=rgba".into(),
            "-an".into(),
            "-threads".into(),
            "2".into(),
            "-fps_mode".into(),
            "passthrough".into(),
            "-f".into(),
            "rawvideo".into(),
            "pipe:1".into(),
        ])
        .collect::<Vec<_>>();
    let reference = tools().run(&tools().ffmpeg, &args).await?;
    assert_eq!(&composite[3 * 128 * 72 * 4..], &reference);
    assert_ne!(pixel(&reference, 0, 64, 36), pixel(&reference, 3, 64, 36));
    for frame in 0..3 {
        assert_eq!(pixel(&composite, frame, 64, 36), [0, 0, 0, 255]);
    }

    let still = directory.path().join("large-canvas.png");
    generate(&still, "red", true).await?;
    let mut project = Project::new(
        "Large project-space canvas".into(),
        7680,
        4320,
        FrameRate::new(30, 1)?,
    )?;
    let mut metadata = tools().probe(&still).await?;
    metadata.blake3 = Some(content_hash(&still).await?);
    let media_id = Uuid::new_v4();
    let track_id = Uuid::new_v4();
    project.media.push(Media {
        id: media_id,
        path: still,
        alias: None,
        metadata,
    });
    project.visual_tracks.push(VisualTrack {
        id: track_id,
        name: "Still".into(),
    });
    project.visual_clips.push(VisualClip {
        id: Uuid::new_v4(),
        media_id,
        track_id,
        at_frame: 0,
        duration_frames: 2,
        source: VisualSource::Image {},
        placement: None,
    });
    let database = directory.path().join("project.lumeryn");
    let admitted = VisualPlan::from_project(&project, &database).await?;
    assert_eq!(admitted.pictures[0].rect.width, 7680);
    assert!(
        visual_backend::lower(&admitted, 3840, 2160)?
            .filters
            .contains("scale=3840:2160")
    );
    let result = render::export_with_options(
        &project,
        &database,
        &directory.path().join("uhd.mp4"),
        &tools(),
        &RenderOptions {
            resolution: RenderResolution::Uhd2160,
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(
        (result.width, result.height, result.frames),
        (3840, 2160, 2)
    );
    assert!(result.verified_full_decode);
    Ok(())
}
