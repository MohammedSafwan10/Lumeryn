//! Shared-service audio contracts; media acceptance uses only generated fixtures.
use serde_json::{Value, json};
use std::{
    path::Path,
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
    assert!(output.stderr.is_empty());
    let envelope: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(envelope["ok"], true);
    Ok(envelope["result"].clone())
}

fn failure(directory: &Path, args: &[&str], code: &str, exit: i32) -> TestResult {
    let output = run(directory, args)?;
    assert_eq!(output.status.code(), Some(exit));
    assert!(output.stdout.is_empty());
    let envelope: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(envelope["error"]["code"], code, "{envelope}");
    Ok(())
}

fn inspect(directory: &Path) -> TestResult<Value> {
    success(
        directory,
        &["project", "inspect", "--project", "project.lumeryn"],
    )
}

fn audio_edit(
    directory: &Path,
    command: &str,
    clip: &str,
    revision: &str,
    args: &[&str],
) -> TestResult<Value> {
    let mut arguments = vec![
        "timeline",
        "audio",
        command,
        "--project",
        "project.lumeryn",
        "--clip-id",
        clip,
        "--expected-revision",
        revision,
    ];
    arguments.extend_from_slice(args);
    success(directory, &arguments)
}

#[test]
fn independent_audio_discovery_and_arguments_are_strict() -> TestResult {
    let directory = tempfile::tempdir()?;
    let directory = directory.path();
    let capabilities = success(directory, &["capabilities"])?;
    assert_eq!(
        capabilities["project_schema_version"],
        lumeryn_core::model::SCHEMA_VERSION
    );
    let implemented = capabilities["implemented"]
        .as_array()
        .ok_or("missing capabilities")?;
    for command in ["add", "move", "trim", "range", "properties", "delete"] {
        assert!(implemented.contains(&json!(format!("timeline.audio.{command}"))));
        failure(
            directory,
            &["timeline", "audio", command],
            "invalid_arguments",
            2,
        )?;
    }
    let schema = success(directory, &["schema"])?;
    let variants = schema["$defs"]["Edit"]["oneOf"]
        .as_array()
        .ok_or("missing edits")?;
    for operation in [
        "add_audio_clip",
        "move_audio_clip",
        "trim_audio_clip",
        "set_audio_range",
        "set_audio_properties",
        "delete_audio_clip",
    ] {
        assert!(
            variants
                .iter()
                .any(|variant| variant["properties"]["operation"]["const"] == operation)
        );
    }
    let media = Uuid::new_v4().to_string();
    failure(
        directory,
        &[
            "timeline",
            "audio",
            "add",
            "--project",
            "missing.lumeryn",
            "--media-id",
            &media,
            "--duration-frames",
            "30",
            "--idempotency-key",
            "missing-explicit-id",
        ],
        "invalid_input",
        2,
    )?;
    Ok(())
}

#[cfg(windows)]
#[tokio::test]
async fn explicit_legacy_migration_notifies_live_clients_without_fabricating_revision() -> TestResult
{
    let directory = tempfile::tempdir()?;
    let directory = directory.path();
    let mut legacy =
        lumeryn_core::Project::new("Legacy audio upgrade".into(), 320, 180, "30".parse()?)?;
    legacy.schema_version = 1;
    legacy.audio_track_id = None;
    drop(lumeryn_core::Store::create(&directory.join("project.lumeryn"), &legacy).await?);
    let watch = success(
        directory,
        &[
            "project",
            "watch",
            "--project",
            "project.lumeryn",
            "--wait-ms",
            "0",
        ],
    )?;
    assert_eq!(watch["snapshot"]["schema_version"], 1);
    assert_eq!(watch["snapshot"]["revision"], 0);
    let sequence = watch["sequence"]
        .as_u64()
        .ok_or("missing sequence")?
        .to_string();
    failure(
        directory,
        &[
            "timeline",
            "canvas",
            "--project",
            "project.lumeryn",
            "--width",
            "640",
            "--height",
            "360",
            "--expected-revision",
            "0",
        ],
        "unsupported",
        4,
    )?;
    assert_eq!(inspect(directory)?["schema_version"], 1);
    success(
        directory,
        &[
            "project",
            "backup",
            "--project",
            "project.lumeryn",
            "--output",
            "legacy-backup.lumeryn",
        ],
    )?;
    assert_eq!(
        success(
            directory,
            &["project", "inspect", "--project", "legacy-backup.lumeryn"]
        )?["schema_version"],
        1
    );
    let migration = success(
        directory,
        &["project", "migrate", "--project", "project.lumeryn"],
    )?;
    assert_eq!(migration["graph_from"], 1);
    assert_eq!(migration["graph_to"], lumeryn_core::model::SCHEMA_VERSION);
    assert_eq!(migration["project_revision"], 0);
    let changed = success(
        directory,
        &[
            "project",
            "watch",
            "--project",
            "project.lumeryn",
            "--wait-ms",
            "0",
            "--after-sequence",
            &sequence,
        ],
    )?;
    assert_eq!(
        changed["snapshot"]["schema_version"],
        lumeryn_core::model::SCHEMA_VERSION
    );
    assert_eq!(changed["snapshot"]["revision"], 0);
    assert!(changed["snapshot"]["audio_track_id"].as_str().is_some());
    assert!(changed["sequence"].as_u64().ok_or("missing sequence")? > sequence.parse()?);
    assert!(
        changed["events"]
            .as_array()
            .ok_or("missing events")?
            .iter()
            .any(|event| event["kind"] == "storage_migrated")
    );
    assert_eq!(
        success(
            directory,
            &["project", "history", "--project", "project.lumeryn"]
        )?
        .as_array()
        .ok_or("missing history")?
        .len(),
        1
    );
    success(
        directory,
        &["project", "check", "--project", "project.lumeryn"],
    )?;
    Ok(())
}

