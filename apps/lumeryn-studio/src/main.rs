mod app;
mod assets;
mod backend;
mod capture;
mod design;
mod gpu;
mod smoke;
mod timeline;

use clap::Parser;
use std::{path::PathBuf, process::ExitCode};

#[derive(Debug, Clone, Copy, clap::ValueEnum, PartialEq, Eq)]
pub enum CaptureState {
    Active,
    Export,
    Inspector,
    Tools,
    Empty,
}

#[derive(Debug, Parser)]
#[command(version, about = "Lumeryn native project review and editing Studio")]
pub struct Options {
    #[arg(skip)]
    launch_started: Option<std::time::Instant>,
    #[arg(long)]
    project: Option<PathBuf>,
    /// Scripted verification modifies the supplied test project; use a disposable fixture.
    #[arg(long, requires_all = ["project", "artifact_dir"])]
    smoke: bool,
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
    let mut options = Options::parse();
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
            eprintln!("Smoke artifacts must be new; choose a fresh artifact directory");
            return ExitCode::FAILURE;
        }
        if let Err(error) = std::fs::create_dir_all(directory) {
            eprintln!("Create smoke artifact directory: {error}");
            return ExitCode::FAILURE;
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
        eprintln!("Native Studio failed: {error}");
        return ExitCode::FAILURE;
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
