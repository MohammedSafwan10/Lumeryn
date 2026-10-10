use lumeryn_core::{
    Batch, Edit, Error, MediaTools, Project, Result, Store,
    media::{Metadata, Video},
    model::{Clip, Media},
    storage::{HistoryState, history_state},
};
use uuid::Uuid;

fn fixture() -> Result<Project> {
    let mut project = Project::new("History".into(), 1920, 1080, "30/1".parse()?)?;
    let media_id = Uuid::new_v4();
    project.media.push(Media {
        id: media_id,
        path: "source.mp4".into(),
        metadata: Metadata {
            size_bytes: 1,
            blake3: None,
            audio: None,
            video: Some(Video {
                codec: "fixture".into(),
                width: 1920,
                height: 1080,
                frame_rate: project.frame_rate,
                frames: 1000,
                start_time_micros: 0,
                sample_aspect_ratio: "1:1".into(),
                color_transfer: None,
                constant_rate: true,
            }),
        },
    });
    project.clips.push(Clip {
        id: Uuid::new_v4(),
        media_id,
        track_id: project.video_track_id,
        at_frame: 0,
        source_in_frame: 10,
        duration_frames: 60,
    });
    project.validate()?;
    Ok(project)
}
fn batch(operation: Edit) -> Batch {
    Batch {
        operations: vec![operation],
    }
}
async fn execute(store: &mut Store, operation: Edit, revision: u64) -> Result<()> {
    store
        .execute(
            &batch(operation),
            Some(revision),
            None,
            false,
            &MediaTools::default(),
        )
        .await?;
    Ok(())
}
fn same_graph(mut a: Project, mut b: Project) -> Result<()> {
    a.revision = 0;
    b.revision = 0;
    assert_eq!(serde_json::to_value(a)?, serde_json::to_value(b)?);
    Ok(())
}
async fn state(store: &mut Store) -> Result<HistoryState> {
    history_state(&store.history().await?)
}

#[tokio::test]
async fn monotonic_undo_redo_and_branch_edit_keep_every_history_row() -> Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("source.mp4"), b"immutable")?;
    let original = fixture()?;
    let clip = original.clips[0].id;
    let mut store = Store::create(&dir.path().join("project.lumeryn"), &original).await?;
    execute(
        &mut store,
        Edit::SetCanvas {
            width: 1080,
            height: 1920,
        },
        0,
    )
    .await?;
    let first = store.inspect().await?;
    execute(
        &mut store,
        Edit::DeleteClip {
            clip_id: clip,
            ripple: true,
        },
        1,
    )
    .await?;
    let second = store.inspect().await?;
    assert_eq!(state(&mut store).await?.undo_depth, 2);
    execute(&mut store, Edit::Undo {}, 2).await?;
    let restored = store.inspect().await?;
    assert_eq!(restored.revision, 3);
    same_graph(restored, first.clone())?;
    assert_eq!(
        state(&mut store).await?,
        HistoryState {
            can_undo: true,
            can_redo: true,
            current_logical_revision: 1,
            undo_depth: 1,
            redo_depth: 1
        }
    );
    execute(&mut store, Edit::Undo {}, 3).await?;
    same_graph(store.inspect().await?, original.clone())?;
    assert_eq!(state(&mut store).await?.redo_depth, 2);
    execute(&mut store, Edit::Redo {}, 4).await?;
    same_graph(store.inspect().await?, first)?;
    execute(&mut store, Edit::Redo {}, 5).await?;
    same_graph(store.inspect().await?, second)?;
    execute(&mut store, Edit::Undo {}, 6).await?;
    execute(
        &mut store,
        Edit::SetCanvas {
            width: 1080,
            height: 1080,
        },
        7,
    )
    .await?;
    assert!(!state(&mut store).await?.can_redo);
    assert_eq!(state(&mut store).await?.current_logical_revision, 8);
    assert_eq!(store.history().await?.len(), 9);
    assert!(matches!(
        store
            .execute(
                &batch(Edit::Redo {}),
                Some(8),
                None,
                false,
                &MediaTools::default()
            )
            .await,
        Err(Error::Invalid(_))
    ));
    execute(&mut store, Edit::Undo {}, 8).await?;
    execute(&mut store, Edit::Undo {}, 9).await?;
    same_graph(store.inspect().await?, original)?;
    assert_eq!(std::fs::read(dir.path().join("source.mp4"))?, b"immutable");
    store.integrity_check().await?;
    Ok(())
}

