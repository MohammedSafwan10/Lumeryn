//! Regression checks for the independent storage review. No media worker is executed.
use lumeryn_core::{Batch, Edit, Error, MediaTools, Project, Store};
use lumeryn_core::{
    media::{Metadata, Video},
    model::Media,
};
use sqlx::{Connection, Row, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{error::Error as StdError, fs, path::Path};
use uuid::Uuid;

type TestResult = Result<(), Box<dyn StdError>>;

async fn connection(path: &Path) -> Result<SqliteConnection, sqlx::Error> {
    SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true),
    )
    .await
}

fn indexed_fixture() -> lumeryn_core::Result<Project> {
    let mut project = Project::new("alpha OR beta quoted title".into(), 320, 180, "30".parse()?)?;
    project.media.push(Media {
        id: Uuid::new_v4(),
        path: "source.bin".into(),
        metadata: Metadata {
            size_bytes: 6,
            blake3: None,
            audio: None,
            video: Some(Video {
                codec: "mpeg4".into(),
                width: 320,
                height: 180,
                frame_rate: project.frame_rate,
                frames: 90,
                start_time_micros: 0,
                sample_aspect_ratio: "1:1".into(),
                color_transfer: None,
                constant_rate: true,
            }),
        },
    });
    Ok(project)
}

async fn legacy_project(path: &Path, project: &Project) -> TestResult {
    let mut conn = connection(path).await?;
    sqlx::raw_sql("CREATE TABLE project (id INTEGER PRIMARY KEY, schema_version INTEGER NOT NULL, revision INTEGER NOT NULL, snapshot TEXT NOT NULL); CREATE TABLE history (revision INTEGER PRIMARY KEY, commands TEXT NOT NULL, snapshot TEXT NOT NULL); CREATE TABLE receipts (key TEXT PRIMARY KEY, commands TEXT NOT NULL, revision INTEGER NOT NULL, object_ids TEXT NOT NULL);").execute(&mut conn).await?;
    let snapshot = serde_json::to_string(project)?;
    sqlx::query("INSERT INTO project VALUES (1, 1, 0, ?)")
        .bind(&snapshot)
        .execute(&mut conn)
        .await?;
    sqlx::query("INSERT INTO history VALUES (0, '[]', ?)")
        .bind(snapshot)
        .execute(&mut conn)
        .await?;
    conn.close().await?;
    Ok(())
}

#[tokio::test]
async fn legacy_migration_populates_search_once_without_editing_project_or_history() -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("legacy.lumeryn");
    let project = indexed_fixture()?;
    legacy_project(&path, &project).await?;
    let mut store = Store::open(&path).await?;
    assert_eq!(store.storage_version().await?, 0);
    assert!(matches!(
        store.search("alpha", 10).await,
        Err(Error::Unsupported(_))
    ));
    let history = store.history().await?;
    assert_eq!(store.migrate().await?["changed"], true);
    assert_eq!(store.storage_version().await?, 1);
    assert_eq!(
        serde_json::to_value(store.inspect().await?)?,
        serde_json::to_value(&project)?
    );
    assert_eq!(store.history().await?, history);
    assert_eq!(store.search("source", 10).await?.len(), 1);
    assert_eq!(store.migrate().await?["changed"], false);
    assert_eq!(store.search("source", 10).await?.len(), 1);
    assert_eq!(store.integrity_check().await?["revision"], 0);
    Ok(())
}

#[tokio::test]
async fn failed_migration_rolls_back_version_and_keeps_legacy_graph() -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("collision.lumeryn");
    let project = indexed_fixture()?;
    legacy_project(&path, &project).await?;
    let mut conn = connection(&path).await?;
    sqlx::raw_sql("CREATE TABLE search_index (sentinel TEXT); INSERT INTO search_index VALUES ('preserve me');").execute(&mut conn).await?;
    let original_tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master ORDER BY name")
            .fetch_all(&mut conn)
            .await?;
    let mut store = Store::open(&path).await?;
    assert!(matches!(store.migrate().await, Err(Error::Storage(_))));
    assert_eq!(store.storage_version().await?, 0);
    assert_eq!(
        serde_json::to_value(store.inspect().await?)?,
        serde_json::to_value(&project)?
    );
    assert_eq!(store.history().await?.len(), 1);
    assert_eq!(
        sqlx::query_scalar::<_, Vec<u8>>("SELECT CAST(sentinel AS BLOB) FROM search_index")
            .fetch_one(&mut conn)
            .await?,
        b"preserve me"
    );
    let actual_tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master ORDER BY name")
            .fetch_all(&mut conn)
            .await?;
    assert_eq!(actual_tables, original_tables);
    conn.close().await?;
    Ok(())
}

