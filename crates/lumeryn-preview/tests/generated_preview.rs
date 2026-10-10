//! Explicit external-tool tests. Fixtures are generated locally, never downloaded.
use lumeryn_core::{
    MediaTools, Project,
    model::{Clip, Media},
    time::FrameRate,
};
use lumeryn_preview::{AudioStatus, PreviewConfig, PreviewFrame, PreviewPlayer};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
use uuid::Uuid;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn tools() -> MediaTools {
    MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffmpeg".into()),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffprobe".into()),
        timeout: Duration::from_secs(10),
    }
}
fn fixture(rate: FrameRate) -> TestResult<(tempfile::TempDir, Project)> {
    fixture_size(rate, 64, 48)
}
fn fixture_size(
    rate: FrameRate,
    width: u32,
    height: u32,
) -> TestResult<(tempfile::TempDir, Project)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.lumeryn-scratch/media");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("native-preview-")
        .tempdir_in(root)?;
    let source = directory.path().join("source.mkv");
    let output = Command::new(tools().ffmpeg)
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-nostdin",
            "-n",
            "-f",
            "lavfi",
            "-i",
        ])
        .arg(format!(
            "testsrc=size={width}x{height}:rate={}/{}",
            rate.numerator(),
            rate.denominator()
        ))
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-frames:v",
            "60",
            "-c:v",
            "ffv1",
            "-pix_fmt",
            "bgr0",
            "-c:a",
            "pcm_s16le",
            "-shortest",
        ])
        .arg(&source)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let runtime = tokio::runtime::Runtime::new()?;
    let metadata = runtime.block_on(tools().probe(&source))?;
    let mut project = Project::new("Preview fixture".into(), width, height, rate)?;
    let media_id = Uuid::new_v4();
    project.media.push(Media {
        alias: None,
        id: media_id,
        path: PathBuf::from("source.mkv"),
        metadata,
    });
    project.clips.push(Clip {
        id: Uuid::new_v4(),
        media_id,
        track_id: project.video_track_id,
        at_frame: 0,
        source_in_frame: 5,
        duration_frames: 20,
    });
    project.clips.push(Clip {
        id: Uuid::new_v4(),
        media_id,
        track_id: project.video_track_id,
        at_frame: 20,
        source_in_frame: 30,
        duration_frames: 20,
    });
    project.validate()?;
    Ok((directory, project))
}
fn wait_frame(player: &mut PreviewPlayer, wanted: u64) -> TestResult<PreviewFrame> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let status = player.status();
        assert!(status.error.is_none(), "preview error: {:?}", status.error);
        if let Some(frame) = player.try_frame()
            && frame.frame_index == wanted
        {
            return Ok(frame);
        }
        if Instant::now() > deadline {
            return Err(format!("frame {wanted} timed out: {status:?}").into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn reference(source: &Path, source_frame: u64) -> TestResult<Vec<u8>> {
    let output = Command::new(tools().ffmpeg)
        .args(["-hide_banner", "-v", "error", "-nostdin", "-i"])
        .arg(source)
        .args(["-map", "0:v:0", "-an", "-vf"])
        .arg(format!("select=eq(n\\,{source_frame}),format=rgba"))
        .args(["-frames:v", "1", "-f", "rawvideo", "pipe:1"])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

fn wait_idle_generation(player: &PreviewPlayer, generation: u64) -> TestResult {
    let deadline = Instant::now() + Duration::from_secs(3);
    while player.status().generation != generation {
        if Instant::now() > deadline {
            return Err(format!(
                "idle generation {generation} timed out: {:?}",
                player.status()
            )
            .into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

fn assert_idle_cleared(player: &mut PreviewPlayer, revision: u64, generation: u64) {
    let status = player.status();
    assert_eq!(status.project_revision, revision);
    assert_eq!(status.generation, generation);
    assert!(!status.playing);
    assert_eq!(status.position_frame, 0);
    assert_eq!(status.total_frames, 0);
    assert_eq!(status.decoded_frames, 0);
    assert_eq!(status.dropped_frames, 0);
    assert_eq!(status.buffered_frames, 0);
    assert_eq!(status.decode_time_ms, 0.0);
    assert_eq!(status.seek_latency_ms, 0.0);
    assert_eq!(status.audio, AudioStatus::Disabled);
    assert!(status.error.is_none(), "{:?}", status.error);
    assert!(player.try_frame().is_none());
}

#[test]
fn valid_empty_project_is_idle_without_tools_and_recovers_after_invalid_empty() -> TestResult {
    let missing = MediaTools {
        ffmpeg: "missing-lumeryn-empty-ffmpeg".into(),
        ffprobe: "missing-lumeryn-empty-ffprobe".into(),
        timeout: Duration::from_millis(10),
    };
    // Audio is enabled so success also proves no output-device startup is required.
    let mut player = PreviewPlayer::new(missing, PreviewConfig::default())?;
    let mut empty = Project::new("Clean agent project".into(), 1920, 1080, "30".parse()?)?;
    empty.revision = 42;
    let database = PathBuf::from("no-existing-empty-project-directory/project.lumeryn");
    player.load(empty.clone(), database.clone())?;
    wait_idle_generation(&player, 1)?;
    assert_idle_cleared(&mut player, 42, 1);
    player.play()?;
    std::thread::sleep(Duration::from_millis(30));
    assert_idle_cleared(&mut player, 42, 1);
    let mut invalid = empty.clone();
    invalid.revision = 43;
    invalid.width = 3;
    assert!(invalid.validate().is_err());
    player.load(invalid, database.clone())?;
    wait_idle_generation(&player, 2)?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while player.status().error.is_none() {
        assert!(
            Instant::now() < deadline,
            "invalid empty graph was accepted"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(player.status().project_revision, 43);
    assert!(!player.status().playing);
    assert!(player.try_frame().is_none());
    empty.revision = 44;
    player.load(empty, database)?;
    wait_idle_generation(&player, 3)?;
    assert_idle_cleared(&mut player, 44, 3);
    std::thread::sleep(Duration::from_millis(50));
    assert_idle_cleared(&mut player, 44, 3);
    Ok(())
}

#[test]
#[ignore = "requires installed FFmpeg/FFprobe; populated preview cancellation and empty recovery"]
fn populated_to_empty_cancels_decode_resets_metrics_and_reloads_without_stale_frames() -> TestResult
{
    let (directory, mut project) = fixture(FrameRate::new(30, 1)?)?;
    project.revision = 7;
    let mut player = PreviewPlayer::new(
        tools(),
        PreviewConfig {
            audio: false,
            ..Default::default()
        },
    )?;
    let database = directory.path().join("project.lumeryn");
    player.load(project.clone(), database.clone())?;
    wait_frame(&mut player, 0)?;
    player.play()?;
    wait_frame(&mut player, 5)?;
    assert!(player.status().decoded_frames > 0);
    let mut empty = project.clone();
    empty.revision = 8;
    empty.clips.clear();
    empty.validate()?;
    player.load(empty.clone(), database.clone())?;
    wait_idle_generation(&player, 2)?;
    assert_idle_cleared(&mut player, 8, 2);
    std::thread::sleep(Duration::from_millis(80));
    assert_idle_cleared(&mut player, 8, 2);
    project.revision = 9;
    player.load(project.clone(), database.clone())?;
    let frame = wait_frame(&mut player, 0)?;
    assert_eq!(frame.generation, 3);
    assert_eq!(frame.project_revision, 9);
    // Cancel an outstanding plan build as well as a running decode session.
    player.load(project, database.clone())?;
    empty.revision = 10;
    player.load(empty, database)?;
    wait_idle_generation(&player, 5)?;
    assert_idle_cleared(&mut player, 10, 5);
    std::thread::sleep(Duration::from_millis(80));
    assert_idle_cleared(&mut player, 10, 5);
    Ok(())
}

#[test]
#[ignore = "requires installed FFmpeg/FFprobe; generates trusted fixtures"]
fn exact_seek_step_contiguous_clip_frames_and_bounded_paused_queue() -> TestResult {
    let (dir, project) = fixture(FrameRate::new(30, 1)?)?;
    let mut player = PreviewPlayer::new(
        tools(),
        PreviewConfig {
            audio: false,
            ..Default::default()
        },
    )?;
    player.load(project, dir.path().join("project.lumeryn"))?;
    player.seek(22)?;
    let frame = wait_frame(&mut player, 22)?;
    assert_eq!(
        &*frame.rgba,
        reference(&dir.path().join("source.mkv"), 32)?.as_slice()
    );
    assert_eq!((frame.pts_numerator, frame.pts_denominator), (22, 30));
    assert_eq!(frame.rgba.len(), 64 * 48 * 4);
    player.step(-3)?;
    let frame = wait_frame(&mut player, 19)?;
    assert_eq!(
        &*frame.rgba,
        reference(&dir.path().join("source.mkv"), 24)?.as_slice()
    );
    std::thread::sleep(Duration::from_millis(150));
    assert!(player.try_frame().is_none());
    assert!(player.status().buffered_frames <= 5);
    assert_eq!(player.status().audio, AudioStatus::Disabled);
    player.seek(0)?;
    wait_frame(&mut player, 0)?;
    player.play()?;
    let started = Instant::now();
    wait_frame(&mut player, 30)?;
    assert!(
        (Duration::from_millis(850)..Duration::from_millis(1400)).contains(&started.elapsed()),
        "video clock timing {:?}",
        started.elapsed()
    );
    player.pause()?;
    std::thread::sleep(Duration::from_millis(30));
    player.try_frame();
    let at = player.status().position_frame;
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(player.status().position_frame, at);
    player.shutdown();
    Ok(())
}

#[test]
#[ignore = "requires installed FFmpeg/FFprobe; generates fractional-rate fixtures"]
fn fractional_rate_seek_timestamps_are_exact() -> TestResult {
    let (dir, project) = fixture(FrameRate::new(30_000, 1001)?)?;
    let mut player = PreviewPlayer::new(
        tools(),
        PreviewConfig {
            audio: false,
            ..Default::default()
        },
    )?;
    player.load(project, dir.path().join("project.lumeryn"))?;
    player.seek(25)?;
    let frame = wait_frame(&mut player, 25)?;
    assert_eq!(
        (frame.pts_numerator, frame.pts_denominator),
        (25 * 1001, 30_000)
    );
    assert_eq!(
        &*frame.rgba,
        reference(&dir.path().join("source.mkv"), 35)?.as_slice()
    );
    Ok(())
}

#[test]
#[ignore = "requires installed FFmpeg/FFprobe and native output device; no audible-loopback assertion"]
fn native_audio_device_play_pause_seek_smoke() -> TestResult {
    let (dir, project) = fixture(FrameRate::new(30, 1)?)?;
    let mut player = PreviewPlayer::new(tools(), PreviewConfig::default())?;
    player.load(project, dir.path().join("project.lumeryn"))?;
    wait_frame(&mut player, 0)?;
    assert!(
        matches!(player.status().audio, AudioStatus::Ready { .. }),
        "native audio availability: {:?}",
        player.status().audio
    );
    player.play()?;
    let audio_started = Instant::now();
    wait_frame(&mut player, 15)?;
    println!(
        "native_audio status={:?} frame15_elapsed_ms={:.2}",
        player.status().audio,
        audio_started.elapsed().as_secs_f64() * 1000.0
    );
    assert!(
        (Duration::from_millis(350)..Duration::from_millis(1100))
            .contains(&audio_started.elapsed()),
        "native audio clock timing {:?}",
        audio_started.elapsed()
    );
    assert!(matches!(player.status().audio, AudioStatus::Playing { .. }));
    player.pause()?;
    player.seek(22)?;
    wait_frame(&mut player, 22)?;
    player.play()?;
    wait_frame(&mut player, 30)?;
    assert!(player.status().error.is_none());
    let deadline = Instant::now() + Duration::from_secs(3);
    while player.status().playing {
        player.try_frame();
        assert!(
            Instant::now() < deadline,
            "audio EOS stalled: {:?}",
            player.status()
        );
        std::thread::sleep(Duration::from_millis(3));
    }
    Ok(())
}

#[test]
#[ignore = "actual 720p CPU decoding benchmark; generated fixture, debug build"]
fn measured_720p_first_seek_and_eos_latency() -> TestResult {
    let (dir, project) = fixture_size(FrameRate::new(30, 1)?, 1280, 720)?;
    for run in 0..3 {
        let mut player = PreviewPlayer::new(
            tools(),
            PreviewConfig {
                audio: false,
                ..Default::default()
            },
        )?;
        let first_started = Instant::now();
        player.load(project.clone(), dir.path().join("project.lumeryn"))?;
        let first = wait_frame(&mut player, 0)?;
        let first_ms = first_started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(first.rgba.len(), 1280 * 720 * 4);
        let seek_started = Instant::now();
        player.seek(25)?;
        wait_frame(&mut player, 25)?;
        let seek_ms = seek_started.elapsed().as_secs_f64() * 1000.0;
        player.play()?;
        let playback_started = Instant::now();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut max_buffered = 0;
        let mut seen = 0;
        loop {
            if player.try_frame().is_some() {
                seen += 1;
            }
            let status = player.status();
            assert!(status.error.is_none(), "{:?}", status.error);
            max_buffered = max_buffered.max(status.buffered_frames);
            if !status.playing && playback_started.elapsed() > Duration::from_millis(50) {
                println!(
                    "720p run={run} first_ms={first_ms:.2} seek_ms={seek_ms:.2} playback_eos_ms={:.2} last_frame={} seen={} max_buffered={} last_read_ms={:.2} dropped={}",
                    playback_started.elapsed().as_secs_f64() * 1000.0,
                    status.position_frame,
                    seen,
                    max_buffered,
                    status.decode_time_ms,
                    status.dropped_frames
                );
                assert_eq!(status.position_frame, 39);
                assert!(max_buffered <= 5);
                break;
            }
            assert!(Instant::now() < deadline, "video EOS stalled: {status:?}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    Ok(())
}