#[tokio::test]
async fn history_actions_are_atomic_dry_run_replay_and_revision_checked() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("project.lumeryn");
    let original = fixture()?;
    let mut store = Store::create(&path, &original).await?;
    assert!(
        store
            .execute(
                &batch(Edit::Undo {}),
                Some(0),
                None,
                false,
                &MediaTools::default()
            )
            .await
            .is_err()
    );
    execute(
        &mut store,
        Edit::SetCanvas {
            width: 720,
            height: 1280,
        },
        0,
    )
    .await?;
    let edited = store.inspect().await?;
    let request = batch(Edit::Undo {});
    let dry = store
        .execute(
            &request,
            Some(1),
            Some("undo-one"),
            true,
            &MediaTools::default(),
        )
        .await?;
    assert!(dry.dry_run);
    assert_eq!(dry.object_ids, vec![original.id]);
    assert_eq!(store.inspect().await?.revision, 1);
    assert_eq!(state(&mut store).await?.current_logical_revision, 1);
    let committed = store
        .execute(
            &request,
            Some(1),
            Some("undo-one"),
            false,
            &MediaTools::default(),
        )
        .await?;
    assert_eq!(committed.revision, 2);
    let replay = store
        .execute(
            &request,
            Some(1),
            Some("undo-one"),
            false,
            &MediaTools::default(),
        )
        .await?;
    assert!(replay.replayed);
    assert_eq!(replay.revision, 2);
    assert!(matches!(
        store
            .execute(
                &batch(Edit::Redo {}),
                Some(1),
                None,
                false,
                &MediaTools::default()
            )
            .await,
        Err(Error::Conflict { current: 2, .. })
    ));
    assert!(
        store
            .execute(
                &batch(Edit::Redo {}),
                Some(2),
                Some("undo-one"),
                false,
                &MediaTools::default()
            )
            .await
            .is_err()
    );
    let compound = Batch {
        operations: vec![
            Edit::Redo {},
            Edit::SetCanvas {
                width: 640,
                height: 480,
            },
        ],
    };
    assert!(
        store
            .execute(&compound, Some(2), None, false, &MediaTools::default())
            .await
            .is_err()
    );
    assert_eq!(store.inspect().await?.revision, 2);
    let mut reopened = Store::open(&path).await?;
    assert!(
        reopened
            .execute(
                &request,
                Some(1),
                Some("undo-one"),
                false,
                &MediaTools::default()
            )
            .await?
            .replayed
    );
    execute(&mut reopened, Edit::Redo {}, 2).await?;
    same_graph(reopened.inspect().await?, edited)?;
    reopened.integrity_check().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_undo_writers_cannot_restore_the_same_revision_twice() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("project.lumeryn");
    let mut first = Store::create(&path, &fixture()?).await?;
    execute(
        &mut first,
        Edit::SetCanvas {
            width: 640,
            height: 480,
        },
        0,
    )
    .await?;
    let mut second = Store::open(&path).await?;
    let one = batch(Edit::Undo {});
    let two = batch(Edit::Undo {});
    let tools = MediaTools::default();
    let (a, b) = tokio::join!(
        first.execute(&one, Some(1), None, false, &tools),
        second.execute(&two, Some(1), None, false, &tools)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(
        matches!(a, Err(Error::Conflict { current: 2, .. }))
            || matches!(b, Err(Error::Conflict { current: 2, .. }))
    );
    assert_eq!(first.inspect().await?.revision, 2);
    assert_eq!(first.history().await?.len(), 3);
    first.integrity_check().await?;
    Ok(())
}

