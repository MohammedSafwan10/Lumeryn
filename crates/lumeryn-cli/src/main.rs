use clap::{Args, Parser, Subcommand, ValueEnum};
use lumeryn_core::{
    Batch, Edit, Error, MediaTools, Project, Result, Store, render, time::FrameRate,
};
use serde_json::{Value, json};
use std::{path::PathBuf, process::ExitCode, time::Duration};
use tracing::Instrument;
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "lumeryn",
    version,
    about = "Lumeryn — proprietary agent-first editing CLI",
    after_help = "Every project is explicitly selected. Editing time is integer frames. See docs/cli/WORKFLOWS.md."
)]
struct Cli {
    /// Trusted, locally installed FFmpeg executable. No shell is used.
    #[arg(long, global = true, default_value = "ffmpeg", env = "LUMERYN_FFMPEG")]
    ffmpeg: PathBuf,
    #[arg(
        long,
        global = true,
        default_value = "ffprobe",
        env = "LUMERYN_FFPROBE"
    )]
    ffprobe: PathBuf,
    /// Deadline per external process, seconds (1..3600).
    #[arg(long, global = true, default_value = "300", value_parser = clap::value_parser!(u64).range(1..=3600))]
    timeout_seconds: u64,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Discover implemented and planned capabilities.
    Capabilities,
    /// JSON Schema generated from the core's atomic batch command types.
    Schema,
    /// Installed external media tool versions/configuration.
    Doctor,
    #[command(subcommand)]
    Project(ProjectCommand),
    #[command(subcommand)]
    Media(MediaCommand),
    #[command(subcommand)]
    Timeline(TimelineCommand),
    /// Export and fully decode-verify baseline MPEG-4/AAC MP4 to a new path.
    Render {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        expected_revision: Option<u64>,
    },
    #[command(subcommand)]
    Hardware(HardwareCommand),
}

