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
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

pub const STORAGE_VERSION: i64 = 1;
const SEARCH_MIGRATION: &str = include_str!("../migrations/0001_metadata_search.sql");
const MAX_HISTORY_RECORDS: usize = 100_000;
const MAX_HISTORY_COMMAND_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HistoryState {
    pub can_undo: bool,
    pub can_redo: bool,
    pub current_logical_revision: u64,
    pub undo_depth: usize,
    pub redo_depth: usize,
}

#[derive(Default)]
struct LogicalHistory {
    revisions: Vec<u64>,
    cursor: usize,
}
impl LogicalHistory {
    fn push(&mut self, revision: u64, operations: &[Edit]) -> Result<()> {
        if revision == 0 {
            if !self.revisions.is_empty() || !operations.is_empty() {
                return Err(Error::Invalid(
                    "initial history must be revision zero with no operations".into(),
                ));
            }
            self.revisions.push(0);
            return Ok(());
        }
        validate_operations(operations)?;
        match operations {
            [Edit::Undo {}] => {
                self.cursor = self
                    .cursor
                    .checked_sub(1)
                    .ok_or_else(|| Error::Invalid("nothing to undo".into()))?
            }
            [Edit::Redo {}] => {
                if self.cursor + 1 >= self.revisions.len() {
                    return Err(Error::Invalid("nothing to redo".into()));
                }
                self.cursor += 1;
            }
            _ => {
                self.revisions.truncate(self.cursor + 1);
                self.revisions.push(revision);
                self.cursor = self.revisions.len() - 1;
            }
        }
        Ok(())
    }
    fn state(&self) -> Result<HistoryState> {
        let current_logical_revision = *self
            .revisions
            .get(self.cursor)
            .ok_or_else(|| Error::Invalid("history requires initial revision zero".into()))?;
        let redo_depth = self.revisions.len() - self.cursor - 1;
        Ok(HistoryState {
            can_undo: self.cursor > 0,
            can_redo: redo_depth > 0,
            current_logical_revision,
            undo_depth: self.cursor,
            redo_depth,
        })
    }
}

/// Derive logical undo/redo state from ordered durable {revision, operations} metadata.
/// Ordinary batches are one undo step; branch edits invalidate logical redo without deleting rows.
pub fn history_state(history: &[serde_json::Value]) -> Result<HistoryState> {
    logical_history(history)?.state()
}

fn logical_history(history: &[serde_json::Value]) -> Result<LogicalHistory> {
    if history.is_empty() || history.len() > MAX_HISTORY_RECORDS {
        return Err(Error::Invalid(
            "history requires 1..100000 ordered records".into(),
        ));
    }
    let mut logical = LogicalHistory::default();
    let mut bytes = 0_usize;
    for (index, record) in history.iter().enumerate() {
        let revision = record
            .get("revision")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| Error::Invalid("history revision must be an unsigned integer".into()))?;
        if revision != index as u64 {
            return Err(Error::Invalid(
                "history revisions must be contiguous from zero".into(),
            ));
        }
        let value = record
            .get("operations")
            .ok_or_else(|| Error::Invalid("history operations missing".into()))?;
        bytes = bytes
            .checked_add(serde_json::to_vec(value)?.len())
            .filter(|bytes| *bytes <= MAX_HISTORY_COMMAND_BYTES)
            .ok_or_else(|| Error::Invalid("history commands exceed 64 MiB".into()))?;
        let operations: Vec<Edit> = serde_json::from_value(value.clone())?;
        logical.push(revision, &operations)?;
    }
    Ok(logical)
}

pub struct Store {
    connection: SqliteConnection,
    path: PathBuf,
    writable: bool,
}

/// Prepared imports can only be created by validated probing, outside the service writer lock.
pub struct PreparedImports {
    project_path: PathBuf,
    commands: String,
    imports: Vec<Option<Media>>,
    audio_proofs: Vec<Option<AudioProof>>,
    visual_proofs: Vec<Option<VisualProof>>,
}

#[derive(Clone)]
struct VisualProof {
    media_id: Uuid,
    path: PathBuf,
    digest: String,
    size_bytes: u64,
    modified: std::time::SystemTime,
}

