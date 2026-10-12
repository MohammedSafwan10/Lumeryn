use crate::{
    EVENT_CAPACITY, Event, EventBatch, ImportFailure, ImportJob, ImportSource, ImportSourceStatus,
    ImportStage, PROTOCOL_VERSION, Request, ToolConfig,
    protocol::{Envelope, Response},
    read_frame, write_frame,
};
use lumeryn_core::{
    Batch, Edit, Error, MediaTools, Project, Result, Store,
    media::{ProbeProgress, ProbeStage, VerifiedMetadata},
    render,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Mutex, Notify, Semaphore, mpsc},
};
use uuid::Uuid;

struct Cancellation {
    cancelled: AtomicBool,
    notify: Notify,
}
const IMPORT_QUEUE_CAPACITY: usize = 16;
const IMPORT_STATUS_CAPACITY: usize = 32;
const IMPORT_CACHE_CAPACITY: usize = 16;

struct ImportRecord {
    status: ImportJob,
    cancellation: Arc<Cancellation>,
}
#[derive(Clone)]
struct CachedImport {
    policy: ToolPolicy,
    verified: VerifiedMetadata,
}
#[derive(Clone, PartialEq, Eq)]
struct ToolPolicy {
    config: ToolConfig,
    ffmpeg: Option<(PathBuf, u64, SystemTime)>,
    ffprobe: Option<(PathBuf, u64, SystemTime)>,
}
impl ToolPolicy {
    fn current(config: &ToolConfig) -> Result<Self> {
        fn identity(tool: &Path) -> Result<Option<(PathBuf, u64, SystemTime)>> {
            let mut candidates = vec![tool.to_owned()];
            if tool.components().count() == 1
                && let Some(paths) = std::env::var_os("PATH")
            {
                for directory in std::env::split_paths(&paths) {
                    candidates.push(directory.join(tool));
                    #[cfg(windows)]
                    if tool.extension().is_none() {
                        candidates.push(directory.join(tool).with_extension("exe"));
                    }
                }
            }
            for candidate in candidates {
                match std::fs::metadata(&candidate) {
                    Ok(metadata) if metadata.is_file() => {
                        return Ok(Some((
                            std::fs::canonicalize(candidate)?,
                            metadata.len(),
                            metadata.modified()?,
                        )));
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(None)
        }
        Ok(Self {
            config: config.clone(),
            ffmpeg: identity(&config.ffmpeg)?,
            ffprobe: identity(&config.ffprobe)?,
        })
    }
}
struct ImportWork {
    job_id: Uuid,
    sources: Vec<ImportSource>,
    batch: Batch,
    expected_revision: u64,
    key: Option<String>,
    tools: ToolConfig,
    match_first_video: bool,
    cancellation: Arc<Cancellation>,
}
struct State {
    store: Option<Store>,
    sequence: u64,
    events: VecDeque<Event>,
    jobs: HashMap<Uuid, Arc<Cancellation>>,
    imports: HashMap<Uuid, ImportRecord>,
    import_order: VecDeque<Uuid>,
    import_cache: VecDeque<CachedImport>,
}

struct Service {
    path: PathBuf,
    state: Mutex<State>,
    changed: Notify,
    render_slots: Semaphore,
    import_slots: Semaphore,
}

impl Service {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            state: Mutex::new(State {
                store: None,
                sequence: 0,
                events: VecDeque::new(),
                jobs: HashMap::new(),
                imports: HashMap::new(),
                import_order: VecDeque::new(),
                import_cache: VecDeque::new(),
            }),
            changed: Notify::new(),
            render_slots: Semaphore::new(1),
            import_slots: Semaphore::new(2),
        }
    }
    async fn store<'a>(&self, state: &'a mut State) -> Result<&'a mut Store> {
        if state.store.is_none() {
            state.store = Some(Store::open(&self.path).await?);
        }
        state
            .store
            .as_mut()
            .ok_or_else(|| Error::NotFound("project".into()))
    }
    fn emit(&self, state: &mut State, revision: Option<u64>, kind: &str, data: Value) {
        state.sequence = state.sequence.saturating_add(1);
        state.events.push_back(Event {
            sequence: state.sequence,
            revision,
            kind: kind.into(),
            data,
        });
        if state.events.len() > EVENT_CAPACITY {
            state.events.pop_front();
        }
        self.changed.notify_waiters();
    }
    async fn watch(&self, after: Option<u64>, wait_ms: u32) -> Result<Value> {
        if wait_ms > 30_000 {
            return Err(Error::Invalid("watch deadline exceeds 30 seconds".into()));
        }
        // Register before inspecting state so a commit cannot fall between inspection and waiting.
        let notified = self.changed.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        {
            let state = self.state.lock().await;
            if after == Some(state.sequence) && wait_ms > 0 {
                drop(state);
                match tokio::time::timeout(Duration::from_millis(u64::from(wait_ms)), notified)
                    .await
                {
                    Ok(()) | Err(_) => {}
                }
            }
        }
        let mut state = self.state.lock().await;
        let snapshot = if self.path.exists() {
            Some(self.store(&mut state).await?.inspect().await?)
        } else {
            None
        };
        let resync = after.is_none_or(|cursor| {
            cursor > state.sequence
                || state
                    .events
                    .front()
                    .is_some_and(|first| cursor.saturating_add(1) < first.sequence)
        });
        let events = state
            .events
            .iter()
            .filter(|event| resync || after.is_none_or(|cursor| event.sequence > cursor))
            .cloned()
            .collect();
        let import_jobs = if resync {
            state
                .import_order
                .iter()
                .filter_map(|id| state.imports.get(id).map(|record| record.status.clone()))
                .collect()
        } else {
            Vec::new()
        };
        Ok(serde_json::to_value(EventBatch {
            sequence: state.sequence,
            snapshot,
            events,
            resync,
            import_jobs,
        })?)
    }
    fn import_event(&self, state: &mut State, job_id: Uuid) -> Result<()> {
        let status = &state
            .imports
            .get(&job_id)
            .ok_or_else(|| Error::NotFound(format!("import job {job_id}")))?
            .status;
        let kind = match status.stage {
            ImportStage::Queued => "import_queued",
            ImportStage::Completed => "import_completed",
            ImportStage::Failed => "import_failed",
            ImportStage::Cancelled => "import_cancelled",
            _ => "import_progress",
        };
        let revision = status.revision;
        let data = serde_json::to_value(status)?;
        self.emit(state, revision, kind, data);
        Ok(())
    }
    async fn enqueue_import(
        self: &Arc<Self>,
        mut sources: Vec<ImportSource>,
        expected_revision: u64,
        key: Option<String>,
        tools: ToolConfig,
        match_first_video: bool,
    ) -> Result<Value> {
        let _: MediaTools = tools.clone().try_into()?;
        if sources.is_empty() || sources.len() > 32 {
            return Err(Error::Invalid("import must contain 1..32 sources".into()));
        }
        for source in &mut sources {
            source.path = std::path::absolute(&source.path)?;
            if source.name.trim().is_empty()
                || source.name.len() > 256
                || source.name.chars().any(char::is_control)
            {
                return Err(Error::Invalid("import name must be nonblank, at most 256 bytes and free of control characters".into()));
            }
        }
        let batch = Batch {
            operations: sources
                .iter()
                .flat_map(|source| {
                    [
                        Edit::Import {
                            id: source.media_id,
                            path: source.path.clone(),
                        },
                        Edit::RenameMedia {
                            media_id: source.media_id,
                            alias: Some(source.name.clone()),
                        },
                    ]
                })
                .collect(),
        };
        let cancellation = Arc::new(Cancellation {
            cancelled: AtomicBool::new(false),
            notify: Notify::new(),
        });
        let job_id = Uuid::new_v4();
        let mut state = self.state.lock().await;
        if state
            .imports
            .values()
            .filter(|record| !record.status.stage.is_terminal())
            .count()
            >= IMPORT_QUEUE_CAPACITY
        {
            return Err(Error::Invalid("import queue is full (16 jobs)".into()));
        }
        let store = self.store(&mut state).await?;
        let replay = store
            .replay_import(&batch, key.as_deref(), match_first_video)
            .await?;
        let project = store.inspect().await?;
        if replay.is_none() && project.revision != expected_revision {
            return Err(Error::Conflict {
                expected: expected_revision,
                current: project.revision,
            });
        }
        let match_first_video = match_first_video
            && project.clips.is_empty()
            && project.audio_clips.is_empty()
            && project.visual_clips.is_empty()
            && !project.media.iter().any(|source| {
                source
                    .metadata
                    .video
                    .as_ref()
                    .is_some_and(|video| video.codec != "png")
            });
        while state.imports.len() >= IMPORT_STATUS_CAPACITY {
            let Some(index) = state.import_order.iter().position(|id| {
                state
                    .imports
                    .get(id)
                    .is_some_and(|record| record.status.stage.is_terminal())
            }) else {
                return Err(Error::Invalid("import status capacity exhausted".into()));
            };
            if let Some(old) = state.import_order.remove(index) {
                state.imports.remove(&old);
            }
        }
        let status = ImportJob {
            job_id,
            sources: sources
                .iter()
                .map(|source| ImportSourceStatus {
                    media_id: source.media_id,
                    name: source.name.clone(),
                    quick: None,
                })
                .collect(),
            current_source: None,
            stage: ImportStage::Queued,
            revision: Some(expected_revision),
            elapsed_ms: 0,
            cache_hit: false,
            result: None,
            error: None,
        };
        state.imports.insert(
            job_id,
            ImportRecord {
                status,
                cancellation: Arc::clone(&cancellation),
            },
        );
        state.import_order.push_back(job_id);
        self.import_event(&mut state, job_id)?;
        if let Some(receipt) = replay {
            let record = state
                .imports
                .get_mut(&job_id)
                .ok_or_else(|| Error::NotFound(format!("import job {job_id}")))?;
            record.status.stage = ImportStage::Completed;
            record.status.revision = Some(receipt.revision);
            record.status.result = Some(serde_json::to_value(receipt)?);
            self.import_event(&mut state, job_id)?;
        } else {
            let service = Arc::clone(self);
            tokio::spawn(async move {
                service
                    .import_worker(ImportWork {
                        job_id,
                        sources,
                        batch,
                        expected_revision,
                        key,
                        tools,
                        match_first_video,
                        cancellation,
                    })
                    .await;
            });
        }
        Ok(serde_json::to_value(
            &state
                .imports
                .get(&job_id)
                .ok_or_else(|| Error::NotFound(format!("import job {job_id}")))?
                .status,
        )?)
    }
    async fn import_progress(
        &self,
        job_id: Uuid,
        source: usize,
        progress: ProbeProgress,
        started: Instant,
    ) -> Result<()> {
        let mut state = self.state.lock().await;
        let record = state
            .imports
            .get_mut(&job_id)
            .ok_or_else(|| Error::NotFound(format!("import job {job_id}")))?;
        record.status.stage = match progress.stage {
            ProbeStage::Inspecting => ImportStage::Inspecting,
            ProbeStage::Hashing => ImportStage::Hashing,
            _ => ImportStage::Validating,
        };
        record.status.current_source = Some(source);
        record.status.elapsed_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
        if let Some(quick) = progress.quick {
            let source = record
                .status
                .sources
                .get_mut(source)
                .ok_or_else(|| Error::Invalid("invalid import source index".into()))?;
            source.quick = Some(quick);
        }
        self.import_event(&mut state, job_id)
    }
    async fn prepare_job(
        &self,
        work: &ImportWork,
        started: Instant,
    ) -> Result<(Batch, lumeryn_core::storage::PreparedImports)> {
        let _permit = self
            .import_slots
            .acquire()
            .await
            .map_err(|_| Error::Invalid("import scheduler closed".into()))?;
        let tools: MediaTools = work.tools.clone().try_into()?;
        let policy = ToolPolicy::current(&work.tools)?;
        let mut verified_sources = Vec::with_capacity(work.sources.len());
        for (index, source) in work.sources.iter().enumerate() {
            let cached: Vec<VerifiedMetadata> = {
                let state = self.state.lock().await;
                state
                    .import_cache
                    .iter()
                    .filter(|entry| entry.policy == policy)
                    .map(|entry| entry.verified.clone())
                    .collect()
            };
            let (sender, mut progress) = mpsc::channel(16);
            let probe = tools.probe_with_cached_progress(&source.path, &cached, |update| {
                if let Err(error) = sender.try_send(update) {
                    tracing::debug!(%error, "import progress was coalesced");
                }
            });
            tokio::pin!(probe);
            let verified = loop {
                tokio::select! {
                    biased;
                    Some(update) = progress.recv() => self.import_progress(work.job_id, index, update, started).await?,
                    result = &mut probe => break result?,
                }
            };
            while let Ok(update) = progress.try_recv() {
                self.import_progress(work.job_id, index, update, started)
                    .await?;
            }
            if ToolPolicy::current(&work.tools)? != policy {
                return Err(Error::Invalid(
                    "configured media tools changed during import preparation".into(),
                ));
            }
            {
                let mut state = self.state.lock().await;
                let hit = cached.iter().any(|entry| {
                    entry.metadata().blake3 == verified.metadata().blake3
                        && entry.metadata().size_bytes == verified.metadata().size_bytes
                });
                if let Some(record) = state.imports.get_mut(&work.job_id) {
                    record.status.cache_hit |= hit;
                }
                state.import_cache.retain(|entry| {
                    !(entry.policy == policy
                        && entry.verified.metadata().blake3 == verified.metadata().blake3)
                });
                state.import_cache.push_back(CachedImport {
                    policy: policy.clone(),
                    verified: verified.clone(),
                });
                if state.import_cache.len() > IMPORT_CACHE_CAPACITY {
                    state.import_cache.pop_front();
                }
            }
            verified_sources.push(verified);
        }
        let mut batch = work.batch.clone();
        if work.match_first_video
            && let Some(video) = verified_sources
                .iter()
                .filter_map(|source| source.metadata().video.as_ref())
                .find(|video| video.codec != "png")
                .filter(|video| video.constant_rate)
        {
            batch.operations.push(Edit::SetFrameRate {
                frame_rate: video.frame_rate,
            });
        }
        let prepared =
            lumeryn_core::storage::prepare_verified_imports(&self.path, &batch, verified_sources)
                .await?;
        Ok((batch, prepared))
    }
    async fn import_worker(self: Arc<Self>, work: ImportWork) {
        let started = Instant::now();
        let cancelled = async {
            let notified = work.cancellation.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if !work.cancellation.cancelled.load(Ordering::Acquire) {
                notified.await;
            }
        };
        let prepared = tokio::select! {
            result = self.prepare_job(&work, started) => result,
            () = cancelled => Err(Error::Io(std::io::Error::new(std::io::ErrorKind::Interrupted, "import cancelled"))),
        };
        // Cancellation owns only preparation. The committing transition and transaction share
        // the state lock with Cancel; publication cannot later be relabelled cancelled.
        let result = async {
            let (batch, prepared) = prepared?;
            let mut state = self.state.lock().await;
            if work.cancellation.cancelled.load(Ordering::Acquire) {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "import cancelled",
                )));
            }
            let record = state
                .imports
                .get_mut(&work.job_id)
                .ok_or_else(|| Error::NotFound(format!("import job {}", work.job_id)))?;
            record.status.stage = ImportStage::Committing;
            record.status.elapsed_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
            self.import_event(&mut state, work.job_id)?;
            let receipt = self
                .store(&mut state)
                .await?
                .execute_prepared(
                    &batch,
                    Some(work.expected_revision),
                    work.key.as_deref(),
                    false,
                    prepared,
                )
                .await?;
            let value = serde_json::to_value(&receipt)?;
            if !receipt.replayed {
                self.emit(
                    &mut state,
                    Some(receipt.revision),
                    "project_committed",
                    value.clone(),
                );
            }
            Ok((receipt.revision, value))
        }
        .await;
        let mut state = self.state.lock().await;
        if let Some(record) = state.imports.get_mut(&work.job_id) {
            record.status.elapsed_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
            match result {
                Ok((revision, value)) => {
                    record.status.stage = ImportStage::Completed;
                    record.status.revision = Some(revision);
                    record.status.result = Some(value);
                }
                Err(error) => {
                    record.status.stage = if work.cancellation.cancelled.load(Ordering::Acquire)
                        && record.status.stage != ImportStage::Committing
                    {
                        ImportStage::Cancelled
                    } else {
                        ImportStage::Failed
                    };
                    record.status.error = Some(ImportFailure::from(&error));
                }
            }
        }
        if let Err(error) = self.import_event(&mut state, work.job_id) {
            tracing::error!(code = error.code(), "import completion event failed");
        }
    }
    async fn request(self: &Arc<Self>, request: Request) -> Result<Value> {
        if let Request::Import {
            sources,
            expected_revision,
            key,
            tools,
            match_first_video,
        } = request
        {
            return self
                .enqueue_import(sources, expected_revision, key, tools, match_first_video)
                .await;
        }
        if let Request::Watch {
            after_sequence,
            wait_ms,
        } = request
        {
            return self.watch(after_sequence, wait_ms).await;
        }
        if let Request::Render {
            output,
            expected_revision,
            tools,
            job_id,
            options,
        } = request
        {
            return self
                .render(
                    output,
                    expected_revision,
                    tools.try_into()?,
                    job_id,
                    options,
                )
                .await;
        }
        if let Request::Execute {
            batch,
            expected_revision,
            key,
            dry_run,
            tools,
        } = request
        {
            let tools: MediaTools = tools.try_into()?;
            {
                let mut state = self.state.lock().await;
                if let Some(receipt) = self
                    .store(&mut state)
                    .await?
                    .replay(&batch, key.as_deref(), dry_run)
                    .await?
                {
                    return Ok(serde_json::to_value(receipt)?);
                }
            }
            let _permit = if batch.operations.iter().any(|operation| {
                matches!(
                    operation,
                    lumeryn_core::Edit::Import { .. } | lumeryn_core::Edit::AddAudioClip { .. }
                )
            }) {
                Some(
                    self.import_slots
                        .acquire()
                        .await
                        .map_err(|_| Error::Invalid("import scheduler closed".into()))?,
                )
            } else {
                None
            };
            let prepared =
                lumeryn_core::storage::prepare_imports(&self.path, &batch, &tools).await?;
            let mut state = self.state.lock().await;
            let receipt = self
                .store(&mut state)
                .await?
                .execute_prepared(&batch, expected_revision, key.as_deref(), dry_run, prepared)
                .await?;
            let value = serde_json::to_value(&receipt)?;
            if !receipt.dry_run && !receipt.replayed {
                self.emit(
                    &mut state,
                    Some(receipt.revision),
                    "project_committed",
                    value.clone(),
                );
            }
            return Ok(value);
        }
        if let Request::Check { assets } = request {
            let (mut check, project) = {
                let mut state = self.state.lock().await;
                let store = self.store(&mut state).await?;
                let check = store.integrity_check().await?;
                let project = if assets {
                    Some(store.inspect().await?)
                } else {
                    None
                };
                (check, project)
            };
            if let Some(project) = project {
                let parent = self
                    .path
                    .parent()
                    .ok_or_else(|| Error::Invalid("missing project parent".into()))?;
                let mut checked = 0;
                for media in project.media {
                    let path = if media.path.is_absolute() {
                        media.path
                    } else {
                        parent.join(media.path)
                    };
                    let expected = media.metadata.blake3.ok_or_else(|| {
                        Error::Unsupported("source asset has no imported content digest".into())
                    })?;
                    let hash = lumeryn_core::media::content_hash(&path).await?;
                    if hash != expected {
                        return Err(Error::Invalid(format!(
                            "source content changed: {}",
                            path.display()
                        )));
                    }
                    checked += 1;
                }
                check["source_assets"] = json!({"status":"verified","count":checked});
            }
            return Ok(check);
        }
        let mut state = self.state.lock().await;
        match request {
            Request::Create { project } => {
                // Validate/create even if this service already has a Store; never replace it.
                let mut store = Store::create(&self.path, &project).await?;
                let result = json!({"project":store.inspect().await?,"sqlite_version":store.sqlite_version().await?});
                state.store = Some(store);
                self.emit(
                    &mut state,
                    Some(0),
                    "project_created",
                    json!({"project_id":project.id}),
                );
                Ok(result)
            }
            Request::Inspect => Ok(serde_json::to_value(
                self.store(&mut state).await?.inspect().await?,
            )?),
            Request::History => Ok(serde_json::to_value(
                self.store(&mut state).await?.history().await?,
            )?),
            Request::Migrate => {
                let value = self.store(&mut state).await?.migrate().await?;
                let revision = self.store(&mut state).await?.inspect().await?.revision;
                self.emit(
                    &mut state,
                    Some(revision),
                    "storage_migrated",
                    value.clone(),
                );
                Ok(value)
            }
            Request::Backup { output } => self.store(&mut state).await?.backup(&output).await,
            Request::Search { query, limit } => Ok(serde_json::to_value(
                self.store(&mut state).await?.search(&query, limit).await?,
            )?),
            Request::Cancel { job_id } => {
                if let Some(record) = state.imports.get(&job_id) {
                    if record.status.stage == ImportStage::Committing
                        || record.status.stage.is_terminal()
                    {
                        return Err(Error::Invalid(
                            "import cancellation is too late once commit begins or job is terminal"
                                .into(),
                        ));
                    }
                    record.cancellation.cancelled.store(true, Ordering::Release);
                    record.cancellation.notify.notify_waiters();
                    let data = serde_json::to_value(&record.status)?;
                    self.emit(&mut state, None, "import_cancelling", data);
                    return Ok(json!({"job_id":job_id,"cancellation_requested":true}));
                }
                let cancellation = state
                    .jobs
                    .get(&job_id)
                    .ok_or_else(|| Error::NotFound(format!("active job {job_id}")))?;
                cancellation.cancelled.store(true, Ordering::Release);
                cancellation.notify.notify_waiters();
                self.emit(&mut state, None, "job_cancelling", json!({"job_id":job_id}));
                Ok(json!({"job_id":job_id,"cancellation_requested":true}))
            }
            Request::ImportStatus { job_id } => Ok(serde_json::to_value(
                &state
                    .imports
                    .get(&job_id)
                    .ok_or_else(|| Error::NotFound(format!("import job {job_id}")))?
                    .status,
            )?),
            Request::Check { .. }
            | Request::Import { .. }
            | Request::Execute { .. }
            | Request::Render { .. }
            | Request::Watch { .. } => Err(Error::Invalid("invalid dispatch".into())),
        }
    }
    async fn render(
        &self,
        output: PathBuf,
        expected: Option<u64>,
        tools: MediaTools,
        job_id: Uuid,
        options: render::RenderOptions,
    ) -> Result<Value> {
        options.validate()?;
        let cancellation = Arc::new(Cancellation {
            cancelled: AtomicBool::new(false),
            notify: Notify::new(),
        });
        let project: Project;
        {
            let mut state = self.state.lock().await;
            if state.jobs.len() >= 16 {
                return Err(Error::Invalid("render queue is full (16 jobs)".into()));
            }
            if state.jobs.contains_key(&job_id) {
                return Err(Error::Invalid("job ID is already active".into()));
            }
            project = self.store(&mut state).await?.inspect().await?;
            if let Some(expected) = expected
                && expected != project.revision
            {
                return Err(Error::Conflict {
                    expected,
                    current: project.revision,
                });
            }
            state.jobs.insert(job_id, Arc::clone(&cancellation));
            self.emit(
                &mut state,
                Some(project.revision),
                "job_queued",
                json!({"job_id":job_id,"options":options}),
            );
        }
        let work = async {
            let _permit = self
                .render_slots
                .acquire()
                .await
                .map_err(|_| Error::Invalid("render scheduler closed".into()))?;
            {
                let mut state = self.state.lock().await;
                self.emit(
                    &mut state,
                    Some(project.revision),
                    "job_running",
                    json!({"job_id":job_id}),
                );
            }
            let revision = project.revision;
            let progress = |stage| async move {
                let mut state = self.state.lock().await;
                self.emit(
                    &mut state,
                    Some(revision),
                    "job_progress",
                    json!({"job_id":job_id,"stage":stage}),
                );
            };
            Ok(serde_json::to_value(
                render::export_with_progress(
                    &project, &self.path, &output, &tools, &options, progress,
                )
                .await?,
            )?)
        };
        let cancelled = async {
            let notified = cancellation.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if !cancellation.cancelled.load(Ordering::Acquire) {
                notified.await;
            }
        };
        let result = tokio::select! {result=work=>result, ()=cancelled=>Err(Error::Io(std::io::Error::new(std::io::ErrorKind::Interrupted,"render cancelled")))};
        let mut state = self.state.lock().await;
        state.jobs.remove(&job_id);
        // Successful publication is irrevocable. A late cancellation while waiting for
        // this lock cannot turn an already verified output into a cancelled outcome.
        let kind = if result.is_ok() {
            "job_completed"
        } else if cancellation.cancelled.load(Ordering::Acquire) {
            "job_cancelled"
        } else {
            "job_failed"
        };
        let data = match &result {
            Ok(value) => json!({"job_id":job_id,"result":value}),
            Err(error) => {
                json!({"job_id":job_id,"error":{"code":error.code(),"message":error.to_string()}})
            }
        };
        self.emit(&mut state, Some(project.revision), kind, data);
        result
    }
}

