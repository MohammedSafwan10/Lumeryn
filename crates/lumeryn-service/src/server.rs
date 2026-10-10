use crate::{
    EVENT_CAPACITY, Event, EventBatch, PROTOCOL_VERSION, Request,
    protocol::{Envelope, Response},
    read_frame, write_frame,
};
use lumeryn_core::{Error, MediaTools, Project, Result, Store, render};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Mutex, Notify, Semaphore},
};
use uuid::Uuid;

struct Cancellation {
    cancelled: AtomicBool,
    notify: Notify,
}
struct State {
    store: Option<Store>,
    sequence: u64,
    events: VecDeque<Event>,
    jobs: HashMap<Uuid, Arc<Cancellation>>,
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
        Ok(serde_json::to_value(EventBatch {
            sequence: state.sequence,
            snapshot,
            events,
            resync,
        })?)
    }
    async fn request(&self, request: Request) -> Result<Value> {
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
                let cancellation = state
                    .jobs
                    .get(&job_id)
                    .ok_or_else(|| Error::NotFound(format!("active job {job_id}")))?;
                cancellation.cancelled.store(true, Ordering::Release);
                cancellation.notify.notify_waiters();
                self.emit(&mut state, None, "job_cancelling", json!({"job_id":job_id}));
                Ok(json!({"job_id":job_id,"cancellation_requested":true}))
            }
            Request::Check { .. }
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
                ()=tokio::time::sleep(Duration::from_secs(10)),if workers.is_empty()=>return Ok(()),
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
}
