use lumeryn_core::MediaTools;
use lumeryn_preview::{AuditionPlayer, AuditionState};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn missing_source_fails_and_stop_replaces_latest_request() -> TestResult {
    let player = AuditionPlayer::new(MediaTools::default())?;
    let dir = tempfile::tempdir()?;
    player.play(dir.path().join("missing.wav"), None)?;
    let until = Instant::now() + Duration::from_secs(3);
    while player.status().state == AuditionState::Loading && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(player.status().state, AuditionState::Failed);
    assert!(player.status().error.is_some());
    player.stop()?;
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(player.status().state, AuditionState::Idle);
    assert_eq!(player.status().generation, 2);
    Ok(())
}

#[test]
#[ignore = "requires installed FFmpeg/FFprobe; actual native output device or explicit unavailable state"]
fn standalone_audio_audition_clock_bounds_digest_and_replacement() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.lumeryn-scratch/media");
    std::fs::create_dir_all(&root)?;
    let dir = tempfile::tempdir_in(root)?;
    let path = dir.path().join("tone.wav");
    let tools = MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG").map_or_else(|| "ffmpeg".into(), PathBuf::from),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map_or_else(|| "ffprobe".into(), PathBuf::from),
        ..Default::default()
    };
    let result = std::process::Command::new(&tools.ffmpeg)
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-nostdin",
            "-n",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-t",
            "1",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&path)
        .output()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let source = std::fs::read(&path)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let digest = runtime.block_on(lumeryn_core::media::content_hash(&path))?;
    let player = AuditionPlayer::new(tools)?;
    player.set_volume(0.0)?;
    player.play(path.clone(), Some("0".repeat(64)))?;
    let until = Instant::now() + Duration::from_secs(5);
    while player.status().state == AuditionState::Loading && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(player.status().state, AuditionState::Failed);
    assert!(
        player
            .status()
            .error
            .is_some_and(|error| error.contains("digest"))
    );
    player.play(path.clone(), Some(digest))?;
    let until = Instant::now() + Duration::from_secs(8);
    let mut advanced = false;
    loop {
        let status = player.status();
        assert!(status.buffered_samples <= status.capacity_samples);
        assert!(status.position_millis <= 30_000);
        advanced |= status.position_millis > 0;
        match status.state {
            AuditionState::Finished => {
                assert!(advanced);
                assert!((980..=1020).contains(&status.position_millis));
                assert!(status.decoded_samples > 0);
                break;
            }
            AuditionState::Unavailable => {
                assert!(status.error.is_some());
                println!("AUDITION_NATIVE_UNAVAILABLE {:?}", status.error);
                break;
            }
            AuditionState::Failed => {
                return Err(format!("audition failed: {:?}", status.error).into());
            }
            _ => {}
        }
        if Instant::now() > until {
            return Err("audition did not finish within bounded deadline".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    player.play(path.clone(), None)?;
    player.stop()?;
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(player.status().state, AuditionState::Idle);
    assert_eq!(player.status().generation, 4);
    assert_eq!(std::fs::read(path)?, source);
    Ok(())
}
