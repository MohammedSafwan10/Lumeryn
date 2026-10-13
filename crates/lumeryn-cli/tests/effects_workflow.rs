//! Colour/fade CLI acceptance, including real decoded fade endpoints.
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

fn effects(
    root: &Path,
    id: &str,
    revision: &str,
    extra: &[&str],
    succeeds: bool,
) -> TestResult<Value> {
    let mut args = vec![
        "timeline",
        "visual",
        "effects",
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

fn fade_document() -> Value {
    json!({"version":1,"exposure_millistops":0,"contrast_basis_points":10000,"saturation_basis_points":10000,"temperature_basis_points":0,"fades":{"in_frames":10,"out_frames":10}})
}

#[test]
fn strict_effect_files_and_exclusive_modes_do_not_mutate_a_project() -> TestResult {
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
            "Strict effects",
        ],
        true,
    )?;
    let before = inspect(root)?;
    let id = "00000000-0000-4000-8000-000000000001";
    let mut unknown = fade_document();
    unknown["lut_path"] = "untrusted".into();
    let mut nested = fade_document();
    nested["fades"]["extra"] = true.into();
    let mut missing = fade_document();
    missing
        .as_object_mut()
        .ok_or("effects object")?
        .remove("version");
    for bytes in [
        b"{".to_vec(),
        serde_json::to_vec(&unknown)?,
        serde_json::to_vec(&nested)?,
        serde_json::to_vec(&missing)?,
        vec![b' '; 16 * 1024 + 1],
    ] {
        fs::write(root.join("effects.json"), bytes)?;
        assert_eq!(
            effects(root, id, "0", &["--effects-file", "effects.json"], false)?["error"]["code"],
            "invalid_input"
        );
        assert_eq!(inspect(root)?, before);
    }
    for arguments in [
        vec!["--reset", "--effects-file", "effects.json"],
        vec!["--reset", "--preset", "neutral"],
        vec!["--preset", "soft_mono", "--intensity-basis-points", "10001"],
        vec!["--intensity-basis-points", "5000", "--reset"],
    ] {
        assert_eq!(
            effects(root, id, "0", &arguments, false)?["error"]["code"],
            "invalid_arguments"
        );
        assert_eq!(inspect(root)?, before);
    }
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
    Ok(())
}

