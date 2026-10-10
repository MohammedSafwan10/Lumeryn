use clap::{Args, Parser, Subcommand, ValueEnum};
use lumeryn_core::render::{
    Acceleration, RenderOptions, RenderQuality, RenderResolution, VideoCodec,
};
use lumeryn_core::{Batch, Edit, Error, MediaTools, Project, Result, time::FrameRate};
use lumeryn_service::{Client, Request, ToolConfig};
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
    /// Run the local authoritative project service (normally launched automatically).
    #[command(subcommand)]
    Service(ServiceCommand),
    /// Control a known background render job.
    #[command(subcommand)]
    Jobs(JobCommand),
    /// JSON Schema generated from shared batch, render or clipboard types.
    Schema {
        #[arg(long, value_enum, default_value = "batch")]
        kind: SchemaKind,
    },
    /// Installed external media tool versions/configuration.
    Doctor,
    #[command(subcommand)]
    Project(ProjectCommand),
    #[command(subcommand)]
    Media(MediaCommand),
    /// Discover and verify the locally installed licensed asset catalog.
    #[command(subcommand)]
    Assets(AssetCommand),
    #[command(subcommand)]
    Timeline(TimelineCommand),
    /// Export and fully decode-verify local MP4 to a new path.
    Render {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        expected_revision: Option<u64>,
        /// Supply an ID to cancel this render from another CLI client.
        #[arg(long)]
        job_id: Option<Uuid>,
        /// Final resolution: project, hd720, fhd1080 or uhd2160; follows project orientation.
        #[arg(long, default_value = "project")]
        resolution: RenderResolution,
        /// Output FPS: 24, 25, 30 or 60; omitted preserves project rate.
        #[arg(long)]
        fps: Option<u32>,
        /// mpeg4, h264 or h265; availability depends on the installed FFmpeg build.
        #[arg(long, default_value = "mpeg4")]
        codec: VideoCodec,
        /// draft, balanced or high.
        #[arg(long, default_value = "high")]
        quality: RenderQuality,
        #[arg(long)]
        bitrate_kbps: Option<u32>,
        /// cpu or auto; auto probes actual encoder initialization and reports CPU fallback.
        #[arg(long, default_value = "cpu")]
        acceleration: Acceleration,
    },
    #[command(subcommand)]
    Hardware(HardwareCommand),
}