#[tokio::test]
async fn backup_rebases_all_undo_targets_and_keeps_source_files_unchanged() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let source_dir = dir.path().join("original");
    let backup_dir = dir.path().join("copy");
    std::fs::create_dir_all(&source_dir)?;
    std::fs::create_dir_all(&backup_dir)?;
    std::fs::write(source_dir.join("source.mp4"), b"immutable")?;
    let original = fixture()?;
    let mut store = Store::create(&source_dir.join("project.lumeryn"), &original).await?;
    execute(
        &mut store,
        Edit::DeleteClip {
            clip_id: original.clips[0].id,
            ripple: true,
        },
        0,
    )
    .await?;
    execute(&mut store, Edit::Undo {}, 1).await?;
    let path = backup_dir.join("project.lumeryn");
    store.backup(&path).await?;
    let mut backup = Store::open(&path).await?;
    execute(&mut backup, Edit::Redo {}, 2).await?;
    assert!(backup.inspect().await?.clips.is_empty());
    execute(&mut backup, Edit::Undo {}, 3).await?;
    let restored = backup.inspect().await?;
    assert_eq!(restored.clips.len(), 1);
    assert_eq!(
        restored.media[0].path,
        source_dir.canonicalize()?.join("source.mp4")
    );
    assert_eq!(store.inspect().await?.revision, 2);
    assert_eq!(std::fs::read(source_dir.join("source.mp4"))?, b"immutable");
    backup.integrity_check().await?;
    Ok(())
}

#[test]
fn strict_history_commands_and_metadata_reject_malformed_sequences() -> Result<()> {
    assert!(history_state(&vec![serde_json::Value::Null; 100_001]).is_err());
    for value in [
        serde_json::json!({"operation":"undo","revision":0}),
        serde_json::json!({"operation":"redo","anything":true}),
    ] {
        assert!(serde_json::from_value::<Edit>(value).is_err());
    }
    for history in [
        vec![],
        vec![serde_json::json!({"revision":0,"operations":[{"operation":"undo"}]})],
        vec![
            serde_json::json!({"revision":0,"operations":[]}),
            serde_json::json!({"revision":1,"operations":[{"operation":"redo"}]}),
        ],
        vec![serde_json::json!({"revision":1,"operations":[]})],
        vec![
            serde_json::json!({"revision":0,"operations":[]}),
            serde_json::json!({"revision":2,"operations":[{"operation":"set_canvas","width":640,"height":480}]}),
        ],
    ] {
        assert!(history_state(&history).is_err());
    }
    assert_eq!(
        serde_json::to_value(Edit::Undo {})?,
        serde_json::json!({"operation":"undo"})
    );
    Ok(())
}

#[tokio::test]
async fn integrity_detects_valid_but_wrong_restored_graph() -> Result<()> {
    use sqlx::{Connection, SqliteConnection};
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("project.lumeryn");
    let original = fixture()?;
    let mut store = Store::create(&path, &original).await?;
    execute(
        &mut store,
        Edit::SetCanvas {
            width: 640,
            height: 480,
        },
        0,
    )
    .await?;
    execute(&mut store, Edit::Undo {}, 1).await?;
    let mut wrong = store.inspect().await?;
    wrong.width = 320;
    wrong.height = 180;
    wrong.validate()?;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(&path);
    let mut connection = SqliteConnection::connect_with(&options).await?;
    let mut transaction = connection.begin().await?;
    let snapshot = serde_json::to_string(&wrong)?;
    sqlx::query("UPDATE project SET snapshot=? WHERE id=1")
        .bind(&snapshot)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("UPDATE history SET snapshot=? WHERE revision=2")
        .bind(&snapshot)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    assert!(
        matches!(store.integrity_check().await,Err(Error::Invalid(message)) if message.contains("restore target"))
    );
    Ok(())
}

