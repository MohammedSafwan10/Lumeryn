//! Bounded native preview service. All validation, decoding and device work lives off the UI thread.
mod audio;
mod decode;
mod plan;

use lumeryn_core::{Error, MediaTools, Project, Result, time::FrameRate};
use plan::Plan;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, watch};

#[derive(Clone, Copy, Debug)]
pub struct PreviewConfig {
    pub max_width: u32,
    pub max_height: u32,
    pub frame_capacity: usize,
    pub audio: bool,
}
impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            max_width: 1280,
            max_height: 720,
            frame_capacity: 3,
            audio: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioStatus {
    Disabled,
    Unavailable(String),
    Ready {
        sample_rate: u32,
        channels: u16,
    },
    Playing {
        sample_rate: u32,
        channels: u16,
        underruns: u64,
    },
}

#[derive(Clone, Debug)]
pub struct PreviewStatus {
    pub playing: bool,
    pub position_frame: u64,
    pub total_frames: u64,
    pub project_revision: u64,
    pub generation: u64,
    pub decoded_frames: u64,
    pub dropped_frames: u64,
    pub buffered_frames: usize,
    pub error: Option<String>,
    pub decode_time_ms: f64,
    pub seek_latency_ms: f64,
    pub audio: AudioStatus,
}
impl Default for PreviewStatus {
    fn default() -> Self {
        Self {
            playing: false,
            position_frame: 0,
            total_frames: 0,
            project_revision: 0,
            generation: 0,
            decoded_frames: 0,
            dropped_frames: 0,
            buffered_frames: 0,
            error: None,
            decode_time_ms: 0.0,
            seek_latency_ms: 0.0,
            audio: AudioStatus::Disabled,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PreviewFrame {
    pub rgba: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
    pub frame_index: u64,
    pub project_revision: u64,
    pub generation: u64,
    pub pts_numerator: u64,
    pub pts_denominator: u32,
    pub decode_time_ms: f64,
}

enum Command {
    Load(Project, PathBuf, u64),
    Seek(u64, u64),
    Step(i64, u64),
    Play,
    Pause,
    Audio(bool, u64),
}

/// `new` starts a worker without a GUI or existing Tokio runtime. Commands never wait for decoding.
pub struct PreviewPlayer {
    commands: mpsc::Sender<Command>,
    frames: mpsc::Receiver<PreviewFrame>,
    status: Arc<Mutex<PreviewStatus>>,
    generation: Arc<AtomicU64>,
    volume: Arc<AtomicU32>,
    stop: watch::Sender<bool>,
}
impl PreviewPlayer {
    pub fn new(tools: MediaTools, config: PreviewConfig) -> Result<Self> {
        if !(2..=4096).contains(&config.max_width)
            || !(2..=4096).contains(&config.max_height)
            || !(1..=3).contains(&config.frame_capacity)
        {
            return Err(Error::Invalid(
                "preview dimensions must be 2..4096 and queue capacity 1..3".into(),
            ));
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (commands, receiver) = mpsc::channel(8);
        let (output, frames) = mpsc::channel(config.frame_capacity);
        let (stop, stopped) = watch::channel(false);
        let status = Arc::new(Mutex::new(PreviewStatus::default()));
        let generation = Arc::new(AtomicU64::new(0));
        let volume = Arc::new(AtomicU32::new(1.0_f32.to_bits()));
        let shared = Arc::clone(&status);
        let current = Arc::clone(&generation);
        let gain = Arc::clone(&volume);
        std::thread::Builder::new()
            .name("lumeryn-preview".into())
            .spawn(move || {
                runtime.block_on(run(
                    tools,
                    config,
                    receiver,
                    output,
                    shared,
                    (current, gain),
                    stopped,
                ));
            })?;
        Ok(Self {
            commands,
            frames,
            status,
            generation,
            volume,
            stop,
        })
    }

    fn send(&self, command: Command) -> Result<()> {
        self.commands
            .try_send(command)
            .map_err(|e| Error::Media(format!("preview command queue: {e}")))
    }
    fn changing(&self, make: impl FnOnce(u64) -> Command) -> Result<()> {
        let permit = self
            .commands
            .try_reserve()
            .map_err(|e| Error::Media(format!("preview command queue: {e}")))?;
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        permit.send(make(generation));
        Ok(())
    }
    /// Accepted asynchronously; inspect `status().error` for validation/decode failures.
    pub fn load(&self, project: Project, database: PathBuf) -> Result<()> {
        self.changing(|g| Command::Load(project, database, g))
    }
    pub fn seek(&self, frame: u64) -> Result<()> {
        self.changing(|g| Command::Seek(frame, g))
    }
    pub fn step(&self, delta: i64) -> Result<()> {
        self.changing(|g| Command::Step(delta, g))
    }
    pub fn play(&self) -> Result<()> {
        self.send(Command::Play)
    }
    pub fn pause(&self) -> Result<()> {
        self.send(Command::Pause)
    }
    pub fn set_audio_enabled(&self, enabled: bool) -> Result<()> {
        self.changing(|g| Command::Audio(enabled, g))
    }
    /// Monitor gain only; does not modify source PCM, project mixing, export, or the playback clock.
    pub fn set_volume(&self, gain: f32) -> Result<()> {
        if !gain.is_finite() || !(0.0..=1.0).contains(&gain) {
            return Err(Error::Invalid(
                "preview volume must be finite and within 0..1".into(),
            ));
        }
        self.volume.store(gain.to_bits(), Ordering::Relaxed);
        Ok(())
    }
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }
    pub fn status(&self) -> PreviewStatus {
        snapshot(&self.status)
    }
    /// Drain to the newest current-generation frame. Retain it on the GPU until this returns another frame.
    pub fn try_frame(&mut self) -> Option<PreviewFrame> {
        let generation = self.generation.load(Ordering::Acquire);
        let mut newest = None;
        while let Ok(frame) = self.frames.try_recv() {
            if frame.generation == generation {
                newest = Some(frame);
            }
        }
        newest
    }
    pub fn shutdown(&self) {
        self.stop.send_replace(true);
    }
}
impl Drop for PreviewPlayer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn snapshot(status: &Mutex<PreviewStatus>) -> PreviewStatus {
    status
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}
fn update(status: &Mutex<PreviewStatus>, edit: impl FnOnce(&mut PreviewStatus)) {
    edit(
        &mut status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
}

#[derive(Default)]
struct Clock {
    frame: u64,
    started: Option<Instant>,
}
impl Clock {
    fn position(&self, rate: FrameRate, now: Instant) -> u64 {
        self.frame
            + self.started.map_or(0, |start| {
                frames_for_duration(now.saturating_duration_since(start), rate)
            })
    }
    fn play(&mut self) {
        if self.started.is_none() {
            self.started = Some(Instant::now());
        }
    }
    fn pause(&mut self, rate: FrameRate) {
        self.frame = self.position(rate, Instant::now());
        self.started = None;
    }
}
fn frames_for_duration(duration: Duration, rate: FrameRate) -> u64 {
    ((duration.as_nanos() * u128::from(rate.numerator()))
        / (1_000_000_000 * u128::from(rate.denominator()))) as u64
}

struct Session {
    plan: Plan,
    packets: mpsc::Receiver<Result<PreviewFrame>>,
    pending: Option<PreviewFrame>,
    task: tokio::task::JoinHandle<()>,
    cancel: watch::Sender<u64>,
    audio: Option<audio::Output>,
    clock: Clock,
    first: bool,
    requested: Instant,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.cancel.send_replace(1);
        self.task.abort();
    }
}
fn start_session(
    plan: Plan,
    start: u64,
    generation: u64,
    tools: &MediaTools,
    audio_enabled: bool,
    status: &Mutex<PreviewStatus>,
    volume: &Arc<AtomicU32>,
) -> Session {
    let (sender, packets) = mpsc::channel(1);
    let (cancel, cancelled) = watch::channel(0);
    let task = tokio::spawn(decode::video(
        plan.clone(),
        start,
        generation,
        tools.clone(),
        sender,
        cancelled.clone(),
    ));
    let audio = if audio_enabled {
        match audio::Output::start(
            plan.clone(),
            start,
            tools.clone(),
            cancelled,
            Arc::clone(volume),
        ) {
            Ok(output) => {
                update(status, |s| s.audio = output.status(false));
                Some(output)
            }
            Err(error) => {
                update(status, |s| {
                    s.audio = AudioStatus::Unavailable(error.to_string())
                });
                None
            }
        }
    } else {
        update(status, |s| s.audio = AudioStatus::Disabled);
        None
    };
    update(status, |s| {
        s.generation = generation;
        s.position_frame = start;
        s.total_frames = plan.total;
        s.project_revision = plan.revision;
        s.error = None;
        s.buffered_frames = 0;
    });
    Session {
        plan,
        packets,
        pending: None,
        task,
        cancel,
        audio,
        clock: Clock {
            frame: start,
            started: None,
        },
        first: true,
        requested: Instant::now(),
    }
}

async fn run(
    tools: MediaTools,
    config: PreviewConfig,
    mut commands: mpsc::Receiver<Command>,
    output: mpsc::Sender<PreviewFrame>,
    status: Arc<Mutex<PreviewStatus>>,
    controls: (Arc<AtomicU64>, Arc<AtomicU32>),
    mut stopped: watch::Receiver<bool>,
) {
    let (generation, volume) = controls;
    let mut session: Option<Session> = None;
    let mut building: Option<tokio::task::JoinHandle<(u64, Result<Plan>)>> = None;
    let mut playing = false;
    let mut audio_enabled = config.audio;
    let mut pending_start = 0_u64;
    let mut loading_generation = 0;
    let mut tick = tokio::time::interval(Duration::from_millis(4));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _ = stopped.changed() => break,
            command = commands.recv() => {
                let Some(command) = command else { break; };
                match command {
                    Command::Load(project, database, g) => {
                        session = None; playing = false;
                        pending_start = 0; loading_generation = g;
                        if let Some(task) = building.take() { task.abort(); }
                        update(&status, |s| { s.playing = false; s.error = None; s.generation = g; s.total_frames = 0; });
                        building = Some(tokio::spawn(async move { (g, Plan::build(project, &database, config).await) }));
                    }
                    Command::Play => { playing = session.is_some() || building.is_some(); if let Some(s) = &mut session && !s.first { s.clock.play(); if let Some(a) = &s.audio { a.set_playing(true); } } }
                    Command::Pause => { playing = false; if let Some(s) = &mut session { s.clock.pause(s.plan.rate); if let Some(a) = &s.audio { a.set_playing(false); } } }
                    Command::Seek(frame, g) => { pending_start = frame; if let Some(s) = session.take() { let frame = frame.min(s.plan.total - 1); let plan = s.plan.clone(); drop(s); session = Some(start_session(plan, frame, g, &tools, audio_enabled, &status, &volume)); } }
                    Command::Step(delta, g) => { playing = false; pending_start = pending_start.saturating_add_signed(delta); if let Some(s) = session.take() { let at = snapshot(&status).position_frame.saturating_add_signed(delta).min(s.plan.total - 1); let plan = s.plan.clone(); drop(s); session = Some(start_session(plan, at, g, &tools, audio_enabled, &status, &volume)); } }
                    Command::Audio(enabled, g) => { audio_enabled = enabled; if let Some(s) = session.take() { let at = snapshot(&status).position_frame; let plan = s.plan.clone(); drop(s); session = Some(start_session(plan, at, g, &tools, audio_enabled, &status, &volume)); } }
                }
                update(&status, |s| s.playing = playing);
            }
            _ = tick.tick(), if playing || building.is_some() || session.as_ref().is_some_and(|s| s.first) => {
                if building.as_ref().is_some_and(|task| task.is_finished())
                    && let Some(task) = building.take() {
                        match task.await {
                            Ok((g, Ok(plan))) if g == loading_generation => { let at = pending_start.min(plan.total - 1); session = Some(start_session(plan, at, generation.load(Ordering::Acquire), &tools, audio_enabled, &status, &volume)); },
                            Ok((g, Err(error))) if g == loading_generation => update(&status, |s| s.error = Some(error.to_string())),
                            Err(error) => update(&status, |s| s.error = Some(error.to_string())),
                            _ => {}
                        }
                    }
                let Some(s) = &mut session else { continue; };
                if s.pending.is_none() {
                    match s.packets.try_recv() {
                        Ok(Ok(frame)) => { update(&status, |st| { st.decoded_frames += 1; st.decode_time_ms = frame.decode_time_ms; }); s.pending = Some(frame); }
                        Ok(Err(error)) => { playing = false; s.first = false; if let Some(a) = &s.audio { a.set_playing(false); } update(&status, |st| { st.error = Some(error.to_string()); st.playing = false; }); }
                        Err(_) => {}
                    }
                }
                if let Some(a) = &s.audio {
                    if let Some(error) = a.error() { playing = false; a.set_playing(false); update(&status, |st| { st.error = Some(error); st.playing = false; }); }
                    update(&status, |st| st.audio = a.status(playing));
                }
                let target = s.audio.as_ref().filter(|_| playing && !s.first).map_or_else(|| s.clock.position(s.plan.rate, Instant::now()), |a| a.position_frame(s.plan.rate));
                if playing && !s.first && snapshot(&status).position_frame + 1 == s.plan.total && (target >= s.plan.total || s.audio.as_ref().is_some_and(audio::Output::finished)) {
                    playing = false; s.clock.pause(s.plan.rate); if let Some(a) = &s.audio { a.set_playing(false); }
                    update(&status, |st| st.playing = false);
                }
                let due = s.pending.as_ref().is_some_and(|frame| s.first || (playing && frame.frame_index <= target));
                if due
                    && let Some(frame) = s.pending.take() {
                        let index = frame.frame_index;
                        match output.try_send(frame) {
                            Ok(()) => update(&status, |st| st.position_frame = index),
                            Err(mpsc::error::TrySendError::Full(frame)) => { if playing && index + 1 < s.plan.total { update(&status, |st| st.dropped_frames += 1); } else { s.pending = Some(frame); } }
                            Err(mpsc::error::TrySendError::Closed(_)) => break,
                        }
                        if s.first && s.pending.is_none() { s.first = false; update(&status, |st| st.seek_latency_ms = s.requested.elapsed().as_secs_f64() * 1000.0); if playing { s.clock.play(); if let Some(a) = &s.audio { a.set_playing(true); } } }
                    }
                update(&status, |st| st.buffered_frames = output.max_capacity() - output.capacity() + s.packets.len() + usize::from(s.pending.is_some()));
            }
        }
    }
    if let Some(task) = building {
        task.abort();
    }
    drop(session);
    update(&status, |s| s.playing = false);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monitor_volume_validation_does_not_change_generation_or_position() -> Result<()> {
        let player = PreviewPlayer::new(
            MediaTools::default(),
            PreviewConfig {
                audio: false,
                ..Default::default()
            },
        )?;
        assert_eq!(player.volume(), 1.0);
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 1.1] {
            assert!(player.set_volume(invalid).is_err());
            assert_eq!(player.volume(), 1.0);
        }
        for valid in [0.0, 0.8, 1.0] {
            player.set_volume(valid)?;
            assert_eq!(player.volume(), valid);
        }
        assert_eq!(player.generation.load(Ordering::Relaxed), 0);
        assert_eq!(player.status().position_frame, 0);
        assert!(!player.status().playing);
        Ok(())
    }
    #[test]
    fn rational_clock_has_exact_boundaries_and_pause_freezes() -> Result<()> {
        let rate = FrameRate::new(30_000, 1001)?;
        assert_eq!(frames_for_duration(Duration::from_micros(33_366), rate), 0);
        assert_eq!(frames_for_duration(Duration::from_micros(33_367), rate), 1);
        assert_eq!(frames_for_duration(Duration::from_millis(1001), rate), 30);
        let mut clock = Clock {
            frame: 120,
            ..Default::default()
        };
        clock.play();
        clock.pause(rate);
        assert_eq!(
            clock.position(rate, Instant::now() + Duration::from_secs(10)),
            120
        );
        Ok(())
    }
    #[test]
    fn invalid_queue_budget_is_rejected() {
        assert!(
            PreviewPlayer::new(
                MediaTools::default(),
                PreviewConfig {
                    frame_capacity: 4,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