#[derive(Debug, Subcommand)]
enum ProjectCommand {
    /// Create a new database; existing paths are never overwritten.
    Create {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        width: Option<u32>,
        #[arg(long)]
        height: Option<u32>,
        #[arg(long, value_enum, default_value = "landscape")]
        preset: ProjectPreset,
        #[arg(long, default_value = "30/1")]
        fps: FrameRate,
    },
    /// Reopen, validate and inspect a committed project snapshot.
    Inspect {
        #[arg(long)]
        project: PathBuf,
    },
    /// Reopen a project headlessly; emits the latest committed snapshot.
    Open {
        #[arg(long)]
        project: PathBuf,
    },
    History {
        #[arg(long)]
        project: PathBuf,
    },
    /// Verify SQLite structures, project invariants, history and derived metadata.
    Check {
        #[arg(long)]
        project: PathBuf,
        /// Also verify referenced source files and their imported content digests.
        #[arg(long)]
        assets: bool,
    },
    /// Explicitly migrate legacy storage indexes in a transaction.
    Migrate {
        #[arg(long)]
        project: PathBuf,
    },
    /// Consistent SQLite snapshot to a new file; asset references are not copied.
    Backup {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Literal-phrase metadata search using SQLite FTS5; transcripts are planned.
    Search {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        query: String,
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    /// Apply a JSON file of typed operations atomically.
    Batch {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Args)]
struct Write {
    #[arg(long)]
    project: PathBuf,
    /// Reject stale edits; conflict exit 3 includes the current revision.
    #[arg(long)]
    expected_revision: Option<u64>,
    /// Safe retry key. Use the same operation parameters and stable IDs on retries.
    #[arg(long)]
    idempotency_key: Option<String>,
}

#[derive(Debug, Subcommand)]
enum MediaCommand {
    Probe {
        #[arg(long)]
        path: PathBuf,
    },
    Import {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        path: PathBuf,
        /// Supply an ID for safe idempotent retries / batch references.
        #[arg(long)]
        id: Option<Uuid>,
    },
}

#[derive(Debug, Subcommand)]
enum TimelineCommand {
    Inspect {
        #[arg(long)]
        project: PathBuf,
    },
    #[command(subcommand)]
    Clip(ClipCommand),
}

#[derive(Debug, Subcommand)]
enum ClipCommand {
    Add {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        id: Option<Uuid>,
        #[arg(long)]
        media_id: Uuid,
        #[arg(long, default_value = "0")]
        at_frame: u64,
        #[arg(long, default_value = "0")]
        source_in_frame: u64,
        #[arg(long)]
        duration_frames: u64,
    },
    Trim {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        clip_id: Uuid,
        #[arg(long)]
        source_in_frame: u64,
        #[arg(long)]
        duration_frames: u64,
    },
}

#[derive(Debug, Subcommand)]
enum HardwareCommand {
    Scan,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ProjectPreset {
    Landscape,
    Portrait,
    Square,
}

impl ProjectPreset {
    fn dimensions(self) -> (u32, u32) {
        match self {
            Self::Landscape => (1920, 1080),
            Self::Portrait => (1080, 1920),
            Self::Square => (1080, 1080),
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let request_id = Uuid::new_v4();
    let started = std::time::Instant::now();
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if error.use_stderr() {
                eprintln!(
                    "{}",
                    json!({"ok":false,"error":{"code":"invalid_arguments","message":error.to_string()}})
                );
                return ExitCode::from(2);
            }
            if let Err(error) = error.print() {
                eprintln!(
                    "{}",
                    json!({"ok":false,"error":{"code":"io_failure","message":error.to_string()}})
                );
                return ExitCode::from(1);
            }
            return ExitCode::SUCCESS;
        }
    };
    let result = tokio::select! {
        result = execute(cli).instrument(tracing::info_span!("cli_request", request_id = %request_id)) => result,
        signal = tokio::signal::ctrl_c() => match signal {
            Ok(()) => { eprintln!("{}", json!({"ok":false,"error":{"code":"cancelled","message":"operation cancelled; inspect revision before retry"}})); return ExitCode::from(130); },
            Err(error) => Err(Error::Io(error)),
        }
    };
    match result {
        Ok(value) => {
            println!(
                "{}",
                json!({"ok": true, "request_id":request_id,"elapsed_ms":started.elapsed().as_millis(),"result": value})
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            let conflict = match &error {
                Error::Conflict { expected, current } => {
                    Some(json!({"expected":expected,"current":current}))
                }
                _ => None,
            };
            eprintln!(
                "{}",
                json!({"ok": false,"request_id":request_id,"elapsed_ms":started.elapsed().as_millis(), "error": {"code": error.code(), "message": error.to_string(), "conflict": conflict}})
            );
            ExitCode::from(error.exit_code())
        }
    }
}

fn operation_id(id: Option<Uuid>, write: &Write) -> Result<Uuid> {
    if write.idempotency_key.is_some() && id.is_none() {
        return Err(Error::Invalid(
            "--id is required with --idempotency-key for import/add".into(),
        ));
    }
    Ok(id.unwrap_or_else(Uuid::new_v4))
}

async fn edit(write: Write, batch: Batch, dry_run: bool, tools: &MediaTools) -> Result<Value> {
    let mut store = Store::open(&write.project).await?;
    Ok(serde_json::to_value(
        store
            .execute(
                &batch,
                write.expected_revision,
                write.idempotency_key.as_deref(),
                dry_run,
                tools,
            )
            .await?,
    )?)
}

async fn execute(cli: Cli) -> Result<Value> {
    let tools = MediaTools {
        ffmpeg: cli.ffmpeg,
        ffprobe: cli.ffprobe,
        timeout: Duration::from_secs(cli.timeout_seconds),
    };
    match cli.command {
        Command::Capabilities => Ok(json!({
            "api_version":1,"project_schema_version":1,"output":"json",
            "implemented":["project.create","project.open","project.inspect","project.history","project.batch","project.check","project.migrate","project.backup","project.search.metadata_fts5","media.probe","media.import","timeline.inspect","timeline.clip.add","timeline.clip.trim","render.baseline_mpeg4_aac","hardware.scan.basic","schema","doctor"],
            "requires_external_tools":["media.probe","media.import","render"],
            "export_limits":{"track":"one video track","layout":"1..64 contiguous nonoverlapping clips starting at frame 0","source":"matching CFR; frame timestamps verified","audio":"embedded audio or silence; stereo 48000Hz","color":"baseline SDR; no managed HDR"},
            "requirements":{"ordinary_editing":{"gpu_required":false,"cloud_required":false,"models_required":false,"database":"per-project SQLite3.51.3","schema_version":1,"storage_version":1},
                "baseline_render":{"runtime":["trusted user-installed ffmpeg","trusted user-installed ffprobe"],"codec":"native mpeg4+aac; no hardware encoder selected","decoder_threads_per_input":2,"maximum_clips":64,"measured_peak_ram_bytes":null,"vram_bytes":null,"cpu_fallback":"the implemented baseline"}},
            "capability_status":{"gpu_encode_decode":"not_benchmarked","native_text":"planned","device_audio":"planned","semantic_vectors":"not_selected","application_database":"planned","fts5":"implemented_metadata_only","cache":"source_content_hashes_only","live_events":"planned"},
            "planned":["studio","authenticated_service","preview","gpu_renderer","resource_scheduler","models","blender","effects","undo_redo","independent_audio_tracks","h264_preset","transcript_fts5","native_typography","audio_device_output","vector_index"]
        })),
        Command::Schema => Ok(serde_json::to_value(schemars::schema_for!(Batch))?),
        Command::Doctor => tools.versions().await,
        Command::Hardware(HardwareCommand::Scan) => Ok(
            json!({"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,
            "logical_cpu_cores":std::thread::available_parallelism()?.get(),"gpu":"unknown","vram_bytes":null,"available_ram_bytes":null,
            "status":"basic_only","scheduler":"not_implemented"}),
        ),
        Command::Project(command) => match command {
            ProjectCommand::Create {
                project,
                name,
                width,
                height,
                fps,
                preset,
            } => {
                let (default_width, default_height) = preset.dimensions();
                let value = Project::new(
                    name,
                    width.unwrap_or(default_width),
                    height.unwrap_or(default_height),
                    fps,
                )?;
                let mut store = Store::create(&project, &value).await?;
                Ok(
                    json!({"project":store.inspect().await?,"sqlite_version":store.sqlite_version().await?}),
                )
            }
            ProjectCommand::Inspect { project } | ProjectCommand::Open { project } => Ok(
                serde_json::to_value(Store::open(&project).await?.inspect().await?)?,
            ),
            ProjectCommand::History { project } => Ok(serde_json::to_value(
                Store::open(&project).await?.history().await?,
            )?),
            ProjectCommand::Check { project, assets } => {
                let mut store = Store::open(&project).await?;
                let mut result = store.integrity_check().await?;
                if assets {
                    let snapshot = store.inspect().await?;
                    let parent = store
                        .path()
                        .parent()
                        .ok_or_else(|| Error::Invalid("project parent unavailable".into()))?;
                    for media in &snapshot.media {
                        let path = parent.join(&media.path);
                        let expected = media.metadata.blake3.as_ref().ok_or_else(|| {
                            Error::Unsupported(format!("media {} has no content digest", media.id))
                        })?;
                        if &lumeryn_core::media::content_hash(&path).await? != expected {
                            return Err(Error::Invalid(format!(
                                "media {} content changed",
                                media.id
                            )));
                        }
                    }
                    result["source_assets"] =
                        json!({"status":"verified","count":snapshot.media.len()});
                }
                Ok(result)
            }
            ProjectCommand::Migrate { project } => Store::open(&project).await?.migrate().await,
            ProjectCommand::Backup { project, output } => {
                Store::open(&project).await?.backup(&output).await
            }
            ProjectCommand::Search {
                project,
                query,
                limit,
            } => Ok(serde_json::to_value(
                Store::open(&project).await?.search(&query, limit).await?,
            )?),
            ProjectCommand::Batch {
                write,
                file,
                dry_run,
            } => {
                if std::fs::metadata(&file)?.len() > 1024 * 1024 {
                    return Err(Error::Invalid("batch file exceeds 1 MiB".into()));
                }
                let batch: Batch = serde_json::from_slice(&std::fs::read(file)?)?;
                edit(write, batch, dry_run, &tools).await
            }
        },
        Command::Media(command) => match command {
            MediaCommand::Probe { path } => Ok(serde_json::to_value(tools.probe(&path).await?)?),
            MediaCommand::Import { write, path, id } => {
                let id = operation_id(id, &write)?;
                edit(
                    write,
                    Batch {
                        operations: vec![Edit::Import { id, path }],
                    },
                    false,
                    &tools,
                )
                .await
            }
        },
        Command::Timeline(command) => match command {
            TimelineCommand::Inspect { project } => {
                let project = Store::open(&project).await?.inspect().await?;
                Ok(
                    json!({"revision":project.revision,"frame_rate":project.frame_rate,"video_track_id":project.video_track_id,"clips":project.clips}),
                )
            }
            TimelineCommand::Clip(ClipCommand::Add {
                write,
                id,
                media_id,
                at_frame,
                source_in_frame,
                duration_frames,
            }) => {
                let id = operation_id(id, &write)?;
                edit(
                    write,
                    Batch {
                        operations: vec![Edit::AddClip {
                            id,
                            media_id,
                            at_frame,
                            source_in_frame,
                            duration_frames,
                        }],
                    },
                    false,
                    &tools,
                )
                .await
            }
            TimelineCommand::Clip(ClipCommand::Trim {
                write,
                clip_id,
                source_in_frame,
                duration_frames,
            }) => {
                edit(
                    write,
                    Batch {
                        operations: vec![Edit::TrimClip {
                            clip_id,
                            source_in_frame,
                            duration_frames,
                        }],
                    },
                    false,
                    &tools,
                )
                .await
            }
        },
        Command::Render {
            project,
            output,
            expected_revision,
        } => {
            let mut store = Store::open(&project).await?;
            let project = store.inspect().await?;
            if let Some(expected) = expected_revision
                && expected != project.revision
            {
                return Err(Error::Conflict {
                    expected,
                    current: project.revision,
                });
            }
            Ok(serde_json::to_value(
                render::export(&project, store.path(), &output, &tools).await?,
            )?)
        }
    }
}
