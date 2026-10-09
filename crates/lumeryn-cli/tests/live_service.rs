//! Actual CLI service/watch/job contracts. Media case uses generated assets only.
#![cfg(windows)]

use lumeryn_core::{Error, MediaTools};
use serde_json::Value;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
use uuid::Uuid;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn cli(arguments: &[&OsStr]) -> TestResult<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .args(arguments)
        .env_remove("RUST_LOG")
        .output()?)
}

fn success(arguments: &[&OsStr]) -> TestResult<Value> {
    let output = cli(arguments)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let envelope: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(envelope["ok"], true);
    assert!(
        envelope["request_id"]
            .as_str()
            .is_some_and(|id| Uuid::parse_str(id).is_ok())
    );
    Ok(envelope["result"].clone())
}

fn create(project: &Path) -> TestResult {
    success(&[
        "project".as_ref(),
        "create".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--name".as_ref(),
        "CLI live IPC".as_ref(),
        "--width".as_ref(),
        "64".as_ref(),
        "--height".as_ref(),
        "64".as_ref(),
    ])?;
    Ok(())
}

fn watch(project: &Path, after: Option<u64>, wait: u32) -> TestResult<Value> {
    let sequence = after.map(|value| value.to_string());
    let wait = wait.to_string();
    let mut args = vec![
        "project".as_ref(),
        "watch".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--wait-ms".as_ref(),
        wait.as_ref(),
    ];
    if let Some(sequence) = &sequence {
        args.push(OsStr::new("--after-sequence"));
        args.push(OsStr::new(sequence));
    }
    success(&args)
}

#[test]
fn watch_snapshot_and_unknown_job_errors_use_json_contract() -> TestResult {
    let directory = tempfile::tempdir()?;
    let project = directory.path().join("live.lumeryn");
    let started = Instant::now();
    create(&project)?;
    let first = watch(&project, None, 0)?;
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "captured CLI output must not wait for idle service shutdown"
    );
    assert_eq!(first["snapshot"]["revision"], 0);
    assert_eq!(first["snapshot"]["name"], "CLI live IPC");
    assert_eq!(first["resync"], true);
    let sequence = first["sequence"].as_u64().ok_or("missing event sequence")?;
    let next = watch(&project, Some(sequence), 50)?;
    assert_eq!(next["sequence"], sequence);
    assert_eq!(next["snapshot"]["id"], first["snapshot"]["id"]);
    assert_eq!(next["events"], serde_json::json!([]));
    assert_eq!(next["resync"], false);
    let job = Uuid::new_v4().to_string();
    let cancelled = cli(&[
        "jobs".as_ref(),
        "cancel".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--job-id".as_ref(),
        job.as_ref(),
    ])?;
    assert_eq!(cancelled.status.code(), Some(1));
    assert!(cancelled.stdout.is_empty());
    let error: Value = serde_json::from_slice(&cancelled.stderr)?;
    assert_eq!(error["ok"], false);
    assert_eq!(error["error"]["code"], "not_found");
    assert_eq!(watch(&project, Some(sequence), 0)?["sequence"], sequence);
    Ok(())
}

#[tokio::test]
#[ignore = "requires trusted installed FFmpeg/ffprobe; generated assets only"]
async fn headless_known_job_survives_cli_disconnect_and_cancels() -> TestResult {
    let directory = tempfile::tempdir()?;
    let project = directory.path().join("headless.lumeryn");
    let source = directory.path().join("source.mp4");
    let output = directory.path().join("cancelled.mp4");
    let tools = MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffmpeg".into()),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffprobe".into()),
        ..Default::default()
    };
    let args = [
        "-v",
        "error",
        "-nostdin",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=64x64:rate=30",
        "-frames:v",
        "9000",
        "-c:v",
        "mpeg4",
        "-threads",
        "2",
    ]
    .into_iter()
    .map(Into::into)
    .chain([source.as_os_str().to_owned()])
    .collect::<Vec<_>>();
    tools.run(&tools.ffmpeg, &args).await?;
    create(&project)?;
    let media = Uuid::new_v4().to_string();
    success(&[
        "media".as_ref(),
        "import".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--path".as_ref(),
        source.as_os_str(),
        "--id".as_ref(),
        media.as_ref(),
        "--expected-revision".as_ref(),
        "0".as_ref(),
    ])?;
    success(&[
        "timeline".as_ref(),
        "clip".as_ref(),
        "add".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--media-id".as_ref(),
        media.as_ref(),
        "--duration-frames".as_ref(),
        "9000".as_ref(),
        "--expected-revision".as_ref(),
        "1".as_ref(),
    ])?;
    let initial = watch(&project, None, 0)?;
    let sequence = initial["sequence"].as_u64().ok_or("missing sequence")?;
    let job = Uuid::new_v4().to_string();
    let mut rendering = Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .args([
            "render".as_ref(),
            "--project".as_ref(),
            project.as_os_str(),
            "--output".as_ref(),
            output.as_os_str(),
            "--expected-revision".as_ref(),
            "2".as_ref(),
            "--job-id".as_ref(),
            job.as_ref(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let observed = watch(&project, Some(sequence), 2000)?;
    assert!(observed["events"].as_array().is_some_and(|events| {
        events.iter().any(|event| {
            event["data"]["job_id"] == job
                && matches!(event["kind"].as_str(), Some("job_queued" | "job_running"))
        })
    }));
    assert!(
        rendering.try_wait()?.is_none(),
        "fixture must still render when CLI disconnects"
    );
    rendering.kill()?;
    rendering.wait()?;
    // The service-owned worker continues independently; another CLI controls it by known ID.
    let cancel = success(&[
        "jobs".as_ref(),
        "cancel".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--job-id".as_ref(),
        job.as_ref(),
    ])?;
    assert_eq!(cancel["cancellation_requested"], true);
    let after = observed["sequence"].as_u64().ok_or("missing sequence")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut cursor = after;
    loop {
        let batch = watch(&project, Some(cursor), 500)?;
        if batch["events"].as_array().is_some_and(|events| {
            events
                .iter()
                .any(|event| event["kind"] == "job_cancelled" && event["data"]["job_id"] == job)
        }) {
            break;
        }
        cursor = batch["sequence"].as_u64().ok_or("missing sequence")?;
        if Instant::now() > deadline {
            return Err(Error::Timeout.into());
        }
    }
    assert!(!output.exists());
    assert_eq!(watch(&project, None, 0)?["snapshot"]["revision"], 2);
    Ok(())
}
