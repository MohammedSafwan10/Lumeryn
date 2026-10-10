//! Independent CLI contract checks; external media executables are deliberately unavailable.
use lumeryn_core::{Batch, Edit};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    path::Path,
    process::{Command, Output},
};

type TestResult = Result<(), Box<dyn Error>>;

fn run(directory: &Path, arguments: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .current_dir(directory)
        .env_remove("RUST_LOG")
        .env_remove("LUMERYN_FFMPEG")
        .env_remove("LUMERYN_FFPROBE")
        .args([
            "--ffmpeg",
            "missing-test-ffmpeg",
            "--ffprobe",
            "missing-test-ffprobe",
        ])
        .args(arguments)
        .output()
}

fn success(output: Output) -> Result<Value, Box<dyn Error>> {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "success emitted unexpected diagnostics"
    );
    let value: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(value["ok"], true);
    assert!(value.get("result").is_some());
    uuid::Uuid::parse_str(value["request_id"].as_str().ok_or("missing request ID")?)?;
    assert!(value["elapsed_ms"].as_u64().is_some());
    Ok(value["result"].clone())
}

fn domain_error(output: Output, code: &str, exit: i32) -> TestResult {
    assert_eq!(output.status.code(), Some(exit));
    assert!(
        output.stdout.is_empty(),
        "error leaked to machine-success stdout"
    );
    let value: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], code);
    assert!(
        value["error"]["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty())
    );
    Ok(())
}

fn create(directory: &Path) -> Result<Value, Box<dyn Error>> {
    success(run(
        directory,
        &[
            "project",
            "create",
            "--project",
            "project.lumeryn",
            "--name",
            "Independent fixture",
            "--width",
            "320",
            "--height",
            "180",
            "--fps",
            "60000/2002",
        ],
    )?)
}

#[test]
fn discovery_and_reopening_emit_one_machine_envelope() -> TestResult {
    let directory = tempfile::tempdir()?;
    let capabilities = success(run(directory.path(), &["capabilities"])?)?;
    assert_eq!(capabilities["api_version"], 1);
    assert!(
        capabilities["implemented"]
            .as_array()
            .is_some_and(|items| items.contains(&json!("project.create")))
    );
    let hardware = success(run(directory.path(), &["hardware", "scan"])?)?;
    assert_eq!(hardware["gpu"], "unknown");
    assert_eq!(hardware["vram_bytes"], Value::Null);
    let created = create(directory.path())?;
    assert_eq!(created["sqlite_version"], "3.51.3");
    assert_eq!(
        created["project"]["frame_rate"],
        json!({"numerator":30000,"denominator":1001})
    );
    for command in ["inspect", "open"] {
        assert_eq!(
            success(run(
                directory.path(),
                &["project", command, "--project", "project.lumeryn"]
            )?)?,
            created["project"]
        );
    }
    let history = success(run(
        directory.path(),
        &["project", "history", "--project", "project.lumeryn"],
    )?)?;
    assert_eq!(history, json!([{"revision":0,"operations":[]} ]));
    Ok(())
}

#[test]
fn creating_over_existing_bytes_never_overwrites_them() -> TestResult {
    let directory = tempfile::tempdir()?;
    let original = b"existing proprietary document\0not a database";
    fs::write(directory.path().join("existing.lumeryn"), original)?;
    domain_error(
        run(
            directory.path(),
            &[
                "project",
                "create",
                "--project",
                "existing.lumeryn",
                "--name",
                "Overwrite attempt",
            ],
        )?,
        "already_exists",
        1,
    )?;
    assert_eq!(
        fs::read(directory.path().join("existing.lumeryn"))?,
        original
    );
    assert_eq!(fs::read_dir(directory.path())?.count(), 1);
    Ok(())
}

