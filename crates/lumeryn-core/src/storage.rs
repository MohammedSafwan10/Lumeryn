use crate::{
    Batch, Edit, Error, MediaTools, Project, Result,
    media::local_file,
    model::{Media, SCHEMA_VERSION},
};
use serde::Serialize;
use sqlx::{
    Connection, Row, SqliteConnection,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

pub const STORAGE_VERSION: i64 = 1;
const SEARCH_MIGRATION: &str = include_str!("../migrations/0001_metadata_search.sql");

pub struct Store {
    connection: SqliteConnection,
    path: PathBuf,
    writable: bool,
}

#[derive(Debug, Serialize)]
pub struct Receipt {
    pub revision: u64,
    pub object_ids: Vec<Uuid>,
    pub dry_run: bool,
    pub replayed: bool,
}

impl Store {
    pub async fn create(path: &Path, project: &Project) -> Result<Self> {
        project.validate()?;
        if project.revision != 0 {
            return Err(Error::Invalid(
                "new project must start at revision zero".into(),
            ));
        }
        if path.exists() {
            return Err(Error::Exists(path.display().to_string()));
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let temporary = tempfile::Builder::new()
            .prefix(".lumeryn-project-")
            .suffix(".lumeryn")
            .tempfile_in(parent)?;
        let mut store = Self::connect(temporary.path(), true).await?;
        let mut tx = store.connection.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::raw_sql("CREATE TABLE project (id INTEGER PRIMARY KEY CHECK(id=1), schema_version INTEGER NOT NULL, revision INTEGER NOT NULL, snapshot TEXT NOT NULL); CREATE TABLE history (revision INTEGER PRIMARY KEY, commands TEXT NOT NULL, snapshot TEXT NOT NULL); CREATE TABLE receipts (key TEXT PRIMARY KEY, commands TEXT NOT NULL, revision INTEGER NOT NULL, object_ids TEXT NOT NULL);").execute(&mut *tx).await?;
        sqlx::query("INSERT INTO project VALUES (1, ?, 0, ?)")
            .bind(i64::from(SCHEMA_VERSION))
            .bind(serde_json::to_string(project)?)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO history VALUES (0, '[]', ?)")
            .bind(serde_json::to_string(project)?)
            .execute(&mut *tx)
            .await?;
        sqlx::raw_sql(SEARCH_MIGRATION).execute(&mut *tx).await?;
        rebuild_search(&mut tx, project).await?;
        tx.commit().await?;
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&mut store.connection)
            .await?;
        store.connection.close().await?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(path).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                Error::Exists(path.display().to_string())
            } else {
                Error::Io(error.error)
            }
        })?;
        Self::open(path).await
    }

    async fn connect(path: &Path, writable: bool) -> Result<Self> {
        let path = local_file(path)?;
        let mut options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(false)
            .read_only(!writable)
            .synchronous(SqliteSynchronous::Full)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        if writable {
            options = options.journal_mode(SqliteJournalMode::Wal);
        }
        let connection = SqliteConnection::connect_with(&options).await?;
        Ok(Self {
            connection,
            path,
            writable,
        })
    }

    pub async fn open(path: &Path) -> Result<Self> {
        let mut store = Self::connect(path, false).await?;
        store.inspect().await?;
        let version = store.storage_version().await?;
        if !(0..=STORAGE_VERSION).contains(&version) {
            return Err(Error::Unsupported(format!("storage schema {version}")));
        }
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn sqlite_version(&mut self) -> Result<String> {
        Ok(sqlx::query_scalar("SELECT sqlite_version()")
            .fetch_one(&mut self.connection)
            .await?)
    }

    pub async fn inspect(&mut self) -> Result<Project> {
        let row = sqlx::query("SELECT schema_version, revision, snapshot FROM project WHERE id=1")
            .fetch_one(&mut self.connection)
            .await?;
        checked_project(row)
    }

    pub async fn history(&mut self) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query("SELECT revision, commands FROM history ORDER BY revision")
            .fetch_all(&mut self.connection)
            .await?;
        rows.into_iter().map(|r| Ok(serde_json::json!({"revision": r.try_get::<i64,_>("revision")?,
            "operations": serde_json::from_str::<serde_json::Value>(r.try_get::<&str,_>("commands")?)? }))).collect()
    }

    pub async fn execute(
        &mut self,
        batch: &Batch,
        expected_revision: Option<u64>,
        key: Option<&str>,
        dry_run: bool,
        tools: &MediaTools,
    ) -> Result<Receipt> {
        if batch.operations.is_empty() || batch.operations.len() > 1000 {
            return Err(Error::Invalid("batch must have 1..1000 operations".into()));
        }
        if key.is_some_and(|k| k.is_empty() || k.len() > 128) {
            return Err(Error::Invalid(
                "idempotency key must have 1..128 bytes".into(),
            ));
        }
        let commands = serde_json::to_string(&batch.operations)?;
        if commands.len() > 1024 * 1024 {
            return Err(Error::Invalid("commands exceed 1 MiB".into()));
        }
        // Replaying a committed operation must not depend on the source still being present.
        if let Some(key) = key
            && let Some(row) =
                sqlx::query("SELECT commands, revision, object_ids FROM receipts WHERE key=?")
                    .bind(key)
                    .fetch_optional(&mut self.connection)
                    .await?
        {
            return receipt_from_row(row, &commands, dry_run);
        }
        // Probing happens outside the writer lock, while edits are validated against the latest locked snapshot.
        let mut imports = Vec::with_capacity(batch.operations.len());
        for operation in &batch.operations {
            imports.push(match operation {
                Edit::Import { id, path } => {
                    let canonical = local_file(path)?;
                    if canonical == self.path {
                        return Err(Error::Invalid("cannot import the project database".into()));
                    }
                    let metadata = tools.probe(&canonical).await?;
                    let parent = self
                        .path
                        .parent()
                        .ok_or_else(|| Error::Invalid("missing project parent".into()))?;
                    let reference = canonical
                        .strip_prefix(parent)
                        .map_or_else(|_| canonical.clone(), Path::to_path_buf);
                    Some(Media {
                        id: *id,
                        path: reference,
                        metadata,
                    })
                }
                _ => None,
            });
        }
        if self.storage_version().await? != STORAGE_VERSION {
            return Err(Error::Unsupported(
                "run project migrate before editing legacy storage".into(),
            ));
        }
        if !dry_run && !self.writable {
            let writable = Self::connect(&self.path, true).await?;
            self.connection = writable.connection;
            self.writable = true;
        }
        let mut tx = self
            .connection
            .begin_with(if dry_run { "BEGIN" } else { "BEGIN IMMEDIATE" })
            .await?;
        if let Some(key) = key
            && let Some(row) =
                sqlx::query("SELECT commands, revision, object_ids FROM receipts WHERE key=?")
                    .bind(key)
                    .fetch_optional(&mut *tx)
                    .await?
        {
            let receipt = receipt_from_row(row, &commands, dry_run)?;
            tx.rollback().await?;
            return Ok(receipt);
        }
        let row = sqlx::query("SELECT schema_version, revision, snapshot FROM project WHERE id=1")
            .fetch_one(&mut *tx)
            .await?;
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut *tx)
            .await?;
        if version != STORAGE_VERSION {
            return Err(Error::Unsupported(
                "storage schema changed since open".into(),
            ));
        }
        let mut project = checked_project(row)?;
        if let Some(expected) = expected_revision
            && project.revision != expected
        {
            return Err(Error::Conflict {
                expected,
                current: project.revision,
            });
        }
        let previous = project.revision;
        let mut object_ids = Vec::new();
        for (operation, imported) in batch.operations.iter().zip(imports) {
            project.apply(operation, imported)?;
            object_ids.push(match operation {
                Edit::Import { id, .. } | Edit::AddClip { id, .. } => *id,
                Edit::TrimClip { clip_id, .. } => *clip_id,
            });
        }
        project.revision = previous
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("revision overflow".into()))?;
        project.validate()?;
        if dry_run {
            tx.rollback().await?;
            return Ok(Receipt {
                revision: previous,
                object_ids,
                dry_run: true,
                replayed: false,
            });
        }
        let snapshot = serde_json::to_string(&project)?;
        let revision = i64::try_from(project.revision)
            .map_err(|_| Error::Invalid("revision overflow".into()))?;
        let result =
            sqlx::query("UPDATE project SET revision=?, snapshot=? WHERE id=1 AND revision=?")
                .bind(revision)
                .bind(&snapshot)
                .bind(
                    i64::try_from(previous)
                        .map_err(|_| Error::Invalid("revision overflow".into()))?,
                )
                .execute(&mut *tx)
                .await?;
        if result.rows_affected() != 1 {
            return Err(Error::Invalid(
                "project revision storage inconsistency".into(),
            ));
        }
        sqlx::query("INSERT INTO history VALUES (?, ?, ?)")
            .bind(revision)
            .bind(&commands)
            .bind(&snapshot)
            .execute(&mut *tx)
            .await?;
        rebuild_search(&mut tx, &project).await?;
        if let Some(key) = key {
            sqlx::query("INSERT INTO receipts VALUES (?, ?, ?, ?)")
                .bind(key)
                .bind(&commands)
                .bind(revision)
                .bind(serde_json::to_string(&object_ids)?)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(Receipt {
            revision: project.revision,
            object_ids,
            dry_run: false,
            replayed: false,
        })
    }
}

