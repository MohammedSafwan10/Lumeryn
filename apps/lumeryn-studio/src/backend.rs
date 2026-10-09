use egui::Context;
use lumeryn_core::{Batch, Error, MediaTools, Project};
use lumeryn_service::{Client, Event, Request, ToolConfig};
use serde_json::Value;
use std::{io::Write, path::PathBuf};
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Debug)]
pub enum Command {
    Open {
        path: PathBuf,
        epoch: u64,
    },
    Edit {
        batch: Batch,
        revision: u64,
        label: String,
    },
    Render {
        output: PathBuf,
        revision: u64,
        job_id: Uuid,
    },
    Cancel {
        job_id: Uuid,
    },
    History,
    CliTrim {
        clip: Uuid,
        source_in: u64,
        duration: u64,
        revision: u64,
    },
    Screenshot {
        path: PathBuf,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    Report {
        path: PathBuf,
        value: Value,
    },
    Shutdown,
}

#[derive(Debug)]
pub enum Message {
    Snapshot(Project),
    Events(Vec<Event>),
    History(Vec<Value>),
    Complete { label: String, value: Value },
    Connection(String),
    Error { label: String, message: String },
    Artifact(PathBuf),
}

#[derive(Debug)]
pub struct Envelope {
    pub epoch: u64,
    pub message: Message,
}

pub struct Backend {
    pub commands: mpsc::Sender<Command>,
    pub messages: mpsc::Receiver<Envelope>,
}

async fn emit(tx: &mpsc::Sender<Envelope>, ctx: &Context, epoch: u64, message: Message) -> bool {
    // Request repaint before waiting for capacity so the UI drains a full bounded channel.
    ctx.request_repaint();
    tx.send(Envelope { epoch, message }).await.is_ok()
}

impl Backend {
    pub fn start(ctx: Context, tools: ToolConfig) -> Self {
        let (commands, mut requests) = mpsc::channel::<Command>(32);
        let (responses, messages) = mpsc::channel::<Envelope>(128);
        let startup_tx = responses.clone();
        let startup_ctx = ctx.clone();
        let thread = std::thread::Builder::new().name("lumeryn-studio-service".into()).spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2).enable_all().build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    ctx.request_repaint();
                    if let Err(send_error) = responses.blocking_send(Envelope {
                        epoch: 0,
                        message: Message::Error { label: "Runtime".into(), message: error.to_string() },
                    }) {
                        eprintln!("Studio runtime notification failed: {send_error}");
                    }
                    return;
                }
            };
            runtime.block_on(async move {
                let mut selected: Option<(PathBuf, Client, u64)> = None;
                let mut watcher: Option<tokio::task::JoinHandle<()>> = None;
                let mut renderer: Option<tokio::task::JoinHandle<()>> = None;
                while let Some(command) = requests.recv().await {
                    let epoch = selected.as_ref().map_or(0, |(_, _, epoch)| *epoch);
                    match command {
                        Command::Shutdown => break,
                        Command::Open { path, epoch } => {
                            if let Some(watcher) = watcher.take() { watcher.abort(); }
                            selected = None;
                            if !emit(&responses, &ctx, epoch, Message::Connection("Connecting to project service…".into())).await { break; }
                            match Client::connect(&path).await {
                                Ok(client) => {
                                    let database = path.clone();
                                    let watching = client.clone();
                                    let tx = responses.clone();
                                    let wake = ctx.clone();
                                    watcher = Some(tokio::spawn(async move {
                                        let mut sequence = None;
                                        let mut revision = None;
                                        loop {
                                            match watching.watch(sequence, 2000).await {
                                                Ok(batch) => {
                                                    sequence = Some(batch.sequence);
                                                    if let Some(snapshot) = batch.snapshot
                                                        && (revision != Some(snapshot.revision) || batch.resync)
                                                    {
                                                        revision = Some(snapshot.revision);
                                                        if !emit(&tx, &wake, epoch, Message::Snapshot(snapshot)).await { break; }
                                                        match watching.history().await {
                                                            Ok(history) => { if !emit(&tx, &wake, epoch, Message::History(history)).await { break; } }
                                                            Err(error) => { if !emit(&tx, &wake, epoch, Message::Error { label: "History".into(), message: error.to_string() }).await { break; } }
                                                        }
                                                    }
                                                    if !batch.events.is_empty() && !emit(&tx, &wake, epoch, Message::Events(batch.events)).await { break; }
                                                }
                                                Err(error) => {
                                                    if !emit(&tx, &wake, epoch, Message::Connection(format!("Service interrupted: {error}. Reconnecting…"))).await { break; }
                                                    sequence = None;
                                                    revision = None;
                                                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                                    // connect performs authenticated discovery/autostart; no GUI-thread work.
                                                    if let Err(error) = Client::connect(&path).await
                                                        && !emit(&tx, &wake, epoch, Message::Connection(format!("Reconnect failed: {error}"))).await { break; }
                                                }
                                            }
                                        }
                                    }));
                                    // The watcher owns a cloned path inside its task.
                                    selected = Some((database, client, epoch));
                                }
                                Err(error) => { emit(&responses, &ctx, epoch, Message::Error { label: "Open project".into(), message: error.to_string() }).await; }
                            }
                        }
                        Command::Screenshot { path, width, height, rgba } => {
                            let saved = path.clone();
                            let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
                                if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
                                let file = std::fs::File::create_new(&path).map_err(|e| e.to_string())?;
                                let encoder = image::codecs::png::PngEncoder::new(file);
                                image::ImageEncoder::write_image(encoder, &rgba, width, height, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())
                            }).await;
                            match result {
                                Ok(Ok(())) => { emit(&responses, &ctx, 0, Message::Artifact(saved)).await; }
                                other => { emit(&responses, &ctx, 0, Message::Error { label: "Screenshot".into(), message: format!("{other:?}") }).await; }
                            }
                        }
                        Command::Report { path, value } => {
                            let saved = path.clone();
                            let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
                                if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
                                let mut file = std::fs::File::create_new(path).map_err(|e| e.to_string())?;
                                file.write_all(&serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                                file.sync_all().map_err(|e| e.to_string())
                            }).await;
                            match result {
                                Ok(Ok(())) => { emit(&responses, &ctx, 0, Message::Artifact(saved)).await; }
                                other => { emit(&responses, &ctx, 0, Message::Error { label: "Report".into(), message: format!("{other:?}") }).await; }
                            }
                        }
                        command => {
                            let Some((database, client, _)) = &selected else {
                                emit(&responses, &ctx, epoch, Message::Error { label: "Project".into(), message: "Open a project first".into() }).await;
                                continue;
                            };
                            match command {
                                Command::Edit { batch, revision, label } => {
                                    let result = client.execute(batch, Some(revision), Some(format!("studio-{}", Uuid::new_v4())), false, tools.clone()).await;
                                    complete(&responses, &ctx, epoch, label, result).await;
                                }
                                Command::History => {
                                    match client.history().await {
                                        Ok(history) => { emit(&responses, &ctx, epoch, Message::History(history)).await; }
                                        Err(error) => { emit(&responses, &ctx, epoch, Message::Error { label: "History".into(), message: error.to_string() }).await; }
                                    }
                                }
                                Command::Render { output, revision, job_id } => {
                                    if renderer.as_ref().is_some_and(|task| !task.is_finished()) {
                                        emit(&responses, &ctx, epoch, Message::Error { label: "Export".into(), message: "An export request is already active".into() }).await;
                                        continue;
                                    }
                                    let client = client.clone();
                                    let tools = tools.clone();
                                    let tx = responses.clone();
                                    let wake = ctx.clone();
                                    renderer = Some(tokio::spawn(async move {
                                        let result = client.call(Request::Render { output, expected_revision: Some(revision), tools, job_id }).await;
                                        complete(&tx, &wake, epoch, "Export".into(), result).await;
                                    }));
                                }
                                Command::Cancel { job_id } => {
                                    complete(&responses, &ctx, epoch, "Cancel export".into(), client.call(Request::Cancel { job_id }).await).await;
                                }
                                Command::CliTrim { clip, source_in, duration, revision } => {
                                    let result = cli_trim(database, clip, source_in, duration, revision, &tools).await;
                                    complete(&responses, &ctx, epoch, "CLI trim".into(), result).await;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                if let Some(watcher) = watcher { watcher.abort(); }
                // A launched service job is service-owned and can continue after Studio closes.
                if let Some(renderer) = renderer { renderer.abort(); }
            });
            runtime.shutdown_timeout(std::time::Duration::from_secs(2));
        });
        if let Err(error) = thread {
            startup_ctx.request_repaint();
            if let Err(send_error) = startup_tx.try_send(Envelope {
                epoch: 0,
                message: Message::Error {
                    label: "Worker thread".into(),
                    message: error.to_string(),
                },
            }) {
                eprintln!("Studio worker notification failed: {send_error}");
            }
        }
        Self { commands, messages }
    }
}

