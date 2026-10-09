//! Run explicitly with `cargo test --locked --test media_regressions -- --include-ignored`.
//! Generated fixtures only; executable selection honors LUMERYN_FFMPEG/FFPROBE.
use lumeryn_core::{Error, MediaTools};
use serde_json::Value;
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{Duration, Instant},
};
use uuid::Uuid;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn fixture_directory() -> TestResult<tempfile::TempDir> {
    let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.lumeryn-scratch/media");
    std::fs::create_dir_all(&scratch)?;
    Ok(tempfile::Builder::new()
        .prefix("regressions-")
        .tempdir_in(scratch)?)
}

fn tools() -> MediaTools {
    MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffmpeg".into()),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffprobe".into()),
        ..MediaTools::default()
    }
}

fn cli(args: &[&OsStr]) -> TestResult<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .args(args)
        .env_remove("RUST_LOG")
        .output()?)
}

fn success(args: &[&OsStr]) -> TestResult<Value> {
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

fn generate(args: &[&OsStr]) -> TestResult {
    let output = Command::new(tools().ffmpeg)
        .args(["-hide_banner", "-v", "error", "-nostdin", "-n"])
        .args(args)
        .output()?;
    assert!(
        output.status.success(),
        "fixture FFmpeg stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn create(project: &Path) -> TestResult {
    success(&[
        "project".as_ref(),
        "create".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--name".as_ref(),
        "Media regressions".as_ref(),
        "--width".as_ref(),
        "320".as_ref(),
        "--height".as_ref(),
        "180".as_ref(),
    ])?;
    Ok(())
}

fn inspect(project: &Path) -> TestResult<Value> {
    success(&[
        "project".as_ref(),
        "inspect".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
    ])
}

fn place(project: &Path, source: &Path) -> TestResult<Value> {
    create(project)?;
    let media = Uuid::new_v4().to_string();
    assert_eq!(
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
        ])?["revision"],
        1
    );
    assert_eq!(
        success(&[
            "timeline".as_ref(),
            "clip".as_ref(),
            "add".as_ref(),
            "--project".as_ref(),
            project.as_os_str(),
            "--media-id".as_ref(),
            media.as_ref(),
            "--duration-frames".as_ref(),
            "30".as_ref(),
            "--expected-revision".as_ref(),
            "1".as_ref(),
        ])?["revision"],
        2
    );
    inspect(project)
}