#[derive(Debug, Subcommand)]
enum ServiceCommand {
    Run {
        #[arg(long)]
        project: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum JobCommand {
    Cancel {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        job_id: Uuid,
    },
}

#[derive(Debug, Subcommand)]
enum ProjectCommand {
    /// Restore the previous logical state as a new revision; sole-operation transaction.
    Undo {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        dry_run: bool,
    },
    /// Restore the next logical state as a new revision.
    Redo {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        dry_run: bool,
    },
    /// Inspect undo/redo depths from the persisted shared history contract.
    HistoryStatus {
        #[arg(long)]
        project: PathBuf,
    },
    /// Read revision/job notifications; expired cursors include a resynchronization snapshot.
    Watch {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        after_sequence: Option<u64>,
        #[arg(long, default_value = "0", value_parser = clap::value_parser!(u32).range(0..=30000))]
        wait_ms: u32,
    },
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
    /// List project media and their timeline usage without reading source bytes.
    List {
        #[arg(long)]
        project: PathBuf,
    },
    /// Inspect one imported media reference and its project-only display name.
    Inspect {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        media_id: Uuid,
    },
    /// Set a project-only display alias, or reset it to the source filename.
    Rename {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        media_id: Uuid,
        #[arg(long, required_unless_present = "reset", conflicts_with = "reset")]
        name: Option<String>,
        #[arg(long, conflicts_with = "name")]
        reset: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove an unused project reference; the source file remains on disk.
    Remove {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        media_id: Uuid,
        #[arg(long)]
        dry_run: bool,
    },
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

#[derive(Debug, Args)]
struct LibraryLocation {
    #[arg(long)]
    library: Option<PathBuf>,
}
#[derive(Debug, Subcommand)]
enum AssetCommand {
    List {
        #[command(flatten)]
        location: LibraryLocation,
        #[arg(long, default_value = "0")]
        offset: u32,
        #[arg(long, default_value = "128", value_parser = clap::value_parser!(u32).range(1..=128))]
        limit: u32,
    },
    Search {
        #[command(flatten)]
        location: LibraryLocation,
        #[arg(long)]
        query: String,
        #[arg(long, default_value = "20", value_parser = clap::value_parser!(u32).range(1..=128))]
        limit: u32,
    },
    Inspect {
        #[command(flatten)]
        location: LibraryLocation,
        #[arg(long)]
        id: String,
    },
    Check {
        #[command(flatten)]
        location: LibraryLocation,
    },
}

#[derive(Debug, Subcommand)]
enum TimelineCommand {
    /// Independent audio clips, evaluated identically by native preview and final export.
    #[command(subcommand)]
    Audio(AudioCommand),
    /// Set the project canvas; final output fits this aspect without stretching sources.
    Canvas {
        #[command(flatten)]
        write: Write,
        #[arg(long, value_enum, default_value = "landscape")]
        preset: ProjectPreset,
        #[arg(long)]
        width: Option<u32>,
        #[arg(long)]
        height: Option<u32>,
    },
    Inspect {
        #[arg(long)]
        project: PathBuf,
    },
    #[command(subcommand)]
    Clip(ClipCommand),
}

#[derive(Debug, Subcommand)]
enum AudioCommand {
    /// Atomically set timeline position and source range together.
    Range {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        clip_id: Uuid,
        #[arg(long)]
        at_frame: u64,
        #[arg(long)]
        source_in_sample: u64,
        #[arg(long)]
        duration_frames: u64,
    },
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
        source_in_sample: u64,
        #[arg(long)]
        duration_frames: u64,
        #[arg(long, default_value = "0", allow_negative_numbers = true)]
        gain_millidecibels: i32,
        #[arg(long)]
        muted: bool,
    },
    Move {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        clip_id: Uuid,
        #[arg(long)]
        at_frame: u64,
    },
    Trim {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        clip_id: Uuid,
        #[arg(long)]
        source_in_sample: u64,
        #[arg(long)]
        duration_frames: u64,
    },
    Properties {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        clip_id: Uuid,
        #[arg(long, default_value = "0", allow_negative_numbers = true)]
        gain_millidecibels: i32,
        #[arg(long)]
        muted: bool,
    },
    Delete {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        clip_id: Uuid,
    },
}

#[derive(Debug, Subcommand)]
enum ClipCommand {
    /// Insert source media at a clip boundary and ripple the contiguous sequence.
    Insert {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        id: Option<Uuid>,
        #[arg(long)]
        media_id: Uuid,
        #[arg(long)]
        before_clip_id: Option<Uuid>,
        #[arg(long, default_value = "0")]
        source_in_frame: u64,
        #[arg(long)]
        duration_frames: u64,
    },
    /// Copy one exact clip into a versioned same-project payload (result.clipboard).
    Copy {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        clip_id: Uuid,
    },
    /// Paste a bounded clipboard payload object using already-imported source media.
    Paste {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        id: Option<Uuid>,
        #[arg(long)]
        at_frame: u64,
        #[arg(long)]
        dry_run: bool,
    },
    Split {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        clip_id: Uuid,
        #[arg(long)]
        right_clip_id: Option<Uuid>,
        #[arg(long)]
        at_frame: u64,
    },
    Duplicate {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        clip_id: Uuid,
        #[arg(long)]
        id: Option<Uuid>,
    },
    Delete {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        clip_id: Uuid,
        /// Preserve later positions (creates a gap; baseline export then fails explicitly).
        #[arg(long)]
        leave_gap: bool,
    },
    Reorder {
        #[command(flatten)]
        write: Write,
        #[arg(long)]
        clip_id: Uuid,
        /// Omit to append to the contiguous sequence.
        #[arg(long)]
        before_clip_id: Option<Uuid>,
    },
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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SchemaKind {
    Batch,
    Render,
    Clipboard,
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

#[tokio::main(worker_threads = 2)]
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
            Ok(()) => { eprintln!("{}", json!({"ok":false,"error":{"code":"cancelled","message":"CLI request cancelled; inspect revision before retry. An admitted render continues in the service; use jobs cancel with its supplied --job-id to stop it."}})); return ExitCode::from(130); },
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

fn tool_config(tools: &MediaTools) -> ToolConfig {
    ToolConfig {
        ffmpeg: tools.ffmpeg.clone(),
        ffprobe: tools.ffprobe.clone(),
        timeout_seconds: tools.timeout.as_secs(),
    }
}

async fn edit(write: Write, batch: Batch, dry_run: bool, tools: &MediaTools) -> Result<Value> {
    Client::connect(&write.project)
        .await?
        .execute(
            batch,
            write.expected_revision,
            write.idempotency_key,
            dry_run,
            tool_config(tools),
        )
        .await
}

fn media_view(media: &lumeryn_core::model::Media, usage_count: usize) -> Result<Value> {
    let mut value = serde_json::to_value(media)?;
    value["alias"] = json!(media.alias);
    value["display_name"] = json!(media.display_name());
    value["usage_count"] = json!(usage_count);
    Ok(value)
}

async fn project_call(project: PathBuf, request: Request) -> Result<Value> {
    Client::connect(project).await?.call(request).await
}
async fn execute(cli: Cli) -> Result<Value> {
    let tools = MediaTools {
        ffmpeg: cli.ffmpeg,
        ffprobe: cli.ffprobe,
        timeout: Duration::from_secs(cli.timeout_seconds),
    };
    match cli.command {
        Command::Assets(command) => {
            use lumeryn_core::library::{Catalog, default_root};
            let location = match &command {
                AssetCommand::List { location, .. }
                | AssetCommand::Search { location, .. }
                | AssetCommand::Inspect { location, .. }
                | AssetCommand::Check { location } => location,
            };
            let root = match &location.library {
                Some(path) => path.clone(),
                None => default_root()?,
            };
            let catalog = Catalog::load(root)?;
            match command {
                AssetCommand::List { limit, offset, .. } => {
                    let total = catalog.entries().len();
                    let offset = offset as usize;
                    if offset > total {
                        return Err(Error::Invalid(format!(
                            "asset offset exceeds catalog size {total}"
                        )));
                    }
                    let end = offset.saturating_add(limit as usize).min(total);
                    Ok(
                        json!({"assets":&catalog.entries()[offset..end],"total":total,"offset":offset,"limit":limit,"next_offset":(end < total).then_some(end)}),
                    )
                }
                AssetCommand::Search { query, limit, .. } => {
                    Ok(json!({"assets":catalog.search(&query, limit as usize)?}))
                }
                AssetCommand::Inspect { id, .. } => Ok(
                    json!({"asset":catalog.get(&id)?,"verified_path":catalog.verified_path(&id)?}),
                ),
                AssetCommand::Check { .. } => Ok(serde_json::to_value(catalog.check()?)?),
            }
        }
        Command::Capabilities => Ok(json!({
            "api_version":1,"project_schema_version":lumeryn_core::model::SCHEMA_VERSION,"output":"json",
            "implemented":["project.create","project.open","project.inspect","project.history","project.history_status","project.undo","project.redo","project.batch","project.check","project.migrate","project.backup","project.search.metadata_fts5","project.watch","jobs.cancel","service.authoritative_windows_ipc","media.probe","media.import","media.list","media.inspect","media.rename","media.remove","assets.list","assets.search","assets.inspect","assets.check","timeline.inspect","timeline.canvas","timeline.clip.add","timeline.clip.insert","timeline.audio.add","timeline.audio.move","timeline.audio.trim","timeline.audio.range","timeline.audio.properties","timeline.audio.delete","timeline.clip.trim","timeline.clip.split","timeline.clip.duplicate","timeline.clip.delete","timeline.clip.reorder","timeline.clip.copy","timeline.clip.paste","render.baseline_mpeg4_aac","render.local_uhd_presets","render.h264_h265_installed_encoders","render.auto_encoder_probe_cpu_fallback","hardware.scan.basic","schema","doctor"],
            "media_management":{"alias":"project-only; null resets source filename display","alias_maximum_utf8_bytes":256,"remove":"unused project references only; rejects referenced media; source bytes unchanged","undo_redo":true,"dry_run":true},
            "history":{"undo_redo":"new monotonic revisions; logical history restored transactionally","undo_redo_batch":"sole operation only","branch_edit_clears_redo":true},
            "clipboard":{"version":1,"maximum_payload_bytes":16384,"scope":"same project and already-imported BLAKE3 source identity","payload":"result.clipboard object","cross_project_transfer":"staged","paste":"authoritative typed PasteClip with new stable ID and exact source ranges"},
            "requires_external_tools":["media.probe","media.import","render"],
            "export_limits":{"track":"one video track","layout":"1..64 contiguous nonoverlapping clips starting at frame 0","source":"matching CFR; frame timestamps verified","audio":"embedded audio plus independent mono/stereo zero-origin sources; canonical stereo 48000Hz; max64 contributions","color":"baseline SDR; no managed HDR"},
            "export_options":{"resolution":["project","hd720","fhd1080","uhd2160"],"uhd_landscape":[3840,2160],"uhd_portrait":[2160,3840],"fps":[24,25,30,60],"preserve_project_fps":true,"codecs":["mpeg4","h264","h265"],"codec_availability":"installed encoder and actual requested-size initialization","quality":["draft","balanced","high"],"bitrate_kbps_range":[100,200000],"acceleration":["cpu","auto"],"watermark":false,"subscription_required":false,"preview_settings_affect_final":false,"eight_k_hdr_mastering":"staged"},
            "requirements":{"ordinary_editing":{"gpu_required":false,"cloud_required":false,"models_required":false,"database":"per-project SQLite3.51.3","schema_version":lumeryn_core::model::SCHEMA_VERSION,"storage_version":1},
                "baseline_render":{"runtime":["trusted user-installed ffmpeg","trusted user-installed ffprobe"],"codec":"native mpeg4+aac; no hardware encoder selected","decoder_threads_per_input":2,"maximum_clips":64,"measured_peak_ram_bytes":null,"vram_bytes":null,"cpu_fallback":"the implemented baseline"}},
            "capability_status":{"studio":"native_baseline_prototype","preview":"cpu_decode_gpu_upload_prototype","gpu_encode_decode":"auto_encoding_probe_available_decode_not_integrated","native_text":"planned","device_audio":"cpal_prototype_physical_sync_unmeasured","semantic_vectors":"not_selected","application_database":"planned","fts5":"implemented_metadata_only","cache":"source_content_hashes_only","live_events":"implemented_bounded_replay_snapshot_resync"},
            "components":{"project_service":{"platform":"Windows","transport":"current-user ACL named pipe; remote clients rejected","supported_here":cfg!(windows),"cloud_required":false},"studio":{"executable":"lumeryn-studio","separate_install":true,"stack":"egui/eframe/egui_dock/egui-wgpu/wgpu","preview_gpu_required":true,"hardware_zero_copy":false,"physical_av_sync_verified":false}},
            "job_control":{"render_accepts_job_id":true,"client_disconnect_cancels_render":false,"cancel_command":"jobs cancel --project <path> --job-id <uuid>","render_concurrency":1,"render_queue_limit":16},
            "planned":["gpu_compositor","hardware_decode_integration","resource_scheduler","models","blender","effects","audio_detach_fades_multitrack","transcript_fts5","native_typography","professional_audio_mixing","proxies","incremental_render_cache","vector_index"]
        })),
        Command::Service(ServiceCommand::Run { project }) => {
            lumeryn_service::serve(project).await?;
            Ok(json!({"service":"stopped"}))
        }
        Command::Jobs(JobCommand::Cancel { project, job_id }) => {
            project_call(project, Request::Cancel { job_id }).await
        }
        Command::Schema { kind } => Ok(match kind {
            SchemaKind::Batch => serde_json::to_value(schemars::schema_for!(Batch))?,
            SchemaKind::Render => serde_json::to_value(schemars::schema_for!(RenderOptions))?,
            SchemaKind::Clipboard => {
                serde_json::to_value(schemars::schema_for!(lumeryn_core::clipboard::Clipboard))?
            }
        }),
        Command::Doctor => tools.versions().await,
        Command::Hardware(HardwareCommand::Scan) => Ok(
            json!({"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,
            "logical_cpu_cores":std::thread::available_parallelism()?.get(),"gpu":"unknown","vram_bytes":null,"available_ram_bytes":null,
            "status":"basic_only","scheduler":"not_implemented"}),
        ),
        Command::Project(command) => match command {
            ProjectCommand::Undo { write, dry_run } => {
                edit(
                    write,
                    Batch {
                        operations: vec![Edit::Undo {}],
                    },
                    dry_run,
                    &tools,
                )
                .await
            }
            ProjectCommand::Redo { write, dry_run } => {
                edit(
                    write,
                    Batch {
                        operations: vec![Edit::Redo {}],
                    },
                    dry_run,
                    &tools,
                )
                .await
            }
            ProjectCommand::HistoryStatus { project } => {
                let history = Client::connect(project).await?.history().await?;
                Ok(serde_json::to_value(lumeryn_core::storage::history_state(
                    &history,
                )?)?)
            }
            ProjectCommand::Watch {
                project,
                after_sequence,
                wait_ms,
            } => {
                project_call(
                    project,
                    Request::Watch {
                        after_sequence,
                        wait_ms,
                    },
                )
                .await
            }
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
                project_call(project, Request::Create { project: value }).await
            }
            ProjectCommand::Inspect { project } | ProjectCommand::Open { project } => {
                project_call(project, Request::Inspect).await
            }
            ProjectCommand::History { project } => project_call(project, Request::History).await,
            ProjectCommand::Check { project, assets } => {
                project_call(project, Request::Check { assets }).await
            }
            ProjectCommand::Migrate { project } => project_call(project, Request::Migrate).await,
            ProjectCommand::Backup { project, output } => {
                project_call(project, Request::Backup { output }).await
            }
            ProjectCommand::Search {
                project,
                query,
                limit,
            } => project_call(project, Request::Search { query, limit }).await,
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
            MediaCommand::List { project } => {
                let snapshot = Client::connect(project).await?.inspect().await?;
                let usage = snapshot
                    .clips
                    .iter()
                    .map(|clip| clip.media_id)
                    .chain(snapshot.audio_clips.iter().map(|clip| clip.media_id))
                    .fold(
                        std::collections::HashMap::<_, usize>::new(),
                        |mut usage, media_id| {
                            *usage.entry(media_id).or_default() += 1;
                            usage
                        },
                    );
                let media = snapshot
                    .media
                    .iter()
                    .map(|media| media_view(media, usage.get(&media.id).copied().unwrap_or(0)))
                    .collect::<Result<Vec<_>>>()?;
                Ok(json!({"revision":snapshot.revision,"media":media}))
            }
            MediaCommand::Inspect { project, media_id } => {
                let snapshot = Client::connect(project).await?.inspect().await?;
                let media = snapshot
                    .media
                    .iter()
                    .find(|media| media.id == media_id)
                    .ok_or_else(|| Error::NotFound(format!("media {media_id}")))?;
                let usage_count = snapshot
                    .clips
                    .iter()
                    .filter(|clip| clip.media_id == media_id)
                    .count()
                    + snapshot
                        .audio_clips
                        .iter()
                        .filter(|clip| clip.media_id == media_id)
                        .count();
                Ok(json!({"revision":snapshot.revision,"media":media_view(media, usage_count)?}))
            }
            MediaCommand::Rename {
                write,
                media_id,
                name,
                reset,
                dry_run,
            } => {
                edit(
                    write,
                    Batch {
                        operations: vec![Edit::RenameMedia {
                            media_id,
                            alias: if reset { None } else { name },
                        }],
                    },
                    dry_run,
                    &tools,
                )
                .await
            }
            MediaCommand::Remove {
                write,
                media_id,
                dry_run,
            } => {
                edit(
                    write,
                    Batch {
                        operations: vec![Edit::RemoveMedia { media_id }],
                    },
                    dry_run,
                    &tools,
                )
                .await
            }
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
            TimelineCommand::Audio(command) => {
                let (write, operation) = match command {
                    AudioCommand::Add {
                        write,
                        id,
                        media_id,
                        at_frame,
                        source_in_sample,
                        duration_frames,
                        gain_millidecibels,
                        muted,
                    } => {
                        let id = operation_id(id, &write)?;
                        (
                            write,
                            Edit::AddAudioClip {
                                id,
                                media_id,
                                at_frame,
                                source_in_sample,
                                duration_frames,
                                gain_millidecibels,
                                muted,
                            },
                        )
                    }
                    AudioCommand::Move {
                        write,
                        clip_id,
                        at_frame,
                    } => (write, Edit::MoveAudioClip { clip_id, at_frame }),
                    AudioCommand::Trim {
                        write,
                        clip_id,
                        source_in_sample,
                        duration_frames,
                    } => (
                        write,
                        Edit::TrimAudioClip {
                            clip_id,
                            source_in_sample,
                            duration_frames,
                        },
                    ),
                    AudioCommand::Properties {
                        write,
                        clip_id,
                        gain_millidecibels,
                        muted,
                    } => (
                        write,
                        Edit::SetAudioProperties {
                            clip_id,
                            gain_millidecibels,
                            muted,
                        },
                    ),
                    AudioCommand::Delete { write, clip_id } => {
                        (write, Edit::DeleteAudioClip { clip_id })
                    }
                    AudioCommand::Range {
                        write,
                        clip_id,
                        at_frame,
                        source_in_sample,
                        duration_frames,
                    } => (
                        write,
                        Edit::SetAudioRange {
                            clip_id,
                            at_frame,
                            source_in_sample,
                            duration_frames,
                        },
                    ),
                };
                edit(
                    write,
                    Batch {
                        operations: vec![operation],
                    },
                    false,
                    &tools,
                )
                .await
            }
            TimelineCommand::Clip(ClipCommand::Copy { project, clip_id }) => {
                let snapshot = Client::connect(project).await?.inspect().await?;
                Ok(
                    json!({"clipboard":lumeryn_core::clipboard::Clipboard::copy(&snapshot, clip_id)?}),
                )
            }
            TimelineCommand::Clip(ClipCommand::Paste {
                mut write,
                file,
                id,
                at_frame,
                dry_run,
            }) => {
                use lumeryn_core::clipboard::{Clipboard, MAX_BYTES};
                use std::io::Read;
                let id = operation_id(id, &write)?;
                let mut bytes = Vec::new();
                std::fs::File::open(file)?
                    .take(MAX_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)?;
                let clipboard = Clipboard::from_slice(&bytes)?;
                if write.expected_revision.is_none() {
                    write.expected_revision = Some(
                        Client::connect(&write.project)
                            .await?
                            .inspect()
                            .await?
                            .revision,
                    );
                }
                edit(
                    write,
                    Batch {
                        operations: vec![Edit::PasteClip {
                            id,
                            at_frame,
                            clipboard,
                        }],
                    },
                    dry_run,
                    &tools,
                )
                .await
            }
            TimelineCommand::Canvas {
                write,
                preset,
                width,
                height,
            } => {
                let (width, height) = match (width, height) {
                    (Some(width), Some(height)) => (width, height),
                    (None, None) => preset.dimensions(),
                    _ => return Err(Error::Invalid("supply both --width and --height".into())),
                };
                edit(
                    write,
                    Batch {
                        operations: vec![Edit::SetCanvas { width, height }],
                    },
                    false,
                    &tools,
                )
                .await
            }
            TimelineCommand::Clip(ClipCommand::Split {
                write,
                clip_id,
                right_clip_id,
                at_frame,
            }) => {
                let right_clip_id = operation_id(right_clip_id, &write)?;
                edit(
                    write,
                    Batch {
                        operations: vec![Edit::SplitClip {
                            clip_id,
                            right_clip_id,
                            at_frame,
                        }],
                    },
                    false,
                    &tools,
                )
                .await
            }
            TimelineCommand::Clip(ClipCommand::Duplicate { write, clip_id, id }) => {
                let id = operation_id(id, &write)?;
                edit(
                    write,
                    Batch {
                        operations: vec![Edit::DuplicateClip { clip_id, id }],
                    },
                    false,
                    &tools,
                )
                .await
            }
            TimelineCommand::Clip(ClipCommand::Delete {
                write,
                clip_id,
                leave_gap,
            }) => {
                edit(
                    write,
                    Batch {
                        operations: vec![Edit::DeleteClip {
                            clip_id,
                            ripple: !leave_gap,
                        }],
                    },
                    false,
                    &tools,
                )
                .await
            }
            TimelineCommand::Clip(ClipCommand::Reorder {
                write,
                clip_id,
                before_clip_id,
            }) => {
                edit(
                    write,
                    Batch {
                        operations: vec![Edit::ReorderClip {
                            clip_id,
                            before_clip_id,
                        }],
                    },
                    false,
                    &tools,
                )
                .await
            }
            TimelineCommand::Inspect { project } => {
                let project = Client::connect(project).await?.inspect().await?;
                Ok(
                    json!({"revision":project.revision,"frame_rate":project.frame_rate,"video_track_id":project.video_track_id,"clips":project.clips,"audio_track_id":project.audio_track_id,"audio_clips":project.audio_clips}),
                )
            }
            TimelineCommand::Clip(ClipCommand::Insert {
                write,
                id,
                media_id,
                before_clip_id,
                source_in_frame,
                duration_frames,
            }) => {
                let id = operation_id(id, &write)?;
                edit(
                    write,
                    Batch {
                        operations: vec![Edit::InsertClip {
                            id,
                            media_id,
                            before_clip_id,
                            source_in_frame,
                            duration_frames,
                        }],
                    },
                    false,
                    &tools,
                )
                .await
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
            job_id,
            resolution,
            fps,
            codec,
            quality,
            bitrate_kbps,
            acceleration,
        } => {
            let options = RenderOptions {
                resolution,
                fps,
                codec,
                quality,
                bitrate_kbps,
                acceleration,
            };
            options.validate()?;
            project_call(
                project,
                Request::Render {
                    output,
                    expected_revision,
                    tools: tool_config(&tools),
                    job_id: job_id.unwrap_or_else(Uuid::new_v4),
                    options,
                },
            )
            .await
        }
    }
}
