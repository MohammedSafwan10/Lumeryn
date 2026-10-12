use serde_json::Value;
use std::{path::Path, process::Command};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
fn call(root: &Path, args: &[&str]) -> TestResult<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .current_dir(root)
        .env_remove("RUST_LOG")
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(serde_json::from_slice::<Value>(&output.stdout)?["result"].clone())
}
fn fail(root: &Path, args: &[&str]) -> TestResult<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .current_dir(root)
        .args(args)
        .output()?;
    assert!(!output.status.success());
    Ok(serde_json::from_slice(&output.stderr)?)
}
#[cfg(windows)]
fn tone_amplitude(samples: &[f32], start_frame: usize, end_frame: usize, frequency: f64) -> f64 {
    let start = start_frame * 48_000 / 30;
    let end = end_frame * 48_000 / 30;
    let (mut real, mut imaginary, mut weights) = (0.0, 0.0, 0.0);
    for (index, sample) in samples[start * 2..end * 2].chunks_exact(2).enumerate() {
        let weight =
            0.5 - 0.5 * (std::f64::consts::TAU * index as f64 / (end - start - 1) as f64).cos();
        let phase = std::f64::consts::TAU * frequency * index as f64 / 48_000.0;
        real += f64::from(sample[0]) * weight * phase.cos();
        imaginary += f64::from(sample[0]) * weight * phase.sin();
        weights += weight;
    }
    2.0 * real.hypot(imaginary) / weights
}
#[test]
fn visual_commands_have_strict_schema_and_complete_rectangle_arguments() -> TestResult {
    let directory = tempfile::tempdir()?;
    let schema = call(directory.path(), &["schema"])?;
    let operations = schema["$defs"]["Edit"]["oneOf"]
        .as_array()
        .ok_or("schema")?;
    for name in [
        "add_visual_track",
        "add_visual_clip",
        "set_visual_range",
        "set_visual_placement",
        "move_clip",
    ] {
        assert!(
            operations
                .iter()
                .any(|o| o["properties"]["operation"]["const"] == name)
        );
    }
    let id = "00000000-0000-0000-0000-000000000001";
    let error = fail(
        directory.path(),
        &[
            "timeline",
            "visual",
            "placement",
            "--project",
            "none.lumeryn",
            "--clip-id",
            id,
            "--x",
            "10",
        ],
    )?;
    assert_eq!(error["error"]["code"], "invalid_input");
    Ok(())
}

