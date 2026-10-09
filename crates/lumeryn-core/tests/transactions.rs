use lumeryn_core::{
    Batch, Edit, Error, MediaTools, Project, Result, Store,
    media::{Metadata, Video},
    model::Media,
};
use uuid::Uuid;

fn fixture() -> Result<(Project, Uuid)> {
    let mut project = Project::new("test".into(), 320, 180, "30000/1001".parse()?)?;
    let id = Uuid::new_v4();
    project.media.push(Media {
        id,
        path: "source.mp4".into(),
        metadata: Metadata {
            size_bytes: 1,
            blake3: None,
            audio: None,
            video: Some(Video {
                codec: "mpeg4".into(),
                width: 320,
                height: 180,
                frame_rate: project.frame_rate,
                frames: 300,
                constant_rate: true,
                start_time_micros: 0,
                sample_aspect_ratio: "1:1".into(),
                color_transfer: None,
            }),
        },
    });
    Ok((project, id))
}

fn add(media_id: Uuid, id: Uuid, duration_frames: u64) -> Edit {
    Edit::AddClip {
        id,
        media_id,
        at_frame: 0,
        source_in_frame: 0,
        duration_frames,
    }
}

#[tokio::test]
async fn prepared_edits_bind_project_and_commands_and_recheck_revision() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("project.lumeryn");
    let (project, media_id) = fixture()?;
    let mut store = Store::create(&path, &project).await?;
    let batch = Batch {
        operations: vec![add(media_id, Uuid::new_v4(), 60)],
    };
    let tools = MediaTools::default();
    let prepared = lumeryn_core::storage::prepare_imports(&path, &batch, &tools).await?;
    let different = Batch {
        operations: vec![add(media_id, Uuid::new_v4(), 30)],
    };
    assert!(matches!(
        store
            .execute_prepared(&different, Some(0), None, false, prepared)
            .await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(store.inspect().await?.revision, 0);
    let stale = lumeryn_core::storage::prepare_imports(&path, &batch, &tools).await?;
    store
        .execute(&different, Some(0), None, false, &tools)
        .await?;
    assert!(matches!(
        store
            .execute_prepared(&batch, Some(0), None, false, stale)
            .await,
        Err(Error::Conflict { current: 1, .. })
    ));
    let other_path = dir.path().join("other.lumeryn");
    let mut other = Store::create(&other_path, &project).await?;
    let wrong_project = lumeryn_core::storage::prepare_imports(&path, &batch, &tools).await?;
    assert!(matches!(
        other
            .execute_prepared(&batch, Some(0), None, false, wrong_project)
            .await,
        Err(Error::Invalid(_))
    ));
    assert_eq!(other.inspect().await?.revision, 0);
    Ok(())
}

#[tokio::test]
async fn durable_edits_dry_run_rollback_and_idempotency() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("project.lumeryn");
    let (project, media_id) = fixture()?;
    let mut store = Store::create(&path, &project).await?;
    assert_eq!(store.sqlite_version().await?, "3.51.3");
    let clip_id = Uuid::new_v4();
    let batch = Batch {
        operations: vec![add(media_id, clip_id, 60)],
    };
    let tools = MediaTools::default();
    assert!(
        store
            .execute(&batch, Some(0), None, true, &tools)
            .await?
            .dry_run
    );
    assert_eq!(store.inspect().await?.revision, 0);
    let receipt = store
        .execute(&batch, Some(0), Some("add"), false, &tools)
        .await?;
    assert_eq!(receipt.revision, 1);
    let replay = store
        .execute(&batch, Some(0), Some("add"), false, &tools)
        .await?;
    assert!(replay.replayed);
    assert_eq!(replay.object_ids, vec![clip_id]);
    assert!(matches!(
        store.execute(&batch, Some(0), None, false, &tools).await,
        Err(Error::Conflict { current: 1, .. })
    ));
    let bad = Batch {
        operations: vec![
            Edit::TrimClip {
                clip_id,
                source_in_frame: 15,
                duration_frames: 30,
            },
            add(media_id, Uuid::new_v4(), 999),
        ],
    };
    assert!(
        store
            .execute(&bad, Some(1), None, false, &tools)
            .await
            .is_err()
    );
    assert_eq!(store.inspect().await?.clips[0].duration_frames, 60);
    assert_eq!(store.history().await?.len(), 2);
    let changed = Batch {
        operations: vec![add(media_id, clip_id, 59)],
    };
    assert!(
        store
            .execute(&changed, None, Some("add"), false, &tools)
            .await
            .is_err()
    );
    let trim = Batch {
        operations: vec![Edit::TrimClip {
            clip_id,
            source_in_frame: 30,
            duration_frames: 30,
        }],
    };
    store.execute(&trim, Some(1), None, false, &tools).await?;
    drop(store);
    let mut reopened = Store::open(&path).await?;
    let snapshot = reopened.inspect().await?;
    assert_eq!(snapshot.revision, 2);
    assert_eq!(snapshot.clips[0].source_in_frame, 30);
    assert_eq!(snapshot.clips[0].duration_frames, 30);
    Ok(())
}

#[tokio::test]
async fn two_writers_cannot_silently_overwrite() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("project.lumeryn");
    let (project, media) = fixture()?;
    let mut first = Store::create(&path, &project).await?;
    let mut second = Store::open(&path).await?;
    let a = Batch {
        operations: vec![add(media, Uuid::new_v4(), 10)],
    };
    let b = Batch {
        operations: vec![add(media, Uuid::new_v4(), 20)],
    };
    let tools = MediaTools::default();
    let (a, b) = tokio::join!(
        first.execute(&a, Some(0), None, false, &tools),
        second.execute(&b, Some(0), None, false, &tools)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let error = if let Err(error) = a {
        error
    } else {
        b.err()
            .ok_or_else(|| Error::Invalid("expected conflict".into()))?
    };
    assert!(matches!(error, Error::Conflict { current: 1, .. }));
    assert_eq!(first.inspect().await?.clips.len(), 1);
    Ok(())
}

#[tokio::test]
async fn rejects_existing_invalid_and_future_schema_projects() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("project.lumeryn");
    let (mut project, _) = fixture()?;
    let _store = Store::create(&path, &project).await?;
    assert!(matches!(
        Store::create(&path, &project).await,
        Err(Error::Exists(_))
    ));
    project.schema_version = 2;
    assert!(matches!(project.validate(), Err(Error::Unsupported(_))));
    assert!(Project::new("bad".into(), 319, 180, "30".parse()?).is_err());
    let mut connection = <sqlx::SqliteConnection as sqlx::Connection>::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new().filename(&path),
    )
    .await?;
    sqlx::query("UPDATE project SET schema_version=99")
        .execute(&mut connection)
        .await?;
    assert!(matches!(
        Store::open(&path).await,
        Err(Error::Unsupported(_))
    ));
    Ok(())
}
