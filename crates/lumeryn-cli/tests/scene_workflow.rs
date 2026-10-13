//! Editable scene commands exercise the same authoritative graph as Studio.
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const SCENE: &str = "00000000-0000-4000-8000-000000000001";
const TRACK: &str = "00000000-0000-4000-8000-000000000002";
const CLIP: &str = "00000000-0000-4000-8000-000000000003";

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

fn scene(
    root: &Path,
    command: &str,
    revision: &str,
    args: &[&str],
    succeeds: bool,
) -> TestResult<Value> {
    let mut all = vec![
        "timeline",
        "scene",
        command,
        "--project",
        "project.lumeryn",
        "--expected-revision",
        revision,
    ];
    all.extend_from_slice(args);
    request(root, &all, succeeds)
}

fn document(red: bool) -> Value {
    json!({"width":128,"height":72,"nodes":[{"id":"00000000-0000-4000-8000-000000000004","content":{"kind":"rectangle","properties":{"x":16.0,"y":12.0,"width":64.0,"height":40.0,"fill":{"r":if red {255} else {0},"g":if red {0} else {255},"b":0,"a":255}}}}]})
}

fn create_project(root: &Path) -> TestResult {
    request(
        root,
        &[
            "project",
            "create",
            "--project",
            "project.lumeryn",
            "--name",
            "Editable shapes",
            "--width",
            "128",
            "--height",
            "72",
            "--fps",
            "30",
        ],
        true,
    )?;
    fs::write(
        root.join("scene.json"),
        serde_json::to_vec(&document(true))?,
    )?;
    Ok(())
}

#[test]
fn scene_documents_are_bounded_strict_and_atomic_without_media_tools() -> TestResult {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    create_project(root)?;
    let before = inspect(root)?;
    let mut unknown = document(true);
    unknown["shell"] = json!("forbidden");
    let mut nested_unknown = document(true);
    nested_unknown["nodes"][0]["content"]["properties"]["extra"] = true.into();
    let mut mismatched = document(true);
    mismatched["width"] = 256.into();
    for bytes in [
        b"{".to_vec(),
        serde_json::to_vec(&unknown)?,
        serde_json::to_vec(&nested_unknown)?,
        serde_json::to_vec(&mismatched)?,
        vec![b' '; 256 * 1024 + 1],
    ] {
        fs::write(root.join("invalid.json"), bytes)?;
        let error = scene(
            root,
            "create",
            "0",
            &[
                "--id",
                SCENE,
                "--name",
                "Rejected",
                "--document-file",
                "invalid.json",
            ],
            false,
        )?;
        assert_eq!(error["error"]["code"], "invalid_input");
        assert_eq!(inspect(root)?, before);
        assert_eq!(
            request(
                root,
                &["project", "history", "--project", "project.lumeryn"],
                true
            )?
            .as_array()
            .ok_or("history")?
            .len(),
            1
        );
    }
    Ok(())
}