fn ffmpeg(directory: &Path, args: &[&str]) -> TestResult<Vec<u8>> {
    let tool = std::env::var_os("LUMERYN_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    let output = Command::new(tool)
        .current_dir(directory)
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
#[test]
#[ignore = "requires installed FFmpeg/ffprobe; generated FFV1/WAV fixtures only"]
fn independent_audio_edits_live_watch_undo_and_decoded_export() -> TestResult {
    let directory = tempfile::tempdir()?;
    let directory = directory.path();
    ffmpeg(
        directory,
        &[
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=30:duration=2",
            "-c:v",
            "ffv1",
            "-an",
            "footage.mkv",
        ],
    )?;
    ffmpeg(
        directory,
        &[
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=2",
            "-c:a",
            "pcm_s16le",
            "voice.wav",
        ],
    )?;
    let video_bytes = std::fs::read(directory.join("footage.mkv"))?;
    let audio_bytes = std::fs::read(directory.join("voice.wav"))?;
    success(
        directory,
        &[
            "project",
            "create",
            "--project",
            "project.lumeryn",
            "--name",
            "Independent audio acceptance",
            "--width",
            "320",
            "--height",
            "180",
        ],
    )?;
    let video = Uuid::new_v4().to_string();
    let audio = Uuid::new_v4().to_string();
    let clip = Uuid::new_v4().to_string();
    let sound = Uuid::new_v4().to_string();
    for (id, source, revision) in [(&video, "footage.mkv", "0"), (&audio, "voice.wav", "1")] {
        success(
            directory,
            &[
                "media",
                "import",
                "--project",
                "project.lumeryn",
                "--id",
                id,
                "--path",
                source,
                "--expected-revision",
                revision,
            ],
        )?;
    }
    let imported = success(
        directory,
        &[
            "media",
            "inspect",
            "--project",
            "project.lumeryn",
            "--media-id",
            &audio,
        ],
    )?;
    assert_eq!(imported["media"]["metadata"]["video"], Value::Null);
    assert_eq!(
        imported["media"]["metadata"]["audio"]["canonical_samples_48k"],
        96_000
    );
    assert_eq!(imported["media"]["usage_count"], 0);
    success(
        directory,
        &[
            "timeline",
            "clip",
            "add",
            "--project",
            "project.lumeryn",
            "--id",
            &clip,
            "--media-id",
            &video,
            "--duration-frames",
            "60",
            "--expected-revision",
            "2",
        ],
    )?;
    let before = inspect(directory)?;
    let batch = json!({"operations":[{"operation":"add_audio_clip","id":sound,"media_id":audio,"at_frame":0,"source_in_sample":0,"duration_frames":30,"gain_millidecibels":0,"muted":false}]});
    std::fs::write(
        directory.join("audio-batch.json"),
        serde_json::to_vec(&batch)?,
    )?;
    assert_eq!(
        success(
            directory,
            &[
                "project",
                "batch",
                "--project",
                "project.lumeryn",
                "--file",
                "audio-batch.json",
                "--expected-revision",
                "3",
                "--dry-run"
            ]
        )?["dry_run"],
        true
    );
    assert_eq!(inspect(directory)?, before);
    let watch = success(
        directory,
        &[
            "project",
            "watch",
            "--project",
            "project.lumeryn",
            "--wait-ms",
            "0",
        ],
    )?;
    let sequence = watch["sequence"]
        .as_u64()
        .ok_or("missing sequence")?
        .to_string();
    let add = [
        "timeline",
        "audio",
        "add",
        "--project",
        "project.lumeryn",
        "--id",
        &sound,
        "--media-id",
        &audio,
        "--duration-frames",
        "30",
        "--gain-millidecibels",
        "-6000",
        "--expected-revision",
        "3",
        "--idempotency-key",
        "audio-add-once",
    ];
    assert_eq!(success(directory, &add)?["revision"], 4);
    assert_eq!(success(directory, &add)?["replayed"], true);
    let changed = success(
        directory,
        &[
            "project",
            "watch",
            "--project",
            "project.lumeryn",
            "--wait-ms",
            "0",
            "--after-sequence",
            &sequence,
        ],
    )?;
    assert_eq!(changed["snapshot"]["revision"], 4);
    assert_eq!(changed["snapshot"]["audio_clips"][0]["id"], sound);
    assert_eq!(
        success(
            directory,
            &[
                "media",
                "inspect",
                "--project",
                "project.lumeryn",
                "--media-id",
                &audio
            ]
        )?["media"]["usage_count"],
        1
    );
    failure(
        directory,
        &[
            "timeline",
            "audio",
            "move",
            "--project",
            "project.lumeryn",
            "--clip-id",
            &sound,
            "--at-frame",
            "15",
            "--expected-revision",
            "3",
        ],
        "revision_conflict",
        3,
    )?;
    failure(
        directory,
        &[
            "media",
            "remove",
            "--project",
            "project.lumeryn",
            "--media-id",
            &audio,
            "--expected-revision",
            "4",
        ],
        "invalid_input",
        2,
    )?;
    assert_eq!(
        audio_edit(directory, "move", &sound, "4", &["--at-frame", "30"])?["revision"],
        5
    );
    assert_eq!(
        audio_edit(
            directory,
            "trim",
            &sound,
            "5",
            &["--source-in-sample", "12", "--duration-frames", "15"]
        )?["revision"],
        6
    );
    assert_eq!(
        audio_edit(
            directory,
            "range",
            &sound,
            "6",
            &[
                "--at-frame",
                "15",
                "--source-in-sample",
                "24000",
                "--duration-frames",
                "30"
            ]
        )?["revision"],
        7
    );
    assert_eq!(
        audio_edit(
            directory,
            "properties",
            &sound,
            "7",
            &["--gain-millidecibels", "-6000", "--muted"]
        )?["revision"],
        8
    );
    let timeline = success(
        directory,
        &["timeline", "inspect", "--project", "project.lumeryn"],
    )?;
    let audio_clip = &timeline["audio_clips"][0];
    assert_eq!(audio_clip["at_frame"], 15);
    assert_eq!(audio_clip["source_in_sample"], 24_000);
    assert_eq!(audio_clip["duration_frames"], 30);
    assert_eq!(audio_clip["gain_millidecibels"], -6000);
    assert_eq!(audio_clip["muted"], true);
    assert_eq!(timeline["clips"], before["clips"]);
    let exported = success(
        directory,
        &[
            "render",
            "--project",
            "project.lumeryn",
            "--output",
            "muted.mp4",
            "--expected-revision",
            "8",
        ],
    )?;
    assert_eq!(exported["verified_full_decode"], true);
    assert_eq!(exported["frames"], 60);
    let muted_pcm = ffmpeg(
        directory,
        &[
            "-v",
            "error",
            "-nostdin",
            "-i",
            "muted.mp4",
            "-map",
            "0:a:0",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-f",
            "f32le",
            "-",
        ],
    )?;
    assert!(muted_pcm.len() >= 96_000 * 8);
    assert!(muted_pcm.chunks_exact(4).all(|sample| {
        f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]).abs() < 0.00001
    }));
    assert_eq!(
        audio_edit(
            directory,
            "properties",
            &sound,
            "8",
            &["--gain-millidecibels", "-6000"]
        )?["revision"],
        9
    );
    let audible = success(
        directory,
        &[
            "render",
            "--project",
            "project.lumeryn",
            "--output",
            "audible.mp4",
            "--expected-revision",
            "9",
        ],
    )?;
    assert_eq!(audible["verified_full_decode"], true);
    let pcm = ffmpeg(
        directory,
        &[
            "-v",
            "error",
            "-nostdin",
            "-i",
            "audible.mp4",
            "-map",
            "0:a:0",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-f",
            "f32le",
            "-",
        ],
    )?;
    let samples = pcm
        .chunks_exact(8)
        .map(|sample| f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]))
        .collect::<Vec<_>>();
    assert!(samples.len() >= 96_000);
    assert!(
        samples[0..20_000]
            .iter()
            .all(|sample| sample.abs() < 0.00001)
    );
    assert!(
        samples[25_000..70_000]
            .iter()
            .any(|sample| sample.abs() > 0.02)
    );
    assert!(
        samples[76_000..96_000]
            .iter()
            .all(|sample| sample.abs() < 0.00001)
    );
    assert_eq!(
        audio_edit(directory, "delete", &sound, "9", &[])?["revision"],
        10
    );
    assert!(
        success(
            directory,
            &["timeline", "inspect", "--project", "project.lumeryn"]
        )?["audio_clips"]
            .as_array()
            .ok_or("missing audio lane")?
            .is_empty()
    );
    assert_eq!(
        success(
            directory,
            &[
                "project",
                "undo",
                "--project",
                "project.lumeryn",
                "--expected-revision",
                "10"
            ]
        )?["revision"],
        11
    );
    assert_eq!(inspect(directory)?["audio_clips"][0]["id"], sound);
    assert_eq!(inspect(directory)?["clips"], before["clips"]);
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
    assert_eq!(std::fs::read(directory.join("footage.mkv"))?, video_bytes);
    assert_eq!(std::fs::read(directory.join("voice.wav"))?, audio_bytes);
    Ok(())
}
