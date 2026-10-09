//! Explicit media acceptance: cargo test --test workflow -- --include-ignored
use serde_json::{Value, json};
use std::{
    ffi::OsStr,
    path::Path,
    process::{Command, Output},
};
use uuid::Uuid;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn cli(args: &[&OsStr]) -> Result<Output, std::io::Error> {
    Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .args(args)
        .env_remove("RUST_LOG")
        .output()
}

fn success(args: &[&OsStr]) -> Result<Value, Box<dyn std::error::Error>> {
    let output = cli(args)?;
    assert!(
        output.status.success(),
        "CLI stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "unexpected diagnostics");
    let envelope: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(envelope["ok"], true);
    Ok(envelope["result"].clone())
}

fn ffmpeg(args: &[&OsStr]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let tool = std::env::var_os("LUMERYN_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    let output = Command::new(tool).args(args).output()?;
    assert!(
        output.status.success(),
        "FFmpeg: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

fn source(path: &Path) -> TestResult {
    ffmpeg(&[
        "-v".as_ref(),
        "error".as_ref(),
        "-nostdin".as_ref(),
        "-f".as_ref(),
        "lavfi".as_ref(),
        "-i".as_ref(),
        "color=red:s=320x180:r=30:d=1".as_ref(),
        "-f".as_ref(),
        "lavfi".as_ref(),
        "-i".as_ref(),
        "color=green:s=320x180:r=30:d=1".as_ref(),
        "-f".as_ref(),
        "lavfi".as_ref(),
        "-i".as_ref(),
        "color=blue:s=320x180:r=30:d=1".as_ref(),
        "-f".as_ref(),
        "lavfi".as_ref(),
        "-i".as_ref(),
        "sine=frequency=440:sample_rate=48000:duration=3".as_ref(),
        "-filter_complex".as_ref(),
        "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]".as_ref(),
        "-map".as_ref(),
        "[v]".as_ref(),
        "-map".as_ref(),
        "3:a".as_ref(),
        "-c:v".as_ref(),
        "ffv1".as_ref(),
        "-c:a".as_ref(),
        "pcm_s16le".as_ref(),
        path.as_os_str(),
    ])?;
    Ok(())
}

#[test]
#[ignore = "requires installed FFmpeg/ffprobe; generated fixtures only"]
fn create_import_place_trim_export_reopen_and_verify() -> TestResult {
    let dir = tempfile::tempdir()?;
    let source_path = dir.path().join("source with spaces & symbols.mkv");
    source(&source_path)?;
    let original = std::fs::read(&source_path)?;
    let project = dir.path().join("project.lumeryn");
    let output = dir.path().join("trimmed.mp4");
    let created = success(&[
        "project".as_ref(),
        "create".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--name".as_ref(),
        "Generated acceptance".as_ref(),
        "--width".as_ref(),
        "320".as_ref(),
        "--height".as_ref(),
        "180".as_ref(),
    ])?;
    assert_eq!(created["sqlite_version"], "3.51.3");
    let media = Uuid::new_v4().to_string();
    let import_args = [
        "media".as_ref(),
        "import".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--path".as_ref(),
        source_path.as_os_str(),
        "--id".as_ref(),
        media.as_ref(),
        "--expected-revision".as_ref(),
        "0".as_ref(),
        "--idempotency-key".as_ref(),
        "import-fixture".as_ref(),
    ];
    assert_eq!(success(&import_args)?["revision"], 1);
    let clip = Uuid::new_v4().to_string();
    assert_eq!(
        success(&[
            "timeline".as_ref(),
            "clip".as_ref(),
            "add".as_ref(),
            "--project".as_ref(),
            project.as_os_str(),
            "--id".as_ref(),
            clip.as_ref(),
            "--media-id".as_ref(),
            media.as_ref(),
            "--duration-frames".as_ref(),
            "90".as_ref(),
            "--expected-revision".as_ref(),
            "1".as_ref()
        ])?["revision"],
        2
    );
    assert_eq!(
        success(&[
            "timeline".as_ref(),
            "clip".as_ref(),
            "trim".as_ref(),
            "--project".as_ref(),
            project.as_os_str(),
            "--clip-id".as_ref(),
            clip.as_ref(),
            "--source-in-frame".as_ref(),
            "15".as_ref(),
            "--duration-frames".as_ref(),
            "30".as_ref(),
            "--expected-revision".as_ref(),
            "2".as_ref()
        ])?["revision"],
        3
    );
    let snapshot = success(&[
        "project".as_ref(),
        "open".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
    ])?;
    assert_eq!(snapshot["clips"][0]["source_in_frame"], 15);
    assert_eq!(snapshot["clips"][0]["duration_frames"], 30);
    assert_eq!(
        snapshot["media"][0]["path"],
        source_path
            .file_name()
            .ok_or("source filename")?
            .to_string_lossy()
            .as_ref()
    );
    let rendered = success(&[
        "render".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--output".as_ref(),
        output.as_os_str(),
        "--expected-revision".as_ref(),
        "3".as_ref(),
    ])?;
    assert_eq!(rendered["frames"], 30);
    assert_eq!(rendered["verified_full_decode"], true);
    // Verify every lossy output frame across the precise red → green source boundary.
    let pixels = ffmpeg(&[
        "-v".as_ref(),
        "error".as_ref(),
        "-i".as_ref(),
        output.as_os_str(),
        "-map".as_ref(),
        "0:v:0".as_ref(),
        "-vf".as_ref(),
        "scale=1:1".as_ref(),
        "-pix_fmt".as_ref(),
        "rgb24".as_ref(),
        "-f".as_ref(),
        "rawvideo".as_ref(),
        "-".as_ref(),
    ])?;
    assert_eq!(pixels.len(), 30 * 3);
    for (index, rgb) in pixels.chunks_exact(3).enumerate() {
        if index < 15 {
            assert!(
                rgb[0] > 200 && rgb[1] < 30 && rgb[2] < 30,
                "red frame {index}: {rgb:?}"
            );
        } else {
            assert!(
                rgb[0] < 30 && rgb[1] > 90 && rgb[2] < 30,
                "green frame {index}: {rgb:?}"
            );
        }
    }
    let audio = ffmpeg(&[
        "-v".as_ref(),
        "error".as_ref(),
        "-i".as_ref(),
        output.as_os_str(),
        "-map".as_ref(),
        "0:a:0".as_ref(),
        "-ac".as_ref(),
        "1".as_ref(),
        "-f".as_ref(),
        "s16le".as_ref(),
        "-".as_ref(),
    ])?;
    assert!(
        (48_000..=49_024).contains(&(audio.len() / 2)),
        "AAC sample count {}",
        audio.len() / 2
    );
    assert!(
        audio
            .chunks_exact(2)
            .any(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs() > 1000)
    );
    assert_eq!(std::fs::read(&source_path)?, original);
    let exported = std::fs::read(&output)?;
    let existing = cli(&[
        "render".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--output".as_ref(),
        output.as_os_str(),
    ])?;
    assert!(!existing.status.success());
    assert!(existing.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&existing.stderr)?["error"]["code"],
        "already_exists"
    );
    assert_eq!(std::fs::read(&output)?, exported);
    let missing = dir.path().join("source-renamed.mkv");
    std::fs::rename(&source_path, &missing)?;
    assert_eq!(success(&import_args)?["replayed"], true);
    let unpublished = dir.path().join("must-not-exist.mp4");
    assert!(
        !cli(&[
            "render".as_ref(),
            "--project".as_ref(),
            project.as_os_str(),
            "--output".as_ref(),
            unpublished.as_os_str()
        ])?
        .status
        .success()
    );
    assert!(!unpublished.exists());
    assert_eq!(
        success(&[
            "project".as_ref(),
            "inspect".as_ref(),
            "--project".as_ref(),
            project.as_os_str()
        ])?["revision"],
        3
    );
    std::fs::rename(&missing, &source_path)?;
    let mut changed = original.clone();
    changed[0] ^= 1;
    std::fs::write(&source_path, changed)?;
    let rejected = cli(&[
        "render".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--output".as_ref(),
        unpublished.as_os_str(),
    ])?;
    assert_eq!(
        serde_json::from_slice::<Value>(&rejected.stderr)?["error"]["code"],
        "invalid_input"
    );
    assert!(!unpublished.exists());
    std::fs::write(&source_path, original)?;
    Ok(())
}

#[test]
#[ignore = "requires installed FFmpeg/ffprobe; generated fixtures only"]
fn fractional_rate_silent_contiguous_batch_export() -> TestResult {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("fractional.mkv");
    ffmpeg(&[
        "-v".as_ref(),
        "error".as_ref(),
        "-f".as_ref(),
        "lavfi".as_ref(),
        "-i".as_ref(),
        "color=blue:s=320x180:r=30000/1001".as_ref(),
        "-frames:v".as_ref(),
        "60".as_ref(),
        "-c:v".as_ref(),
        "ffv1".as_ref(),
        source.as_os_str(),
    ])?;
    let project = dir.path().join("fractional.lumeryn");
    success(&[
        "project".as_ref(),
        "create".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--name".as_ref(),
        "fractional".as_ref(),
        "--fps".as_ref(),
        "30000/1001".as_ref(),
        "--width".as_ref(),
        "320".as_ref(),
        "--height".as_ref(),
        "180".as_ref(),
    ])?;
    let media = Uuid::new_v4();
    let batch = json!({"operations":[{"operation":"import","id":media,"path":source},
        {"operation":"add_clip","id":Uuid::new_v4(),"media_id":media,"at_frame":0,"source_in_frame":0,"duration_frames":15},
        {"operation":"add_clip","id":Uuid::new_v4(),"media_id":media,"at_frame":15,"source_in_frame":30,"duration_frames":15}]});
    let file = dir.path().join("batch.json");
    std::fs::write(&file, serde_json::to_vec(&batch)?)?;
    assert_eq!(
        success(&[
            "project".as_ref(),
            "batch".as_ref(),
            "--project".as_ref(),
            project.as_os_str(),
            "--file".as_ref(),
            file.as_os_str(),
            "--dry-run".as_ref()
        ])?["revision"],
        0
    );
    assert_eq!(
        success(&[
            "project".as_ref(),
            "batch".as_ref(),
            "--project".as_ref(),
            project.as_os_str(),
            "--file".as_ref(),
            file.as_os_str(),
            "--expected-revision".as_ref(),
            "0".as_ref()
        ])?["revision"],
        1
    );
    let output = dir.path().join("fractional.mp4");
    let rendered = success(&[
        "render".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--output".as_ref(),
        output.as_os_str(),
    ])?;
    assert_eq!(rendered["frames"], 30);
    assert_eq!(rendered["frame_rate"], "30000/1001");
    Ok(())
}
