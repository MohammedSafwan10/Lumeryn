mod app;
mod backend;
mod design;
mod gpu;
mod smoke;
mod timeline;

use clap::Parser;
use std::{path::PathBuf, process::ExitCode};

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
    #[arg(long)]
    artifact_dir: Option<PathBuf>,
    #[arg(long, default_value = "ffmpeg", env = "LUMERYN_FFMPEG")]
    ffmpeg: PathBuf,
    #[arg(long, default_value = "ffprobe", env = "LUMERYN_FFPROBE")]
    ffprobe: PathBuf,
}

fn main() -> ExitCode {
    let launch_started = std::time::Instant::now();
    let mut options = Options::parse();
    options.launch_started = Some(launch_started);
    if options.smoke
        && let Some(directory) = &options.artifact_dir
    {
        if [
            "studio-smoke.json",
            "studio-native.png",
            "studio-smoke-export.mp4",
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
    let native = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        persist_window: !options.smoke,
        persistence_path: options
            .smoke
            .then(|| {
                options
                    .artifact_dir
                    .as_ref()
                    .map(|p| p.join("workspace.ron"))
            })
            .flatten(),
        viewport: egui::ViewportBuilder::default()
            .with_title("Lumeryn Studio")
            .with_inner_size([1440.0, 900.0])
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
    ExitCode::SUCCESS
}