#[test]
fn invalid_dimensions_and_names_fail_before_file_creation() -> TestResult {
    let directory = tempfile::tempdir()?;
    for (flag, value) in [
        ("--width", "319"),
        ("--height", "0"),
        ("--width", "8194"),
        ("--name", " "),
    ] {
        let mut arguments = vec![
            "project",
            "create",
            "--project",
            "invalid.lumeryn",
            "--name",
            "Fixture",
        ];
        if flag == "--name" {
            arguments.truncate(4);
        }
        arguments.extend([flag, value]);
        domain_error(run(directory.path(), &arguments)?, "invalid_input", 2)?;
        assert_eq!(fs::read_dir(directory.path())?.count(), 0);
    }
    Ok(())
}

#[test]
fn malformed_rates_and_unknown_arguments_have_no_side_effects() -> TestResult {
    let directory = tempfile::tempdir()?;
    for rate in ["0/1", "24/0", "-1/1", "1001/1", "1/2/3", "4294967296/1"] {
        let output = run(
            directory.path(),
            &[
                "project",
                "create",
                "--project",
                "invalid.lumeryn",
                "--name",
                "Fixture",
                "--fps",
                rate,
            ],
        )?;
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
        domain_error(output, "invalid_arguments", 2)?;
    }
    for arguments in [
        &["unknown-command"][..],
        &["timeline", "clip", "delete"][..],
        &["capabilities", "--unknown-flag"][..],
    ] {
        let output = run(directory.path(), arguments)?;
        domain_error(output, "invalid_arguments", 2)?;
    }
    assert_eq!(fs::read_dir(directory.path())?.count(), 0);
    Ok(())
}

#[test]
fn help_and_version_remain_successful_human_readable_requests() -> TestResult {
    let directory = tempfile::tempdir()?;
    for arguments in [
        &["--help"][..],
        &["--version"][..],
        &["project", "create", "--help"][..],
    ] {
        let output = run(directory.path(), arguments)?;
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert!(!output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stdout).contains("lumeryn"));
    }
    assert_eq!(fs::read_dir(directory.path())?.count(), 0);
    Ok(())
}

#[test]
fn missing_project_is_not_created_by_read_or_write_commands() -> TestResult {
    let directory = tempfile::tempdir()?;
    for arguments in [
        &["project", "inspect", "--project", "missing.lumeryn"][..],
        &["project", "open", "--project", "missing.lumeryn"][..],
        &["project", "history", "--project", "missing.lumeryn"][..],
        &["timeline", "inspect", "--project", "missing.lumeryn"][..],
        &[
            "timeline",
            "clip",
            "trim",
            "--project",
            "missing.lumeryn",
            "--clip-id",
            "00000000-0000-4000-8000-000000000001",
            "--source-in-frame",
            "0",
            "--duration-frames",
            "1",
        ][..],
    ] {
        let output = run(directory.path(), arguments)?;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr)?;
        assert_eq!(error["ok"], false);
        assert_eq!(fs::read_dir(directory.path())?.count(), 0);
    }
    Ok(())
}

#[test]
fn malformed_batches_preserve_snapshot_and_history() -> TestResult {
    let directory = tempfile::tempdir()?;
    let initial = create(directory.path())?;
    for contents in [
        "{",
        r#"{"operations":[],"extra":true}"#,
        r#"{"operations":[{"operation":"execute_shell","command":"echo unsafe"}]}"#,
        r#"{"operations":[]}"#,
    ] {
        fs::write(directory.path().join("batch.json"), contents)?;
        domain_error(
            run(
                directory.path(),
                &[
                    "project",
                    "batch",
                    "--project",
                    "project.lumeryn",
                    "--file",
                    "batch.json",
                ],
            )?,
            "invalid_input",
            2,
        )?;
    }
    fs::write(
        directory.path().join("batch.json"),
        vec![b' '; 1024 * 1024 + 1],
    )?;
    domain_error(
        run(
            directory.path(),
            &[
                "project",
                "batch",
                "--project",
                "project.lumeryn",
                "--file",
                "batch.json",
            ],
        )?,
        "invalid_input",
        2,
    )?;
    assert_eq!(
        success(run(
            directory.path(),
            &["project", "inspect", "--project", "project.lumeryn"]
        )?)?,
        initial["project"]
    );
    assert_eq!(
        success(run(
            directory.path(),
            &["project", "history", "--project", "project.lumeryn"]
        )?)?,
        json!([{"revision":0,"operations":[]} ])
    );
    Ok(())
}

