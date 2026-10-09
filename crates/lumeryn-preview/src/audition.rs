//! Standalone audio audition. No project, video clock or GUI device is constructed.
use crate::{AudioStatus, audio::Output};
use lumeryn_core::{
    Error, MediaTools, Result,
    media::{content_hash, local_file},
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditionState {
    Idle,
    Loading,
    Playing,
    Finished,
    Unavailable,
    Failed,
}
#[derive(Debug, Clone)]
pub struct AuditionStatus {
    pub state: AuditionState,
    pub generation: u64,
    pub path: Option<PathBuf>,
    pub position_millis: u64,
    pub maximum_seconds: u32,
    pub buffered_samples: usize,
    pub capacity_samples: usize,
    pub decoded_samples: u64,
    pub audio: AudioStatus,
    pub error: Option<String>,
}
impl Default for AuditionStatus {
    fn default() -> Self {
        Self {
            state: AuditionState::Idle,
            generation: 0,
            path: None,
            position_millis: 0,
            maximum_seconds: 30,
            buffered_samples: 0,
            capacity_samples: 0,
            decoded_samples: 0,
            audio: AudioStatus::Disabled,
            error: None,
        }
    }
}
#[derive(Clone)]
enum Request {
    Play {
        generation: u64,
        path: PathBuf,
        digest: Option<String>,
    },
    Stop(u64),
    Shutdown,
}

/// Commands replace a single latest-request slot. File/device/decoder work lives on an owned worker.
pub struct AuditionPlayer {
    requests: Mutex<watch::Sender<Option<Request>>>,
    generation: Arc<AtomicU64>,
    volume: Arc<AtomicU32>,
    status: Arc<Mutex<AuditionStatus>>,
}
impl AuditionPlayer {
    pub fn new(tools: MediaTools) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (requests, receiver) = watch::channel(None);
        let generation = Arc::new(AtomicU64::new(0));
        let volume = Arc::new(AtomicU32::new(1.0_f32.to_bits()));
        let status = Arc::new(Mutex::new(AuditionStatus::default()));
        let worker_status = Arc::clone(&status);
        let worker_volume = Arc::clone(&volume);
        let worker_generation = Arc::clone(&generation);
        std::thread::Builder::new()
            .name("lumeryn-audition".into())
            .spawn(move || {
                runtime.block_on(run(
                    tools,
                    receiver,
                    worker_status,
                    worker_volume,
                    worker_generation,
                ))
            })?;
        Ok(Self {
            requests: Mutex::new(requests),
            generation,
            volume,
            status,
        })
    }
    pub fn play(&self, path: PathBuf, expected_blake3: Option<String>) -> Result<()> {
        if expected_blake3.as_ref().is_some_and(|digest| {
            digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }) {
            return Err(Error::Invalid(
                "audition BLAKE3 must be 64 lowercase hexadecimal characters".into(),
            ));
        }
        let sender = self
            .requests
            .lock()
            .map_err(|_| Error::Media("audition request lock poisoned".into()))?;
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        publish(
            &self.status,
            &self.generation,
            AuditionStatus {
                state: AuditionState::Loading,
                generation,
                path: Some(path.clone()),
                ..Default::default()
            },
        );
        sender
            .send(Some(Request::Play {
                generation,
                path,
                digest: expected_blake3,
            }))
            .map_err(|_| Error::Media("audition worker stopped".into()))
    }
    pub fn stop(&self) -> Result<()> {
        let sender = self
            .requests
            .lock()
            .map_err(|_| Error::Media("audition request lock poisoned".into()))?;
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        publish(
            &self.status,
            &self.generation,
            AuditionStatus {
                generation,
                ..Default::default()
            },
        );
        sender
            .send(Some(Request::Stop(generation)))
            .map_err(|_| Error::Media("audition worker stopped".into()))
    }
    pub fn set_volume(&self, gain: f32) -> Result<()> {
        if !gain.is_finite() || !(0.0..=1.0).contains(&gain) {
            return Err(Error::Invalid(
                "audition volume must be finite and within 0..1".into(),
            ));
        }
        self.volume.store(gain.to_bits(), Ordering::Relaxed);
        Ok(())
    }
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }
    pub fn status(&self) -> AuditionStatus {
        self.status.lock().map_or_else(
            |_| AuditionStatus {
                state: AuditionState::Failed,
                error: Some("audition status lock poisoned".into()),
                ..Default::default()
            },
            |status| status.clone(),
        )
    }
    pub fn shutdown(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        if let Ok(sender) = self.requests.lock() {
            sender.send_replace(Some(Request::Shutdown));
        }
    }
}
impl Drop for AuditionPlayer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn publish(shared: &Mutex<AuditionStatus>, current: &AtomicU64, status: AuditionStatus) {
    if let Ok(mut target) = shared.lock()
        && current.load(Ordering::Acquire) == status.generation
    {
        *target = status;
    }
}
async fn prepare(
    tools: &MediaTools,
    path: PathBuf,
    digest: Option<String>,
) -> Result<(PathBuf, String)> {
    let path = local_file(&path)?;
    let before = content_hash(&path).await?;
    if digest.is_some_and(|digest| digest != before) {
        return Err(Error::Invalid(
            "audition source digest differs from expected identity".into(),
        ));
    }
    // Audition does not require the zero-origin CFR project-placement metadata contract.
    // In particular, ordinary WAV files may have no ffprobe stream start_time.
    let args = [
        "-v",
        "error",
        "-threads",
        "2",
        "-protocol_whitelist",
        "file",
        "-select_streams",
        "a:0",
        "-show_entries",
        "stream=index",
        "-of",
        "csv=p=0",
    ]
    .into_iter()
    .map(std::ffi::OsString::from)
    .chain([path.as_os_str().to_owned()])
    .collect::<Vec<_>>();
    let probe = tools.run(&tools.ffprobe, &args).await?;
    let index = std::str::from_utf8(&probe)
        .map_err(|_| Error::Media("invalid audio probe output".into()))?
        .trim();
    if index.is_empty() {
        return Err(Error::Unsupported(
            "audition requires an audio stream".into(),
        ));
    }
    index
        .parse::<u32>()
        .map_err(|_| Error::Media("invalid audio stream index".into()))?;
    if content_hash(&path).await? != before {
        return Err(Error::Invalid(
            "audition source changed during probing".into(),
        ));
    }
    Ok((path, before))
}
async fn run(
    tools: MediaTools,
    mut receiver: watch::Receiver<Option<Request>>,
    shared: Arc<Mutex<AuditionStatus>>,
    volume: Arc<AtomicU32>,
    current: Arc<AtomicU64>,
) {
    let (cancel, cancelled) = watch::channel(0_u64);
    let mut output: Option<Output> = None;
    let mut status = AuditionStatus::default();
    loop {
        tokio::select! {
            biased;
            change=receiver.changed()=>{
                if change.is_err() {break;}
                let request=receiver.borrow_and_update().clone();
                output=None;
                match request {
                    Some(Request::Shutdown)|None=>break,
                    Some(Request::Stop(generation))=>{cancel.send_replace(generation);status=AuditionStatus {generation,..Default::default()};publish(&shared,&current,status.clone());}
                    Some(Request::Play {generation,path,digest})=>{
                        cancel.send_replace(generation);
                        status=AuditionStatus {state:AuditionState::Loading,generation,path:Some(path.clone()),..Default::default()};
                        publish(&shared,&current,status.clone());
                        let prepared=tokio::select! {biased;_=receiver.changed()=>{receiver.mark_changed();continue;},result=prepare(&tools,path,digest)=>result};
                        if current.load(Ordering::Acquire)!=generation {continue;}
                        let mut source_cancel=cancelled.clone();
                        source_cancel.borrow_and_update();
                        match prepared.and_then(|(path,digest)|Output::audition(path,digest,tools.clone(),source_cancel,Arc::clone(&volume))) {
                            Ok(audio)=>{audio.set_playing(true);status.state=AuditionState::Playing;status.audio=audio.status(true);output=Some(audio);}
                            Err(error)=>{
                                let reason=error.to_string();
                                status.state=if reason.contains("native audio output device") {AuditionState::Unavailable} else {AuditionState::Failed};
                                status.audio=AudioStatus::Unavailable(reason.clone());status.error=Some(reason);
                            }
                        }
                        publish(&shared,&current,status.clone());
                    }
                }
            }
            _=tokio::time::sleep(Duration::from_millis(20)),if output.is_some()=>{
                if let Some(audio)=&output {
                    let (position,buffered,capacity,decoded)=audio.metrics();
                    status.position_millis=position.min(30_000);status.buffered_samples=buffered;status.capacity_samples=capacity;status.decoded_samples=decoded;status.audio=audio.status(true);
                    if let Some(error)=audio.error() {status.state=AuditionState::Failed;status.error=Some(error);output=None;}
                    else if audio.finished() {status.state=AuditionState::Finished;status.audio=audio.status(false);output=None;}
                    publish(&shared,&current,status.clone());
                }
            }
        }
    }
    drop(output);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latest_requests_are_bounded_and_volume_is_monitor_only() -> Result<()> {
        let player = AuditionPlayer::new(MediaTools::default())?;
        for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            assert!(player.set_volume(value).is_err());
        }
        player.set_volume(0.25)?;
        for _ in 0..1000 {
            player.play("missing.wav".into(), None)?;
            player.stop()?;
        }
        assert_eq!(player.status().state, AuditionState::Idle);
        assert_eq!(player.status().generation, 2000);
        assert_eq!(player.volume(), 0.25);
        assert!(
            player
                .play("missing.wav".into(), Some("bad".into()))
                .is_err()
        );
        Ok(())
    }
    #[test]
    fn stale_generation_cannot_republish_playing() {
        let status = Mutex::new(AuditionStatus::default());
        let current = AtomicU64::new(2);
        publish(
            &status,
            &current,
            AuditionStatus {
                state: AuditionState::Playing,
                generation: 1,
                ..Default::default()
            },
        );
        assert_eq!(
            status.lock().map(|s| s.state).ok(),
            Some(AuditionState::Idle)
        );
    }
}