impl Store {
    pub async fn storage_version(&mut self) -> Result<i64> {
        Ok(sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut self.connection)
            .await?)
    }

    /// Explicit upgrade of derived indexes; never silently migrate on read-only open.
    pub async fn migrate(&mut self) -> Result<serde_json::Value> {
        self.inspect().await?;
        let previous = self.storage_version().await?;
        if previous == STORAGE_VERSION {
            return Ok(serde_json::json!({"from":previous,"to":previous,"changed":false}));
        }
        if previous != 0 {
            return Err(Error::Unsupported(format!(
                "storage migration from {previous}"
            )));
        }
        self.make_writable().await?;
        let mut tx = self.connection.begin_with("BEGIN IMMEDIATE").await?;
        let locked: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut *tx)
            .await?;
        if locked == STORAGE_VERSION {
            tx.rollback().await?;
            return Ok(serde_json::json!({"from":locked,"to":locked,"changed":false}));
        }
        if locked != 0 {
            return Err(Error::Unsupported(
                "concurrent storage schema change".into(),
            ));
        }
        let row = sqlx::query("SELECT schema_version, revision, snapshot FROM project WHERE id=1")
            .fetch_one(&mut *tx)
            .await?;
        let project = checked_project(row)?;
        sqlx::raw_sql(SEARCH_MIGRATION).execute(&mut *tx).await?;
        rebuild_search(&mut tx, &project).await?;
        tx.commit().await?;
        Ok(
            serde_json::json!({"from":previous,"to":STORAGE_VERSION,"changed":true,"project_revision":project.revision}),
        )
    }

    async fn make_writable(&mut self) -> Result<()> {
        if !self.writable {
            let writable = Self::connect(&self.path, true).await?;
            self.connection = writable.connection;
            self.writable = true;
        }
        Ok(())
    }

    pub async fn search(&mut self, query: &str, limit: u32) -> Result<Vec<serde_json::Value>> {
        if query.trim().is_empty() || query.len() > 512 || !(1..=100).contains(&limit) {
            return Err(Error::Invalid(
                "search requires 1..512 bytes and limit 1..100".into(),
            ));
        }
        if self.storage_version().await? != STORAGE_VERSION {
            return Err(Error::Unsupported(
                "run project migrate before metadata search".into(),
            ));
        }
        // Literal phrase search. Both SQL and FTS operators stay outside user interpolation.
        let phrase = format!("\"{}\"", query.replace('"', "\"\""));
        let rows = sqlx::query("SELECT object_id, kind, text FROM search_index WHERE search_index MATCH ? ORDER BY rank LIMIT ?")
            .bind(phrase).bind(limit).fetch_all(&mut self.connection).await?;
        rows.into_iter().map(|row| Ok(serde_json::json!({"object_id":row.try_get::<String,_>("object_id")?,"kind":row.try_get::<String,_>("kind")?,"text":row.try_get::<String,_>("text")?}))).collect()
    }

    pub async fn integrity_check(&mut self) -> Result<serde_json::Value> {
        let mut tx = self.connection.begin().await?;
        let checks: Vec<String> = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_all(&mut *tx)
            .await?;
        if checks != ["ok"] {
            return Err(Error::Invalid(format!(
                "SQLite integrity failure: {checks:?}"
            )));
        }
        if !sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&mut *tx)
            .await?
            .is_empty()
        {
            return Err(Error::Invalid("foreign key integrity failure".into()));
        }
        let row = sqlx::query("SELECT schema_version, revision, snapshot FROM project WHERE id=1")
            .fetch_one(&mut *tx)
            .await?;
        let project = checked_project(row)?;
        let history: String = sqlx::query_scalar("SELECT snapshot FROM history WHERE revision=?")
            .bind(
                i64::try_from(project.revision)
                    .map_err(|_| Error::Invalid("revision overflow".into()))?,
            )
            .fetch_one(&mut *tx)
            .await?;
        if serde_json::from_str::<serde_json::Value>(&history)? != serde_json::to_value(&project)? {
            return Err(Error::Invalid(
                "current history snapshot disagrees with project".into(),
            ));
        }
        let rows =
            sqlx::query("SELECT revision, commands, snapshot FROM history ORDER BY revision")
                .fetch_all(&mut *tx)
                .await?;
        if rows.len() as u64 != project.revision + 1 {
            return Err(Error::Invalid(
                "history revisions are not contiguous".into(),
            ));
        }
        for (index, row) in rows.into_iter().enumerate() {
            let previous: Project = serde_json::from_str(row.try_get::<&str, _>("snapshot")?)?;
            previous.validate()?;
            let operations: Vec<Edit> = serde_json::from_str(row.try_get::<&str, _>("commands")?)?;
            if row.try_get::<i64, _>("revision")? != index as i64
                || previous.revision != index as u64
                || previous.id != project.id
                || (index == 0 && !operations.is_empty())
                || (index > 0 && operations.is_empty())
            {
                return Err(Error::Invalid(
                    "invalid history revision or command record".into(),
                ));
            }
        }
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut *tx)
            .await?;
        if version == STORAGE_VERSION {
            let rows =
                sqlx::query("SELECT object_id, kind, text FROM search_index ORDER BY object_id")
                    .fetch_all(&mut *tx)
                    .await?;
            let actual = rows
                .into_iter()
                .map(|r| {
                    Ok((
                        r.try_get::<String, _>("object_id")?,
                        r.try_get::<String, _>("kind")?,
                        r.try_get::<String, _>("text")?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            let mut expected = search_records(&project);
            expected.sort();
            if actual != expected {
                return Err(Error::Invalid(
                    "metadata search index disagrees with project".into(),
                ));
            }
        }
        tx.rollback().await?;
        Ok(
            serde_json::json!({"sqlite_integrity":"ok","foreign_keys":"ok","project_invariants":"ok","history":"ok","storage_version":version,"revision":project.revision}),
        )
    }

    /// Consistent portable snapshot even while source WAL is live. Assets remain references.
    pub async fn backup(&mut self, output: &Path) -> Result<serde_json::Value> {
        if output.exists() {
            return Err(Error::Exists(output.display().to_string()));
        }
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let temporary = tempfile::Builder::new()
            .prefix(".lumeryn-backup-")
            .suffix(".lumeryn")
            .tempfile_in(parent)?;
        let name = temporary
            .path()
            .to_str()
            .ok_or_else(|| Error::Invalid("backup path must be UTF-8".into()))?;
        sqlx::query("VACUUM INTO ?")
            .bind(name)
            .execute(&mut self.connection)
            .await?;
        let mut snapshot = Self::open(temporary.path()).await?;
        let source_parent = self
            .path
            .parent()
            .ok_or_else(|| Error::Invalid("project parent unavailable".into()))?;
        if std::fs::canonicalize(parent)? != source_parent {
            // A copied database must keep resolving to the same sources in a new directory.
            // Assets are not copied; absolute references make that boundary explicit.
            snapshot.make_writable().await?;
            let mut tx = snapshot.connection.begin_with("BEGIN IMMEDIATE").await?;
            let mut project: Project = serde_json::from_str(
                &sqlx::query_scalar::<_, String>("SELECT snapshot FROM project WHERE id=1")
                    .fetch_one(&mut *tx)
                    .await?,
            )?;
            rebase_references(&mut project, source_parent);
            project.validate()?;
            sqlx::query("UPDATE project SET snapshot=? WHERE id=1")
                .bind(serde_json::to_string(&project)?)
                .execute(&mut *tx)
                .await?;
            let history = sqlx::query("SELECT revision, snapshot FROM history ORDER BY revision")
                .fetch_all(&mut *tx)
                .await?;
            for row in history {
                let mut previous: Project =
                    serde_json::from_str(row.try_get::<&str, _>("snapshot")?)?;
                rebase_references(&mut previous, source_parent);
                previous.validate()?;
                sqlx::query("UPDATE history SET snapshot=? WHERE revision=?")
                    .bind(serde_json::to_string(&previous)?)
                    .bind(row.try_get::<i64, _>("revision")?)
                    .execute(&mut *tx)
                    .await?;
            }
            if self.storage_version().await? == STORAGE_VERSION {
                rebuild_search(&mut tx, &project).await?;
            }
            tx.commit().await?;
        }
        let check = snapshot.integrity_check().await?;
        if snapshot.writable {
            sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
                .execute(&mut snapshot.connection)
                .await?;
        }
        snapshot.connection.close().await?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(output).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                Error::Exists(output.display().to_string())
            } else {
                Error::Io(error.error)
            }
        })?;
        Ok(
            serde_json::json!({"path":std::fs::canonicalize(output)?,"integrity":check,"assets":"referenced; not copied"}),
        )
    }
}