fn assert_rejected_export(
    project: &Path,
    source: &Path,
    output: &Path,
    message: &str,
) -> TestResult {
    let original = std::fs::read(source)?;
    let snapshot = inspect(project)?;
    let result = cli(&[
        "render".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--output".as_ref(),
        output.as_os_str(),
        "--expected-revision".as_ref(),
        "2".as_ref(),
    ])?;
    assert_eq!(result.status.code(), Some(4));
    assert!(
        result.stdout.is_empty(),
        "failed render emitted success output"
    );
    let envelope: Value = serde_json::from_slice(&result.stderr)?;
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "unsupported");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .is_some_and(|text| text.contains(message)),
        "unexpected rejection: {envelope}"
    );
    assert!(!output.exists(), "unsupported render published a file");
    assert_eq!(std::fs::read(source)?, original);
    assert_eq!(inspect(project)?, snapshot);
    let parent = output.parent().ok_or("output parent missing")?;
    for entry in std::fs::read_dir(parent)? {
        assert!(
            !entry?
                .file_name()
                .to_string_lossy()
                .starts_with(".lumeryn-export-"),
            "unsupported render leaked a temporary export"
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires installed FFmpeg/ffprobe; generated fixtures only"]
fn delayed_audio_is_rejected_instead_of_silently_shifted() -> TestResult {
    let directory = fixture_directory()?;
    let source = directory.path().join("delayed audio.mp4");
    generate(&[
        "-f".as_ref(),
        "lavfi".as_ref(),
        "-i".as_ref(),
        "testsrc2=size=320x180:rate=30:duration=2".as_ref(),
        "-itsoffset".as_ref(),
        "0.5".as_ref(),
        "-f".as_ref(),
        "lavfi".as_ref(),
        "-i".as_ref(),
        "sine=frequency=440:sample_rate=48000:duration=1.5".as_ref(),
        "-map".as_ref(),
        "0:v:0".as_ref(),
        "-map".as_ref(),
        "1:a:0".as_ref(),
        "-c:v".as_ref(),
        "mpeg4".as_ref(),
        "-threads".as_ref(),
        "2".as_ref(),
        "-c:a".as_ref(),
        "aac".as_ref(),
        source.as_os_str(),
    ])?;
    let project = directory.path().join("project.lumeryn");
    let snapshot = place(&project, &source)?;
    let metadata = &snapshot["media"][0]["metadata"];
    assert_eq!(metadata["video"]["start_time_micros"], 0);
    assert!(
        metadata["audio"]["start_time_micros"]
            .as_i64()
            .is_some_and(|start| start > 400_000),
        "fixture did not preserve delayed audio: {metadata}"
    );
    assert_rejected_export(
        &project,
        &source,
        &directory.path().join("must-not-exist.mp4"),
        "zero-origin",
    )
}

#[test]
#[ignore = "requires installed FFmpeg/ffprobe; generated fixtures only"]
fn anamorphic_video_is_rejected_instead_of_squashed() -> TestResult {
    let directory = fixture_directory()?;
    let source = directory.path().join("anamorphic.mp4");
    generate(&[
        "-f".as_ref(),
        "lavfi".as_ref(),
        "-i".as_ref(),
        "testsrc2=size=320x180:rate=30:duration=1".as_ref(),
        "-vf".as_ref(),
        "setsar=2/1".as_ref(),
        "-c:v".as_ref(),
        "mpeg4".as_ref(),
        "-threads".as_ref(),
        "2".as_ref(),
        source.as_os_str(),
    ])?;
    let project = directory.path().join("project.lumeryn");
    let snapshot = place(&project, &source)?;
    assert_eq!(
        snapshot["media"][0]["metadata"]["video"]["sample_aspect_ratio"],
        "2:1"
    );
    assert_rejected_export(
        &project,
        &source,
        &directory.path().join("must-not-exist.mp4"),
        "square-pixel",
    )
}

#[test]
#[ignore = "requires installed ffprobe; generated malformed fixture only"]
fn malformed_import_fails_without_committing_or_unbounded_diagnostics() -> TestResult {
    let directory = fixture_directory()?;
    let source = directory.path().join("malformed.mp4");
    let invalid = b"Generated regression: these bytes are not a media container.";
    std::fs::write(&source, invalid)?;
    let project = directory.path().join("project.lumeryn");
    create(&project)?;
    let original_snapshot = inspect(&project)?;
    let started = Instant::now();
    let result = cli(&[
        "--timeout-seconds".as_ref(),
        "1".as_ref(),
        "media".as_ref(),
        "import".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--path".as_ref(),
        source.as_os_str(),
        "--expected-revision".as_ref(),
        "0".as_ref(),
    ])?;
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(result.status.code(), Some(5));
    assert!(result.stdout.is_empty());
    assert!(result.stderr.len() < 64 * 1024);
    let envelope: Value = serde_json::from_slice(&result.stderr)?;
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "media_failure");
    assert_eq!(inspect(&project)?, original_snapshot);
    assert_eq!(std::fs::read(&source)?, invalid);
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed FFmpeg; generated infinite lavfi worker only"]
async fn media_worker_timeout_bounds_an_infinite_generated_input() -> TestResult {
    let tools = MediaTools {
        timeout: Duration::from_millis(200),
        ..tools()
    };
    let args = [
        "-hide_banner",
        "-v",
        "error",
        "-nostdin",
        "-re",
        "-f",
        "lavfi",
        "-i",
        "color=red:size=16x16:rate=10",
        "-threads",
        "1",
        "-f",
        "null",
        "-",
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    let started = Instant::now();
    let result = tools.run(&tools.ffmpeg, &args).await;
    assert!(matches!(result, Err(Error::Timeout)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
    Ok(())
}
