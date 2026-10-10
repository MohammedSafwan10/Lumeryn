//! Shared history/clipboard CLI contracts; media acceptance generates its own trusted source.
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};
use uuid::Uuid;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
fn run(directory: &Path, args: &[&str]) -> TestResult<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .current_dir(directory)
        .args(args)
        .env_remove("RUST_LOG")
        .output()?)
}
fn success(directory: &Path, args: &[&str]) -> TestResult<Value> {
    let output = run(directory, args)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(value["ok"], true);
    Ok(value["result"].clone())
}
fn failure(directory: &Path, args: &[&str], code: &str) -> TestResult {
    let output = run(directory, args)?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(value["error"]["code"], code, "{value}");
    Ok(())
}
fn inspect(directory: &Path) -> TestResult<Value> {
    success(
        directory,
        &["project", "inspect", "--project", "project.lumeryn"],
    )
}
#[test]
fn headless_undo_redo_dry_run_replay_conflict_and_branch() -> TestResult {
    let directory = tempfile::tempdir()?;
    let directory = directory.path();
    success(
        directory,
        &[
            "project",
            "create",
            "--project",
            "project.lumeryn",
            "--name",
            "History",
        ],
    )?;
    let initial = success(
        directory,
        &["project", "history-status", "--project", "project.lumeryn"],
    )?;
    assert_eq!(initial["can_undo"], false);
    assert_eq!(initial["can_redo"], false);
    failure(
        directory,
        &[
            "project",
            "undo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "0",
        ],
        "invalid_input",
    )?;
    success(
        directory,
        &[
            "timeline",
            "canvas",
            "--project",
            "project.lumeryn",
            "--preset",
            "square",
            "--expected-revision",
            "0",
        ],
    )?;
    success(
        directory,
        &[
            "timeline",
            "canvas",
            "--project",
            "project.lumeryn",
            "--preset",
            "portrait",
            "--expected-revision",
            "1",
        ],
    )?;
    let dry = success(
        directory,
        &[
            "project",
            "undo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "2",
            "--dry-run",
        ],
    )?;
    assert_eq!(dry["dry_run"], true);
    assert_eq!(inspect(directory)?["revision"], 2);
    let args = [
        "project",
        "undo",
        "--project",
        "project.lumeryn",
        "--expected-revision",
        "2",
        "--idempotency-key",
        "undo-portrait",
    ];
    assert_eq!(success(directory, &args)?["revision"], 3);
    let undo = inspect(directory)?;
    assert_eq!(
        (undo["width"].clone(), undo["height"].clone()),
        (json!(1080), json!(1080))
    );
    assert_eq!(success(directory, &args)?["replayed"], true);
    let status = success(
        directory,
        &["project", "history-status", "--project", "project.lumeryn"],
    )?;
    assert_eq!(status["current_logical_revision"], 1);
    assert_eq!(status["undo_depth"], 1);
    assert_eq!(status["redo_depth"], 1);
    failure(
        directory,
        &[
            "project",
            "redo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "2",
        ],
        "revision_conflict",
    )?;
    assert_eq!(
        success(
            directory,
            &[
                "project",
                "redo",
                "--project",
                "project.lumeryn",
                "--expected-revision",
                "3"
            ]
        )?["revision"],
        4
    );
    assert_eq!(inspect(directory)?["height"], 1920);
    success(
        directory,
        &[
            "project",
            "undo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "4",
        ],
    )?;
    success(
        directory,
        &[
            "timeline",
            "canvas",
            "--project",
            "project.lumeryn",
            "--preset",
            "landscape",
            "--expected-revision",
            "5",
        ],
    )?;
    failure(
        directory,
        &[
            "project",
            "redo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "6",
        ],
        "invalid_input",
    )?;
    let branched = success(
        directory,
        &["project", "history-status", "--project", "project.lumeryn"],
    )?;
    assert_eq!(branched["can_redo"], false);
    assert_eq!(branched["current_logical_revision"], 6);
    std::fs::write(
        directory.join("mixed.json"),
        serde_json::to_vec(
            &json!({"operations":[{"operation":"undo"},{"operation":"set_canvas","width":640,"height":360}]}),
        )?,
    )?;
    failure(
        directory,
        &[
            "project",
            "batch",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "6",
            "--file",
            "mixed.json",
        ],
        "invalid_input",
    )?;
    assert_eq!(inspect(directory)?["revision"], 6);
    success(
        directory,
        &["project", "check", "--project", "project.lumeryn"],
    )?;
    Ok(())
}

