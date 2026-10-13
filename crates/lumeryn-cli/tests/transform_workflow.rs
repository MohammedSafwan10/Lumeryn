//! Actual CLI → service → stored graph → decoded transform acceptance.
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn request(root: &Path, args: &[&str], succeeds: bool) -> TestResult<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .current_dir(root)
        .env_remove("RUST_LOG")
        .args(args)
        .output()?;
    assert_eq!(
        output.status.success(),
        succeeds,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    if succeeds {
        assert!(output.stderr.is_empty());
        let envelope: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(envelope["ok"], true);
        Ok(envelope["result"].clone())
    } else {
        assert!(output.stdout.is_empty());
        let envelope: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(envelope["ok"], false);
        Ok(envelope)
    }
}

fn inspect(root: &Path) -> TestResult<Value> {
    request(
        root,
        &["project", "inspect", "--project", "project.lumeryn"],
        true,
    )
}

fn transform(
    root: &Path,
    id: &str,
    revision: &str,
    extra: &[&str],
    succeeds: bool,
) -> TestResult<Value> {
    let mut args = vec![
        "timeline",
        "visual",
        "transform",
        "--project",
        "project.lumeryn",
        "--clip-id",
        id,
        "--expected-revision",
        revision,
    ];
    args.extend_from_slice(extra);
    request(root, &args, succeeds)
}

fn flip() -> Value {
    json!({"crop":null,"fit":"contain","rotation_millidegrees":0,"pivot_x_basis_points":5000,"pivot_y_basis_points":5000,"flip_horizontal":true,"flip_vertical":false,"opacity_basis_points":10000})
}

#[test]
fn malformed_transform_files_never_change_an_empty_project() -> TestResult {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    request(
        root,
        &[
            "project",
            "create",
            "--project",
            "project.lumeryn",
            "--name",
            "Strict transform",
        ],
        true,
    )?;
    let before = inspect(root)?;
    let history = request(
        root,
        &["project", "history", "--project", "project.lumeryn"],
        true,
    )?;
    let id = "00000000-0000-4000-8000-000000000001";
    let mut unknown = flip();
    unknown["execute"] = json!("unsafe");
    let mut missing = flip();
    missing
        .as_object_mut()
        .ok_or("transform object")?
        .remove("fit");
    for bytes in [
        b"{".to_vec(),
        serde_json::to_vec(&unknown)?,
        serde_json::to_vec(&missing)?,
        vec![b' '; 16 * 1024 + 1],
    ] {
        fs::write(root.join("invalid.json"), bytes)?;
        let error = transform(root, id, "0", &["--transform-file", "invalid.json"], false)?;
        assert_eq!(error["error"]["code"], "invalid_input");
        assert_eq!(inspect(root)?, before);
        assert_eq!(
            request(
                root,
                &["project", "history", "--project", "project.lumeryn"],
                true
            )?,
            history
        );
    }
    Ok(())
}