async fn complete(
    tx: &mpsc::Sender<Envelope>,
    ctx: &Context,
    epoch: u64,
    label: String,
    result: lumeryn_core::Result<Value>,
) {
    let message = match result {
        Ok(value) => Message::Complete { label, value },
        Err(error) => Message::Error {
            label,
            message: error.to_string(),
        },
    };
    emit(tx, ctx, epoch, message).await;
}

async fn cli_trim(
    database: &std::path::Path,
    clip: Uuid,
    source_in: u64,
    duration: u64,
    revision: u64,
    tools: &ToolConfig,
) -> lumeryn_core::Result<Value> {
    let executable = std::env::current_exe()?
        .parent()
        .ok_or_else(|| Error::Invalid("Studio executable has no parent".into()))?
        .join(if cfg!(windows) {
            "lumeryn.exe"
        } else {
            "lumeryn"
        });
    let args = vec![
        "timeline".into(),
        "clip".into(),
        "trim".into(),
        "--project".into(),
        database.as_os_str().to_owned(),
        "--clip-id".into(),
        clip.to_string().into(),
        "--source-in-frame".into(),
        source_in.to_string().into(),
        "--duration-frames".into(),
        duration.to_string().into(),
        "--expected-revision".into(),
        revision.to_string().into(),
        "--ffmpeg".into(),
        tools.ffmpeg.as_os_str().to_owned(),
        "--ffprobe".into(),
        tools.ffprobe.as_os_str().to_owned(),
    ];
    let media = MediaTools {
        ffmpeg: tools.ffmpeg.clone(),
        ffprobe: tools.ffprobe.clone(),
        timeout: std::time::Duration::from_secs(30),
    };
    let bytes = media.run(&executable, &args).await?;
    Ok(serde_json::from_slice(&bytes)?)
}