#[test]
fn schema_operation_contract_matches_shared_command_types() -> TestResult {
    let directory = tempfile::tempdir()?;
    let schema = success(run(directory.path(), &["schema"])?)?;
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["required"], json!(["operations"]));
    let variants = schema["$defs"]["Edit"]["oneOf"]
        .as_array()
        .ok_or("missing operation variants")?;
    assert_eq!(variants.len(), 20);
    for example in [
        json!({"operation":"import","id":"00000000-0000-4000-8000-000000000001","path":"source.mp4"}),
        json!({"operation":"rename_media","media_id":"00000000-0000-4000-8000-000000000001","alias":"Opening shot"}),
        json!({"operation":"remove_media","media_id":"00000000-0000-4000-8000-000000000001"}),
        json!({"operation":"add_clip","id":"00000000-0000-4000-8000-000000000002","media_id":"00000000-0000-4000-8000-000000000001","at_frame":0,"source_in_frame":0,"duration_frames":30}),
        json!({"operation":"insert_clip","id":"00000000-0000-4000-8000-000000000002","media_id":"00000000-0000-4000-8000-000000000001","before_clip_id":null,"source_in_frame":0,"duration_frames":30}),
        json!({"operation":"add_audio_clip","id":"00000000-0000-4000-8000-000000000002","media_id":"00000000-0000-4000-8000-000000000001","at_frame":0,"source_in_sample":0,"duration_frames":30,"gain_millidecibels":0,"muted":false}),
        json!({"operation":"move_audio_clip","clip_id":"00000000-0000-4000-8000-000000000002","at_frame":30}),
        json!({"operation":"trim_audio_clip","clip_id":"00000000-0000-4000-8000-000000000002","source_in_sample":100,"duration_frames":20}),
        json!({"operation":"set_audio_range","clip_id":"00000000-0000-4000-8000-000000000002","at_frame":30,"source_in_sample":100,"duration_frames":20}),
        json!({"operation":"set_audio_properties","clip_id":"00000000-0000-4000-8000-000000000002","gain_millidecibels":-6000,"muted":false}),
        json!({"operation":"delete_audio_clip","clip_id":"00000000-0000-4000-8000-000000000002"}),
        json!({"operation":"trim_clip","clip_id":"00000000-0000-4000-8000-000000000002","source_in_frame":10,"duration_frames":20}),
        json!({"operation":"split_clip","clip_id":"00000000-0000-4000-8000-000000000002","right_clip_id":"00000000-0000-4000-8000-000000000003","at_frame":15}),
        json!({"operation":"duplicate_clip","clip_id":"00000000-0000-4000-8000-000000000002","id":"00000000-0000-4000-8000-000000000003"}),
        json!({"operation":"delete_clip","clip_id":"00000000-0000-4000-8000-000000000002","ripple":true}),
        json!({"operation":"reorder_clip","clip_id":"00000000-0000-4000-8000-000000000002","before_clip_id":null}),
        json!({"operation":"set_canvas","width":2160,"height":3840}),
        json!({"operation":"undo"}),
        json!({"operation":"redo"}),
        json!({"operation":"paste_clip","id":"00000000-0000-4000-8000-000000000003","at_frame":30,"clipboard":{"version":1,"project_id":"00000000-0000-4000-8000-000000000004","media_id":"00000000-0000-4000-8000-000000000001","source_blake3":"a".repeat(64),"frame_rate":{"numerator":30,"denominator":1},"source_in_frame":10,"duration_frames":20}}),
    ] {
        let operation: Edit = serde_json::from_value(example.clone())?;
        let matching = variants
            .iter()
            .find(|variant| variant["properties"]["operation"]["const"] == example["operation"])
            .ok_or("operation omitted from schema")?;
        assert_eq!(matching["additionalProperties"], false);
        let required = matching["required"]
            .as_array()
            .ok_or("missing required fields")?;
        assert_eq!(
            required.len(),
            example
                .as_object()
                .ok_or("example not an object")?
                .keys()
                .filter(|name| !matches!(name.as_str(), "ripple" | "before_clip_id"))
                .count()
        );
        for name in required {
            assert!(
                example
                    .get(name.as_str().ok_or("invalid field name")?)
                    .is_some()
            );
        }
        assert_eq!(serde_json::to_value(operation)?, example);
        let mut invalid = example;
        invalid["unexpected"] = true.into();
        assert!(serde_json::from_value::<Batch>(json!({"operations":[invalid]})).is_err());
    }
    assert_eq!(fs::read_dir(directory.path())?.count(), 0);
    Ok(())
}