#[cfg(windows)]
fn media_command(root: &Path, args: &[&str]) -> TestResult<Vec<u8>> {
    let ffmpeg = std::env::var_os("LUMERYN_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    let output = Command::new(ffmpeg)
        .current_dir(root)
        .args(["-v", "error", "-nostdin"])
        .args(args)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

#[cfg(windows)]
fn decode(root: &Path, path: &str) -> TestResult<Vec<u8>> {
    let bytes = media_command(
        root,
        &[
            "-i",
            path,
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ],
    )?;
    assert_eq!(bytes.len(), 128 * 72 * 3);
    Ok(bytes)
}

#[cfg(windows)]
fn assert_color(frame: &[u8], x: usize, y: usize, channel: usize) {
    let pixel = &frame[(y * 128 + x) * 3..(y * 128 + x) * 3 + 3];
    assert!(
        pixel[channel] > 100
            && pixel
                .iter()
                .enumerate()
                .all(|(index, value)| index == channel || *value < 45),
        "pixel ({x},{y}) was {pixel:?}"
    );
}

#[cfg(windows)]
#[test]
#[ignore = "requires trusted FFmpeg/ffprobe and authoritative Windows service"]
fn asymmetric_video_and_png_transform_export_reset_history_and_clipboard() -> TestResult {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    media_command(
        root,
        &[
            "-n",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=128x72:r=30:d=1,drawbox=x=64:y=0:w=64:h=72:color=blue:t=fill",
            "-c:v",
            "ffv1",
            "source.mkv",
        ],
    )?;
    media_command(
        root,
        &[
            "-n",
            "-f",
            "lavfi",
            "-i",
            "color=green:s=64x36,drawbox=x=32:y=0:w=32:h=36:color=red:t=fill,format=rgba",
            "-frames:v",
            "1",
            "still.png",
        ],
    )?;
    let (video_bytes, png_bytes) = (
        fs::read(root.join("source.mkv"))?,
        fs::read(root.join("still.png"))?,
    );
    request(
        root,
        &[
            "project",
            "create",
            "--project",
            "project.lumeryn",
            "--name",
            "Asymmetric transforms",
            "--width",
            "128",
            "--height",
            "72",
            "--fps",
            "30",
        ],
        true,
    )?;
    for (revision, path) in [("0", "source.mkv"), ("1", "still.png")] {
        request(
            root,
            &[
                "media",
                "import",
                "--project",
                "project.lumeryn",
                "--path",
                path,
                "--expected-revision",
                revision,
            ],
            true,
        )?;
    }
    let imported = inspect(root)?;
    let video = imported["media"][0]["id"].as_str().ok_or("video ID")?;
    let png = imported["media"][1]["id"].as_str().ok_or("PNG ID")?;
    request(
        root,
        &[
            "timeline",
            "clip",
            "add",
            "--project",
            "project.lumeryn",
            "--media-id",
            video,
            "--duration-frames",
            "30",
            "--expected-revision",
            "2",
        ],
        true,
    )?;
    request(
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
            "3",
        ],
        true,
    )?;
    let tracks = inspect(root)?;
    let track = tracks["visual_tracks"][0]["id"]
        .as_str()
        .ok_or("track ID")?;
    request(
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
            track,
            "--image",
            "--at-frame",
            "0",
            "--duration-frames",
            "30",
            "--x",
            "80",
            "--y",
            "40",
            "--width",
            "32",
            "--height",
            "18",
            "--expected-revision",
            "4",
        ],
        true,
    )?;
    let before = inspect(root)?;
    let video_clip = before["clips"][0]["id"].as_str().ok_or("video clip ID")?;
    let png_clip = before["visual_clips"][0]["id"]
        .as_str()
        .ok_or("PNG clip ID")?;
    fs::write(root.join("flip.json"), serde_json::to_vec(&flip())?)?;
    transform(
        root,
        video_clip,
        "5",
        &["--transform-file", "flip.json", "--dry-run"],
        true,
    )?;
    assert_eq!(inspect(root)?, before);
    transform(
        root,
        video_clip,
        "5",
        &["--transform-file", "flip.json"],
        true,
    )?;
    let stale = transform(root, video_clip, "5", &["--reset"], false)?;
    assert_eq!(stale["error"]["code"], "revision_conflict");
    transform(
        root,
        png_clip,
        "6",
        &["--transform-file", "flip.json"],
        true,
    )?;
    let styled = inspect(root)?;
    assert_eq!(styled["visual_settings"][video_clip]["transform"], flip());
    assert_eq!(styled["visual_settings"][png_clip]["transform"], flip());
    let copied = request(
        root,
        &[
            "timeline",
            "clip",
            "copy",
            "--project",
            "project.lumeryn",
            "--clip-id",
            video_clip,
        ],
        true,
    )?;
    assert_eq!(
        copied["clipboard"]["version"],
        lumeryn_core::clipboard::VERSION
    );
    assert_eq!(copied["clipboard"]["visual_settings"]["transform"], flip());
    fs::write(
        root.join("clipboard.json"),
        serde_json::to_vec(&copied["clipboard"])?,
    )?;
    // Invalid source crop must fail without changing either graph or history.
    let mut invalid = flip();
    invalid["crop"] = json!({"x":127,"y":0,"width":2,"height":20});
    fs::write(root.join("crop.json"), serde_json::to_vec(&invalid)?)?;
    transform(
        root,
        video_clip,
        "7",
        &["--transform-file", "crop.json"],
        false,
    )?;
    assert_eq!(inspect(root)?, styled);
    let exported = request(
        root,
        &[
            "render",
            "--project",
            "project.lumeryn",
            "--output",
            "flipped.mp4",
            "--expected-revision",
            "7",
        ],
        true,
    )?;
    assert_eq!(exported["verified_full_decode"], true);
    assert_eq!(exported["frames"], 30);
    assert_eq!(exported["frame_rate"], "30/1");
    assert_eq!(
        (exported["width"].as_u64(), exported["height"].as_u64()),
        (Some(128), Some(72))
    );
    assert!((exported["duration_seconds"].as_f64().ok_or("duration")? - 1.0).abs() < 0.000_001);
    let flipped = decode(root, "flipped.mp4")?;
    assert_color(&flipped, 16, 20, 2);
    assert_color(&flipped, 112, 20, 0);
    assert_color(&flipped, 84, 48, 0);
    assert_color(&flipped, 108, 48, 1);
    transform(root, video_clip, "7", &["--reset"], true)?;
    transform(root, png_clip, "8", &["--reset"], true)?;
    assert!(inspect(root)?.get("visual_settings").is_none());
    request(
        root,
        &[
            "render",
            "--project",
            "project.lumeryn",
            "--output",
            "reset.mp4",
            "--expected-revision",
            "9",
        ],
        true,
    )?;
    let reset = decode(root, "reset.mp4")?;
    assert_color(&reset, 16, 20, 0);
    assert_color(&reset, 112, 20, 2);
    assert_color(&reset, 84, 48, 1);
    assert_color(&reset, 108, 48, 0);
    request(
        root,
        &[
            "project",
            "undo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "9",
        ],
        true,
    )?;
    assert_eq!(
        inspect(root)?["visual_settings"][png_clip]["transform"],
        flip()
    );
    request(
        root,
        &[
            "project",
            "redo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "10",
        ],
        true,
    )?;
    assert!(inspect(root)?.get("visual_settings").is_none());
    request(
        root,
        &[
            "timeline",
            "clip",
            "paste",
            "--project",
            "project.lumeryn",
            "--file",
            "clipboard.json",
            "--at-frame",
            "30",
            "--expected-revision",
            "11",
        ],
        true,
    )?;
    let pasted = inspect(root)?;
    let pasted_id = pasted["clips"][1]["id"].as_str().ok_or("pasted ID")?;
    assert_ne!(pasted_id, video_clip);
    assert_eq!(pasted["visual_settings"][pasted_id]["transform"], flip());
    assert_eq!(pasted["revision"], 12);
    let history = request(
        root,
        &["project", "history", "--project", "project.lumeryn"],
        true,
    )?;
    assert_eq!(history.as_array().ok_or("history")?.len(), 13);
    assert_eq!(fs::read(root.join("source.mkv"))?, video_bytes);
    assert_eq!(fs::read(root.join("still.png"))?, png_bytes);
    for (original, current) in imported["media"]
        .as_array()
        .ok_or("original media")?
        .iter()
        .zip(pasted["media"].as_array().ok_or("current media")?)
    {
        let hash = original["metadata"]["blake3"]
            .as_str()
            .ok_or("source hash")?;
        assert_eq!(hash.len(), 64);
        assert_eq!(current["metadata"]["blake3"], hash);
    }
    assert_eq!(
        request(
            root,
            &[
                "project",
                "check",
                "--project",
                "project.lumeryn",
                "--assets"
            ],
            true
        )?["history"],
        "ok"
    );
    Ok(())
}
