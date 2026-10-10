#![cfg(windows)]

use lumeryn_core::{Batch, Edit, Error, Project, Result, media::content_hash};
use lumeryn_service::{Client, ImportJob, ImportSource, ImportStage, Request, ToolConfig};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::task::JoinHandle;
use uuid::Uuid;

fn tools() -> ToolConfig {
    ToolConfig {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffmpeg".into()),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffprobe".into()),
        timeout_seconds: 30,
    }
}

fn generate(path: &Path, input: &str, codec: &str) -> Result<()> {
    generate_extra(path, input, codec, &[])
}

fn generate_extra(path: &Path, input: &str, codec: &str, extra: &[&str]) -> Result<()> {
    let output = std::process::Command::new(tools().ffmpeg)
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-nostdin",
            "-n",
            "-f",
            "lavfi",
            "-i",
            input,
            "-c",
            codec,
            "-threads",
            "2",
        ])
        .args(extra)
        .arg(path)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

async fn service(path: &Path) -> Result<(Client, JoinHandle<Result<()>>)> {
    let path_owned = path.to_owned();
    let task = tokio::spawn(async move { lumeryn_service::serve(path_owned).await });
    let client = Client::existing(path)?;
    for _ in 0..100 {
        if client.watch(None, 0).await.is_ok() {
            client
                .call(Request::Create {
                    project: Project::new("Import jobs".into(), 64, 36, "30/1".parse()?)?,
                })
                .await?;
            return Ok((client, task));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    task.abort();
    Err(Error::Timeout)
}

fn source(path: &Path, name: &str) -> ImportSource {
    ImportSource {
        path: path.to_owned(),
        media_id: Uuid::new_v4(),
        name: name.into(),
    }
}

async fn start(
    client: &Client,
    sources: Vec<ImportSource>,
    revision: u64,
    key: Option<String>,
    match_first_video: bool,
    tools: ToolConfig,
) -> Result<ImportJob> {
    Ok(serde_json::from_value(
        client
            .call(Request::Import {
                sources,
                expected_revision: revision,
                key,
                tools,
                match_first_video,
            })
            .await?,
    )?)
}

async fn terminal(client: &Client, job: Uuid) -> Result<ImportJob> {
    let started = Instant::now();
    loop {
        let status = client.import_status(job).await?;
        if status.stage.is_terminal() {
            return Ok(status);
        }
        if started.elapsed() > Duration::from_secs(20) {
            return Err(Error::Timeout);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn preparing(client: &Client, job: Uuid) -> Result<ImportJob> {
    let started = Instant::now();
    loop {
        let status = client.import_status(job).await?;
        if matches!(status.stage, ImportStage::Hashing | ImportStage::Validating) {
            return Ok(status);
        }
        assert!(
            !status.stage.is_terminal(),
            "job finished before concurrency test: {:?}",
            status
        );
        if started.elapsed() > Duration::from_secs(10) {
            return Err(Error::Timeout);
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

async fn stop(task: JoinHandle<Result<()>>) {
    task.abort();
    assert!(task.await.is_err_and(|error| error.is_cancelled()));
    tokio::time::sleep(Duration::from_millis(20)).await;
}

#[tokio::test]
#[ignore = "requires installed FFmpeg/ffprobe"]
async fn atomic_multi_source_matching_events_and_durable_replay_without_sources() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let video = directory.path().join("fractional.mp4");
    let audio = directory.path().join("voice.wav");
    generate(&video, "testsrc2=s=64x36:r=60000/1001:d=1", "mpeg4")?;
    generate(&audio, "sine=f=440:r=32000:d=1", "pcm_s16le")?;
    let hashes = [content_hash(&video).await?, content_hash(&audio).await?];
    let sources = vec![
        source(&video, "Fractional video"),
        source(&audio, "Independent voice"),
    ];
    let path = directory.path().join("project.lumeryn");
    let (client, task) = service(&path).await?;
    let started = start(
        &client,
        sources.clone(),
        0,
        Some("atomic-first".into()),
        true,
        tools(),
    )
    .await?;
    assert_eq!(started.stage, ImportStage::Queued);
    let completed = terminal(&client, started.job_id).await?;
    assert_eq!(
        completed.stage,
        ImportStage::Completed,
        "{:?}",
        completed.error
    );
    assert_eq!(completed.revision, Some(1));
    assert!(!completed.cache_hit);
    let project = client.inspect().await?;
    assert_eq!(project.frame_rate, "60000/1001".parse()?);
    assert_eq!(project.revision, 1);
    assert_eq!(project.media.len(), 2);
    for (index, media) in project.media.iter().enumerate() {
        assert_eq!(media.id, sources[index].media_id);
        assert_eq!(media.alias.as_deref(), Some(sources[index].name.as_str()));
        assert_eq!(
            media.metadata.blake3.as_deref(),
            Some(hashes[index].as_str())
        );
        assert!(completed.sources[index].quick.is_some());
    }
    assert_eq!(
        project.media[1]
            .metadata
            .audio
            .as_ref()
            .and_then(|audio| audio.canonical_samples_48k),
        Some(48_000)
    );
    let watch = client.watch(None, 0).await?;
    assert!(
        watch
            .import_jobs
            .iter()
            .any(|job| job.job_id == started.job_id && job.stage == ImportStage::Completed)
    );
    let mut stages = Vec::new();
    for event in watch
        .events
        .iter()
        .filter(|event| event.kind.starts_with("import_"))
    {
        let status: ImportJob = serde_json::from_value(event.data.clone())?;
        assert_eq!(status.job_id, started.job_id);
        if !stages.contains(&status.stage) {
            stages.push(status.stage);
        }
    }
    assert_eq!(
        stages,
        [
            ImportStage::Queued,
            ImportStage::Inspecting,
            ImportStage::Hashing,
            ImportStage::Validating,
            ImportStage::Committing,
            ImportStage::Completed
        ]
    );
    assert!(
        client
            .call(Request::Cancel {
                job_id: started.job_id
            })
            .await
            .is_err()
    );
    assert_eq!(content_hash(&video).await?, hashes[0]);
    assert_eq!(content_hash(&audio).await?, hashes[1]);
    std::fs::remove_file(&video)?;
    std::fs::remove_file(&audio)?;
    stop(task).await;
    // Reconnect to a fresh service with no cache and absent original generated sources.
    let service_path = path.clone();
    let restarted = tokio::spawn(async move { lumeryn_service::serve(service_path).await });
    let reconnected = Client::existing(&path)?;
    for _ in 0..100 {
        if reconnected.inspect().await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let replay = start(
        &reconnected,
        sources.clone(),
        0,
        Some("atomic-first".into()),
        true,
        tools(),
    )
    .await?;
    assert_eq!(replay.stage, ImportStage::Completed);
    assert_eq!(
        replay.result.as_ref().map(|receipt| &receipt["replayed"]),
        Some(&serde_json::json!(true))
    );
    assert_eq!(reconnected.inspect().await?.revision, 1);
    let mut changed = sources;
    changed[0].name = "Different request".into();
    assert!(
        start(
            &reconnected,
            changed,
            0,
            Some("atomic-first".into()),
            true,
            tools()
        )
        .await
        .is_err()
    );
    assert_eq!(reconnected.history().await?.len(), 2);
    stop(restarted).await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed FFmpeg/ffprobe"]
async fn content_cache_reuses_verified_bytes_rejects_same_size_changes_and_changed_policy()
-> Result<()> {
    let directory = tempfile::tempdir()?;
    let audio = directory.path().join("audio.wav");
    generate(&audio, "sine=f=220:r=48000:d=1", "pcm_s16le")?;
    let (client, task) = service(&directory.path().join("project.lumeryn")).await?;
    for (revision, expected_hit) in [(0, false), (1, true)] {
        let job = start(
            &client,
            vec![source(&audio, "Audio")],
            revision,
            None,
            false,
            tools(),
        )
        .await?;
        let status = terminal(&client, job.job_id).await?;
        assert_eq!(status.stage, ImportStage::Completed, "{:?}", status.error);
        assert_eq!(status.cache_hit, expected_hit);
    }
    let mut bytes = std::fs::read(&audio)?;
    let old_hash = content_hash(&audio).await?;
    *bytes
        .last_mut()
        .ok_or_else(|| Error::Invalid("empty generated source".into()))? ^= 1;
    std::fs::write(&audio, &bytes)?;
    let job = start(
        &client,
        vec![source(&audio, "Changed")],
        2,
        None,
        false,
        tools(),
    )
    .await?;
    let status = terminal(&client, job.job_id).await?;
    assert_eq!(status.stage, ImportStage::Completed, "{:?}", status.error);
    assert!(!status.cache_hit);
    let project = client.inspect().await?;
    let changed_hash = content_hash(&audio).await?;
    assert_ne!(old_hash, changed_hash);
    assert_eq!(
        project.media[2].metadata.blake3.as_deref(),
        Some(changed_hash.as_str())
    );
    assert_eq!(std::fs::read(&audio)?, bytes);
    let mut changed_policy = tools();
    changed_policy.timeout_seconds += 1;
    let job = start(
        &client,
        vec![source(&audio, "Policy change")],
        3,
        None,
        false,
        changed_policy,
    )
    .await?;
    let status = terminal(&client, job.job_id).await?;
    assert_eq!(status.stage, ImportStage::Completed, "{:?}", status.error);
    assert!(!status.cache_hit);
    stop(task).await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed FFmpeg/ffprobe"]
async fn preparing_import_allows_inspect_watch_and_unrelated_edit_then_fails_stale_revision()
-> Result<()> {
    let directory = tempfile::tempdir()?;
    let audio = directory.path().join("long.wav");
    generate(&audio, "sine=f=330:r=48000:d=600", "pcm_s16le")?;
    let digest = content_hash(&audio).await?;
    let (client, task) = service(&directory.path().join("project.lumeryn")).await?;
    let second = Client::existing(client.project_path())?;
    let job = start(
        &client,
        vec![source(&audio, "Long audio")],
        0,
        None,
        false,
        tools(),
    )
    .await?;
    preparing(&second, job.job_id).await?;
    let operations = async {
        assert_eq!(second.inspect().await?.revision, 0);
        assert!(
            second
                .watch(None, 0)
                .await?
                .import_jobs
                .iter()
                .any(|status| status.job_id == job.job_id)
        );
        second
            .execute(
                Batch {
                    operations: vec![Edit::SetCanvas {
                        width: 128,
                        height: 72,
                    }],
                },
                Some(0),
                None,
                false,
                tools(),
            )
            .await
    };
    let receipt = tokio::time::timeout(Duration::from_secs(2), operations)
        .await
        .map_err(|_| Error::Timeout)??;
    assert_eq!(receipt["revision"], 1);
    let failed = terminal(&client, job.job_id).await?;
    assert_eq!(failed.stage, ImportStage::Failed);
    assert_eq!(
        failed.error.as_ref().map(|error| error.code.as_str()),
        Some("revision_conflict")
    );
    assert_eq!(
        failed.error.as_ref().and_then(|error| error.conflict),
        Some((0, 1))
    );
    let project = second.inspect().await?;
    assert_eq!(project.revision, 1);
    assert!(project.media.is_empty());
    assert_eq!(content_hash(&audio).await?, digest);
    stop(task).await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed FFmpeg/ffprobe"]
async fn cancellation_during_preparation_never_commits_and_job_remains_queryable() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let audio = directory.path().join("cancel.wav");
    generate(&audio, "sine=f=550:r=48000:d=600", "pcm_s16le")?;
    let digest = content_hash(&audio).await?;
    let (client, task) = service(&directory.path().join("project.lumeryn")).await?;
    let job = start(
        &client,
        vec![source(&audio, "Cancel audio")],
        0,
        None,
        false,
        tools(),
    )
    .await?;
    preparing(&client, job.job_id).await?;
    let response = client.call(Request::Cancel { job_id: job.job_id }).await?;
    assert_eq!(response["cancellation_requested"], true);
    let status = terminal(&client, job.job_id).await?;
    assert_eq!(status.stage, ImportStage::Cancelled);
    assert!(status.result.is_none());
    let project = client.inspect().await?;
    assert_eq!(project.revision, 0);
    assert!(project.media.is_empty());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(client.inspect().await?.revision, 0);
    assert_eq!(content_hash(&audio).await?, digest);
    assert!(
        client
            .watch(None, 0)
            .await?
            .import_jobs
            .iter()
            .any(|job| job.stage == ImportStage::Cancelled)
    );
    stop(task).await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed FFmpeg/ffprobe"]
async fn first_video_matching_ignores_png_and_never_skips_first_vfr_for_later_cfr() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let image = directory.path().join("still.png");
    let video = directory.path().join("moving.mp4");
    generate_extra(
        &image,
        "color=c=blue:s=64x36:d=1",
        "png",
        &["-frames:v", "1"],
    )?;
    generate(&video, "testsrc2=s=64x36:r=25:d=1", "mpeg4")?;
    let (client, task) = service(&directory.path().join("project.lumeryn")).await?;
    let job = start(
        &client,
        vec![source(&image, "Still")],
        0,
        None,
        true,
        tools(),
    )
    .await?;
    assert_eq!(
        terminal(&client, job.job_id).await?.stage,
        ImportStage::Completed
    );
    assert_eq!(client.inspect().await?.frame_rate, "30/1".parse()?);
    let job = start(
        &client,
        vec![source(&video, "First moving video")],
        1,
        None,
        true,
        tools(),
    )
    .await?;
    assert_eq!(
        terminal(&client, job.job_id).await?.stage,
        ImportStage::Completed
    );
    assert_eq!(client.inspect().await?.frame_rate, "25/1".parse()?);
    stop(task).await;

    let vfr = directory.path().join("variable.mkv");
    generate_extra(
        &vfr,
        "testsrc2=s=64x36:r=30:d=1",
        "ffv1",
        &[
            "-vf",
            "setpts=if(lt(N\\,15)\\,N/(30*TB)\\,(15+(N-15)*2)/(30*TB))",
            "-fps_mode",
            "vfr",
        ],
    )?;
    let (client, task) = service(&directory.path().join("vfr-project.lumeryn")).await?;
    let job = start(
        &client,
        vec![source(&vfr, "First VFR"), source(&video, "Later CFR")],
        0,
        None,
        true,
        tools(),
    )
    .await?;
    let status = terminal(&client, job.job_id).await?;
    assert_eq!(status.stage, ImportStage::Completed, "{:?}", status.error);
    let project = client.inspect().await?;
    assert_eq!(project.frame_rate, "30/1".parse()?);
    assert!(
        project.media[0]
            .metadata
            .video
            .as_ref()
            .is_some_and(|video| !video.constant_rate)
    );
    stop(task).await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed FFmpeg/ffprobe"]
async fn invalid_second_source_keeps_atomic_batch_and_project_history_unchanged() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let video = directory.path().join("valid.mp4");
    generate(&video, "testsrc2=s=64x36:r=25:d=1", "mpeg4")?;
    let digest = content_hash(&video).await?;
    let (client, task) = service(&directory.path().join("project.lumeryn")).await?;
    let job = start(
        &client,
        vec![
            source(&video, "Valid first"),
            source(&directory.path().join("absent.wav"), "Missing second"),
        ],
        0,
        None,
        true,
        tools(),
    )
    .await?;
    let failed = terminal(&client, job.job_id).await?;
    assert_eq!(failed.stage, ImportStage::Failed);
    assert!(failed.result.is_none());
    let project = client.inspect().await?;
    assert_eq!(project.revision, 0);
    assert_eq!(project.frame_rate, "30/1".parse()?);
    assert!(project.media.is_empty());
    assert_eq!(client.history().await?.len(), 1);
    assert_eq!(content_hash(&video).await?, digest);
    stop(task).await;
    Ok(())
}