#[test]
fn metadata_stack_benchmark_ten_thousand_revisions() -> Result<()> {
    let mut history = vec![serde_json::json!({"revision":0,"operations":[]})];
    history.extend((1..10_000).map(|revision|serde_json::json!({"revision":revision,"operations":[{"operation":"set_canvas","width":640,"height":480}]})));
    let mut samples = Vec::new();
    for _ in 0..12 {
        let started = std::time::Instant::now();
        let state = history_state(&history)?;
        assert_eq!(state.undo_depth, 9999);
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "HISTORY_METADATA_BENCH revisions=10000 samples=12 p50_ms={:.3} p95_ms={:.3}; CPU metadata-only derivation, no database/snapshot IO",
        samples[6], samples[11]
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed FFmpeg/FFprobe; actual audio import undo/redo and immutable source"]
async fn imported_media_undo_removes_only_project_reference_and_redo_restores_it() -> Result<()> {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.lumeryn-scratch/media");
    std::fs::create_dir_all(&root)?;
    let dir = tempfile::tempdir_in(root)?;
    let source = dir.path().join("tone.ogg");
    let tools = MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG")
            .map_or_else(|| "ffmpeg".into(), std::path::PathBuf::from),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map_or_else(|| "ffprobe".into(), std::path::PathBuf::from),
        ..Default::default()
    };
    let output = tokio::process::Command::new(&tools.ffmpeg)
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-nostdin",
            "-n",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100",
            "-t",
            "0.2",
            "-c:a",
            "libvorbis",
        ])
        .arg(&source)
        .output()
        .await?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&source)?;
    let original = Project::new("Import undo".into(), 640, 480, "30/1".parse()?)?;
    let mut store = Store::create(&dir.path().join("project.lumeryn"), &original).await?;
    let media_id = Uuid::new_v4();
    store
        .execute(
            &batch(Edit::Import {
                id: media_id,
                path: source.clone(),
            }),
            Some(0),
            None,
            false,
            &tools,
        )
        .await?;
    let imported = store.inspect().await?;
    assert_eq!(store.search("tone", 10).await?.len(), 1);
    execute(&mut store, Edit::Undo {}, 1).await?;
    assert!(store.inspect().await?.media.is_empty());
    assert!(store.search("tone", 10).await?.is_empty());
    execute(&mut store, Edit::Redo {}, 2).await?;
    same_graph(store.inspect().await?, imported)?;
    assert_eq!(store.search("tone", 10).await?.len(), 1);
    assert_eq!(std::fs::read(&source)?, bytes);
    store.integrity_check().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed FFmpeg/FFprobe; authoritative paste replay after undo removes imported source reference"]
async fn authoritative_paste_replay_survives_undo_of_import_and_binds_payload() -> Result<()> {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.lumeryn-scratch/media");
    std::fs::create_dir_all(&root)?;
    let dir = tempfile::tempdir_in(root)?;
    let source = dir.path().join("video.mkv");
    let tools = MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG")
            .map_or_else(|| "ffmpeg".into(), std::path::PathBuf::from),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map_or_else(|| "ffprobe".into(), std::path::PathBuf::from),
        ..Default::default()
    };
    let output = tokio::process::Command::new(&tools.ffmpeg)
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-nostdin",
            "-n",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:size=32x24:rate=30",
            "-frames:v",
            "60",
            "-c:v",
            "ffv1",
        ])
        .arg(&source)
        .output()
        .await?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&source)?;
    let original = Project::new("Paste undo".into(), 640, 480, "30/1".parse()?)?;
    let mut store = Store::create(&dir.path().join("project.lumeryn"), &original).await?;
    let media_id = Uuid::new_v4();
    let first = Uuid::new_v4();
    let pasted = Uuid::new_v4();
    store
        .execute(
            &batch(Edit::Import {
                id: media_id,
                path: source.clone(),
            }),
            Some(0),
            None,
            false,
            &tools,
        )
        .await?;
    execute(
        &mut store,
        Edit::AddClip {
            id: first,
            media_id,
            at_frame: 0,
            source_in_frame: 5,
            duration_frames: 20,
        },
        1,
    )
    .await?;
    let clipboard = lumeryn_core::clipboard::Clipboard::copy(&store.inspect().await?, first)?;
    let request = batch(Edit::PasteClip {
        id: pasted,
        at_frame: 20,
        clipboard: clipboard.clone(),
    });
    let receipt = store
        .execute(&request, Some(2), Some("paste-once"), false, &tools)
        .await?;
    assert_eq!(receipt.object_ids, vec![pasted]);
    assert_eq!(store.inspect().await?.clips.len(), 2);
    for revision in 3..6 {
        execute(&mut store, Edit::Undo {}, revision).await?;
    }
    assert!(store.inspect().await?.media.is_empty());
    assert!(store.inspect().await?.clips.is_empty());
    let replay = store
        .execute(&request, Some(2), Some("paste-once"), false, &tools)
        .await?;
    assert!(replay.replayed);
    assert_eq!(replay.revision, 3);
    assert_eq!(store.inspect().await?.revision, 6);
    assert!(store.inspect().await?.media.is_empty());
    let changed = batch(Edit::PasteClip {
        id: pasted,
        at_frame: 20,
        clipboard: lumeryn_core::clipboard::Clipboard {
            duration_frames: 19,
            ..clipboard
        },
    });
    assert!(
        store
            .execute(&changed, Some(6), Some("paste-once"), false, &tools)
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(source)?, bytes);
    store.integrity_check().await?;
    Ok(())
}
