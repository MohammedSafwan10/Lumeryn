use crate::{PreviewFrame, plan::Plan};
use lumeryn_core::{
    Error, MediaTools, Result,
    media::{async_worker_command, content_hash},
};
use std::{ffi::OsString, process::Stdio, sync::Arc, time::Instant};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Child,
    sync::{mpsc, watch},
    task::JoinHandle,
};

pub(crate) struct WorkerPipe {
    child: Child,
    pub stdout: tokio::process::ChildStdout,
    stderr: JoinHandle<Result<Vec<u8>>>,
}

impl WorkerPipe {
    pub fn start(tools: &MediaTools, args: &[OsString]) -> Result<Self> {
        let mut child = async_worker_command(&tools.ffmpeg)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Media("preview stdout missing".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Media("preview stderr missing".into()))?;
        Ok(Self {
            child,
            stdout,
            stderr: tokio::spawn(read_diagnostics(stderr)),
        })
    }

    pub async fn finish(mut self) -> Result<()> {
        let status = self.child.wait().await?;
        let stderr = (&mut self.stderr)
            .await
            .map_err(|e| Error::Media(e.to_string()))??;
        if !status.success() {
            return Err(Error::Media(String::from_utf8_lossy(&stderr).into_owned()));
        }
        Ok(())
    }
}

impl Drop for WorkerPipe {
    fn drop(&mut self) {
        self.stderr.abort();
        // Tokio kill_on_drop terminates the actual decoder; no worker is detached.
    }
}

async fn read_diagnostics(mut input: impl AsyncRead + Unpin) -> Result<Vec<u8>> {
    let mut retained = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = input.read(&mut buffer).await?;
        if count == 0 {
            return Ok(retained);
        }
        if retained.len() < 64 * 1024 {
            let keep = count.min(64 * 1024 - retained.len());
            retained.extend_from_slice(&buffer[..keep]);
        }
    }
}

pub(crate) fn base_args() -> Vec<OsString> {
    [
        "-hide_banner",
        "-v",
        "error",
        "-nostdin",
        "-filter_threads",
        "2",
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

pub(crate) async fn video(
    plan: Plan,
    start: u64,
    generation: u64,
    tools: MediaTools,
    output: mpsc::Sender<Result<PreviewFrame>>,
    mut cancel: watch::Receiver<u64>,
) {
    let result = async {
        if let Some(visual) = &plan.visual_plan {
            visual.verify_sources().await?;
            let program = lumeryn_core::visual_backend::lower(visual, plan.width, plan.height)?;
            let scene_resources = program.resources;
            let mut args = base_args();
            args.extend(["-filter_complex_threads".into(), "2".into()]);
            args.extend(program.input_args);
            args.extend(["-filter_complex".into(), format!("{};[{}]trim=start_frame={start}:end_frame={},setpts=PTS-STARTPTS,format=rgba[lumeryn_preview_rgba]", program.filters, program.output_label, plan.total).into(), "-map".into(), "[lumeryn_preview_rgba]".into(), "-an".into(), "-threads".into(), "2".into(), "-fps_mode".into(), "passthrough".into(), "-f".into(), "rawvideo".into(), "pipe:1".into()]);
            // One bounded composite queue/process for this generation. Seeking trims
            // the reference output from the origin; measured fast seeking is staged.
            let mut pipe = WorkerPipe::start(&tools, &args)?;
            for frame_index in start..plan.total {
                let started = Instant::now();
                let mut rgba = vec![0_u8; plan.width as usize * plan.height as usize * 4];
                tokio::time::timeout(tools.timeout, pipe.stdout.read_exact(&mut rgba))
                    .await
                    .map_err(|_| Error::Timeout)??;
                output
                    .send(Ok(PreviewFrame {
                        rgba: Arc::from(rgba),
                        width: plan.width,
                        height: plan.height,
                        frame_index,
                        generation,
                        project_revision: plan.revision,
                        pts_numerator: frame_index * u64::from(plan.rate.denominator()),
                        pts_denominator: plan.rate.numerator(),
                        decode_time_ms: started.elapsed().as_secs_f64() * 1000.0,
                    }))
                    .await
                    .map_err(|_| Error::Media("preview consumer closed".into()))?;
            }
            tokio::time::timeout(tools.timeout, pipe.finish())
                .await
                .map_err(|_| Error::Timeout)??;
            visual.verify_sources().await?;
            drop(scene_resources);
            return Ok::<_, Error>(());
        }
        for segment in plan.segments.iter().filter(|s| s.at + s.frames > start) {
            let offset = start.saturating_sub(segment.at);
            if content_hash(&segment.path).await? != segment.digest {
                return Err(Error::Invalid(
                    "preview source changed; reimport media".into(),
                ));
            }
            let mut args = base_args();
            args.extend(["-threads".into(), "2".into(), "-protocol_whitelist".into(), "file".into(),
                "-i".into(), segment.path.as_os_str().to_owned(), "-map".into(), "0:v:0".into(), "-an".into(),
                "-vf".into(), format!("trim=start_frame={}:end_frame={},setpts=PTS-STARTPTS,scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,setsar=1,format=rgba",
                    segment.source_in + offset, segment.source_in + segment.frames, plan.width, plan.height, plan.width, plan.height).into(),
                "-threads".into(), "2".into(), "-fps_mode".into(), "passthrough".into(), "-f".into(), "rawvideo".into(), "pipe:1".into()]);
            let mut pipe = WorkerPipe::start(&tools, &args)?;
            for local in offset..segment.frames {
                let started = Instant::now();
                let mut rgba = vec![0_u8; plan.width as usize * plan.height as usize * 4];
                tokio::time::timeout(tools.timeout, pipe.stdout.read_exact(&mut rgba))
                    .await
                    .map_err(|_| Error::Timeout)??;
                let frame_index = segment.at + local;
                let frame = PreviewFrame {
                    rgba: Arc::from(rgba),
                    width: plan.width,
                    height: plan.height,
                    frame_index,
                    generation,
                    project_revision: plan.revision,
                    pts_numerator: frame_index * u64::from(plan.rate.denominator()),
                    pts_denominator: plan.rate.numerator(),
                    decode_time_ms: started.elapsed().as_secs_f64() * 1000.0,
                };
                output
                    .send(Ok(frame))
                    .await
                    .map_err(|_| Error::Media("preview consumer closed".into()))?;
            }
            tokio::time::timeout(tools.timeout, pipe.finish())
                .await
                .map_err(|_| Error::Timeout)??;
            if content_hash(&segment.path).await? != segment.digest {
                return Err(Error::Invalid(
                    "preview source changed during decoding".into(),
                ));
            }
        }
        Ok::<_, Error>(())
    };
    tokio::select! {
        biased;
        _ = cancel.changed() => {},
        result = result => {
            if let Err(error) = result { let _delivered = output.send(Err(error)).await; }
        }
    }
}
