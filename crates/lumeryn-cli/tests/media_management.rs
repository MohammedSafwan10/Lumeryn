//! Project media commands use the authoritative service; ordinary tests need no media tools.
use lumeryn_core::{
    Project,
    media::{Metadata, Video},
    model::{Clip, Media},
    storage::Store,
    time::FrameRate,
};
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
        .env_remove("RUST_LOG")
        .args(args)
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

fn failure(directory: &Path, args: &[&str], code: &str, exit: i32) -> TestResult {
    let output = run(directory, args)?;
    assert_eq!(output.status.code(), Some(exit));
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

fn media_inspect(directory: &Path, media_id: &str) -> TestResult<Value> {
    success(
        directory,
        &[
            "media",
            "inspect",
            "--project",
            "project.lumeryn",
            "--media-id",
            media_id,
        ],
    )
}

async fn seed(directory: &Path, referenced: bool) -> TestResult<(String, String)> {
    let rate = FrameRate::new(30, 1)?;
    let mut project = Project::new("Media commands".into(), 320, 180, rate)?;
    let media_id = Uuid::new_v4();
    let clip_id = Uuid::new_v4();
    // This fixture verifies editing contracts without pretending the bytes are decodable.
    let bytes = b"immutable fixture source bytes";
    std::fs::write(directory.join("source_one.mp4"), bytes)?;
    project.media.push(Media {
        id: media_id,
        path: "source_one.mp4".into(),
        alias: None,
        metadata: Metadata {
            size_bytes: bytes.len() as u64,
            blake3: Some("a".repeat(64)),
            video: Some(Video {
                codec: "mpeg4".into(),
                width: 320,
                height: 180,
                frame_rate: rate,
                frames: 30,
                start_time_micros: 0,
                sample_aspect_ratio: "1:1".into(),
                color_transfer: None,
                constant_rate: true,
            }),
            audio: None,
        },
    });
    if referenced {
        project.clips.push(Clip {
            id: clip_id,
            media_id,
            track_id: project.video_track_id,
            at_frame: 0,
            source_in_frame: 5,
            duration_frames: 10,
        });
    }
    drop(Store::create(&directory.join("project.lumeryn"), &project).await?);
    Ok((media_id.to_string(), clip_id.to_string()))
}

#[test]
fn rename_arguments_and_discovery_are_strict() -> TestResult {
    let directory = tempfile::tempdir()?;
    let directory = directory.path();
    let media_id = Uuid::new_v4().to_string();
    let base = [
        "media",
        "rename",
        "--project",
        "missing.lumeryn",
        "--media-id",
        &media_id,
    ];
    failure(directory, &base, "invalid_arguments", 2)?;
    let mut both = base.to_vec();
    both.extend(["--name", "Opening", "--reset"]);
    failure(directory, &both, "invalid_arguments", 2)?;
    failure(
        directory,
        &["media", "rename", "--media-id", &media_id, "--reset"],
        "invalid_arguments",
        2,
    )?;
    failure(
        directory,
        &["media", "remove", "--project", "missing.lumeryn"],
        "invalid_arguments",
        2,
    )?;
    failure(
        directory,
        &[
            "media",
            "remove",
            "--project",
            "missing.lumeryn",
            "--media-id",
            &media_id,
            "--cascade",
        ],
        "invalid_arguments",
        2,
    )?;
    let capabilities = success(directory, &["capabilities"])?;
    let implemented = capabilities["implemented"]
        .as_array()
        .ok_or("implemented capabilities absent")?;
    for capability in [
        "media.list",
        "media.inspect",
        "media.rename",
        "media.remove",
    ] {
        assert!(implemented.contains(&json!(capability)));
    }
    assert_eq!(capabilities["media_management"]["dry_run"], true);
    let schema = success(directory, &["schema"])?;
    let variants = schema["$defs"]["Edit"]["oneOf"]
        .as_array()
        .ok_or("edit variants absent")?;
    for operation in ["rename_media", "remove_media"] {
        assert!(
            variants
                .iter()
                .any(|variant| { variant["properties"]["operation"]["const"] == operation })
        );
    }
    Ok(())
}

#[tokio::test]
async fn rename_reset_dry_run_history_retries_and_source_identity() -> TestResult {
    let directory = tempfile::tempdir()?;
    let directory = directory.path();
    let (media_id, _) = seed(directory, false).await?;
    let original_bytes = std::fs::read(directory.join("source_one.mp4"))?;
    let original = inspect(directory)?;
    let initial_media = media_inspect(directory, &media_id)?;
    assert_eq!(initial_media["revision"], 0);
    assert_eq!(initial_media["media"]["alias"], Value::Null);
    assert_eq!(initial_media["media"]["display_name"], "source one");
    assert_eq!(initial_media["media"]["usage_count"], 0);
    let rename = [
        "media",
        "rename",
        "--project",
        "project.lumeryn",
        "--media-id",
        &media_id,
        "--name",
        "Opening — शुरुआत",
        "--expected-revision",
        "0",
        "--idempotency-key",
        "opening-name",
    ];
    let mut dry = rename.to_vec();
    dry.push("--dry-run");
    assert_eq!(success(directory, &dry)?["dry_run"], true);
    assert_eq!(inspect(directory)?, original);
    assert_eq!(success(directory, &rename)?["revision"], 1);
    let renamed = media_inspect(directory, &media_id)?;
    assert_eq!(renamed["media"]["alias"], "Opening — शुरुआत");
    assert_eq!(renamed["media"]["display_name"], "Opening — शुरुआत");
    assert_eq!(renamed["media"]["path"], initial_media["media"]["path"]);
    assert_eq!(
        renamed["media"]["metadata"],
        initial_media["media"]["metadata"]
    );
    assert_eq!(success(directory, &rename)?["replayed"], true);
    for (command, revision) in [("undo", "1"), ("redo", "2")] {
        success(
            directory,
            &[
                "project",
                command,
                "--project",
                "project.lumeryn",
                "--expected-revision",
                revision,
            ],
        )?;
        assert_eq!(
            media_inspect(directory, &media_id)?["media"]["alias"],
            if command == "undo" {
                Value::Null
            } else {
                json!("Opening — शुरुआत")
            }
        );
    }
    success(
        directory,
        &[
            "media",
            "rename",
            "--project",
            "project.lumeryn",
            "--media-id",
            &media_id,
            "--reset",
            "--expected-revision",
            "3",
        ],
    )?;
    assert_eq!(
        media_inspect(directory, &media_id)?["media"]["alias"],
        Value::Null
    );
    assert_eq!(success(directory, &rename)?["revision"], 1);
    assert_eq!(inspect(directory)?["revision"], 4);
    let mut changed = rename.to_vec();
    changed[7] = "Different";
    failure(directory, &changed, "invalid_input", 2)?;
    let stale = &rename[..10];
    failure(directory, stale, "revision_conflict", 3)?;
    let list = success(
        directory,
        &["media", "list", "--project", "project.lumeryn"],
    )?;
    assert_eq!(list["revision"], 4);
    assert_eq!(list["media"][0]["display_name"], "source one");
    assert_eq!(
        std::fs::read(directory.join("source_one.mp4"))?,
        original_bytes
    );
    Ok(())
}

#[tokio::test]
async fn invalid_aliases_and_missing_ids_do_not_write() -> TestResult {
    let directory = tempfile::tempdir()?;
    let directory = directory.path();
    let (media_id, _) = seed(directory, false).await?;
    let original = inspect(directory)?;
    let too_long = "字".repeat(86);
    for alias in ["", " \t ", "line\nbreak", &too_long] {
        failure(
            directory,
            &[
                "media",
                "rename",
                "--project",
                "project.lumeryn",
                "--media-id",
                &media_id,
                "--name",
                alias,
                "--expected-revision",
                "0",
            ],
            "invalid_input",
            2,
        )?;
    }
    let missing = Uuid::new_v4().to_string();
    failure(
        directory,
        &[
            "media",
            "inspect",
            "--project",
            "project.lumeryn",
            "--media-id",
            &missing,
        ],
        "not_found",
        1,
    )?;
    for command in ["rename", "remove"] {
        let mut args = vec![
            "media",
            command,
            "--project",
            "project.lumeryn",
            "--media-id",
            &missing,
            "--expected-revision",
            "0",
        ];
        if command == "rename" {
            args.push("--reset");
        }
        failure(directory, &args, "not_found", 1)?;
    }
    assert_eq!(inspect(directory)?, original);
    Ok(())
}

#[tokio::test]
async fn referenced_remove_requires_explicit_atomic_deletes_and_preserves_source() -> TestResult {
    let directory = tempfile::tempdir()?;
    let directory = directory.path();
    let (media_id, clip_id) = seed(directory, true).await?;
    let source_bytes = std::fs::read(directory.join("source_one.mp4"))?;
    let original = inspect(directory)?;
    assert_eq!(
        media_inspect(directory, &media_id)?["media"]["usage_count"],
        1
    );
    let remove = [
        "media",
        "remove",
        "--project",
        "project.lumeryn",
        "--media-id",
        &media_id,
        "--expected-revision",
        "0",
    ];
    failure(directory, &remove, "invalid_input", 2)?;
    let mut dry_remove = remove.to_vec();
    dry_remove.push("--dry-run");
    failure(directory, &dry_remove, "invalid_input", 2)?;
    std::fs::write(
        directory.join("bad-order.json"),
        serde_json::to_vec(&json!({"operations":[
            {"operation":"remove_media","media_id":media_id},
            {"operation":"delete_clip","clip_id":clip_id}
        ]}))?,
    )?;
    failure(
        directory,
        &[
            "project",
            "batch",
            "--project",
            "project.lumeryn",
            "--file",
            "bad-order.json",
            "--expected-revision",
            "0",
        ],
        "invalid_input",
        2,
    )?;
    assert_eq!(inspect(directory)?, original);
    std::fs::write(
        directory.join("remove.json"),
        serde_json::to_vec(&json!({"operations":[
            {"operation":"delete_clip","clip_id":clip_id},
            {"operation":"remove_media","media_id":media_id}
        ]}))?,
    )?;
    let batch = [
        "project",
        "batch",
        "--project",
        "project.lumeryn",
        "--file",
        "remove.json",
        "--expected-revision",
        "0",
        "--idempotency-key",
        "remove-both",
    ];
    let mut dry_batch = batch.to_vec();
    dry_batch.push("--dry-run");
    assert_eq!(success(directory, &dry_batch)?["dry_run"], true);
    assert_eq!(inspect(directory)?, original);
    assert_eq!(success(directory, &batch)?["revision"], 1);
    let removed = inspect(directory)?;
    assert_eq!(removed["media"], json!([]));
    assert_eq!(removed["clips"], json!([]));
    assert_eq!(success(directory, &batch)?["replayed"], true);
    success(
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
    let restored = inspect(directory)?;
    assert_eq!(restored["media"], original["media"]);
    assert_eq!(restored["clips"], original["clips"]);
    success(
        directory,
        &[
            "project",
            "redo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "2",
        ],
    )?;
    assert_eq!(inspect(directory)?["media"], json!([]));
    // The original receipt is replayed although the selected reference no longer exists.
    assert_eq!(success(directory, &batch)?["revision"], 1);
    assert_eq!(inspect(directory)?["revision"], 3);
    assert_eq!(
        std::fs::read(directory.join("source_one.mp4"))?,
        source_bytes
    );
    success(
        directory,
        &["project", "check", "--project", "project.lumeryn"],
    )?;
    Ok(())
}

#[test]
#[ignore = "requires trusted FFmpeg/ffprobe; generates and verifies source bytes"]
fn imported_source_rename_unused_remove_dry_run_and_history() -> TestResult {
    let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.lumeryn-scratch/media");
    std::fs::create_dir_all(&scratch)?;
    let directory = tempfile::Builder::new()
        .prefix("media-management-")
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
            "Source acceptance",
        ],
    )?;
    let media_id = Uuid::new_v4().to_string();
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
            &media_id,
            "--expected-revision",
            "0",
        ],
    )?;
    let original = media_inspect(directory, &media_id)?;
    success(
        directory,
        &[
            "media",
            "rename",
            "--project",
            "project.lumeryn",
            "--media-id",
            &media_id,
            "--name",
            "Original footage",
            "--expected-revision",
            "1",
        ],
    )?;
    let renamed = media_inspect(directory, &media_id)?;
    assert_eq!(renamed["media"]["metadata"], original["media"]["metadata"]);
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
    let remove = [
        "media",
        "remove",
        "--project",
        "project.lumeryn",
        "--media-id",
        &media_id,
        "--expected-revision",
        "2",
        "--idempotency-key",
        "remove-source",
    ];
    let mut dry = remove.to_vec();
    dry.push("--dry-run");
    assert_eq!(success(directory, &dry)?["dry_run"], true);
    assert_eq!(inspect(directory)?["revision"], 2);
    assert_eq!(success(directory, &remove)?["revision"], 3);
    assert_eq!(success(directory, &remove)?["replayed"], true);
    assert_eq!(inspect(directory)?["media"], json!([]));
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
        media_inspect(directory, &media_id)?["media"],
        renamed["media"]
    );
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
    assert_eq!(inspect(directory)?["media"], json!([]));
    assert_eq!(std::fs::read(&source)?, source_bytes);
    Ok(())
}
