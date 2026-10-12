//! Actual native worker composition, seek-generation and headless-reference parity.
use lumeryn_core::{
    MediaTools, Project,
    media::content_hash,
    model::{Clip, Media, VisualClip, VisualPlacement, VisualSource, VisualTrack},
    time::FrameRate,
    visual_backend,
    visual_plan::VisualPlan,
};
use lumeryn_preview::{PreviewConfig, PreviewFrame, PreviewPlayer};
use std::{
    ffi::OsString,
    path::PathBuf,
    time::{Duration, Instant},
};
use uuid::Uuid;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
fn tools() -> MediaTools {
    MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG").map_or_else(|| "ffmpeg".into(), PathBuf::from),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map_or_else(|| "ffprobe".into(), PathBuf::from),
        timeout: Duration::from_secs(15),
    }
}
fn wait_frame(player: &mut PreviewPlayer, index: u64) -> TestResult<PreviewFrame> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let status = player.status();
        if let Some(error) = status.error {
            return Err(error.into());
        }
        if let Some(frame) = player.try_frame()
            && frame.frame_index == index
        {
            return Ok(frame);
        }
        if Instant::now() >= deadline {
            return Err("composition preview timed out".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
#[ignore = "requires configured FFmpeg/ffprobe; headless actual preview worker"]
fn layered_preview_and_backward_seeks_equal_the_shared_reference_pixels() -> TestResult {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("project.lumeryn");
    let runtime = tokio::runtime::Runtime::new()?;
    let mut project = Project::new(
        "Composition preview".into(),
        128,
        72,
        FrameRate::new(30, 1)?,
    )?;
    for (name, color) in [("base", "red"), ("overlay", "blue")] {
        let path = directory.path().join(format!("{name}.mkv"));
        let args = ["-v", "error", "-nostdin", "-n", "-f", "lavfi", "-i"]
            .into_iter()
            .map(OsString::from)
            .chain([
                format!("color=c={color}:s=128x72:r=30:d=1").into(),
                "-c:v".into(),
                "ffv1".into(),
                "-threads".into(),
                "2".into(),
                path.as_os_str().to_owned(),
            ])
            .collect::<Vec<_>>();
        runtime.block_on(tools().run(&tools().ffmpeg, &args))?;
        let mut metadata = runtime.block_on(tools().probe(&path))?;
        metadata.blake3 = Some(runtime.block_on(content_hash(&path))?);
        project.media.push(Media {
            id: Uuid::new_v4(),
            path,
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
        duration_frames: 10,
    });
    let track_id = Uuid::new_v4();
    project.visual_tracks.push(VisualTrack {
        id: track_id,
        name: "Overlay".into(),
    });
    project.visual_clips.push(VisualClip {
        id: Uuid::new_v4(),
        media_id: project.media[1].id,
        track_id,
        at_frame: 5,
        duration_frames: 10,
        source: VisualSource::Video { source_in_frame: 2 },
        placement: Some(VisualPlacement {
            x: 32,
            y: 18,
            width: 64,
            height: 36,
        }),
    });
    let plan = runtime.block_on(VisualPlan::from_project(&project, &database))?;
    let program = visual_backend::lower(&plan, 128, 72)?;
    let mut args = ["-v", "error", "-nostdin", "-filter_complex_threads", "2"]
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    args.extend(program.input_args);
    args.extend([
        "-filter_complex".into(),
        format!(
            "{};[{}]format=rgba[reference]",
            program.filters, program.output_label
        )
        .into(),
        "-map".into(),
        "[reference]".into(),
        "-an".into(),
        "-threads".into(),
        "2".into(),
        "-fps_mode".into(),
        "passthrough".into(),
        "-f".into(),
        "rawvideo".into(),
        "pipe:1".into(),
    ]);
    let reference = runtime.block_on(tools().run(&tools().ffmpeg, &args))?;
    let mut player = PreviewPlayer::new(
        tools(),
        PreviewConfig {
            max_width: 128,
            max_height: 72,
            audio: false,
            ..Default::default()
        },
    )?;
    player.load(project.clone(), database.clone())?;
    assert_eq!(
        &*wait_frame(&mut player, 0)?.rgba,
        &reference[..128 * 72 * 4]
    );
    let mut previous_generation = player.status().generation;
    for index in [5, 14, 4, 10, 0] {
        player.seek(index)?;
        let frame = wait_frame(&mut player, index)?;
        assert!(frame.generation > previous_generation);
        previous_generation = frame.generation;
        let offset = index as usize * 128 * 72 * 4;
        assert_eq!(&*frame.rgba, &reference[offset..offset + 128 * 72 * 4]);
    }
    // A still-only project must load rather than being mistaken for an empty graph.
    let still = directory.path().join("still.png");
    let args = [
        "-v",
        "error",
        "-nostdin",
        "-n",
        "-f",
        "lavfi",
        "-i",
        "color=c=green:s=128x72",
        "-frames:v",
        "1",
        "-threads",
        "2",
    ]
    .into_iter()
    .map(OsString::from)
    .chain([still.as_os_str().to_owned()])
    .collect::<Vec<_>>();
    runtime.block_on(tools().run(&tools().ffmpeg, &args))?;
    let mut metadata = runtime.block_on(tools().probe(&still))?;
    metadata.blake3 = Some(runtime.block_on(content_hash(&still))?);
    let media_id = Uuid::new_v4();
    project.clips.clear();
    project.media = vec![Media {
        id: media_id,
        path: still,
        alias: None,
        metadata,
    }];
    project.visual_clips = vec![VisualClip {
        id: Uuid::new_v4(),
        media_id,
        track_id,
        at_frame: 3,
        duration_frames: 3,
        source: VisualSource::Image {},
        placement: None,
    }];
    project.revision = 1;
    player.load(project, database)?;
    let black = wait_frame(&mut player, 0)?;
    assert_eq!(&black.rgba[..4], [0, 0, 0, 255]);
    player.seek(3)?;
    let green = wait_frame(&mut player, 3)?;
    assert!(green.rgba[1] > 100);
    assert_eq!(green.project_revision, 1);
    Ok(())
}
