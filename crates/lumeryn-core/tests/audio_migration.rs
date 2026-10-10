use lumeryn_core::{Batch, Edit, Error, MediaTools, Project, Result, Store};
use sqlx::{Connection, Row, SqliteConnection, sqlite::SqliteConnectOptions};
use std::path::Path;
use uuid::Uuid;

async fn connect(path: &Path) -> Result<SqliteConnection> {
    Ok(SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(path)).await?)
}

async fn legacy(path: &Path) -> Result<Project> {
    let mut project = Project::new("Legacy".into(), 320, 180, "30".parse()?)?;
    project.schema_version = 1;
    project.audio_track_id = None;
    Store::create(path, &project).await?;
    let mut connection = connect(path).await?;
    let mut changed = project.clone();
    changed.revision = 1;
    changed.width = 640;
    changed.height = 360;
    let command = serde_json::to_string(&vec![Edit::SetCanvas {
        width: 640,
        height: 360,
    }])?;
    sqlx::query("INSERT INTO history VALUES (1, ?, ?)")
        .bind(&command)
        .bind(serde_json::to_string(&changed)?)
        .execute(&mut connection)
        .await?;
    sqlx::query("INSERT INTO receipts VALUES ('legacy-key', ?, 1, ?)")
        .bind(command)
        .bind(serde_json::to_string(&vec![project.id])?)
        .execute(&mut connection)
        .await?;
    let mut restored = project.clone();
    restored.revision = 2;
    sqlx::query("INSERT INTO history VALUES (2, ?, ?)")
        .bind(serde_json::to_string(&vec![Edit::Undo {}])?)
        .bind(serde_json::to_string(&restored)?)
        .execute(&mut connection)
        .await?;
    sqlx::query("UPDATE project SET revision=2, snapshot=? WHERE id=1")
        .bind(serde_json::to_string(&restored)?)
        .execute(&mut connection)
        .await?;
    connection.close().await?;
    Ok(restored)
}

async fn bytes(connection: &mut SqliteConnection) -> Result<Vec<(i64, String)>> {
    sqlx::query("SELECT revision, commands FROM history ORDER BY revision")
        .fetch_all(connection)
        .await?
        .into_iter()
        .map(|row| Ok((row.try_get("revision")?, row.try_get("commands")?)))
        .collect()
}

#[tokio::test]
async fn explicit_graph_migration_preserves_history_receipts_and_legacy_read_shape() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let original = legacy(&path).await?;
    let mut store = Store::open(&path).await?;
    assert_eq!(
        serde_json::to_value(store.inspect().await?)?,
        serde_json::to_value(&original)?
    );
    let value = serde_json::to_value(&original)?;
    assert!(value.get("audio_track_id").is_none());
    assert!(value.get("audio_clips").is_none());
    store.integrity_check().await?;
    let backup = directory.path().join("legacy-backup.lumeryn");
    store.backup(&backup).await?;
    assert_eq!(
        Store::open(&backup).await?.inspect().await?.schema_version,
        1
    );
    let batch = Batch {
        operations: vec![Edit::SetCanvas {
            width: 640,
            height: 360,
        }],
    };
    assert!(
        store
            .execute(&batch, Some(2), None, false, &MediaTools::default())
            .await
            .is_err()
    );
    assert!(
        store
            .execute(
                &batch,
                Some(99),
                Some("legacy-key"),
                false,
                &MediaTools::default()
            )
            .await?
            .replayed
    );
    let mut connection = connect(&path).await?;
    let commands = bytes(&mut connection).await?;
    let receipts: Vec<(String, String, i64, String)> =
        sqlx::query_as("SELECT key, commands, revision, object_ids FROM receipts ORDER BY key")
            .fetch_all(&mut connection)
            .await?;
    let migration = store.migrate().await?;
    assert_eq!(migration["graph_from"], 1);
    assert_eq!(migration["graph_to"], 2);
    assert_eq!(migration["project_revision"], 2);
    let migrated = store.inspect().await?;
    let track = migrated
        .audio_track_id
        .ok_or_else(|| Error::Invalid("missing migrated track".into()))?;
    assert_ne!(track, migrated.id);
    assert_ne!(track, migrated.video_track_id);
    assert_eq!(migrated.revision, original.revision);
    assert_eq!(bytes(&mut connection).await?, commands);
    let actual: Vec<(String, String, i64, String)> =
        sqlx::query_as("SELECT key, commands, revision, object_ids FROM receipts ORDER BY key")
            .fetch_all(&mut connection)
            .await?;
    assert_eq!(actual, receipts);
    let histories: Vec<String> =
        sqlx::query_scalar("SELECT snapshot FROM history ORDER BY revision")
            .fetch_all(&mut connection)
            .await?;
    for snapshot in histories {
        let previous: Project = serde_json::from_str(&snapshot)?;
        assert_eq!(previous.schema_version, 2);
        assert_eq!(previous.audio_track_id, Some(track));
        assert!(previous.audio_clips.is_empty());
    }
    store
        .execute(
            &Batch {
                operations: vec![Edit::Redo {}],
            },
            Some(2),
            None,
            false,
            &MediaTools::default(),
        )
        .await?;
    let restored = store.inspect().await?;
    assert_eq!(restored.width, 640);
    assert_eq!(restored.audio_track_id, Some(track));
    store
        .execute(
            &Batch {
                operations: vec![Edit::Undo {}],
            },
            Some(3),
            None,
            false,
            &MediaTools::default(),
        )
        .await?;
    assert_eq!(store.inspect().await?.width, 320);
    store.integrity_check().await?;
    assert_eq!(store.migrate().await?["changed"], false);
    let destination = directory.path().join("new");
    std::fs::create_dir(&destination)?;
    store.backup(&destination.join("migrated.lumeryn")).await?;
    Ok(())
}

