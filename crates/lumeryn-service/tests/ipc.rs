#![cfg(windows)]

use lumeryn_core::{
    Batch, Edit, Error, Project, Result,
    media::{Metadata, Video},
    model::{Clip, Media},
    time::FrameRate,
};
use lumeryn_service::{Client, Request, ToolConfig};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::task::JoinHandle;
use uuid::Uuid;

async fn service(path: &Path) -> Result<(Client, JoinHandle<Result<()>>)> {
    let project_path = path.to_owned();
    let task = tokio::spawn(async move { lumeryn_service::serve(project_path).await });
    let client = Client::existing(path)?;
    for _ in 0..100 {
        if client.watch(None, 0).await.is_ok() {
            return Ok((client, task));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    task.abort();
    Err(Error::Timeout)
}

fn fixture() -> Result<Project> {
    let mut project = Project::new("IPC fixture".into(), 64, 64, "30/1".parse::<FrameRate>()?)?;
    let media_id = Uuid::new_v4();
    project.media.push(Media {
        id: media_id,
        path: PathBuf::from("source.mp4"),
        metadata: Metadata {
            size_bytes: 1,
            blake3: None,
            audio: None,
            video: Some(Video {
                codec: "mpeg4".into(),
                width: 64,
                height: 64,
                frame_rate: project.frame_rate,
                frames: 600,
                start_time_micros: 0,
                sample_aspect_ratio: "1:1".into(),
                color_transfer: None,
                constant_rate: true,
            }),
        },
    });
    project.clips.push(Clip {
        id: Uuid::new_v4(),
        media_id,
        track_id: project.video_track_id,
        at_frame: 0,
        source_in_frame: 0,
        duration_frames: 300,
    });
    project.validate()?;
    Ok(project)
}

fn trim(project: &Project, duration_frames: u64) -> Batch {
    Batch {
        operations: vec![Edit::TrimClip {
            clip_id: project.clips[0].id,
            source_in_frame: 0,
            duration_frames,
        }],
    }
}

async fn stop(task: JoinHandle<Result<()>>) {
    task.abort();
    assert!(task.await.is_err_and(|error| error.is_cancelled()));
    tokio::time::sleep(Duration::from_millis(25)).await;
}

#[tokio::test]
async fn two_clients_atomic_conflict_replay_and_live_events() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let (first, task) = service(&path).await?;
    let second = Client::existing(&path)?;
    let project = fixture()?;
    first
        .call(Request::Create {
            project: project.clone(),
        })
        .await?;
    let initial = second.watch(None, 0).await?;
    assert_eq!(
        initial.snapshot.as_ref().map(|snapshot| snapshot.revision),
        Some(0)
    );
    let watching = tokio::spawn({
        let second = second.clone();
        async move { second.watch(Some(initial.sequence), 2000).await }
    });
    let tools = ToolConfig::default();
    let (a, b) = tokio::join!(
        first.execute(
            trim(&project, 200),
            Some(0),
            Some("a".into()),
            false,
            tools.clone()
        ),
        second.execute(trim(&project, 150), Some(0), Some("b".into()), false, tools)
    );
    assert!(matches!(
        (&a, &b),
        (
            Ok(_),
            Err(Error::Conflict {
                expected: 0,
                current: 1
            })
        ) | (
            Err(Error::Conflict {
                expected: 0,
                current: 1
            }),
            Ok(_)
        )
    ));
    let events = watching
        .await
        .map_err(|error| Error::Io(std::io::Error::other(error)))??;
    assert_eq!(
        events.snapshot.as_ref().map(|snapshot| snapshot.revision),
        Some(1)
    );
    assert!(
        events
            .events
            .iter()
            .any(|event| event.kind == "project_committed" && event.revision == Some(1))
    );
    let (batch, key) = if a.is_ok() {
        (trim(&project, 200), "a")
    } else {
        (trim(&project, 150), "b")
    };
    let replay = first
        .execute(
            batch,
            Some(0),
            Some(key.into()),
            false,
            ToolConfig::default(),
        )
        .await?;
    assert_eq!(replay["replayed"], true);
    assert_eq!(second.history().await?.len(), 2);
    assert_eq!(
        second.watch(Some(events.sequence), 0).await?.sequence,
        events.sequence
    );
    stop(task).await;
    Ok(())
}

#[tokio::test]
async fn reconnect_snapshot_revision_and_bounded_event_replay() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let (client, task) = service(&path).await?;
    let project = fixture()?;
    client
        .call(Request::Create {
            project: project.clone(),
        })
        .await?;
    for revision in 0..260 {
        client
            .execute(
                trim(&project, if revision % 2 == 0 { 100 } else { 101 }),
                Some(revision),
                None,
                false,
                ToolConfig::default(),
            )
            .await?;
    }
    let replay = client.watch(Some(1), 0).await?;
    assert!(replay.resync);
    assert_eq!(replay.events.len(), lumeryn_service::EVENT_CAPACITY);
    assert_eq!(
        replay.snapshot.as_ref().map(|snapshot| snapshot.revision),
        Some(260)
    );
    stop(task).await;
    let (reconnected, restart) = service(&path).await?;
    let current = reconnected.watch(Some(replay.sequence), 0).await?;
    assert!(current.resync);
    assert_eq!(
        current.snapshot.as_ref().map(|snapshot| snapshot.revision),
        Some(260)
    );
    stop(restart).await;
    Ok(())
}