fn checked_project(row: sqlx::sqlite::SqliteRow) -> Result<Project> {
    if row.try_get::<i64, _>("schema_version")? != i64::from(SCHEMA_VERSION) {
        return Err(Error::Unsupported("database project schema".into()));
    }
    let snapshot: &str = row.try_get("snapshot")?;
    if snapshot.len() > 16 * 1024 * 1024 {
        return Err(Error::Invalid("project snapshot exceeds 16 MiB".into()));
    }
    let project: Project = serde_json::from_str(snapshot)?;
    if row.try_get::<i64, _>("revision")?
        != i64::try_from(project.revision)
            .map_err(|_| Error::Invalid("revision overflow".into()))?
    {
        return Err(Error::Invalid(
            "snapshot revision disagrees with database".into(),
        ));
    }
    project.validate()?;
    Ok(project)
}

fn search_records(project: &Project) -> Vec<(String, String, String)> {
    let mut records = vec![(
        project.id.to_string(),
        "project".into(),
        project.name.clone(),
    )];
    records.extend(project.media.iter().map(|media| {
        (
            media.id.to_string(),
            "media".into(),
            format!(
                "{} {} {}",
                media.path.display(),
                media
                    .metadata
                    .video
                    .as_ref()
                    .map_or("", |v| v.codec.as_str()),
                media
                    .metadata
                    .audio
                    .as_ref()
                    .map_or("", |a| a.codec.as_str())
            ),
        )
    }));
    records
}

