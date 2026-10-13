use lumeryn_core::{
    scene_cache::{DEFAULT_CACHE_BYTES, DEFAULT_CACHE_ENTRIES, StaticSceneCache},
    scene_text::{
        FontRef, Rgba8, SceneContent, SceneDocument, SceneNode, ShapeBox, TextAlignment, TextNode,
    },
    time::FrameRate,
    visual_backend,
    visual_plan::{ScenePicture, VisualPlan},
};
use std::{ffi::OsString, path::PathBuf, time::Duration};
use uuid::Uuid;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn font() -> FontRef {
    FontRef {
        sha256: "29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031".into(),
        face_index: 0,
    }
}
fn scene(width: u32, height: u32) -> SceneDocument {
    SceneDocument {
        width,
        height,
        nodes: vec![
            SceneNode {
                id: Uuid::from_u128(1),
                content: SceneContent::Rectangle(ShapeBox {
                    x: 0.,
                    y: 0.,
                    width: width as f32,
                    height: height as f32,
                    fill: Rgba8 {
                        r: 4,
                        g: 9,
                        b: 18,
                        a: 255,
                    },
                }),
            },
            SceneNode {
                id: Uuid::from_u128(2),
                content: SceneContent::Text(TextNode {
                    text: "Lumeryn café".into(),
                    font: font(),
                    x: 20.,
                    y: 20.,
                    width: width as f32 - 40.,
                    height: 80.,
                    font_size: 24.,
                    line_height: 32.,
                    color: Rgba8 {
                        r: 71,
                        g: 210,
                        b: 160,
                        a: 255,
                    },
                    alignment: TextAlignment::Start,
                }),
            },
        ],
    }
}
fn plan(document: SceneDocument) -> lumeryn_core::Result<VisualPlan> {
    VisualPlan::new_with_scenes(
        FrameRate::new(30, 1)?,
        2,
        document.width,
        document.height,
        Vec::new(),
        vec![ScenePicture {
            document,
            layer: 1,
            at_frame: 0,
            duration_frames: 2,
            animation: Default::default(),
        }],
    )
}

#[test]
fn scene_program_owns_exact_resolution_raster_until_last_resource_guard() -> TestResult {
    let plan = plan(scene(320, 180))?;
    let program = visual_backend::lower(&plan, 640, 360)?;
    let directory = program
        .resources
        .as_ref()
        .ok_or("scene resource guard missing")?
        .clone();
    let path = directory.path().join("scene-0.rgba");
    assert_eq!(std::fs::metadata(&path)?.len(), 640 * 360 * 4);
    assert!(!program.filters.contains("scale="));
    assert_eq!(program.input_count, 1);
    assert!(program.input_args.iter().any(|arg| arg == "rawvideo"));
    drop(program);
    assert!(path.exists());
    drop(directory);
    assert!(!path.exists());
    Ok(())
}

#[test]
fn combined_scene_admission_and_rounded_aspect_fail_explicitly() -> TestResult {
    let mut input = plan(scene(320, 180))?;
    let mut overlapping = input.scenes[0].clone();
    overlapping.layer = 2;
    assert!(
        VisualPlan::new_with_scenes(
            input.rate,
            2,
            320,
            180,
            Vec::new(),
            vec![input.scenes[0].clone(), overlapping]
        )
        .is_err()
    );
    input.scenes[0].duration_frames = 0;
    assert!(visual_backend::lower(&input, 640, 360).is_err());
    let awkward = plan(scene(320, 180))?;
    // 642x360 fits an exact640x360 canvas and is valid letterboxing.
    assert!(visual_backend::lower(&awkward, 642, 360).is_ok());
    assert!(visual_backend::lower(&awkward, 642, 362).is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "root-coordinated actual FFmpeg final-resolution scene pixel acceptance"]
async fn exact_uhd_and_portrait_scene_composites_match_reference_pixels() -> TestResult {
    let tools = lumeryn_core::MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG").map_or_else(|| "ffmpeg".into(), PathBuf::from),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map_or_else(|| "ffprobe".into(), PathBuf::from),
        timeout: Duration::from_secs(60),
    };
    for (project_width, project_height, width, height) in
        [(320, 180, 3840, 2160), (180, 320, 2160, 3840)]
    {
        let document = scene(project_width, project_height);
        let plan = plan(document.clone())?;
        let program = visual_backend::lower(&plan, width, height)?;
        let _resources = program.resources.clone();
        let mut args = ["-v", "error", "-nostdin", "-filter_complex_threads", "1"]
            .map(OsString::from)
            .to_vec();
        args.extend(program.input_args);
        let output_directory = tempfile::tempdir()?;
        let output = output_directory.path().join("decoded.rgba");
        args.extend([
            "-filter_complex".into(),
            program.filters.into(),
            "-map".into(),
            format!("[{}]", program.output_label).into(),
            "-frames:v".into(),
            "2".into(),
            "-threads".into(),
            "2".into(),
            "-f".into(),
            "rawvideo".into(),
            "-pix_fmt".into(),
            "rgba".into(),
            output.as_os_str().to_owned(),
        ]);
        tools.run(&tools.ffmpeg, &args).await?;
        let decoded = std::fs::read(output)?;
        let mut renderer = StaticSceneCache::new(
            font(),
            include_bytes!("../../../assets/library/packs/inter.v4.1/Inter[opsz,wght].ttf")
                .to_vec(),
            DEFAULT_CACHE_BYTES,
            DEFAULT_CACHE_ENTRIES,
        )?;
        let expected = renderer.render_cached(&document, width, height)?;
        assert_eq!(decoded.len(), expected.pixels.len() * 2);
        for frame in decoded.chunks_exact(expected.pixels.len()) {
            assert_eq!(frame, expected.pixels.as_ref());
        }
    }
    Ok(())
}
