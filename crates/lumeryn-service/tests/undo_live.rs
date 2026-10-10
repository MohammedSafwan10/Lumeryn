#![cfg(windows)]

use lumeryn_core::{
    Batch, Edit, Error, Project, Result, Store,
    media::{Metadata, Video},
    model::{Clip, Media},
    storage::history_state,
};
use lumeryn_service::{Client, Request, ToolConfig};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{path::Path, time::Duration};
use tokio::task::JoinHandle;
use uuid::Uuid;

async fn service(path: &Path) -> Result<(Client, JoinHandle<Result<()>>)> {
    let path_owned = path.to_owned();
    let task = tokio::spawn(async move { lumeryn_service::serve(path_owned).await });
    let client = Client::existing(path)?;
    for _ in 0..100 {
        if client.watch(None, 0).await.is_ok() {
            return Ok((client, task));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    task.abort();
    Err(Error::Timeout)
}

async fn stop(task: JoinHandle<Result<()>>) {
    task.abort();
    assert!(task.await.is_err_and(|error| error.is_cancelled()));
    tokio::time::sleep(Duration::from_millis(25)).await;
}

fn fixture() -> Result<Project> {
    let mut project = Project::new("History fixture".into(), 64, 64, "30/1".parse()?)?;
    let media_id = Uuid::new_v4();
    project.media.push(Media {
        id: media_id,
        path: "source.mp4".into(),
        metadata: Metadata {
            size_bytes: 1,
            blake3: None,
            audio: None,
            video: Some(Video {
                codec: "mpeg4".into(),
                width: 64,
                height: 64,
                frame_rate: project.frame_rate,
                frames: 600,
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
        source_in_frame: 0,
        duration_frames: 300,
    });
    project.validate()?;
    Ok(project)
}

fn batch(edit: Edit) -> Batch {
    Batch {
        operations: vec![edit],
    }
}

fn trim(project: &Project, duration: u64) -> Batch {
    batch(Edit::TrimClip {
        clip_id: project.clips[0].id,
        source_in_frame: 0,
        duration_frames: duration,
    })
}

async fn execute(client: &Client, command: Batch, revision: u64) -> Result<serde_json::Value> {
    client
        .execute(command, Some(revision), None, false, ToolConfig::default())
        .await
}

#[tokio::test]
async fn concurrent_undo_replay_dry_run_branch_and_reconnect_are_monotonic() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let (first, task) = service(&path).await?;
    let second = Client::existing(&path)?;
    let original = fixture()?;
    first
        .call(Request::Create {
            project: original.clone(),
        })
        .await?;
    execute(&first, trim(&original, 200), 0).await?;
    let cursor = second.watch(None, 0).await?;
    let dry = first
        .execute(
            batch(Edit::Undo {}),
            Some(1),
            None,
            true,
            ToolConfig::default(),
        )
        .await?;
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["revision"], 1);
    assert_eq!(second.inspect().await?.clips[0].duration_frames, 200);
    assert_eq!(second.history().await?.len(), 2);
    assert_eq!(
        second.watch(Some(cursor.sequence), 0).await?.sequence,
        cursor.sequence
    );

    let (a, b) = tokio::join!(
        first.execute(
            batch(Edit::Undo {}),
            Some(1),
            Some("undo-a".into()),
            false,
            ToolConfig::default()
        ),
        second.execute(
            batch(Edit::Undo {}),
            Some(1),
            Some("undo-b".into()),
            false,
            ToolConfig::default()
        )
    );
    assert!(matches!(
        (&a, &b),
        (
            Ok(_),
            Err(Error::Conflict {
                expected: 1,
                current: 2
            })
        ) | (
            Err(Error::Conflict {
                expected: 1,
                current: 2
            }),
            Ok(_)
        )
    ));
    let events = second.watch(Some(cursor.sequence), 0).await?;
    assert_eq!(events.snapshot.as_ref().map(|p| p.revision), Some(2));
    let commits = events
        .events
        .iter()
        .filter(|e| e.kind == "project_committed")
        .collect::<Vec<_>>();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].revision, Some(2));
    assert!(commits[0].sequence > cursor.sequence);
    let key = if a.is_ok() { "undo-a" } else { "undo-b" };
    let replay = first
        .execute(
            batch(Edit::Undo {}),
            Some(1),
            Some(key.into()),
            false,
            ToolConfig::default(),
        )
        .await?;
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["revision"], 2);
    assert_eq!(second.inspect().await?.clips[0].duration_frames, 300);
    assert_eq!(
        second.watch(Some(events.sequence), 0).await?.sequence,
        events.sequence
    );

    execute(&second, batch(Edit::Redo {}), 2).await?;
    assert_eq!(first.inspect().await?.clips[0].duration_frames, 200);
    execute(&first, batch(Edit::Undo {}), 3).await?;
    execute(&second, trim(&original, 150), 4).await?;
    assert!(matches!(
        execute(&first, batch(Edit::Redo {}), 5).await,
        Err(Error::Invalid(_))
    ));
    let mixed = Batch {
        operations: vec![
            Edit::Undo {},
            Edit::SetCanvas {
                width: 128,
                height: 128,
            },
        ],
    };
    assert!(matches!(
        execute(&first, mixed, 5).await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(first.inspect().await?.revision, 5);
    let durable = first.history().await?;
    assert_eq!(durable.len(), 6);
    let state = history_state(&durable)?;
    assert!(state.can_undo);
    assert!(!state.can_redo);
    assert_eq!(state.current_logical_revision, 5);
    let branch_events = second.watch(Some(events.sequence), 0).await?;
    let commits = branch_events
        .events
        .iter()
        .filter(|event| event.kind == "project_committed")
        .collect::<Vec<_>>();
    assert_eq!(
        commits
            .iter()
            .filter_map(|event| event.revision)
            .collect::<Vec<_>>(),
        [3, 4, 5]
    );
    assert!(
        commits
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
    );
    first.call(Request::Check { assets: false }).await?;
    stop(task).await;

    let (reconnected, restart) = service(&path).await?;
    execute(&reconnected, batch(Edit::Undo {}), 5).await?;
    assert_eq!(reconnected.inspect().await?.clips[0].duration_frames, 300);
    assert!(history_state(&reconnected.history().await?)?.can_redo);
    let backup_directory = directory.path().join("backup");
    std::fs::create_dir(&backup_directory)?;
    let backup = backup_directory.join("project.lumeryn");
    reconnected
        .call(Request::Backup {
            output: backup.clone(),
        })
        .await?;
    assert_eq!(reconnected.inspect().await?.revision, 6);
    let mut copy = Store::open(&backup).await?;
    assert_eq!(copy.inspect().await?.schema_version, 1);
    assert_eq!(copy.integrity_check().await?["storage_version"], 1);
    copy.execute(
        &batch(Edit::Redo {}),
        Some(6),
        None,
        false,
        &lumeryn_core::MediaTools::default(),
    )
    .await?;
    let copied = copy.inspect().await?;
    assert_eq!(copied.revision, 7);
    assert_eq!(copied.clips[0].duration_frames, 150);
    assert!(copied.media[0].path.is_absolute());
    copy.integrity_check().await?;
    drop(copy);
    execute(&reconnected, batch(Edit::Redo {}), 6).await?;
    assert_eq!(reconnected.inspect().await?.revision, 7);
    stop(restart).await;
    Ok(())
}

#[tokio::test]
async fn corrupt_restore_target_is_rejected_without_commit_or_event() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let original = fixture()?;
    let (client, task) = service(&path).await?;
    client
        .call(Request::Create {
            project: original.clone(),
        })
        .await?;
    execute(&client, trim(&original, 200), 0).await?;
    stop(task).await;
    let mut connection =
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&path)).await?;
    let mut damaged = original.clone();
    damaged.id = Uuid::new_v4();
    sqlx::query("UPDATE history SET snapshot=? WHERE revision=0")
        .bind(serde_json::to_string(&damaged)?)
        .execute(&mut connection)
        .await?;
    connection.close().await?;
    let (client, restart) = service(&path).await?;
    let cursor = client.watch(None, 0).await?;
    assert!(matches!(
        execute(&client, batch(Edit::Undo {}), 1).await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(client.inspect().await?.revision, 1);
    assert_eq!(client.inspect().await?.clips[0].duration_frames, 200);
    assert_eq!(client.history().await?.len(), 2);
    assert_eq!(
        client.watch(Some(cursor.sequence), 0).await?.sequence,
        cursor.sequence
    );
    stop(restart).await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires trusted installed FFmpeg/ffprobe; generated fixture only"]
async fn render_retains_undone_snapshot_while_redo_and_cancel_preserve_project() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let source = directory.path().join("source.mp4");
    let tools = lumeryn_core::MediaTools::default();
    let args = [
        "-v",
        "error",
        "-nostdin",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=640x360:rate=30",
        "-frames:v",
        "1800",
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
    let digest = lumeryn_core::media::content_hash(&source).await?;
    let mut original = fixture()?;
    original.width = 640;
    original.height = 360;
    original.media[0].metadata = tools.probe(&source).await?;
    original.clips[0].duration_frames = 1800;
    let (client, task) = service(&path).await?;
    let second = Client::existing(&path)?;
    client
        .call(Request::Create {
            project: original.clone(),
        })
        .await?;
    execute(&client, trim(&original, 600), 0).await?;
    execute(&second, batch(Edit::Undo {}), 1).await?;
    let cursor = client.watch(None, 0).await?;
    let job_id = Uuid::new_v4();
    let output = directory.path().join("historical.mp4");
    let renderer = client.clone();
    let render_task = tokio::spawn(async move {
        renderer
            .call(Request::Render {
                output,
                expected_revision: Some(2),
                tools: ToolConfig::default(),
                job_id,
                options: lumeryn_core::render::RenderOptions::default(),
            })
            .await
    });
    let started = second.watch(Some(cursor.sequence), 2000).await?;
    assert!(
        started
            .events
            .iter()
            .any(|e| e.data["job_id"] == job_id.to_string())
    );
    assert!(
        !render_task.is_finished(),
        "fixture must overlap the live redo"
    );
    tokio::time::timeout(
        Duration::from_secs(2),
        execute(&second, batch(Edit::Redo {}), 2),
    )
    .await
    .map_err(|_| Error::Timeout)??;
    assert_eq!(second.inspect().await?.revision, 3);
    assert_eq!(second.inspect().await?.clips[0].duration_frames, 600);
    let rendered = render_task
        .await
        .map_err(|e| Error::Io(std::io::Error::other(e)))??;
    assert_eq!(rendered["project_revision"], 2);
    assert_eq!(rendered["frames"], 1800);
    assert_eq!(rendered["verified_full_decode"], true);
    let before_cancel = second.inspect().await?;
    let history = second.history().await?;
    let cursor = second.watch(None, 0).await?;
    let job_id = Uuid::new_v4();
    let cancelled_output = directory.path().join("cancelled.mp4");
    let absent = cancelled_output.clone();
    let renderer = client.clone();
    let cancelling = tokio::spawn(async move {
        renderer
            .call(Request::Render {
                output: cancelled_output,
                expected_revision: Some(3),
                tools: ToolConfig::default(),
                job_id,
                options: lumeryn_core::render::RenderOptions {
                    resolution: lumeryn_core::render::RenderResolution::Uhd2160,
                    ..Default::default()
                },
            })
            .await
    });
    let queued = second.watch(Some(cursor.sequence), 2000).await?;
    assert!(
        queued
            .events
            .iter()
            .any(|e| e.data["job_id"] == job_id.to_string())
    );
    second.call(Request::Cancel { job_id }).await?;
    let cancelled = cancelling
        .await
        .map_err(|e| Error::Io(std::io::Error::other(e)))?;
    assert!(matches!(cancelled, Err(Error::Io(_))));
    assert!(!absent.exists());
    assert!(
        second
            .watch(Some(queued.sequence), 0)
            .await?
            .events
            .iter()
            .any(|event| event.kind == "job_cancelled"
                && event.revision == Some(3)
                && event.data["job_id"] == job_id.to_string())
    );
    assert_eq!(
        serde_json::to_value(second.inspect().await?)?,
        serde_json::to_value(before_cancel)?
    );
    assert_eq!(second.history().await?, history);
    assert_eq!(lumeryn_core::media::content_hash(&source).await?, digest);
    second.call(Request::Check { assets: true }).await?;
    stop(task).await;
    Ok(())
}

#[tokio::test]
async fn clipboard_paste_is_authoritative_conflict_aware_and_replays_after_undo() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let (first, task) = service(&path).await?;
    let second = Client::existing(&path)?;
    let mut original = fixture()?;
    original.media[0].metadata.blake3 = Some("a".repeat(64));
    first
        .call(Request::Create {
            project: original.clone(),
        })
        .await?;
    let clipboard = lumeryn_core::clipboard::Clipboard::copy(&original, original.clips[0].id)?;
    let bytes = serde_json::to_vec(&clipboard)?;
    assert_eq!(
        lumeryn_core::clipboard::Clipboard::from_slice(&bytes)?,
        clipboard
    );
    assert_eq!(second.inspect().await?.revision, 0);
    assert_eq!(second.history().await?.len(), 1);
    let clip_id = Uuid::new_v4();
    let paste = batch(Edit::PasteClip {
        id: clip_id,
        at_frame: 300,
        clipboard: clipboard.clone(),
    });
    execute(&second, trim(&original, 200), 0).await?;
    assert!(matches!(
        execute(&first, paste.clone(), 0).await,
        Err(Error::Conflict {
            expected: 0,
            current: 1
        })
    ));
    let mut wrong = clipboard.clone();
    wrong.source_blake3 = "b".repeat(64);
    assert!(matches!(
        execute(
            &first,
            batch(Edit::PasteClip {
                id: clip_id,
                at_frame: 200,
                clipboard: wrong
            }),
            1
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(second.inspect().await?.revision, 1);
    let paste = batch(Edit::PasteClip {
        id: clip_id,
        at_frame: 200,
        clipboard,
    });
    let receipt = first
        .execute(
            paste.clone(),
            Some(1),
            Some("clipboard-paste".into()),
            false,
            ToolConfig::default(),
        )
        .await?;
    assert_eq!(receipt["revision"], 2);
    let pasted = second.inspect().await?;
    assert_eq!(pasted.clips.len(), 2);
    assert_eq!(pasted.clips[1].id, clip_id);
    assert_eq!(pasted.clips[1].media_id, original.media[0].id);
    assert_eq!(pasted.clips[1].source_in_frame, 0);
    assert_eq!(pasted.clips[1].duration_frames, 300);
    execute(&second, batch(Edit::Undo {}), 2).await?;
    assert_eq!(first.inspect().await?.clips.len(), 1);
    let cursor = second.watch(None, 0).await?;
    let replay = first
        .execute(
            paste,
            Some(1),
            Some("clipboard-paste".into()),
            false,
            ToolConfig::default(),
        )
        .await?;
    assert_eq!(replay["revision"], 2);
    assert_eq!(replay["replayed"], true);
    assert_eq!(second.inspect().await?.revision, 3);
    assert_eq!(second.inspect().await?.clips.len(), 1);
    assert_eq!(
        second.watch(Some(cursor.sequence), 0).await?.sequence,
        cursor.sequence
    );
    execute(&first, batch(Edit::Redo {}), 3).await?;
    assert_eq!(second.inspect().await?.clips[1].id, clip_id);
    first.call(Request::Check { assets: false }).await?;
    stop(task).await;
    Ok(())
}

#[tokio::test]
async fn pre_undo_schema_one_fixture_undo_and_backup_require_no_new_tables() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("legacy.lumeryn");
    let original = fixture()?;
    let mut current = original.clone();
    current.revision = 1;
    current.clips[0].duration_frames = 200;
    let mut connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true),
    )
    .await?;
    // Exact pre-Undo persistent layout, constructed independently of Store::create.
    sqlx::raw_sql("CREATE TABLE project (id INTEGER PRIMARY KEY CHECK(id=1), schema_version INTEGER NOT NULL, revision INTEGER NOT NULL, snapshot TEXT NOT NULL); CREATE TABLE history (revision INTEGER PRIMARY KEY, commands TEXT NOT NULL, snapshot TEXT NOT NULL); CREATE TABLE receipts (key TEXT PRIMARY KEY, commands TEXT NOT NULL, revision INTEGER NOT NULL, object_ids TEXT NOT NULL); CREATE VIRTUAL TABLE search_index USING fts5(object_id UNINDEXED, kind UNINDEXED, text, tokenize='unicode61'); PRAGMA user_version=1;").execute(&mut connection).await?;
    sqlx::query("INSERT INTO project VALUES (1,1,1,?)")
        .bind(serde_json::to_string(&current)?)
        .execute(&mut connection)
        .await?;
    sqlx::query("INSERT INTO history VALUES (0,'[]',?)")
        .bind(serde_json::to_string(&original)?)
        .execute(&mut connection)
        .await?;
    sqlx::query("INSERT INTO history VALUES (1,?,?)")
        .bind(serde_json::to_string(&trim(&original, 200).operations)?)
        .bind(serde_json::to_string(&current)?)
        .execute(&mut connection)
        .await?;
    sqlx::query("INSERT INTO search_index(object_id,kind,text) VALUES (?,'project',?)")
        .bind(original.id.to_string())
        .bind(&original.name)
        .execute(&mut connection)
        .await?;
    sqlx::query(
        "INSERT INTO search_index(object_id,kind,text) VALUES (?,'media','source.mp4 mpeg4 ')",
    )
    .bind(original.media[0].id.to_string())
    .execute(&mut connection)
    .await?;
    connection.close().await?;
    let (client, task) = service(&path).await?;
    assert_eq!(client.inspect().await?.revision, 1);
    assert!(history_state(&client.history().await?)?.can_undo);
    client.call(Request::Check { assets: false }).await?;
    execute(&client, batch(Edit::Undo {}), 1).await?;
    assert_eq!(client.inspect().await?.clips[0].duration_frames, 300);
    client.call(Request::Check { assets: false }).await?;
    let backup = directory.path().join("legacy-backup.lumeryn");
    client
        .call(Request::Backup {
            output: backup.clone(),
        })
        .await?;
    let mut copied = Store::open(&backup).await?;
    assert_eq!(copied.inspect().await?.revision, 2);
    copied.integrity_check().await?;
    drop(copied);
    let mut connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&backup)
            .read_only(true),
    )
    .await?;
    let names: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'search_index%' ORDER BY name").fetch_all(&mut connection).await?;
    assert_eq!(names, ["history", "project", "receipts"]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&mut connection)
            .await?,
        1
    );
    connection.close().await?;
    assert_eq!(client.inspect().await?.revision, 2);
    stop(task).await;
    Ok(())
}
