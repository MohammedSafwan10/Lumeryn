//! Real local UHD exports; run explicitly with --include-ignored. No downloaded/customer media.
use lumeryn_core::{
    MediaTools, Project,
    media::content_hash,
    model::{Clip, Media},
    render::{self, Acceleration, RenderOptions, RenderQuality, RenderResolution, VideoCodec},
    time::FrameRate,
};
use std::{
    ffi::OsString,
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
        timeout: Duration::from_secs(90),
    }
}
fn scratch() -> TestResult<tempfile::TempDir> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.lumeryn-scratch/media");
    std::fs::create_dir_all(&root)?;
    Ok(tempfile::Builder::new()
        .prefix("uhd-export-")
        .tempdir_in(root)?)
}
async fn fixture(directory: &Path, width: u32, height: u32) -> TestResult<Project> {
    let source = directory.join(format!("source-{width}x{height}.mkv"));
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
            "testsrc2=size={width}x{height}:rate=60:duration=0.2"
        ))
        .args([
            "-f",
            "lavfi",
            "-i",
            "aevalsrc=if(between(t\\,0.05\\,0.06)\\,0.8*sin(2*PI*1000*t)\\,0):s=48000:d=0.2",
            "-c:v",
            "ffv1",
            "-threads",
            "2",
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
    let metadata = tools().probe(&source).await?;
    let mut project = Project::new(
        "Real UHD fixture".into(),
        width,
        height,
        FrameRate::new(60, 1)?,
    )?;
    let media_id = Uuid::new_v4();
    project.media.push(Media {
        id: media_id,
        path: source,
        metadata,
    });
    project.clips.push(Clip {
        id: Uuid::new_v4(),
        media_id,
        track_id: project.video_track_id,
        at_frame: 0,
        source_in_frame: 0,
        duration_frames: 12,
    });
    project.validate()?;
    Ok(project)
}
async fn audio_acceptance(output: &Path, expected_seconds: f64) -> TestResult {
    let bytes = tools()
        .run(
            &tools().ffmpeg,
            &[
                "-v".into(),
                "error".into(),
                "-nostdin".into(),
                "-i".into(),
                output.as_os_str().to_owned(),
                "-map".into(),
                "0:a:0".into(),
                "-ac".into(),
                "1".into(),
                "-f".into(),
                "f32le".into(),
                "pipe:1".into(),
            ],
        )
        .await?;
    let samples = bytes
        .chunks_exact(4)
        .map(|s| f32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .collect::<Vec<_>>();
    let duration = samples.len() as f64 / 48_000.0;
    assert!(
        (duration - expected_seconds).abs() <= 1024.0 / 48_000.0 + 1.0 / 48_000.0,
        "AAC duration {duration} vs {expected_seconds}"
    );
    let total_energy: f64 = samples.iter().map(|sample| f64::from(sample.abs())).sum();
    assert!(total_energy > 1.0);
    let center = samples
        .iter()
        .enumerate()
        .map(|(i, s)| i as f64 / 48_000.0 * f64::from(s.abs()))
        .sum::<f64>()
        / total_energy;
    assert!(
        (center - 0.055).abs() < 0.004,
        "audio marker center {center}"
    );
    println!(
        "audio decoded_seconds={duration:.6} marker_center={center:.6} expected_seconds={expected_seconds:.6}"
    );
    Ok(())
}
async fn pixels(output: &Path) -> TestResult<Vec<u8>> {
    pixels_filter(output, "scale=192:108,format=rgb24").await
}
async fn pixels_filter(output: &Path, filter: &str) -> TestResult<Vec<u8>> {
    let args = ["-v", "error", "-nostdin", "-i"]
        .into_iter()
        .map(OsString::from)
        .chain([
            output.as_os_str().to_owned(),
            "-map".into(),
            "0:v:0".into(),
            "-an".into(),
            "-frames:v".into(),
            "1".into(),
            "-vf".into(),
            filter.into(),
            "-f".into(),
            "rawvideo".into(),
            "pipe:1".into(),
        ])
        .collect::<Vec<_>>();
    Ok(tools().run(&tools().ffmpeg, &args).await?)
}

#[tokio::test]
#[ignore = "verifies full-resolution UHD source detail survives a 1080p editing canvas"]
async fn uhd_source_detail_survives_lower_resolution_project_canvas() -> TestResult {
    let directory = scratch()?;
    let native = fixture(directory.path(), 3840, 2160).await?;
    let mut low_canvas = native.clone();
    low_canvas.width = 1920;
    low_canvas.height = 1080;
    let options = RenderOptions {
        resolution: RenderResolution::Uhd2160,
        fps: Some(30),
        ..Default::default()
    };
    let mut decoded = Vec::new();
    for (name, project) in [("native", &native), ("low-canvas", &low_canvas)] {
        let output = directory.path().join(format!("{name}.mp4"));
        let result = render::export_with_options(
            project,
            &directory.path().join("project.lumeryn"),
            &output,
            &tools(),
            &options,
        )
        .await?;
        assert!(!result.upscaled);
        decoded.push(pixels_filter(&output, "format=gray").await?);
        if name == "low-canvas" && std::env::var_os("LUMERYN_UHD_RETAIN_EVIDENCE").is_some() {
            let evidence = directory
                .path()
                .parent()
                .ok_or("scratch parent absent")?
                .join(format!("uhd-evidence-{}", Uuid::new_v4()));
            std::fs::create_dir(&evidence)?;
            let retained = evidence.join("final-source-uhd.mp4");
            std::fs::copy(&output, &retained)?;
            let image = evidence.join("decoded-detail.png");
            tools()
                .run(
                    &tools().ffmpeg,
                    &[
                        "-v".into(),
                        "error".into(),
                        "-nostdin".into(),
                        "-n".into(),
                        "-i".into(),
                        retained.as_os_str().to_owned(),
                        "-frames:v".into(),
                        "1".into(),
                        "-vf".into(),
                        "crop=768:432:0:0".into(),
                        image.as_os_str().to_owned(),
                    ],
                )
                .await?;
            println!(
                "retained actual UHD output and independently decoded detail: {}",
                evidence.display()
            );
        }
    }
    assert_eq!(decoded[0].len(), 3840 * 2160);
    assert_eq!(
        decoded[0], decoded[1],
        "1080p canvas must not erase final-source UHD detail"
    );
    let baseline = directory.path().join("native.mp4");
    let lost_detail =
        pixels_filter(&baseline, "scale=1920:1080,scale=3840:2160,format=gray").await?;
    let changed = decoded[0]
        .iter()
        .zip(&lost_detail)
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        changed > 10_000,
        "fixture must contain fine detail susceptible to intermediate scaling"
    );
    println!(
        "UHD 1080p-canvas final-source detail: exact 8294400-pixel match; deliberately downscaled reference changed {changed} pixels"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "generates real 3840x2160/2160x3840 sources and eight actual UHD outputs"]
async fn uhd_both_orientations_all_output_fps_audio_pixels_and_identity() -> TestResult {
    let directory = scratch()?;
    for (width, height) in [(3840, 2160), (2160, 3840)] {
        let project = fixture(directory.path(), width, height).await?;
        let source = &project.media[0].path;
        let original_hash = content_hash(source).await?;
        let reference = pixels(source).await?;
        for fps in [24, 25, 30, 60] {
            let output = directory
                .path()
                .join(format!("uhd-{width}x{height}-{fps}.mp4"));
            let options = RenderOptions {
                resolution: RenderResolution::Uhd2160,
                fps: Some(fps),
                quality: RenderQuality::High,
                ..Default::default()
            };
            let started = Instant::now();
            let result = render::export_with_options(
                &project,
                &directory.path().join("project.lumeryn"),
                &output,
                &tools(),
                &options,
            )
            .await?;
            assert_eq!((result.width, result.height), (width, height));
            assert_eq!(result.frames, (12_u64 * u64::from(fps)).div_ceil(60));
            assert_eq!(result.frame_rate, format!("{fps}/1"));
            assert!(result.verified_full_decode);
            assert!(!result.upscaled);
            audio_acceptance(&output, result.duration_seconds).await?;
            let decoded = pixels(&output).await?;
            assert_eq!(decoded.len(), reference.len());
            let error = decoded
                .iter()
                .zip(&reference)
                .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
                .sum::<f64>()
                / decoded.len() as f64;
            assert!(error < 10.0, "decoded picture error {error}");
            println!(
                "actual UHD {width}x{height} fps={fps} encoder={} frames={} wall_ms={:.2} picture_mae={error:.3}",
                result.encoder,
                result.frames,
                started.elapsed().as_secs_f64() * 1000.0
            );
            assert!(
                render::export_with_options(
                    &project,
                    &directory.path().join("project.lumeryn"),
                    &output,
                    &tools(),
                    &options
                )
                .await
                .is_err()
            );
            assert_eq!(content_hash(source).await?, original_hash);
        }
    }
    Ok(())
}

#[tokio::test]
#[ignore = "generates actual UHD H264/H265 via installed software and auto encoder probes"]
async fn actual_uhd_h264_h265_cpu_and_auto() -> TestResult {
    let directory = scratch()?;
    let project = fixture(directory.path(), 3840, 2160).await?;
    for codec in [VideoCodec::H264, VideoCodec::H265] {
        for acceleration in [Acceleration::Cpu, Acceleration::Auto] {
            let output = directory
                .path()
                .join(format!("{codec:?}-{acceleration:?}.mp4"));
            let options = RenderOptions {
                resolution: RenderResolution::Uhd2160,
                fps: Some(30),
                codec,
                quality: RenderQuality::Draft,
                acceleration,
                bitrate_kbps: Some(25_000),
            };
            let started = Instant::now();
            let result = render::export_with_options(
                &project,
                &directory.path().join("project.lumeryn"),
                &output,
                &tools(),
                &options,
            )
            .await?;
            assert_eq!(
                (result.width, result.height, result.frames),
                (3840, 2160, 6)
            );
            audio_acceptance(&output, result.duration_seconds).await?;
            println!(
                "codec={codec:?} acceleration={acceleration:?} encoder={} hardware={} fallback={:?} wall_ms={:.2}",
                result.encoder,
                result.hardware,
                result.fallback_reason,
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
    Ok(())
}

#[tokio::test]
#[ignore = "injects trusted hardware failures and verifies actual UHD CPU fallback files"]
async fn auto_falls_back_after_probe_and_runtime_hardware_failures() -> TestResult {
    let directory = scratch()?;
    let project = fixture(directory.path(), 3840, 2160).await?;
    let marker = directory.path().join("allow-probe");
    let source = directory.path().join("ffmpeg_failure.rs");
    let executable = directory.path().join(if cfg!(windows) {
        "ffmpeg_failure.exe"
    } else {
        "ffmpeg_failure"
    });
    // This adapter deliberately simulates hardware failures only. All CPU encoding,
    // generated source decoding and final verification use the real configured tool.
    let adapter = format!(
        r#"
fn main() {{
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let hardware = args.windows(2).any(|w| w[0] == "-c:v" && w[1].to_string_lossy().ends_with("_nvenc"));
    if args.iter().any(|a| a == "-encoders") {{
        println!(" V..... h264_nvenc simulated\n V..... libx264 actual");
        return;
    }}
    if hardware {{
        if std::path::Path::new({marker:?}).exists() && args.windows(2).any(|w| w[0] == "-f" && w[1] == "null") {{ return; }}
        eprintln!("injected hardware unavailable");
        std::process::exit(42);
    }}
    match std::process::Command::new({ffmpeg:?}).args(&args).status() {{
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => {{ eprintln!("{{error}}"); std::process::exit(1); }}
    }}
}}
"#,
        marker = marker.to_string_lossy(),
        ffmpeg = tools().ffmpeg.to_string_lossy()
    );
    std::fs::write(&source, adapter)?;
    let compiled = Command::new("rustc")
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()?;
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let mut injected_tools = tools();
    injected_tools.ffmpeg = executable;
    for runtime_failure in [false, true] {
        if runtime_failure {
            std::fs::write(&marker, "trusted simulated probe accepted")?;
        }
        let output = directory
            .path()
            .join(format!("fallback-{runtime_failure}.mp4"));
        let options = RenderOptions {
            resolution: RenderResolution::Uhd2160,
            fps: Some(30),
            codec: VideoCodec::H264,
            acceleration: Acceleration::Auto,
            quality: RenderQuality::Draft,
            ..Default::default()
        };
        let result = render::export_with_options(
            &project,
            &directory.path().join("project.lumeryn"),
            &output,
            &injected_tools,
            &options,
        )
        .await?;
        assert_eq!(result.encoder, "libx264");
        assert!(!result.hardware);
        let reason = result
            .fallback_reason
            .ok_or("missing explicit fallback reason")?;
        assert!(reason.contains(if runtime_failure {
            "Hardware encode"
        } else {
            "Hardware probes failed"
        }));
        assert!(result.verified_full_decode);
        audio_acceptance(&output, result.duration_seconds).await?;
        println!("actual UHD CPU fallback, simulated runtime_failure={runtime_failure}: {reason}");
    }
    Ok(())
}