fn rebase_references(project: &mut Project, original_parent: &Path) {
    for media in &mut project.media {
        if !media.path.is_absolute() {
            media.path = original_parent.join(&media.path);
        }
    }
}

async fn rebuild_search(connection: &mut SqliteConnection, project: &Project) -> Result<()> {
    sqlx::query("DELETE FROM search_index")
        .execute(&mut *connection)
        .await?;
    for (id, kind, text) in search_records(project) {
        sqlx::query("INSERT INTO search_index(object_id, kind, text) VALUES (?, ?, ?)")
            .bind(id)
            .bind(kind)
            .bind(text)
            .execute(&mut *connection)
            .await?;
    }
    Ok(())
}

fn receipt_from_row(
    row: sqlx::sqlite::SqliteRow,
    commands: &str,
    dry_run: bool,
) -> Result<Receipt> {
    if row.try_get::<&str, _>("commands")? != commands {
        return Err(Error::Invalid(
            "idempotency key reused with different operations".into(),
        ));
    }
    Ok(Receipt {
        revision: u64::try_from(row.try_get::<i64, _>("revision")?)
            .map_err(|_| Error::Invalid("negative revision".into()))?,
        object_ids: serde_json::from_str(row.try_get::<&str, _>("object_ids")?)?,
        dry_run,
        replayed: true,
    })
}