#[tokio::test]
async fn validation_and_create_existing_are_non_destructive() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let (client, task) = service(&path).await?;
    assert!(client.inspect().await.is_err());
    assert!(!path.exists());
    let project = fixture()?;
    let mut invalid = project.clone();
    invalid.revision = 5;
    assert!(matches!(
        client.call(Request::Create { project: invalid }).await,
        Err(Error::Invalid(_))
    ));
    assert!(!path.exists());
    client
        .call(Request::Create {
            project: project.clone(),
        })
        .await?;
    assert!(matches!(
        client
            .call(Request::Create {
                project: project.clone()
            })
            .await,
        Err(Error::Exists(_))
    ));
    assert!(matches!(
        client.watch(None, 30_001).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        client
            .execute(
                trim(&project, 100),
                Some(0),
                None,
                false,
                ToolConfig {
                    timeout_seconds: 0,
                    ..Default::default()
                }
            )
            .await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(client.inspect().await?.revision, 0);
    assert_eq!(client.history().await?.len(), 1);
    stop(task).await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires trusted installed FFmpeg and ffprobe; generated media only"]
async fn render_snapshot_jobs_reads_and_edits_remain_responsive() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let source = directory.path().join("source.mp4");
    let tools = lumeryn_core::MediaTools::default();
    let args = [
        "-v",
        "error",
        "-nostdin",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=640x360:rate=30",
        "-frames:v",
        "1800",
        "-c:v",
        "mpeg4",
        "-threads",
        "2",
    ]
    .into_iter()
    .map(Into::into)
    .chain([source.as_os_str().to_owned()])
    .collect::<Vec<_>>();
    tools.run(&tools.ffmpeg, &args).await?;
    let (client, task) = service(&path).await?;
    let mut project = fixture()?;
    project.width = 640;
    project.height = 360;
    project.media[0].metadata = tools.probe(&source).await?;
    project.clips[0].duration_frames = 1800;
    client
        .call(Request::Create {
            project: project.clone(),
        })
        .await?;
    let watch = client.watch(None, 0).await?;
    let render_client = client.clone();
    let job_id = Uuid::new_v4();
    let output = directory.path().join("output.mp4");
    let rendering = tokio::spawn(async move {
        render_client
            .call(Request::Render {
                output,
                expected_revision: Some(0),
                tools: ToolConfig::default(),
                job_id,
            })
            .await
    });
    let events = client.watch(Some(watch.sequence), 2000).await?;
    assert!(
        events
            .events
            .iter()
            .any(|event| event.kind == "job_queued" || event.kind == "job_running")
    );
    assert!(
        !rendering.is_finished(),
        "fixture must still be rendering when responsive edit begins"
    );
    let started = Instant::now();
    let updated = client
        .execute(
            trim(&project, 1000),
            Some(0),
            None,
            false,
            ToolConfig::default(),
        )
        .await?;
    assert_eq!(updated["revision"], 1);
    assert_eq!(client.inspect().await?.revision, 1);
    assert!(started.elapsed() < Duration::from_secs(2));
    eprintln!("IPC edit+inspect during render: {:?}", started.elapsed());
    let rendered = rendering
        .await
        .map_err(|error| Error::Io(std::io::Error::other(error)))??;
    assert_eq!(rendered["project_revision"], 0);
    assert_eq!(rendered["frames"], 1800);
    assert_eq!(rendered["verified_full_decode"], true);
    let finished = client.watch(Some(events.sequence), 0).await?;
    assert!(
        finished
            .events
            .iter()
            .any(|event| event.kind == "job_completed" && event.revision == Some(0))
    );
    let cancelled_output = directory.path().join("cancelled.mp4");
    let cancelled_path = cancelled_output.clone();
    let render_client = client.clone();
    let cancelled_id = Uuid::new_v4();
    let cursor = finished.sequence;
    let cancelling = tokio::spawn(async move {
        render_client
            .call(Request::Render {
                output: cancelled_output,
                expected_revision: Some(1),
                tools: ToolConfig::default(),
                job_id: cancelled_id,
            })
            .await
    });
    let queued = client.watch(Some(cursor), 2000).await?;
    assert!(
        queued
            .events
            .iter()
            .any(|event| event.data["job_id"] == cancelled_id.to_string())
    );
    client
        .call(Request::Cancel {
            job_id: cancelled_id,
        })
        .await?;
    let cancelled = cancelling
        .await
        .map_err(|error| Error::Io(std::io::Error::other(error)))?;
    assert!(matches!(cancelled, Err(Error::Io(_))));
    assert!(!cancelled_path.exists());
    assert!(
        client
            .watch(Some(queued.sequence), 0)
            .await?
            .events
            .iter()
            .any(|event| event.kind == "job_cancelled")
    );
    stop(task).await;
    Ok(())
}