#[cfg(windows)]
#[test]
#[ignore = "requires trusted FFmpeg/ffprobe"]
fn real_cli_layer_image_gap_mix_history_and_verified_export() -> TestResult {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    let ffmpeg = std::env::var_os("LUMERYN_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    for (name, color, tone) in [("red.mkv", "red", "440"), ("blue.mkv", "blue", "880")] {
        let status = Command::new(&ffmpeg)
            .current_dir(root)
            .args([
                "-v",
                "error",
                "-nostdin",
                "-n",
                "-f",
                "lavfi",
                "-i",
                &format!("color={color}:s=128x72:r=30:d=2"),
                "-f",
                "lavfi",
                "-i",
                &format!("sine=frequency={tone}:sample_rate=48000:duration=2"),
                "-c:v",
                "ffv1",
                "-c:a",
                "pcm_s16le",
                name,
            ])
            .status()?;
        assert!(status.success());
    }
    assert!(
        Command::new(&ffmpeg)
            .current_dir(root)
            .args([
                "-v",
                "error",
                "-nostdin",
                "-n",
                "-f",
                "lavfi",
                "-i",
                "color=green@0.5:s=64x36,format=rgba",
                "-frames:v",
                "1",
                "still.png"
            ])
            .status()?
            .success()
    );
    call(
        root,
        &[
            "project",
            "create",
            "--project",
            "project.lumeryn",
            "--name",
            "Actual composition",
            "--width",
            "128",
            "--height",
            "72",
        ],
    )?;
    for (rev, name) in [("0", "red.mkv"), ("1", "blue.mkv"), ("2", "still.png")] {
        call(
            root,
            &[
                "media",
                "import",
                "--project",
                "project.lumeryn",
                "--path",
                name,
                "--expected-revision",
                rev,
            ],
        )?;
    }
    let snapshot = call(
        root,
        &["project", "inspect", "--project", "project.lumeryn"],
    )?;
    let red = snapshot["media"][0]["id"].as_str().ok_or("red")?;
    let blue = snapshot["media"][1]["id"].as_str().ok_or("blue")?;
    let png = snapshot["media"][2]["id"].as_str().ok_or("png")?;
    call(
        root,
        &[
            "timeline",
            "clip",
            "add",
            "--project",
            "project.lumeryn",
            "--media-id",
            red,
            "--duration-frames",
            "30",
            "--expected-revision",
            "3",
        ],
    )?;
    call(
        root,
        &[
            "timeline",
            "visual",
            "track-add",
            "--project",
            "project.lumeryn",
            "--name",
            "Picture in picture",
            "--expected-revision",
            "4",
        ],
    )?;
    call(
        root,
        &[
            "timeline",
            "visual",
            "track-add",
            "--project",
            "project.lumeryn",
            "--name",
            "Artwork",
            "--expected-revision",
            "5",
        ],
    )?;
    let layers = call(
        root,
        &["timeline", "inspect", "--project", "project.lumeryn"],
    )?;
    let first = layers["visual_tracks"][0]["id"].as_str().ok_or("layer")?;
    let second = layers["visual_tracks"][1]["id"].as_str().ok_or("layer")?;
    let add = [
        "timeline",
        "visual",
        "add",
        "--project",
        "project.lumeryn",
        "--media-id",
        blue,
        "--track-id",
        first,
        "--at-frame",
        "10",
        "--duration-frames",
        "30",
        "--x",
        "64",
        "--y",
        "0",
        "--width",
        "64",
        "--height",
        "36",
        "--expected-revision",
        "6",
    ];
    let mut dry = add.to_vec();
    dry.push("--dry-run");
    call(root, &dry)?;
    assert_eq!(
        call(
            root,
            &["project", "inspect", "--project", "project.lumeryn"]
        )?["revision"],
        6
    );
    call(root, &add)?;
    let stale = fail(root, &add)?;
    assert_eq!(stale["error"]["code"], "revision_conflict");
    call(
        root,
        &[
            "timeline",
            "visual",
            "add",
            "--project",
            "project.lumeryn",
            "--media-id",
            png,
            "--track-id",
            second,
            "--image",
            "--at-frame",
            "20",
            "--duration-frames",
            "30",
            "--expected-revision",
            "7",
        ],
    )?;
    let timeline = call(
        root,
        &["timeline", "inspect", "--project", "project.lumeryn"],
    )?;
    assert_eq!(timeline["visual_clips"].as_array().ok_or("clips")?.len(), 2);
    assert_eq!(timeline["frame_rate_locked"], true);
    assert!(
        fail(
            root,
            &[
                "media",
                "remove",
                "--project",
                "project.lumeryn",
                "--media-id",
                png,
                "--expected-revision",
                "8"
            ]
        )?["error"]
            .is_object()
    );
    let rendered = call(
        root,
        &[
            "render",
            "--project",
            "project.lumeryn",
            "--output",
            "actual.mp4",
            "--expected-revision",
            "8",
        ],
    )?;
    // CLI render response is the completed background render receipt.
    assert_eq!(rendered["verified_full_decode"], true);
    assert_eq!(rendered["frames"], 50);
    assert_eq!(rendered["frame_rate"], "30/1");
    assert_eq!(
        (rendered["width"].as_u64(), rendered["height"].as_u64()),
        (Some(128), Some(72))
    );
    assert!(
        (rendered["duration_seconds"]
            .as_f64()
            .ok_or("render duration")?
            - 50.0 / 30.0)
            .abs()
            < 0.000_001
    );
    assert!(root.join("actual.mp4").is_file());
    let ffprobe = std::env::var_os("LUMERYN_FFPROBE").unwrap_or_else(|| "ffprobe".into());
    let probe = Command::new(ffprobe).current_dir(root).args(["-v", "error", "-count_frames", "-show_entries", "stream=codec_type,width,height,avg_frame_rate,nb_read_frames,sample_rate,channels,duration:format=duration", "-of", "json", "actual.mp4"]).output()?;
    assert!(
        probe.status.success(),
        "{}",
        String::from_utf8_lossy(&probe.stderr)
    );
    let metadata: Value = serde_json::from_slice(&probe.stdout)?;
    let streams = metadata["streams"].as_array().ok_or("export streams")?;
    let video = streams
        .iter()
        .find(|stream| stream["codec_type"] == "video")
        .ok_or("export video")?;
    assert_eq!(
        (video["width"].as_u64(), video["height"].as_u64()),
        (Some(128), Some(72))
    );
    assert_eq!(video["avg_frame_rate"], "30/1");
    assert_eq!(video["nb_read_frames"], "50");
    assert!(
        (metadata["format"]["duration"]
            .as_str()
            .ok_or("container duration")?
            .parse::<f64>()?
            - 50.0 / 30.0)
            .abs()
            < 0.001
    );
    let audio = streams
        .iter()
        .find(|stream| stream["codec_type"] == "audio")
        .ok_or("export audio")?;
    assert_eq!(audio["sample_rate"], "48000");
    assert_eq!(audio["channels"], 2);
    let decoded = Command::new(&ffmpeg)
        .current_dir(root)
        .args([
            "-v",
            "error",
            "-nostdin",
            "-i",
            "actual.mp4",
            "-map",
            "0:a:0",
            "-f",
            "f32le",
            "pipe:1",
        ])
        .output()?;
    assert!(
        decoded.status.success(),
        "{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    assert_eq!(decoded.stdout.len() % 8, 0);
    let samples = decoded
        .stdout
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect::<Vec<_>>();
    assert!((80_000..=81_024).contains(&(samples.len() / 2)));
    assert!(samples.iter().all(|sample| sample.is_finite()));
    // Interior windows avoid AAC priming and boundary ringing while proving both
    // embedded contributions survive occlusion, and silence follows their endpoints.
    for (start, end, has_440, has_880) in [
        (2, 8, true, false),
        (14, 26, true, true),
        (33, 38, false, true),
        (43, 48, false, false),
    ] {
        for (frequency, present) in [(440.0, has_440), (880.0, has_880)] {
            let amplitude = tone_amplitude(&samples, start, end, frequency);
            if present {
                assert!(
                    (0.06..0.13).contains(&amplitude),
                    "missing {frequency}Hz at frames {start}..{end}: {amplitude}"
                );
            } else {
                assert!(
                    amplitude < 0.003,
                    "unexpected {frequency}Hz at frames {start}..{end}: {amplitude}"
                );
            }
        }
    }
    let silence = &samples[(43 * 48_000 / 30) * 2..(48 * 48_000 / 30) * 2];
    let silence_rms = (silence
        .iter()
        .map(|sample| f64::from(*sample).powi(2))
        .sum::<f64>()
        / silence.len() as f64)
        .sqrt();
    assert!(silence_rms < 0.001, "non-silent tail: {silence_rms}");
    assert_eq!(
        call(
            root,
            &[
                "media",
                "inspect",
                "--project",
                "project.lumeryn",
                "--media-id",
                blue
            ]
        )?["media"]["usage_count"],
        1
    );
    call(
        root,
        &[
            "project",
            "undo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "8",
        ],
    )?;
    assert_eq!(
        call(
            root,
            &["project", "inspect", "--project", "project.lumeryn"]
        )?["visual_clips"]
            .as_array()
            .ok_or("undo")?
            .len(),
        1
    );
    call(
        root,
        &[
            "project",
            "redo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "9",
        ],
    )?;
    assert_eq!(
        call(
            root,
            &["project", "inspect", "--project", "project.lumeryn"]
        )?["visual_clips"]
            .as_array()
            .ok_or("redo")?
            .len(),
        2
    );
    assert_eq!(
        call(
            root,
            &[
                "project",
                "check",
                "--project",
                "project.lumeryn",
                "--assets"
            ]
        )?["sqlite_integrity"],
        serde_json::json!("ok")
    );
    Ok(())
}