async fn connection(
    mut pipe: impl AsyncRead + AsyncWrite + Unpin,
    service: Arc<Service>,
) -> Result<()> {
    let envelope: Envelope = tokio::time::timeout(Duration::from_secs(5), read_frame(&mut pipe))
        .await
        .map_err(|_| Error::Timeout)??;
    let result = if envelope.version != PROTOCOL_VERSION {
        Err(Error::Unsupported(format!(
            "IPC protocol {}",
            envelope.version
        )))
    } else {
        service.request(envelope.request).await
    };
    let response = Response {
        version: PROTOCOL_VERSION,
        id: envelope.id,
        result: result.map_err(Into::into),
    };
    tokio::time::timeout(Duration::from_secs(5), write_frame(&mut pipe, &response))
        .await
        .map_err(|_| Error::Timeout)??;
    Ok(())
}

/// Run one authoritative service for this project. Windows transport uses an explicit user ACL.
pub async fn serve(project: impl AsRef<Path>) -> Result<()> {
    #[cfg(windows)]
    {
        let path = crate::absolute_project(project.as_ref())?;
        let name = crate::windows::pipe_name(&path)?;
        let service = Arc::new(Service::new(path));
        let slots = Arc::new(Semaphore::new(32));
        let mut listener = crate::windows::create_pipe(&name, true)?;
        let mut workers = tokio::task::JoinSet::new();
        loop {
            // Do not exit while a watcher/render/worker is active. Idle services release the writer.
            tokio::select! {
                connected=listener.connect()=>{
                    connected?;
                    let next=crate::windows::create_pipe(&name,false)?;
                    let pipe=std::mem::replace(&mut listener,next);
                    let permit=Arc::clone(&slots).try_acquire_owned().map_err(|_|Error::Invalid("too many service clients".into()));
                    match permit {
                        Ok(permit)=>{let service=Arc::clone(&service);workers.spawn(async move {let _permit=permit;connection(pipe,service).await});},
                        Err(error)=>{tracing::warn!(code=error.code(),"service client limit reached");drop(pipe);},
                    }
                },
                completed=workers.join_next(),if !workers.is_empty()=>match completed {
                    Some(Ok(Ok(())))|None=>{},
                    Some(Ok(Err(error)))=>tracing::debug!(code=error.code(),"service connection closed"),
                    Some(Err(error))=>tracing::error!(%error,"service worker failed"),
                },
                ()=tokio::time::sleep(Duration::from_secs(10))=>{
                    if workers.is_empty() && Arc::strong_count(&service) == 1 { return Ok(()); }
                },
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = project;
        Err(Error::Unsupported(
            "secure local service transport currently requires Windows".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unsupported_version_has_structured_correlated_error() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let service = Arc::new(Service::new(directory.path().join("project.lumeryn")));
        let (mut client, server) = tokio::io::duplex(4096);
        let task = tokio::spawn(connection(server, service));
        let id = Uuid::new_v4();
        write_frame(
            &mut client,
            &Envelope {
                version: 99,
                id,
                request: Request::Inspect,
            },
        )
        .await?;
        let response: Response = read_frame(&mut client).await?;
        assert_eq!(response.id, id);
        assert_eq!(response.version, PROTOCOL_VERSION);
        assert_eq!(
            response.result.err().map(|error| error.code),
            Some("unsupported".into())
        );
        task.await
            .map_err(|error| Error::Io(std::io::Error::other(error)))??;
        Ok(())
    }

    #[tokio::test]
    async fn bounded_import_queue_cancellation_snapshot_recovery_and_retention() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let service = Arc::new(Service::new(directory.path().join("project.lumeryn")));
        service
            .request(Request::Create {
                project: Project::new("Queue".into(), 64, 36, "30/1".parse()?)?,
            })
            .await?;
        let permit = service
            .import_slots
            .acquire_many(2)
            .await
            .map_err(|_| Error::Invalid("closed import scheduler".into()))?;
        let request = |revision| Request::Import {
            sources: vec![ImportSource {
                path: directory.path().join("missing.wav"),
                media_id: Uuid::new_v4(),
                name: "Queued source".into(),
            }],
            expected_revision: revision,
            key: None,
            tools: ToolConfig {
                ffmpeg: directory.path().join("missing-ffmpeg"),
                ffprobe: directory.path().join("missing-ffprobe"),
                timeout_seconds: 1,
            },
            match_first_video: false,
        };
        let mut jobs = Vec::new();
        for _ in 0..IMPORT_QUEUE_CAPACITY {
            let job: ImportJob = serde_json::from_value(service.request(request(0)).await?)?;
            assert_eq!(job.stage, ImportStage::Queued);
            assert!(job.sources.iter().all(|source| source.quick.is_none()));
            jobs.push(job.job_id);
        }
        assert!(
            matches!(service.request(request(0)).await, Err(Error::Invalid(message)) if message.contains("queue is full"))
        );
        assert!(
            Arc::strong_count(&service) > 1,
            "detached queued imports keep idle service alive"
        );
        let value = service
            .request(Request::Execute {
                batch: Batch {
                    operations: vec![Edit::SetCanvas {
                        width: 128,
                        height: 72,
                    }],
                },
                expected_revision: Some(0),
                key: None,
                dry_run: false,
                tools: ToolConfig::default(),
            })
            .await?;
        assert_eq!(value["revision"], 1);
        service.request(Request::Cancel { job_id: jobs[0] }).await?;
        let started = Instant::now();
        loop {
            let job: ImportJob = serde_json::from_value(
                service
                    .request(Request::ImportStatus { job_id: jobs[0] })
                    .await?,
            )?;
            if job.stage == ImportStage::Cancelled {
                break;
            }
            if started.elapsed() > Duration::from_secs(2) {
                return Err(Error::Timeout);
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        assert!(
            service
                .request(Request::Cancel { job_id: jobs[0] })
                .await
                .is_err()
        );
        let snapshot: EventBatch = serde_json::from_value(service.watch(None, 0).await?)?;
        assert_eq!(snapshot.import_jobs.len(), IMPORT_QUEUE_CAPACITY);
        assert!(
            snapshot
                .import_jobs
                .iter()
                .any(|job| job.stage == ImportStage::Cancelled)
        );
        assert_eq!(snapshot.snapshot.map(|project| project.revision), Some(1));
        drop(permit);
        let started = Instant::now();
        loop {
            if service
                .state
                .lock()
                .await
                .imports
                .values()
                .all(|record| record.status.stage.is_terminal())
            {
                break;
            }
            if started.elapsed() > Duration::from_secs(3) {
                return Err(Error::Timeout);
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        for _ in 0..IMPORT_STATUS_CAPACITY {
            let job: ImportJob = serde_json::from_value(service.request(request(1)).await?)?;
            let started = Instant::now();
            loop {
                let status: ImportJob = serde_json::from_value(
                    service
                        .request(Request::ImportStatus { job_id: job.job_id })
                        .await?,
                )?;
                if status.stage.is_terminal() {
                    assert_eq!(status.stage, ImportStage::Failed);
                    break;
                }
                if started.elapsed() > Duration::from_secs(2) {
                    return Err(Error::Timeout);
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
        let state = service.state.lock().await;
        assert_eq!(state.imports.len(), IMPORT_STATUS_CAPACITY);
        assert_eq!(state.import_order.len(), IMPORT_STATUS_CAPACITY);
        assert!(state.import_cache.is_empty());
        assert!(!state.imports.contains_key(&jobs[0]));
        drop(state);
        let project: Project = serde_json::from_value(service.request(Request::Inspect).await?)?;
        assert_eq!(project.revision, 1);
        assert!(project.media.is_empty());
        Ok(())
    }
}
