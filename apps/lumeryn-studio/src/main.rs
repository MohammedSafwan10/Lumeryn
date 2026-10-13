// Studio is a GUI in debug builds too; the separate lumeryn binary remains a console CLI.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod assets;
mod backend;
mod capture;
mod composition_timeline;
mod design;
mod gpu;
mod library;
mod smoke;
mod timeline;

use clap::Parser;
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Debug, Clone, Copy, clap::ValueEnum, PartialEq, Eq)]
pub enum CaptureState {
    Active,
    Media,
    Export,
    Inspector,
    Visual,
    Scenes,
    Audio,
    Project,
    Importing,
    Tools,
    Empty,
    Library,
    LibraryAudio,
    Fullscreen,
    Collapsed,
}

#[derive(Debug, Parser)]
#[command(version, about = "Lumeryn native project review and editing Studio")]
pub struct Options {
    #[arg(skip)]
    launch_started: Option<std::time::Instant>,
    #[arg(long)]
    project: Option<PathBuf>,
    /// Write bounded native startup diagnostics to a new local file; never overwrite one.
    #[arg(long)]
    launch_log: Option<PathBuf>,
    /// Optional curated offline library root (catalog.json and asset files).
    #[arg(long)]
    library: Option<PathBuf>,
    /// Scripted verification modifies the supplied test project; use a disposable fixture.
    #[arg(long, requires_all = ["project", "artifact_dir"])]
    smoke: bool,
    /// Exercise Studio's real UHD/H.264/60 FPS/automatic encoder settings on a disposable project.
    #[arg(long, requires = "smoke")]
    smoke_uhd: bool,
    /// Exercise shared undo/redo and clipboard paths on the disposable smoke project.
    #[arg(long, requires = "smoke")]
    smoke_history: bool,
    /// Verify native independent audio placement, editing and mixed export on disposable media.
    #[arg(long, requires = "smoke")]
    smoke_audio: bool,
    /// Verify native layer/image placement and editing on a disposable project.
    #[arg(long, requires = "smoke", conflicts_with = "smoke_audio")]
    smoke_composition: bool,
    /// Capture an actual native UI state without editing the project.
    #[arg(long, requires = "artifact_dir", conflicts_with = "smoke")]
    capture_only: bool,
    #[arg(long, value_enum, default_value = "active")]
    capture_state: CaptureState,
    /// Initial review position in project frames; clamped to the committed timeline.
    #[arg(long = "start-frame", alias = "capture-frame", default_value_t = 0)]
    capture_frame: u64,
    #[arg(long)]
    artifact_dir: Option<PathBuf>,
    #[arg(long, default_value_t = 1600, value_parser = clap::value_parser!(u32).range(980..=4096))]
    width: u32,
    #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u32).range(640..=2160))]
    height: u32,
    #[arg(long, default_value = "ffmpeg", env = "LUMERYN_FFMPEG")]
    ffmpeg: PathBuf,
    #[arg(long, default_value = "ffprobe", env = "LUMERYN_FFPROBE")]
    ffprobe: PathBuf,
}

