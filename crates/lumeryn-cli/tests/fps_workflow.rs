use serde_json::{Value, json};
use std::{path::Path, process::Command};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn run(directory: &Path, args: &[&str]) -> TestResult<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .current_dir(directory)
        .env_remove("RUST_LOG")
        .args(args)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let envelope: Value = serde_json::from_slice(&output.stdout)?;
    Ok(envelope["result"].clone())
}

#[test]
fn fps_and_background_import_are_discoverable_with_strict_arguments() -> TestResult {
    let directory = tempfile::tempdir()?;
    let capabilities = run(directory.path(), &["capabilities"])?;
    let implemented = capabilities["implemented"]
        .as_array()
        .ok_or("missing capabilities")?;
    for name in ["timeline.fps", "media.import_start", "media.import_status"] {
        assert!(implemented.contains(&json!(name)));
    }
    assert_eq!(
        capabilities["import_options"]["pending_media_editable"],
        false
    );
    let schema = run(directory.path(), &["schema"])?;
    assert!(
        schema["$defs"]["Edit"]["oneOf"]
            .as_array()
            .ok_or("missing operations")?
            .iter()
            .any(|operation| operation["properties"]["operation"]["const"] == "set_frame_rate")
    );
    for args in [
        vec![
            "timeline",
            "fps",
            "--project",
            "missing.lumeryn",
            "--fps",
            "60/0",
        ],
        vec![
            "media",
            "import-start",
            "--project",
            "missing.lumeryn",
            "--path",
            "source.mp4",
        ],
        vec!["media", "import-status", "--project", "missing.lumeryn"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_lumeryn"))
            .current_dir(directory.path())
            .args(args)
            .output()?;
        assert_eq!(output.status.code(), Some(2));
        let error: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(error["error"]["code"], "invalid_arguments");
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn exact_fps_cli_change_supports_dry_run_conflict_and_undo() -> TestResult {
    let directory = tempfile::tempdir()?;
    let directory = directory.path();
    run(
        directory,
        &[
            "project",
            "create",
            "--project",
            "project.lumeryn",
            "--name",
            "FPS review",
        ],
    )?;
    let args = [
        "timeline",
        "fps",
        "--project",
        "project.lumeryn",
        "--fps",
        "60000/1001",
        "--expected-revision",
        "0",
    ];
    let mut dry = args.to_vec();
    dry.push("--dry-run");
    run(directory, &dry)?;
    assert_eq!(
        run(
            directory,
            &["timeline", "inspect", "--project", "project.lumeryn"]
        )?["frame_rate"],
        json!({"numerator":30,"denominator":1})
    );
    run(directory, &args)?;
    let timeline = run(
        directory,
        &["timeline", "inspect", "--project", "project.lumeryn"],
    )?;
    assert_eq!(
        timeline["timeline_frame_rate"],
        json!({"numerator":60000,"denominator":1001})
    );
    assert_eq!(timeline["frame_rate_locked"], false);
    let stale = Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .current_dir(directory)
        .args(args)
        .output()?;
    assert_eq!(stale.status.code(), Some(3));
    run(
        directory,
        &[
            "project",
            "undo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "1",
        ],
    )?;
    assert_eq!(
        run(
            directory,
            &["timeline", "inspect", "--project", "project.lumeryn"]
        )?["frame_rate"],
        json!({"numerator":30,"denominator":1})
    );
    run(
        directory,
        &["project", "check", "--project", "project.lumeryn"],
    )?;
    Ok(())
}

#[cfg(windows)]
#[test]
#[ignore = "requires configured FFmpeg/ffprobe"]
fn background_first_video_matches_exact_source_rate_and_remains_locked_after_placement()
-> TestResult {
    let temporary = tempfile::tempdir()?;
    let directory = temporary.path();
    let output =
        Command::new(std::env::var_os("LUMERYN_FFMPEG").unwrap_or_else(|| "ffmpeg".into()))
            .current_dir(directory)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-n",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=60000/1001:duration=1",
                "-c:v",
                "mpeg4",
                "-threads",
                "2",
                "source.mp4",
            ])
            .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let before = std::fs::read(directory.join("source.mp4"))?;
    run(
        directory,
        &[
            "project",
            "create",
            "--project",
            "project.lumeryn",
            "--name",
            "Source match",
        ],
    )?;
    let media_id = uuid::Uuid::new_v4().to_string();
    let started = run(
        directory,
        &[
            "media",
            "import-start",
            "--project",
            "project.lumeryn",
            "--path",
            "source.mp4",
            "--expected-revision",
            "0",
            "--id",
            &media_id,
            "--idempotency-key",
            "source-match",
            "--match-first-video",
        ],
    )?;
    let job = started["job_id"].as_str().ok_or("missing job ID")?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let completed = loop {
        let status = run(
            directory,
            &[
                "media",
                "import-status",
                "--project",
                "project.lumeryn",
                "--job-id",
                job,
            ],
        )?;
        match status["stage"].as_str() {
            Some("completed") => break status,
            Some("failed" | "cancelled") => {
                return Err(format!("import did not complete: {status}").into());
            }
            _ => {
                if std::time::Instant::now() > deadline {
                    return Err("import status timed out".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    };
    assert_eq!(completed["revision"], 1);
    let timeline = run(
        directory,
        &["timeline", "inspect", "--project", "project.lumeryn"],
    )?;
    assert_eq!(
        timeline["timeline_frame_rate"],
        json!({"numerator":60000,"denominator":1001})
    );
    let media = run(
        directory,
        &[
            "media",
            "inspect",
            "--project",
            "project.lumeryn",
            "--media-id",
            &media_id,
        ],
    )?;
    assert_eq!(
        media["media"]["source_frame_rate"],
        timeline["timeline_frame_rate"]
    );
    assert_eq!(media["media"]["metadata"]["video"]["constant_rate"], true);
    let frames = media["media"]["metadata"]["video"]["frames"]
        .as_u64()
        .ok_or("missing frames")?
        .to_string();
    run(
        directory,
        &[
            "timeline",
            "clip",
            "add",
            "--project",
            "project.lumeryn",
            "--media-id",
            &media_id,
            "--duration-frames",
            &frames,
            "--expected-revision",
            "1",
        ],
    )?;
    let rejected = Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .current_dir(directory)
        .args([
            "timeline",
            "fps",
            "--project",
            "project.lumeryn",
            "--fps",
            "30",
            "--expected-revision",
            "2",
        ])
        .output()?;
    assert_eq!(rejected.status.code(), Some(4));
    let timeline = run(
        directory,
        &["timeline", "inspect", "--project", "project.lumeryn"],
    )?;
    assert_eq!(timeline["revision"], 2);
    assert_eq!(timeline["frame_rate_locked"], true);
    assert_eq!(
        timeline["timeline_frame_rate"],
        json!({"numerator":60000,"denominator":1001})
    );
    run(
        directory,
        &[
            "project",
            "check",
            "--project",
            "project.lumeryn",
            "--assets",
        ],
    )?;
    run(
        directory,
        &[
            "project",
            "undo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "2",
        ],
    )?;
    run(
        directory,
        &[
            "project",
            "undo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "3",
        ],
    )?;
    let restored = run(
        directory,
        &["project", "inspect", "--project", "project.lumeryn"],
    )?;
    assert_eq!(restored["revision"], 4);
    assert_eq!(
        restored["frame_rate"],
        json!({"numerator":30,"denominator":1})
    );
    assert_eq!(restored["media"], json!([]));
    assert_eq!(restored["clips"], json!([]));
    assert_eq!(std::fs::read(directory.join("source.mp4"))?, before);
    Ok(())
}