#[test]
#[ignore = "requires trusted FFmpeg/ffprobe; generates and checks immutable footage"]
fn clipboard_actual_source_ranges_new_id_replay_bounds_and_undo() -> TestResult {
    let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.lumeryn-scratch/media");
    std::fs::create_dir_all(&scratch)?;
    let directory = tempfile::Builder::new()
        .prefix("clipboard-cli-")
        .tempdir_in(scratch)?;
    let directory = directory.path();
    let source = directory.join("source.mp4");
    let output = Command::new(
        std::env::var_os("LUMERYN_FFMPEG")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffmpeg".into()),
    )
    .args([
        "-v",
        "error",
        "-nostdin",
        "-n",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=320x180:rate=30:duration=1",
        "-c:v",
        "mpeg4",
        "-threads",
        "2",
        "-an",
    ])
    .arg(&source)
    .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source_bytes = std::fs::read(&source)?;
    success(
        directory,
        &[
            "project",
            "create",
            "--project",
            "project.lumeryn",
            "--name",
            "Clipboard",
            "--width",
            "320",
            "--height",
            "180",
        ],
    )?;
    let media = Uuid::new_v4().to_string();
    let clip = Uuid::new_v4().to_string();
    success(
        directory,
        &[
            "media",
            "import",
            "--project",
            "project.lumeryn",
            "--path",
            "source.mp4",
            "--id",
            &media,
            "--expected-revision",
            "0",
        ],
    )?;
    success(
        directory,
        &[
            "timeline",
            "clip",
            "add",
            "--project",
            "project.lumeryn",
            "--media-id",
            &media,
            "--id",
            &clip,
            "--source-in-frame",
            "5",
            "--duration-frames",
            "10",
            "--expected-revision",
            "1",
        ],
    )?;
    let copied = success(
        directory,
        &[
            "timeline",
            "clip",
            "copy",
            "--project",
            "project.lumeryn",
            "--clip-id",
            &clip,
        ],
    )?;
    assert_eq!(copied["clipboard"]["version"], 1);
    assert_eq!(copied["clipboard"]["source_in_frame"], 5);
    assert_eq!(copied["clipboard"]["duration_frames"], 10);
    std::fs::write(
        directory.join("clipboard.json"),
        serde_json::to_vec(&copied["clipboard"])?,
    )?;
    let pasted_id = Uuid::new_v4().to_string();
    let paste = [
        "timeline",
        "clip",
        "paste",
        "--project",
        "project.lumeryn",
        "--file",
        "clipboard.json",
        "--id",
        &pasted_id,
        "--at-frame",
        "10",
        "--expected-revision",
        "2",
        "--idempotency-key",
        "paste-exact",
    ];
    let mut dry = paste.to_vec();
    dry.push("--dry-run");
    assert_eq!(success(directory, &dry)?["dry_run"], true);
    assert_eq!(
        inspect(directory)?["clips"]
            .as_array()
            .ok_or("missing clips")?
            .len(),
        1
    );
    assert_eq!(success(directory, &paste)?["revision"], 3);
    assert_eq!(success(directory, &paste)?["replayed"], true);
    let project = inspect(directory)?;
    let pasted = project["clips"]
        .as_array()
        .ok_or("missing clips")?
        .iter()
        .find(|clip| clip["id"] == pasted_id)
        .ok_or("new clip absent")?;
    assert_eq!(pasted["source_in_frame"], 5);
    assert_eq!(pasted["duration_frames"], 10);
    assert_eq!(pasted["at_frame"], 10);
    // Complete payload participates in receipt identity, including its digest.
    let mut tampered = copied["clipboard"].clone();
    tampered["source_blake3"] = "b".repeat(64).into();
    std::fs::write(
        directory.join("tampered.json"),
        serde_json::to_vec(&tampered)?,
    )?;
    let mut changed = paste.to_vec();
    changed[6] = "tampered.json";
    failure(directory, &changed, "invalid_input")?;
    std::fs::write(directory.join("oversized.json"), vec![b' '; 16 * 1024 + 1])?;
    changed[6] = "oversized.json";
    failure(directory, &changed, "invalid_input")?;
    assert_eq!(inspect(directory)?["revision"], 3);
    success(
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
    assert_eq!(
        inspect(directory)?["clips"]
            .as_array()
            .ok_or("missing clips")?
            .len(),
        1
    );
    success(
        directory,
        &[
            "project",
            "redo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "4",
        ],
    )?;
    assert_eq!(
        inspect(directory)?["clips"]
            .as_array()
            .ok_or("missing clips")?
            .len(),
        2
    );
    // Undo paste, placement and import, then repeat the original exact paste key:
    // receipt replay must succeed although current imported media no longer exists.
    for revision in [5, 6, 7] {
        success(
            directory,
            &[
                "project",
                "undo",
                "--project",
                "project.lumeryn",
                "--expected-revision",
                &revision.to_string(),
            ],
        )?;
    }
    assert!(
        inspect(directory)?["media"]
            .as_array()
            .ok_or("missing media")?
            .is_empty()
    );
    assert_eq!(success(directory, &paste)?["revision"], 3);
    assert_eq!(inspect(directory)?["revision"], 8);
    assert_eq!(std::fs::read(&source)?, source_bytes);
    success(
        directory,
        &[
            "project",
            "check",
            "--project",
            "project.lumeryn",
            "--assets",
        ],
    )?;
    Ok(())
}