impl VisualProof {
    fn check(&self, project: &Project, database: &Path) -> Result<()> {
        let media = project
            .media
            .iter()
            .find(|media| media.id == self.media_id)
            .ok_or_else(|| Error::NotFound(format!("media {}", self.media_id)))?;
        if media.path != self.path
            || media.metadata.size_bytes != self.size_bytes
            || media.metadata.blake3.as_deref() != Some(self.digest.as_str())
        {
            return Err(Error::Invalid(
                "visual proof source identity changed before commit".into(),
            ));
        }
        let parent = database
            .parent()
            .ok_or_else(|| Error::Invalid("missing project parent".into()))?;
        let source = std::fs::metadata(parent.join(&media.path))?;
        if source.len() != self.size_bytes || source.modified()? != self.modified {
            return Err(Error::Invalid(
                "visual source identity changed before commit; reimport media".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone)]
struct AudioProof {
    media_id: Uuid,
    path: PathBuf,
    digest: String,
    size_bytes: u64,
    samples: u64,
}

impl AudioProof {
    fn apply(&self, project: &mut Project) -> Result<()> {
        let media = project
            .media
            .iter_mut()
            .find(|media| media.id == self.media_id)
            .ok_or_else(|| Error::NotFound(format!("media {}", self.media_id)))?;
        if media.path != self.path
            || media.metadata.size_bytes != self.size_bytes
            || media.metadata.blake3.as_deref() != Some(self.digest.as_str())
        {
            return Err(Error::Invalid(
                "audio proof source identity changed before commit".into(),
            ));
        }
        let audio = media
            .metadata
            .audio
            .as_mut()
            .ok_or_else(|| Error::Invalid("audio proof requires an audio stream".into()))?;
        if audio
            .canonical_samples_48k
            .is_some_and(|samples| samples != self.samples)
        {
            return Err(Error::Invalid(
                "audio proof sample count changed before commit".into(),
            ));
        }
        audio.canonical_samples_48k = Some(self.samples);
        Ok(())
    }
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
            .bind(i64::from(project.schema_version))
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

    pub async fn replay(
        &mut self,
        batch: &Batch,
        key: Option<&str>,
        dry_run: bool,
    ) -> Result<Option<Receipt>> {
        let commands = validated_commands(batch, key)?;
        // Replaying a committed operation must not depend on the source still being present.
        if let Some(key) = key
            && let Some(row) =
                sqlx::query("SELECT commands, revision, object_ids FROM receipts WHERE key=?")
                    .bind(key)
                    .fetch_optional(&mut self.connection)
                    .await?
        {
            return Ok(Some(receipt_from_row(row, &commands, dry_run)?));
        }
        Ok(None)
    }

    /// Replay an asynchronous import before source probing, including its original optional
    /// first-video frame-rate match. Eligibility changes after that first commit.
    pub async fn replay_import(
        &mut self,
        batch: &Batch,
        key: Option<&str>,
        match_first_video: bool,
    ) -> Result<Option<Receipt>> {
        let commands = validated_commands(batch, key)?;
        if !batch.operations.len().is_multiple_of(2) || !batch.operations.chunks_exact(2).all(|pair| matches!(pair, [Edit::Import { id, .. }, Edit::RenameMedia { media_id, .. }] if id == media_id)) {
            return Err(Error::Invalid("import replay requires import and alias commands".into()));
        }
        let Some(key) = key else { return Ok(None) };
        let Some(row) =
            sqlx::query("SELECT commands, revision, object_ids FROM receipts WHERE key=?")
                .bind(key)
                .fetch_optional(&mut self.connection)
                .await?
        else {
            return Ok(None);
        };
        let stored: &str = row.try_get("commands")?;
        if stored == commands {
            return Ok(Some(receipt_from_row(row, &commands, false)?));
        }
        let stored_operations: Vec<Edit> = serde_json::from_str(stored)?;
        if match_first_video
            && matches!(stored_operations.last(), Some(Edit::SetFrameRate { .. }))
            && stored_operations.len() == batch.operations.len() + 1
            && serde_json::to_string(&stored_operations[..batch.operations.len()])? == commands
        {
            let stored_commands = stored.to_owned();
            return Ok(Some(receipt_from_row(row, &stored_commands, false)?));
        }
        Err(Error::Invalid(
            "idempotency key reused with different operations".into(),
        ))
    }

    pub async fn execute(
        &mut self,
        batch: &Batch,
        expected_revision: Option<u64>,
        key: Option<&str>,
        dry_run: bool,
        tools: &MediaTools,
    ) -> Result<Receipt> {
        if let Some(receipt) = self.replay(batch, key, dry_run).await? {
            return Ok(receipt);
        }
        let prepared = prepare_imports(&self.path, batch, tools).await?;
        self.execute_prepared(batch, expected_revision, key, dry_run, prepared)
            .await
    }

    pub async fn execute_prepared(
        &mut self,
        batch: &Batch,
        expected_revision: Option<u64>,
        key: Option<&str>,
        dry_run: bool,
        prepared: PreparedImports,
    ) -> Result<Receipt> {
        let commands = validated_commands(batch, key)?;
        if prepared.project_path != self.path || prepared.commands != commands {
            return Err(Error::Invalid(
                "prepared imports do not match project and batch".into(),
            ));
        }
        let PreparedImports {
            imports,
            audio_proofs,
            visual_proofs,
            ..
        } = prepared;
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
        if project.schema_version != SCHEMA_VERSION {
            return Err(Error::Unsupported(
                "run project migrate before editing legacy project schema".into(),
            ));
        }
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
        if matches!(
            batch.operations.as_slice(),
            [Edit::Undo {}] | [Edit::Redo {}]
        ) {
            let mut records = history_metadata(&mut tx).await?;
            if records.len() as u64 != previous + 1 {
                return Err(Error::Invalid(
                    "history does not match current revision".into(),
                ));
            }
            records.push(serde_json::json!({"revision":previous+1,"operations":batch.operations}));
            let logical = logical_history(&records)?;
            let target = logical.state()?.current_logical_revision;
            let row = sqlx::query("SELECT revision, snapshot FROM history WHERE revision=?")
                .bind(
                    i64::try_from(target)
                        .map_err(|_| Error::Invalid("history revision overflow".into()))?,
                )
                .fetch_one(&mut *tx)
                .await?;
            let restored =
                historical_project(row.try_get::<&str, _>("snapshot")?, target, project.id)?;
            if restored.schema_version != project.schema_version
                || restored.audio_track_id != project.audio_track_id
            {
                return Err(Error::Invalid("history graph schema/audio track differs from current project; check or migrate project".into()));
            }
            project = restored;
            object_ids.push(project.id);
        } else {
            for (((operation, imported), proof), visual_proof) in batch
                .operations
                .iter()
                .zip(imports)
                .zip(audio_proofs)
                .zip(visual_proofs)
            {
                if let Some(proof) = proof {
                    proof.apply(&mut project)?;
                }
                if let Some(proof) = visual_proof {
                    proof.check(&project, &self.path)?;
                }
                project.apply(operation, imported)?;
                match operation {
                    Edit::Import { id, .. }
                    | Edit::AddClip { id, .. }
                    | Edit::InsertClip { id, .. }
                    | Edit::AddAudioClip { id, .. }
                    | Edit::AddVisualTrack { id, .. }
                    | Edit::AddVisualClip { id, .. }
                    | Edit::PasteClip { id, .. }
                    | Edit::DuplicateClip { id, .. } => object_ids.push(*id),
                    Edit::TrimClip { clip_id, .. }
                    | Edit::MoveClip { clip_id, .. }
                    | Edit::DeleteClip { clip_id, .. }
                    | Edit::ReorderClip { clip_id, .. } => object_ids.push(*clip_id),
                    Edit::MoveAudioClip { clip_id, .. }
                    | Edit::TrimAudioClip { clip_id, .. }
                    | Edit::SetAudioRange { clip_id, .. }
                    | Edit::SetAudioProperties { clip_id, .. }
                    | Edit::DeleteAudioClip { clip_id } => object_ids.push(*clip_id),
                    Edit::MoveVisualClip { clip_id, .. }
                    | Edit::SetVisualRange { clip_id, .. }
                    | Edit::SetVisualPlacement { clip_id, .. }
                    | Edit::DeleteVisualClip { clip_id } => object_ids.push(*clip_id),
                    Edit::ReorderVisualTrack { track_id, .. }
                    | Edit::DeleteVisualTrack { track_id } => object_ids.push(*track_id),
                    Edit::RenameMedia { media_id, .. } | Edit::RemoveMedia { media_id } => {
                        object_ids.push(*media_id)
                    }
                    Edit::SplitClip {
                        clip_id,
                        right_clip_id,
                        ..
                    } => object_ids.extend([*clip_id, *right_clip_id]),
                    Edit::SetCanvas { .. } | Edit::SetFrameRate { .. } => {
                        object_ids.push(project.id)
                    }
                    Edit::Undo {} | Edit::Redo {} => {
                        return Err(Error::Invalid(
                            "undo/redo must be the only batch operation".into(),
                        ));
                    }
                }
            }
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

    /// Explicit upgrade of storage indexes and every persisted graph snapshot.
    /// Commands, receipts and revisions retain their exact durable meaning and bytes.
    pub async fn migrate(&mut self) -> Result<serde_json::Value> {
        let inspected = self.inspect().await?;
        let previous = self.storage_version().await?;
        if previous == STORAGE_VERSION && inspected.schema_version == SCHEMA_VERSION {
            return Ok(
                serde_json::json!({"from":previous,"to":previous,"graph_from":inspected.schema_version,"graph_to":inspected.schema_version,"changed":false}),
            );
        }
        if !(0..=STORAGE_VERSION).contains(&previous) {
            return Err(Error::Unsupported(format!(
                "storage migration from {previous}"
            )));
        }
        self.make_writable().await?;
        let mut tx = self.connection.begin_with("BEGIN IMMEDIATE").await?;
        let locked: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut *tx)
            .await?;
        if !(0..=STORAGE_VERSION).contains(&locked) {
            return Err(Error::Unsupported(
                "concurrent storage schema change".into(),
            ));
        }
        let row = sqlx::query("SELECT schema_version, revision, snapshot FROM project WHERE id=1")
            .fetch_one(&mut *tx)
            .await?;
        let mut project = checked_project(row)?;
        let graph_from = project.schema_version;
        if locked == STORAGE_VERSION && graph_from == SCHEMA_VERSION {
            tx.rollback().await?;
            return Ok(
                serde_json::json!({"from":locked,"to":locked,"graph_from":graph_from,"graph_to":graph_from,"changed":false}),
            );
        }
        if graph_from < SCHEMA_VERSION {
            // Bound memory before collecting all histories needed for collision-free identity.
            let sizes = sqlx::query("SELECT count(*) AS records, coalesce(sum(length(CAST(snapshot AS BLOB))),0) AS bytes, coalesce(sum(length(CAST(commands AS BLOB))),0) AS command_bytes FROM history").fetch_one(&mut *tx).await?;
            if sizes.try_get::<i64, _>("records")? > MAX_HISTORY_RECORDS as i64
                || sizes.try_get::<i64, _>("bytes")? > 128 * 1024 * 1024
                || sizes.try_get::<i64, _>("command_bytes")? > MAX_HISTORY_COMMAND_BYTES as i64
            {
                return Err(Error::Invalid("graph migration exceeds 100000 snapshots or 128 MiB; staged migration required".into()));
            }
            let rows =
                sqlx::query("SELECT revision, commands, snapshot FROM history ORDER BY revision")
                    .fetch_all(&mut *tx)
                    .await?;
            if rows.len() as u64 != project.revision + 1 {
                return Err(Error::Invalid("migration history is not contiguous".into()));
            }
            let mut previous = Vec::with_capacity(rows.len());
            let mut ids = HashSet::new();
            let mut logical = LogicalHistory::default();
            for (index, row) in rows.iter().enumerate() {
                if row.try_get::<i64, _>("revision")? != index as i64 {
                    return Err(Error::Invalid("migration history revision mismatch".into()));
                }
                let snapshot = historical_project(
                    row.try_get::<&str, _>("snapshot")?,
                    index as u64,
                    project.id,
                )?;
                if snapshot.schema_version != graph_from {
                    return Err(Error::Invalid(
                        "migration history has mixed graph schemas".into(),
                    ));
                }
                let commands: Vec<Edit> =
                    serde_json::from_str(row.try_get::<&str, _>("commands")?)?;
                logical.push(index as u64, &commands)?;
                if matches!(commands.as_slice(), [Edit::Undo {}] | [Edit::Redo {}]) {
                    let target = logical.state()?.current_logical_revision;
                    let mut expected: Project = previous
                        .get(target as usize)
                        .cloned()
                        .ok_or_else(|| Error::Invalid("migration undo target is missing".into()))?;
                    expected.revision = index as u64;
                    if serde_json::to_value(&expected)? != serde_json::to_value(&snapshot)? {
                        return Err(Error::Invalid(
                            "migration undo snapshot differs from target".into(),
                        ));
                    }
                }
                ids.extend([snapshot.id, snapshot.video_track_id]);
                ids.extend(snapshot.media.iter().map(|media| media.id));
                ids.extend(snapshot.clips.iter().map(|clip| clip.id));
                ids.extend(snapshot.audio_track_id);
                ids.extend(snapshot.audio_clips.iter().map(|clip| clip.id));
                previous.push(snapshot);
            }
            if previous.last().map(serde_json::to_value).transpose()?
                != Some(serde_json::to_value(&project)?)
            {
                return Err(Error::Invalid(
                    "migration current snapshot disagrees with history".into(),
                ));
            }
            let audio_track = if graph_from == 1 {
                (0..100)
                    .map(|_| Uuid::new_v4())
                    .find(|id| !ids.contains(id))
                    .ok_or_else(|| {
                        Error::Invalid("cannot allocate unique audio track identity".into())
                    })?
            } else {
                project.audio_track_id.ok_or_else(|| {
                    Error::Invalid("schema 2 requires audio track identity".into())
                })?
            };
            for snapshot in &mut previous {
                if graph_from == 2 && snapshot.audio_track_id != Some(audio_track) {
                    return Err(Error::Invalid(
                        "migration history audio track identity differs".into(),
                    ));
                }
                // Compose 1 -> 2 -> 3 without changing durable commands, receipts or revisions.
                if snapshot.schema_version == 1 {
                    snapshot.schema_version = 2;
                    snapshot.audio_track_id = Some(audio_track);
                    snapshot.validate()?;
                }
                snapshot.schema_version = SCHEMA_VERSION;
                snapshot.validate()?;
                sqlx::query("UPDATE history SET snapshot=? WHERE revision=?")
                    .bind(serde_json::to_string(snapshot)?)
                    .bind(snapshot.revision as i64)
                    .execute(&mut *tx)
                    .await?;
            }
            project.schema_version = SCHEMA_VERSION;
            project.audio_track_id = Some(audio_track);
            project.validate()?;
            sqlx::query("UPDATE project SET schema_version=?, snapshot=? WHERE id=1")
                .bind(i64::from(SCHEMA_VERSION))
                .bind(serde_json::to_string(&project)?)
                .execute(&mut *tx)
                .await?;
        }
        if locked == 0 {
            sqlx::raw_sql(SEARCH_MIGRATION).execute(&mut *tx).await?;
        }
        rebuild_search(&mut tx, &project).await?;
        tx.commit().await?;
        Ok(
            serde_json::json!({"from":locked,"to":STORAGE_VERSION,"graph_from":graph_from,"graph_to":project.schema_version,"changed":true,"project_revision":project.revision}),
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
        let mut logical = LogicalHistory::default();
        for (index, row) in rows.iter().enumerate() {
            let previous = historical_project(
                row.try_get::<&str, _>("snapshot")?,
                index as u64,
                project.id,
            )?;
            if previous.schema_version != project.schema_version
                || previous.audio_track_id != project.audio_track_id
            {
                return Err(Error::Invalid(
                    "history graph schema/audio track differs from current project".into(),
                ));
            }
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
            logical.push(index as u64, &operations)?;
            if matches!(operations.as_slice(), [Edit::Undo {}] | [Edit::Redo {}]) {
                let target = logical.state()?.current_logical_revision;
                let target_row = rows
                    .get(target as usize)
                    .ok_or_else(|| Error::Invalid("history restore target is missing".into()))?;
                let mut expected = historical_project(
                    target_row.try_get::<&str, _>("snapshot")?,
                    target,
                    project.id,
                )?;
                expected.revision = index as u64;
                if serde_json::to_value(expected)? != serde_json::to_value(previous)? {
                    return Err(Error::Invalid(
                        "undo/redo snapshot differs from logical restore target".into(),
                    ));
                }
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
    let version = row.try_get::<i64, _>("schema_version")?;
    if !(1..=i64::from(SCHEMA_VERSION)).contains(&version) {
        return Err(Error::Unsupported("database project schema".into()));
    }
    let snapshot: &str = row.try_get("snapshot")?;
    if snapshot.len() > 16 * 1024 * 1024 {
        return Err(Error::Invalid("project snapshot exceeds 16 MiB".into()));
    }
    let project: Project = serde_json::from_str(snapshot)?;
    if i64::from(project.schema_version) != version {
        return Err(Error::Invalid(
            "snapshot schema disagrees with database".into(),
        ));
    }
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

fn historical_project(snapshot: &str, revision: u64, project_id: Uuid) -> Result<Project> {
    if snapshot.len() > 16 * 1024 * 1024 {
        return Err(Error::Invalid("history snapshot exceeds 16 MiB".into()));
    }
    let project: Project = serde_json::from_str(snapshot)?;
    project.validate()?;
    if project.id != project_id || project.revision != revision {
        return Err(Error::Invalid(
            "history snapshot identity/revision mismatch".into(),
        ));
    }
    Ok(project)
}

async fn history_metadata(connection: &mut SqliteConnection) -> Result<Vec<serde_json::Value>> {
    let sizes=sqlx::query("SELECT count(*) AS records, coalesce(sum(length(CAST(commands AS BLOB))),0) AS bytes FROM history").fetch_one(&mut *connection).await?;
    if sizes.try_get::<i64, _>("records")? > MAX_HISTORY_RECORDS as i64
        || sizes.try_get::<i64, _>("bytes")? > MAX_HISTORY_COMMAND_BYTES as i64
    {
        return Err(Error::Invalid("undo history exceeds 100000 records or 64 MiB commands; indexed history migration required".into()));
    }
    let rows = sqlx::query("SELECT revision, commands FROM history ORDER BY revision")
        .fetch_all(&mut *connection)
        .await?;
    rows.into_iter().map(|row|Ok(serde_json::json!({"revision":row.try_get::<i64,_>("revision")?,"operations":serde_json::from_str::<serde_json::Value>(row.try_get::<&str,_>("commands")?)?}))).collect()
}

fn search_records(project: &Project) -> Vec<(String, String, String)> {
    let mut records = vec![(
        project.id.to_string(),
        "project".into(),
        project.name.clone(),
    )];
    records.extend(project.media.iter().map(|media| {
        // Preserve legacy derived-index text exactly when alias is absent.
        let mut text = format!(
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
        );
        if let Some(alias) = &media.alias {
            text.push(' ');
            text.push_str(alias);
        }
        (media.id.to_string(), "media".into(), text)
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

/// Decode/probe imports without borrowing the authoritative Store. Commit validates the latest revision.
/// Bind a source proved by the staged media worker to its exact project and commands.
/// The private-construction wrapper prevents unverified metadata from entering this path.
pub async fn prepare_verified_imports(
    project_path: &Path,
    batch: &Batch,
    verified: Vec<crate::media::VerifiedMetadata>,
) -> Result<PreparedImports> {
    if verified.is_empty() || verified.len() > 32 {
        return Err(Error::Invalid(
            "verified import requires 1..32 source proofs".into(),
        ));
    }
    let project_path = local_file(project_path)?;
    let commands = validated_commands(batch, None)?;
    let parent = project_path
        .parent()
        .ok_or_else(|| Error::Invalid("missing project parent".into()))?;
    let mut imports = Vec::with_capacity(batch.operations.len());
    let mut verified = verified.into_iter();
    for operation in &batch.operations {
        imports.push(match operation {
            Edit::Import { id, path } => {
                let canonical = local_file(path)?;
                if canonical == project_path {
                    return Err(Error::Invalid("cannot import the project database".into()));
                }
                let proof = verified
                    .next()
                    .ok_or_else(|| Error::Invalid("missing verified source".into()))?;
                if proof.source_path() != canonical {
                    return Err(Error::Invalid(
                        "verified source does not match import path".into(),
                    ));
                }
                let metadata = proof.into_metadata();
                // Multi-source preparation can be long. Recheck every earlier source after all
                // probes, outside the writer lock, rather than trusting its original mtime/size.
                let digest = crate::media::content_hash(&canonical).await?;
                if std::fs::metadata(&canonical)?.len() != metadata.size_bytes
                    || metadata.blake3.as_deref() != Some(digest.as_str())
                {
                    return Err(Error::Invalid(
                        "verified import source size or hash changed".into(),
                    ));
                }
                Some(Media {
                    id: *id,
                    path: canonical
                        .strip_prefix(parent)
                        .map_or_else(|_| canonical.clone(), Path::to_path_buf),
                    alias: None,
                    metadata,
                })
            }
            Edit::RenameMedia { .. } | Edit::SetFrameRate { .. } => None,
            _ => {
                return Err(Error::Invalid(
                    "verified import accepts import, alias and frame-rate commands only".into(),
                ));
            }
        });
    }
    if verified.next().is_some() {
        return Err(Error::Invalid("unused verified sources".into()));
    }
    Ok(PreparedImports {
        project_path,
        commands,
        audio_proofs: vec![None; imports.len()],
        visual_proofs: vec![None; imports.len()],
        imports,
    })
}

pub async fn prepare_imports(
    project_path: &Path,
    batch: &Batch,
    tools: &MediaTools,
) -> Result<PreparedImports> {
    let project_path = local_file(project_path)?;
    let commands = validated_commands(batch, None)?;
    // Probing happens outside the writer lock, while edits are validated against the latest locked snapshot.
    let mut imports = Vec::with_capacity(batch.operations.len());
    for operation in &batch.operations {
        imports.push(match operation {
            Edit::Import { id, path } => {
                let canonical = local_file(path)?;
                if canonical == project_path {
                    return Err(Error::Invalid("cannot import the project database".into()));
                }
                let metadata = tools.probe(&canonical).await?;
                let parent = project_path
                    .parent()
                    .ok_or_else(|| Error::Invalid("missing project parent".into()))?;
                let reference = canonical
                    .strip_prefix(parent)
                    .map_or_else(|_| canonical.clone(), Path::to_path_buf);
                Some(Media {
                    id: *id,
                    path: reference,
                    alias: None,
                    metadata,
                })
            }
            _ => None,
        });
    }

    let mut audio_proofs = Vec::with_capacity(batch.operations.len());
    let mut proofs = HashMap::<Uuid, AudioProof>::new();
    let current = if batch.operations.iter().any(|operation| {
        matches!(
            operation,
            Edit::AddAudioClip { .. }
                | Edit::AddVisualClip { .. }
                | Edit::SetVisualRange { .. }
                | Edit::SetVisualPlacement { .. }
                | Edit::MoveVisualClip { .. }
        )
    }) {
        Some(Store::open(&project_path).await?.inspect().await?)
    } else {
        None
    };
    for operation in &batch.operations {
        let proof = if let Edit::AddAudioClip { media_id, .. } = operation {
            if let Some(proof) = proofs.get(media_id) {
                Some(proof.clone())
            } else {
                let media = imports
                    .iter()
                    .flatten()
                    .find(|media| media.id == *media_id)
                    .or_else(|| {
                        current.as_ref().and_then(|project| {
                            project.media.iter().find(|media| media.id == *media_id)
                        })
                    })
                    .ok_or_else(|| Error::NotFound(format!("media {media_id}")))?;
                let audio = media.metadata.audio.as_ref().ok_or_else(|| {
                    Error::Unsupported("independent placement requires an audio source".into())
                })?;
                if audio.canonical_samples_48k.is_some() {
                    None
                } else {
                    let digest = media.metadata.blake3.clone().ok_or_else(|| {
                        Error::Invalid(
                            "audio sample proof requires source hash; reimport media".into(),
                        )
                    })?;
                    let path = if media.path.is_absolute() {
                        media.path.clone()
                    } else {
                        project_path
                            .parent()
                            .ok_or_else(|| Error::Invalid("missing project parent".into()))?
                            .join(&media.path)
                    };
                    let samples = tools.canonical_audio_samples(&path, &digest).await?;
                    let proof = AudioProof {
                        media_id: *media_id,
                        path: media.path.clone(),
                        digest,
                        size_bytes: media.metadata.size_bytes,
                        samples,
                    };
                    proofs.insert(*media_id, proof.clone());
                    Some(proof)
                }
            }
        } else {
            None
        };
        audio_proofs.push(proof);
    }
    let mut visual_proofs = Vec::with_capacity(batch.operations.len());
    let mut visual_sources = HashMap::<Uuid, VisualProof>::new();
    for operation in &batch.operations {
        let media_id = match operation {
            Edit::AddVisualClip { media_id, .. } => Some(*media_id),
            Edit::SetVisualRange { clip_id, .. }
            | Edit::SetVisualPlacement { clip_id, .. }
            | Edit::MoveVisualClip { clip_id, .. } => batch
                .operations
                .iter()
                .find_map(|operation| match operation {
                    Edit::AddVisualClip { id, media_id, .. } if id == clip_id => Some(*media_id),
                    _ => None,
                })
                .or_else(|| {
                    current.as_ref().and_then(|project| {
                        project
                            .visual_clips
                            .iter()
                            .find(|clip| clip.id == *clip_id)
                            .map(|clip| clip.media_id)
                    })
                })
                .ok_or_else(|| Error::NotFound(format!("visual clip {clip_id}")))?
                .into(),
            _ => None,
        };
        let proof = if let Some(media_id) = media_id {
            if let Some(proof) = visual_sources.get(&media_id) {
                Some(proof.clone())
            } else {
                let media = imports
                    .iter()
                    .flatten()
                    .find(|media| media.id == media_id)
                    .or_else(|| {
                        current.as_ref().and_then(|project| {
                            project.media.iter().find(|media| media.id == media_id)
                        })
                    })
                    .ok_or_else(|| Error::NotFound(format!("media {media_id}")))?;
                let digest = media.metadata.blake3.clone().ok_or_else(|| {
                    Error::Invalid(
                        "visual placement requires verified source digest; reimport media".into(),
                    )
                })?;
                let parent = project_path
                    .parent()
                    .ok_or_else(|| Error::Invalid("missing project parent".into()))?;
                let path = local_file(&parent.join(&media.path))?;
                let before = std::fs::metadata(&path)?;
                if before.len() != media.metadata.size_bytes
                    || crate::media::content_hash(&path).await? != digest
                {
                    return Err(Error::Invalid(
                        "visual source changed; reimport media".into(),
                    ));
                }
                let after = std::fs::metadata(&path)?;
                if before.len() != after.len() || before.modified()? != after.modified()? {
                    return Err(Error::Invalid(
                        "visual source changed while proving placement; reimport media".into(),
                    ));
                }
                let proof = VisualProof {
                    media_id,
                    path: media.path.clone(),
                    digest,
                    size_bytes: media.metadata.size_bytes,
                    modified: after.modified()?,
                };
                visual_sources.insert(media_id, proof.clone());
                Some(proof)
            }
        } else {
            None
        };
        visual_proofs.push(proof);
    }
    Ok(PreparedImports {
        project_path,
        commands,
        imports,
        audio_proofs,
        visual_proofs,
    })
}

fn validated_commands(batch: &Batch, key: Option<&str>) -> Result<String> {
    validate_operations(&batch.operations)?;
    if key.is_some_and(|k| k.is_empty() || k.len() > 128) {
        return Err(Error::Invalid(
            "idempotency key must have 1..128 bytes".into(),
        ));
    }
    let commands = serde_json::to_string(&batch.operations)?;
    if commands.len() > 1024 * 1024 {
        return Err(Error::Invalid("commands exceed 1 MiB".into()));
    }
    Ok(commands)
}

fn validate_operations(operations: &[Edit]) -> Result<()> {
    if operations.is_empty() || operations.len() > 1000 {
        return Err(Error::Invalid("batch must have 1..1000 operations".into()));
    }
    if operations.len() != 1
        && operations
            .iter()
            .any(|operation| matches!(operation, Edit::Undo {} | Edit::Redo {}))
    {
        return Err(Error::Invalid(
            "undo/redo must be the only batch operation".into(),
        ));
    }
    Ok(())
}