fn main() -> ExitCode {
    let launch_started = std::time::Instant::now();
    let mut options = match Options::try_parse() {
        Ok(options) => options,
        Err(error) => {
            let failed = error.use_stderr();
            if let Err(print_error) = error.print() {
                eprintln!("Studio argument output failed: {print_error}");
            }
            if failed {
                // Malformed automated requests must still return an exit code, never block on a dialog.
                let automated =
                    std::env::args_os().any(|arg| arg == "--smoke" || arg == "--capture-only");
                show_startup_error(&error.to_string(), !automated);
            }
            return if failed {
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            };
        }
    };
    let interactive = !(options.smoke || options.capture_only);
    let mut startup_log = match options
        .launch_log
        .as_deref()
        .map(StartupLog::create)
        .transpose()
    {
        Ok(log) => log,
        Err(error) => {
            return startup_failure(
                &format!("Create native startup diagnostic file: {error}"),
                interactive,
                None,
            );
        }
    };
    options.launch_started = Some(launch_started);
    if (options.smoke || options.capture_only)
        && let Some(directory) = &options.artifact_dir
    {
        if [
            "studio-smoke.json",
            "studio-native.png",
            "studio-smoke-export.mp4",
            "studio-capture.json",
            "screenshot-capture.png",
        ]
        .iter()
        .any(|file| directory.join(file).exists())
        {
            return startup_failure(
                "Smoke artifacts must be new; choose a fresh artifact directory",
                interactive,
                startup_log.as_mut(),
            );
        }
        if let Err(error) = std::fs::create_dir_all(directory) {
            return startup_failure(
                &format!("Create smoke artifact directory: {error}"),
                interactive,
                startup_log.as_mut(),
            );
        }
    }
    let report = options
        .smoke
        .then(|| {
            options
                .artifact_dir
                .as_ref()
                .map(|p| p.join("studio-smoke.json"))
        })
        .flatten();
    let capture_report = options
        .capture_only
        .then(|| {
            options
                .artifact_dir
                .as_ref()
                .map(|p| p.join("studio-capture.json"))
        })
        .flatten();
    let native = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        persist_window: !(options.smoke || options.capture_only),
        persistence_path: (options.smoke || options.capture_only)
            .then(|| {
                options
                    .artifact_dir
                    .as_ref()
                    .map(|p| p.join("workspace.ron"))
            })
            .flatten(),
        viewport: egui::ViewportBuilder::default()
            .with_title("Lumeryn Studio")
            .with_inner_size([options.width as f32, options.height as f32])
            .with_min_inner_size([980.0, 640.0]),
        ..Default::default()
    };
    if let Err(error) = eframe::run_native(
        "Lumeryn Studio",
        native,
        Box::new(move |cc| Ok(Box::new(app::Studio::new(cc, options)))),
    ) {
        return startup_failure(
            &format!("Native Studio failed: {error}"),
            interactive,
            startup_log.as_mut(),
        );
    }
    if let Some(path) = report {
        let passed = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|report| report.get("passed").and_then(|v| v.as_bool()));
        if passed != Some(true) {
            eprintln!("Studio smoke did not produce a passing report");
            return ExitCode::FAILURE;
        }
    }
    if let Some(path) = capture_report {
        let passed = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .is_some_and(|report| report.get("passed").and_then(|v| v.as_bool()) == Some(true));
        if !passed {
            eprintln!("Native capture did not produce a passing report");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

/// This explicit sink has no global subscriber, argument dump, project data or inherited pipe.
struct StartupLog(File);

impl StartupLog {
    fn create(path: &Path) -> io::Result<Self> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(b"{\"event\":\"startup_requested\"}\n")?;
        file.sync_data()?;
        Ok(Self(file))
    }

    fn failure(&mut self, message: &str) -> io::Result<()> {
        // Driver errors can be large. Keep this diagnostic bounded on Unicode boundaries.
        let bounded: String = message.chars().take(4096).collect();
        let record = serde_json::json!({"event": "startup_failed", "message": bounded});
        writeln!(self.0, "{record}")?;
        self.0.sync_data()
    }
}

fn startup_failure(
    message: &str,
    interactive: bool,
    startup_log: Option<&mut StartupLog>,
) -> ExitCode {
    eprintln!("{message}");
    if let Some(log) = startup_log
        && let Err(error) = log.failure(message)
    {
        eprintln!("Write native startup diagnostic: {error}");
    }
    show_startup_error(message, interactive);
    ExitCode::FAILURE
}

fn show_startup_error(message: &str, interactive: bool) {
    #[cfg(windows)]
    if interactive {
        rfd::MessageDialog::new()
            .set_title("Lumeryn Studio could not start")
            .set_description(message)
            .set_level(rfd::MessageLevel::Error)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
    #[cfg(not(windows))]
    let _ = (message, interactive);
}

#[cfg(test)]
mod launch_tests {
    use super::*;

    #[test]
    fn startup_log_is_create_new_and_bounded_without_arguments() {
        let path =
            std::env::temp_dir().join(format!("lumeryn-launch-{}.log", uuid::Uuid::new_v4()));
        let mut log = StartupLog::create(&path).expect("create isolated startup log");
        let original = std::fs::read(&path).expect("startup log exists");
        assert!(matches!(
            StartupLog::create(&path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists
        ));
        assert_eq!(std::fs::read(&path).expect("read preserved log"), original);
        log.failure(&"é".repeat(5000))
            .expect("write bounded Unicode error");
        drop(log);
        let bytes = std::fs::read(&path).expect("read startup diagnostic");
        assert!(bytes.len() < 20_000);
        let records: Vec<serde_json::Value> = std::str::from_utf8(&bytes)
            .expect("valid UTF-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("valid JSON line"))
            .collect();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0], serde_json::json!({"event":"startup_requested"}));
        assert_eq!(records[1]["event"], "startup_failed");
        assert_eq!(
            records[1]["message"]
                .as_str()
                .expect("error text")
                .chars()
                .count(),
            4096
        );
        std::fs::remove_file(path).expect("remove this test's isolated log");
    }
}