#[tokio::test]
async fn migration_failure_rolls_back_every_snapshot_and_schema_column() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let original = legacy(&path).await?;
    let mut connection = connect(&path).await?;
    let snapshots: Vec<String> =
        sqlx::query_scalar("SELECT snapshot FROM history ORDER BY revision")
            .fetch_all(&mut connection)
            .await?;
    sqlx::raw_sql("CREATE TRIGGER reject_history_upgrade BEFORE UPDATE OF snapshot ON history WHEN OLD.revision=1 BEGIN SELECT RAISE(ABORT, 'injected migration failure'); END;").execute(&mut connection).await?;
    let mut store = Store::open(&path).await?;
    assert!(matches!(store.migrate().await, Err(Error::Storage(_))));
    assert_eq!(
        serde_json::to_value(store.inspect().await?)?,
        serde_json::to_value(&original)?
    );
    let actual: Vec<String> = sqlx::query_scalar("SELECT snapshot FROM history ORDER BY revision")
        .fetch_all(&mut connection)
        .await?;
    assert_eq!(actual, snapshots);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT schema_version FROM project WHERE id=1")
            .fetch_one(&mut connection)
            .await?,
        1
    );
    sqlx::raw_sql("DROP TRIGGER reject_history_upgrade")
        .execute(&mut connection)
        .await?;
    assert_eq!(store.migrate().await?["changed"], true);
    store.integrity_check().await?;
    Ok(())
}

#[test]
fn legacy_schema_rejects_audio_extensions_and_future_versions() -> Result<()> {
    let mut project = Project::new("Compatibility".into(), 320, 180, "30".parse()?)?;
    project.schema_version = 1;
    assert!(project.validate().is_err());
    project.audio_track_id = None;
    project.validate()?;
    project.schema_version = 3;
    assert!(matches!(project.validate(), Err(Error::Unsupported(_))));
    let track = Uuid::new_v4();
    let mut value = serde_json::to_value(project)?;
    value["schema_version"] = 1.into();
    value["audio_track_id"] = track.to_string().into();
    assert!(
        serde_json::from_value::<Project>(value)?
            .validate()
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn corrupted_legacy_restore_target_prevents_migration_without_rewriting() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    legacy(&path).await?;
    let mut connection = connect(&path).await?;
    let snapshot: String = sqlx::query_scalar("SELECT snapshot FROM history WHERE revision=0")
        .fetch_one(&mut connection)
        .await?;
    let mut previous: Project = serde_json::from_str(&snapshot)?;
    previous.name = "corrupt logical target".into();
    sqlx::query("UPDATE history SET snapshot=? WHERE revision=0")
        .bind(serde_json::to_string(&previous)?)
        .execute(&mut connection)
        .await?;
    let originals: Vec<String> =
        sqlx::query_scalar("SELECT snapshot FROM history ORDER BY revision")
            .fetch_all(&mut connection)
            .await?;
    let mut store = Store::open(&path).await?;
    assert!(store.migrate().await.is_err());
    let actual: Vec<String> = sqlx::query_scalar("SELECT snapshot FROM history ORDER BY revision")
        .fetch_all(&mut connection)
        .await?;
    assert_eq!(actual, originals);
    assert_eq!(store.inspect().await?.schema_version, 1);
    Ok(())
}

#[tokio::test]
async fn mixed_graph_history_cannot_downgrade_current_project_on_undo() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let project = Project::new("Mixed".into(), 320, 180, "30".parse()?)?;
    let mut store = Store::create(&path, &project).await?;
    store
        .execute(
            &Batch {
                operations: vec![Edit::SetCanvas {
                    width: 640,
                    height: 360,
                }],
            },
            Some(0),
            None,
            false,
            &MediaTools::default(),
        )
        .await?;
    let mut connection = connect(&path).await?;
    let mut historical = project.clone();
    historical.schema_version = 1;
    historical.audio_track_id = None;
    sqlx::query("UPDATE history SET snapshot=? WHERE revision=0")
        .bind(serde_json::to_string(&historical)?)
        .execute(&mut connection)
        .await?;
    assert!(
        store
            .execute(
                &Batch {
                    operations: vec![Edit::Undo {}]
                },
                Some(1),
                None,
                false,
                &MediaTools::default()
            )
            .await
            .is_err()
    );
    assert!(store.integrity_check().await.is_err());
    let current = store.inspect().await?;
    assert_eq!(current.schema_version, 2);
    assert_eq!(current.revision, 1);
    assert_eq!(current.audio_track_id, project.audio_track_id);
    Ok(())
}