#[tokio::test]
async fn integrity_check_detects_derived_index_and_current_history_tampering() -> TestResult {
    let directory = tempfile::tempdir()?;
    for index_tamper in [true, false] {
        let path = directory.path().join(if index_tamper {
            "index.lumeryn"
        } else {
            "history.lumeryn"
        });
        let project = indexed_fixture()?;
        let mut store = Store::create(&path, &project).await?;
        assert_eq!(store.integrity_check().await?["history"], "ok");
        let mut conn = connection(&path).await?;
        if index_tamper {
            sqlx::query("UPDATE search_index SET text='forged index'")
                .execute(&mut conn)
                .await?;
        } else {
            let mut forged = project.clone();
            forged.name = "forged history".into();
            sqlx::query("UPDATE history SET snapshot=? WHERE revision=0")
                .bind(serde_json::to_string(&forged)?)
                .execute(&mut conn)
                .await?;
        }
        assert!(matches!(
            store.integrity_check().await,
            Err(Error::Invalid(_))
        ));
        assert_eq!(
            serde_json::to_value(store.inspect().await?)?,
            serde_json::to_value(&project)?
        );
        conn.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn live_wal_backup_contains_committed_edits_without_copying_or_modifying_assets() -> TestResult
{
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("live.lumeryn");
    let output = directory.path().join("backup.lumeryn");
    let source = directory.path().join("source.bin");
    fs::write(&source, b"source")?;
    let project = indexed_fixture()?;
    let mut store = Store::create(&path, &project).await?;
    let batch = Batch {
        operations: vec![Edit::AddClip {
            id: Uuid::new_v4(),
            media_id: project.media[0].id,
            at_frame: 0,
            source_in_frame: 10,
            duration_frames: 20,
        }],
    };
    store
        .execute(&batch, Some(0), None, false, &MediaTools::default())
        .await?;
    assert!(fs::metadata(path.with_extension("lumeryn-wal"))?.len() > 0);
    let expected = serde_json::to_value(store.inspect().await?)?;
    let backup = store.backup(&output).await?;
    assert_eq!(backup["assets"], "referenced; not copied");
    let mut reopened = Store::open(&output).await?;
    assert_eq!(serde_json::to_value(reopened.inspect().await?)?, expected);
    assert_eq!(reopened.history().await?.len(), 2);
    assert_eq!(reopened.integrity_check().await?["revision"], 1);
    assert_eq!(fs::read(&source)?, b"source");
    assert_eq!(serde_json::to_value(store.inspect().await?)?, expected);
    let existing = fs::read(&output)?;
    assert!(matches!(store.backup(&output).await, Err(Error::Exists(_))));
    assert_eq!(fs::read(&output)?, existing);
    Ok(())
}

#[tokio::test]
async fn cross_directory_backup_preserves_existing_and_missing_asset_reference_meaning()
-> TestResult {
    let directory = tempfile::tempdir()?;
    let original_parent = directory.path().join("original");
    let backup_parent = directory.path().join("backups");
    fs::create_dir(&original_parent)?;
    fs::create_dir(&backup_parent)?;
    let reference_parent = fs::canonicalize(&original_parent)?;
    fs::write(original_parent.join("source.bin"), b"source")?;
    let path = original_parent.join("project.lumeryn");
    let output = backup_parent.join("copy.lumeryn");
    let mut project = indexed_fixture()?;
    let mut missing = project.media[0].clone();
    missing.id = Uuid::new_v4();
    missing.path = "missing.bin".into();
    project.media.push(missing);
    let mut store = Store::create(&path, &project).await?;
    let graph_before = serde_json::to_value(store.inspect().await?)?;
    let bytes_before = fs::read(&path)?;
    store.backup(&output).await?;
    assert_eq!(fs::read(&path)?, bytes_before);
    assert_eq!(serde_json::to_value(store.inspect().await?)?, graph_before);
    let mut copied = Store::open(&output).await?;
    let copy = copied.inspect().await?;
    assert_eq!(copy.media[0].path, reference_parent.join("source.bin"));
    assert_eq!(copy.media[1].path, reference_parent.join("missing.bin"));
    assert_eq!(fs::read(&copy.media[0].path)?, b"source");
    assert!(!copy.media[1].path.exists());
    assert!(!backup_parent.join("source.bin").exists());
    assert!(!backup_parent.join("missing.bin").exists());
    assert_eq!(copied.integrity_check().await?["history"], "ok");
    let mut conn = connection(&output).await?;
    let snapshots: Vec<String> =
        sqlx::query_scalar("SELECT snapshot FROM history ORDER BY revision")
            .fetch_all(&mut conn)
            .await?;
    for snapshot in snapshots {
        let past: Project = serde_json::from_str(&snapshot)?;
        assert_eq!(past.media[0].path, reference_parent.join("source.bin"));
        assert_eq!(past.media[1].path, reference_parent.join("missing.bin"));
    }
    conn.close().await?;
    Ok(())
}

#[tokio::test]
async fn integrity_check_rejects_missing_or_inconsistent_historical_revisions() -> TestResult {
    let directory = tempfile::tempdir()?;
    for missing_revision in [true, false] {
        let path = directory.path().join(if missing_revision {
            "missing-history.lumeryn"
        } else {
            "invalid-history.lumeryn"
        });
        let project = indexed_fixture()?;
        let mut store = Store::create(&path, &project).await?;
        for revision in 0..2 {
            store
                .execute(
                    &Batch {
                        operations: vec![Edit::AddClip {
                            id: Uuid::new_v4(),
                            media_id: project.media[0].id,
                            at_frame: revision * 10,
                            source_in_frame: revision * 10,
                            duration_frames: 10,
                        }],
                    },
                    Some(revision),
                    None,
                    false,
                    &MediaTools::default(),
                )
                .await?;
        }
        let mut conn = connection(&path).await?;
        if missing_revision {
            sqlx::query("DELETE FROM history WHERE revision=1")
                .execute(&mut conn)
                .await?;
        } else {
            let mut invalid = project.clone();
            invalid.revision = 99;
            sqlx::query("UPDATE history SET snapshot=? WHERE revision=0")
                .bind(serde_json::to_string(&invalid)?)
                .execute(&mut conn)
                .await?;
        }
        assert!(matches!(
            store.integrity_check().await,
            Err(Error::Invalid(_))
        ));
        assert_eq!(store.inspect().await?.revision, 2);
        conn.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn fts_search_treats_user_operators_as_literal_phrase_and_binds_sql() -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("search.lumeryn");
    let project = indexed_fixture()?;
    let mut store = Store::create(&path, &project).await?;
    assert_eq!(store.search("alpha OR beta", 10).await?.len(), 1);
    assert!(store.search("alpha OR absent", 10).await?.is_empty());
    assert!(store.search("alpha\" OR source", 10).await?.is_empty());
    assert!(
        store
            .search("'; DROP TABLE project; --", 10)
            .await?
            .is_empty()
    );
    for query in ["", "   "] {
        assert!(matches!(
            store.search(query, 10).await,
            Err(Error::Invalid(_))
        ));
    }
    for limit in [0, 101] {
        assert!(matches!(
            store.search("alpha", limit).await,
            Err(Error::Invalid(_))
        ));
    }
    assert!(matches!(
        store.search(&"a".repeat(513), 10).await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(store.inspect().await?.name, project.name);
    assert_eq!(store.integrity_check().await?["revision"], 0);
    Ok(())
}

#[tokio::test]
async fn rejected_unrelated_and_future_databases_are_read_without_mutating_bytes() -> TestResult {
    let directory = tempfile::tempdir()?;
    for future_schema in [false, true] {
        let path = directory.path().join(if future_schema {
            "future.sqlite"
        } else {
            "unrelated.sqlite"
        });
        let mut conn = connection(&path).await?;
        if future_schema {
            sqlx::raw_sql("CREATE TABLE project (id INTEGER PRIMARY KEY, schema_version INTEGER, revision INTEGER, snapshot TEXT); INSERT INTO project VALUES (1,99,0,'{}');").execute(&mut conn).await?;
        } else {
            sqlx::raw_sql("CREATE TABLE unrelated (private_content TEXT); INSERT INTO unrelated VALUES ('owner data');").execute(&mut conn).await?;
        }
        assert_eq!(
            sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
                .fetch_one(&mut conn)
                .await?,
            "delete"
        );
        conn.close().await?;
        let original = fs::read(&path)?;
        let permissions = fs::metadata(&path)?.permissions();
        let mut read_only = permissions.clone();
        read_only.set_readonly(true);
        fs::set_permissions(&path, read_only)?;
        let result = Store::open(&path).await;
        fs::set_permissions(&path, permissions)?;
        if future_schema {
            assert!(matches!(result, Err(Error::Unsupported(_))));
        } else {
            assert!(matches!(result, Err(Error::Storage(_))));
        }
        assert_eq!(fs::read(&path)?, original);
        assert!(!path.with_extension("sqlite-wal").exists());
        assert!(!path.with_extension("sqlite-shm").exists());
        let mut conn = connection(&path).await?;
        assert_eq!(
            sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
                .fetch_one(&mut conn)
                .await?,
            "delete"
        );
        conn.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn nonzero_initial_revision_is_rejected_without_creating_any_file() -> TestResult {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("invalid.lumeryn");
    let mut project = Project::new("Revision fixture".into(), 320, 180, "30".parse()?)?;
    project.revision = 1;
    assert!(matches!(
        Store::create(&path, &project).await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(fs::read_dir(directory.path())?.count(), 0);
    Ok(())
}

#[tokio::test]
async fn schema_and_revision_tampering_after_open_cannot_commit_an_edit() -> TestResult {
    let directory = tempfile::tempdir()?;
    for tamper_schema in [true, false] {
        let path = directory.path().join(if tamper_schema {
            "schema.lumeryn"
        } else {
            "revision.lumeryn"
        });
        let project = Project::new("Tamper fixture".into(), 320, 180, "30".parse()?)?;
        let mut store = Store::create(&path, &project).await?;
        let mut conn = connection(&path).await?;
        let original_snapshot: String =
            sqlx::query_scalar("SELECT snapshot FROM project WHERE id=1")
                .fetch_one(&mut conn)
                .await?;
        if tamper_schema {
            sqlx::query("UPDATE project SET schema_version=99 WHERE id=1")
                .execute(&mut conn)
                .await?;
        } else {
            sqlx::query("UPDATE project SET revision=99 WHERE id=1")
                .execute(&mut conn)
                .await?;
        }
        let batch = Batch {
            operations: vec![Edit::TrimClip {
                clip_id: Uuid::new_v4(),
                source_in_frame: 0,
                duration_frames: 1,
            }],
        };
        let error = store
            .execute(
                &batch,
                Some(0),
                Some("must-not-commit"),
                false,
                &MediaTools::default(),
            )
            .await;
        if tamper_schema {
            assert!(matches!(error, Err(Error::Unsupported(_))));
        } else {
            assert!(
                matches!(error, Err(Error::Invalid(ref message)) if message.contains("revision"))
            );
        }
        drop(store);
        let row = sqlx::query("SELECT schema_version, revision, snapshot FROM project WHERE id=1")
            .fetch_one(&mut conn)
            .await?;
        assert_eq!(
            row.try_get::<i64, _>("schema_version")?,
            if tamper_schema { 99 } else { 1 }
        );
        assert_eq!(
            row.try_get::<i64, _>("revision")?,
            if tamper_schema { 0 } else { 99 }
        );
        assert_eq!(row.try_get::<String, _>("snapshot")?, original_snapshot);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM history")
                .fetch_one(&mut conn)
                .await?,
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM receipts")
                .fetch_one(&mut conn)
                .await?,
            0
        );
        conn.close().await?;
    }
    Ok(())
}