#[test]
fn project_presets_and_explicit_dimensions_are_persisted() -> TestResult {
    let directory = tempfile::tempdir()?;
    for (preset, width, height) in [
        ("landscape", 1920, 1080),
        ("portrait", 1080, 1920),
        ("square", 1080, 1080),
    ] {
        let path = format!("{preset}.lumeryn");
        let created = success(run(
            directory.path(),
            &[
                "project",
                "create",
                "--project",
                &path,
                "--name",
                "Preset",
                "--preset",
                preset,
            ],
        )?)?;
        assert_eq!(created["project"]["width"], width);
        assert_eq!(created["project"]["height"], height);
        assert_eq!(
            success(run(
                directory.path(),
                &["project", "open", "--project", &path]
            )?)?,
            created["project"]
        );
    }
    let custom = success(run(
        directory.path(),
        &[
            "project",
            "create",
            "--project",
            "custom.lumeryn",
            "--name",
            "Custom",
            "--preset",
            "portrait",
            "--width",
            "320",
            "--height",
            "180",
        ],
    )?)?;
    assert_eq!(custom["project"]["width"], 320);
    assert_eq!(custom["project"]["height"], 180);
    Ok(())
}

#[test]
fn maintenance_commands_work_headlessly_without_external_tools() -> TestResult {
    let directory = tempfile::tempdir()?;
    let initial = create(directory.path())?;
    let checked = success(run(
        directory.path(),
        &["project", "check", "--project", "project.lumeryn"],
    )?)?;
    assert_eq!(checked["sqlite_integrity"], "ok");
    assert_eq!(checked["history"], "ok");
    assert_eq!(checked["storage_version"], 1);
    let search = success(run(
        directory.path(),
        &[
            "project",
            "search",
            "--project",
            "project.lumeryn",
            "--query",
            "Independent fixture",
        ],
    )?)?;
    assert_eq!(
        search
            .as_array()
            .ok_or("search result is not an array")?
            .len(),
        1
    );
    assert_eq!(search[0]["object_id"], initial["project"]["id"]);
    assert_eq!(
        success(run(
            directory.path(),
            &["project", "migrate", "--project", "project.lumeryn"]
        )?)?["changed"],
        false
    );
    fs::create_dir(directory.path().join("backups"))?;
    let backup = success(run(
        directory.path(),
        &[
            "project",
            "backup",
            "--project",
            "project.lumeryn",
            "--output",
            "backups/copy.lumeryn",
        ],
    )?)?;
    assert_eq!(backup["assets"], "referenced; not copied");
    assert_eq!(
        success(run(
            directory.path(),
            &["project", "open", "--project", "backups/copy.lumeryn"]
        )?)?,
        initial["project"]
    );
    assert_eq!(
        success(run(
            directory.path(),
            &["project", "inspect", "--project", "project.lumeryn"]
        )?)?,
        initial["project"]
    );
    Ok(())
}