#[test]
fn scene_create_edit_place_move_range_delete_history_and_receipts() -> TestResult {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    create_project(root)?;
    let source_document = fs::read(root.join("scene.json"))?;
    let create = [
        "--id",
        SCENE,
        "--name",
        "Shape",
        "--document-file",
        "scene.json",
        "--idempotency-key",
        "create-shape",
    ];
    let mut dry = create.to_vec();
    dry.push("--dry-run");
    scene(root, "create", "0", &dry, true)?;
    assert_eq!(inspect(root)?["revision"], 0);
    let receipt = scene(root, "create", "0", &create, true)?;
    let replay = scene(root, "create", "0", &create, true)?;
    assert_eq!(replay["revision"], receipt["revision"]);
    assert_eq!(replay["object_ids"], receipt["object_ids"]);
    assert_eq!(replay["replayed"], true);
    assert_eq!(inspect(root)?["scenes"][0]["document"], document(true));
    request(
        root,
        &[
            "timeline",
            "visual",
            "track-add",
            "--project",
            "project.lumeryn",
            "--id",
            TRACK,
            "--name",
            "Shapes",
            "--expected-revision",
            "1",
        ],
        true,
    )?;
    let add = [
        "--id",
        CLIP,
        "--scene-id",
        SCENE,
        "--track-id",
        TRACK,
        "--at-frame",
        "0",
        "--duration-frames",
        "30",
    ];
    let mut dry_add = add.to_vec();
    dry_add.push("--dry-run");
    scene(root, "add", "2", &dry_add, true)?;
    assert_eq!(inspect(root)?["revision"], 2);
    scene(root, "add", "2", &add, true)?;
    assert_eq!(
        scene(
            root,
            "range",
            "2",
            &[
                "--clip-id",
                CLIP,
                "--at-frame",
                "0",
                "--duration-frames",
                "15"
            ],
            false
        )?["error"]["code"],
        "revision_conflict"
    );
    let before = inspect(root)?;
    scene(root, "delete", "3", &["--scene-id", SCENE], false)?;
    assert_eq!(inspect(root)?, before);
    fs::write(
        root.join("edited.json"),
        serde_json::to_vec(&document(false))?,
    )?;
    scene(
        root,
        "set",
        "3",
        &["--scene-id", SCENE, "--document-file", "edited.json"],
        true,
    )?;
    scene(
        root,
        "range",
        "4",
        &[
            "--clip-id",
            CLIP,
            "--at-frame",
            "5",
            "--duration-frames",
            "20",
        ],
        true,
    )?;
    scene(
        root,
        "move",
        "5",
        &["--clip-id", CLIP, "--track-id", TRACK, "--at-frame", "10"],
        true,
    )?;
    let listing = request(
        root,
        &[
            "timeline",
            "scene",
            "inspect",
            "--project",
            "project.lumeryn",
        ],
        true,
    )?;
    assert_eq!(listing["revision"], 6);
    assert_eq!(listing["scenes"][0]["document"], document(false));
    assert_eq!(listing["scene_placements"][0]["at_frame"], 10);
    assert_eq!(listing["scene_placements"][0]["duration_frames"], 20);
    request(
        root,
        &[
            "project",
            "undo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "6",
        ],
        true,
    )?;
    assert_eq!(inspect(root)?["scene_placements"][0]["at_frame"], 5);
    request(
        root,
        &[
            "project",
            "redo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "7",
        ],
        true,
    )?;
    assert_eq!(inspect(root)?["scene_placements"][0]["at_frame"], 10);
    scene(root, "delete-placement", "8", &["--clip-id", CLIP], true)?;
    scene(root, "delete", "9", &["--scene-id", SCENE], true)?;
    let after = inspect(root)?;
    assert_eq!(after["revision"], 10);
    assert!(after.get("scenes").is_none());
    assert!(after.get("scene_placements").is_none());
    assert_eq!(fs::read(root.join("scene.json"))?, source_document);
    assert_eq!(
        request(
            root,
            &["project", "check", "--project", "project.lumeryn"],
            true
        )?["history"],
        "ok"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
#[ignore = "requires trusted FFmpeg/ffprobe and integrated scene export backend"]
fn scene_only_export_decodes_the_editable_shape_at_requested_frames() -> TestResult {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    create_project(root)?;
    scene(
        root,
        "create",
        "0",
        &[
            "--id",
            SCENE,
            "--name",
            "Shape",
            "--document-file",
            "scene.json",
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
            "--id",
            TRACK,
            "--name",
            "Shapes",
            "--expected-revision",
            "1",
        ],
        true,
    )?;
    scene(
        root,
        "add",
        "2",
        &[
            "--id",
            CLIP,
            "--scene-id",
            SCENE,
            "--track-id",
            TRACK,
            "--at-frame",
            "0",
            "--duration-frames",
            "30",
        ],
        true,
    )?;
    let result = request(
        root,
        &[
            "render",
            "--project",
            "project.lumeryn",
            "--output",
            "shape.mp4",
            "--expected-revision",
            "3",
            "--fps",
            "60",
        ],
        true,
    )?;
    assert_eq!(result["verified_full_decode"], true);
    assert_eq!(result["frames"], 60);
    assert_eq!(result["frame_rate"], "60/1");
    let ffmpeg = std::env::var_os("LUMERYN_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    let output = Command::new(ffmpeg)
        .current_dir(root)
        .args([
            "-v",
            "error",
            "-nostdin",
            "-i",
            "shape.mp4",
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout.len(), 128 * 72 * 3);
    let pixel = &output.stdout[(30 * 128 + 40) * 3..(30 * 128 + 40) * 3 + 3];
    assert!(
        pixel[0] > 200 && pixel[1] < 30 && pixel[2] < 30,
        "shape pixel {pixel:?}"
    );
    assert!(output.stdout[0..3].iter().all(|value| *value < 20));
    Ok(())
}