#[cfg(windows)]
#[test]
#[ignore = "requires trusted FFmpeg/ffprobe and integrated effects compositor"]
fn real_colour_preset_fades_reset_and_history_share_the_stored_graph() -> TestResult {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    let ffmpeg = std::env::var_os("LUMERYN_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    let generated = Command::new(&ffmpeg)
        .current_dir(root)
        .args([
            "-v",
            "error",
            "-nostdin",
            "-n",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=128x72:r=30:d=1",
            "-c:v",
            "ffv1",
            "source.mkv",
        ])
        .output()?;
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let source = fs::read(root.join("source.mkv"))?;
    request(
        root,
        &[
            "project",
            "create",
            "--project",
            "project.lumeryn",
            "--name",
            "Colour and fades",
            "--width",
            "128",
            "--height",
            "72",
            "--fps",
            "30",
        ],
        true,
    )?;
    request(
        root,
        &[
            "media",
            "import",
            "--project",
            "project.lumeryn",
            "--path",
            "source.mkv",
            "--expected-revision",
            "0",
        ],
        true,
    )?;
    let imported = inspect(root)?;
    let media_id = imported["media"][0]["id"].as_str().ok_or("media ID")?;
    request(
        root,
        &[
            "timeline",
            "clip",
            "add",
            "--project",
            "project.lumeryn",
            "--media-id",
            media_id,
            "--duration-frames",
            "30",
            "--expected-revision",
            "1",
        ],
        true,
    )?;
    let before = inspect(root)?;
    let clip = before["clips"][0]["id"].as_str().ok_or("clip ID")?;
    fs::write(
        root.join("fades.json"),
        serde_json::to_vec(&fade_document())?,
    )?;
    effects(
        root,
        clip,
        "2",
        &["--effects-file", "fades.json", "--dry-run"],
        true,
    )?;
    assert_eq!(inspect(root)?, before);
    effects(
        root,
        clip,
        "2",
        &["--effects-file", "fades.json", "--idempotency-key", "fades"],
        true,
    )?;
    let faded = inspect(root)?;
    assert_eq!(faded["visual_settings"][clip]["effects"], fade_document());
    assert_eq!(
        effects(root, clip, "2", &["--reset"], false)?["error"]["code"],
        "revision_conflict"
    );
    let mut invalid = fade_document();
    invalid["fades"]["in_frames"] = 21.into();
    fs::write(root.join("invalid.json"), serde_json::to_vec(&invalid)?)?;
    effects(root, clip, "3", &["--effects-file", "invalid.json"], false)?;
    assert_eq!(inspect(root)?, faded);
    let result = request(
        root,
        &[
            "render",
            "--project",
            "project.lumeryn",
            "--output",
            "fades.mp4",
            "--expected-revision",
            "3",
        ],
        true,
    )?;
    assert_eq!(result["verified_full_decode"], true);
    assert_eq!(result["frames"], 30);
    assert_eq!(result["frame_rate"], "30/1");
    let decoded = Command::new(&ffmpeg)
        .current_dir(root)
        .args([
            "-v",
            "error",
            "-nostdin",
            "-i",
            "fades.mp4",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()?;
    assert!(
        decoded.status.success(),
        "{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    const FRAME_BYTES: usize = 128 * 72 * 3;
    assert_eq!(decoded.stdout.len(), 30 * FRAME_BYTES);
    let red_at = |frame: usize| decoded.stdout[frame * FRAME_BYTES + (36 * 128 + 64) * 3];
    assert!(red_at(0) < 20, "fade-in start {}", red_at(0));
    assert!(
        (90..=165).contains(&red_at(5)),
        "fade midpoint {}",
        red_at(5)
    );
    assert!(red_at(15) > 220, "fade plateau {}", red_at(15));
    assert!(red_at(29) < 45, "fade-out tail {}", red_at(29));
    effects(
        root,
        clip,
        "3",
        &["--preset", "soft_mono", "--intensity-basis-points", "10000"],
        true,
    )?;
    let mono = inspect(root)?;
    assert_eq!(
        mono["visual_settings"][clip]["effects"]["saturation_basis_points"],
        0
    );
    assert_eq!(
        mono["visual_settings"][clip]["effects"]["fades"],
        json!({"in_frames":0,"out_frames":0})
    );
    request(
        root,
        &[
            "render",
            "--project",
            "project.lumeryn",
            "--output",
            "mono.mp4",
            "--expected-revision",
            "4",
        ],
        true,
    )?;
    let grey = Command::new(&ffmpeg)
        .current_dir(root)
        .args([
            "-v",
            "error",
            "-nostdin",
            "-i",
            "mono.mp4",
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()?;
    assert!(grey.status.success());
    assert_eq!(grey.stdout.len(), FRAME_BYTES);
    let pixel = &grey.stdout[(36 * 128 + 64) * 3..(36 * 128 + 64) * 3 + 3];
    assert!(
        pixel[0].abs_diff(pixel[1]) < 8 && pixel[1].abs_diff(pixel[2]) < 8,
        "monochrome {pixel:?}"
    );
    effects(root, clip, "4", &["--reset"], true)?;
    assert!(inspect(root)?.get("visual_settings").is_none());
    request(
        root,
        &[
            "project",
            "undo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "5",
        ],
        true,
    )?;
    assert_eq!(
        inspect(root)?["visual_settings"][clip]["effects"],
        mono["visual_settings"][clip]["effects"]
    );
    request(
        root,
        &[
            "project",
            "redo",
            "--project",
            "project.lumeryn",
            "--expected-revision",
            "6",
        ],
        true,
    )?;
    assert!(inspect(root)?.get("visual_settings").is_none());
    assert_eq!(fs::read(root.join("source.mkv"))?, source);
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
