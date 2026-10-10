use lumeryn_core::{
    Batch, Edit, Error, MediaTools, Project, Result, Store,
    media::{Audio, Metadata, Video},
    model::{AudioClip, Clip, Media},
};
use uuid::Uuid;

fn fixture() -> Result<(Project, Uuid)> {
    let mut project = Project::new("Audio editing".into(), 320, 180, "30000/1001".parse()?)?;
    let video_id = Uuid::new_v4();
    let audio_id = Uuid::new_v4();
    project.media.push(Media {
        id: video_id,
        path: "video.bin".into(),
        alias: None,
        metadata: Metadata {
            size_bytes: 1,
            blake3: None,
            audio: None,
            video: Some(Video {
                codec: "fixture".into(),
                width: 320,
                height: 180,
                frame_rate: project.frame_rate,
                frames: 300,
                start_time_micros: 0,
                sample_aspect_ratio: "1:1".into(),
                color_transfer: None,
                constant_rate: true,
            }),
        },
    });
    project.clips.push(Clip {
        id: Uuid::new_v4(),
        media_id: video_id,
        track_id: project.video_track_id,
        at_frame: 0,
        source_in_frame: 0,
        duration_frames: 150,
    });
    project.media.push(Media {
        id: audio_id,
        path: "audio.bin".into(),
        alias: None,
        metadata: Metadata {
            size_bytes: 1,
            blake3: Some("a".repeat(64)),
            video: None,
            audio: Some(Audio {
                codec: "pcm".into(),
                sample_rate: 44_100,
                channels: 1,
                start_time_micros: 0,
                canonical_samples_48k: Some(480_000),
            }),
        },
    });
    project.validate()?;
    Ok((project, audio_id))
}

fn add(id: Uuid, media_id: Uuid, at_frame: u64, duration_frames: u64) -> Edit {
    Edit::AddAudioClip {
        id,
        media_id,
        at_frame,
        source_in_sample: 23,
        duration_frames,
        gain_millidecibels: -6_000,
        muted: false,
    }
}

async fn edit(store: &mut Store, operation: Edit, revision: u64, dry_run: bool) -> Result<()> {
    store
        .execute(
            &Batch {
                operations: vec![operation],
            },
            Some(revision),
            None,
            dry_run,
            &MediaTools::default(),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn audio_edits_overlap_independently_and_restore_as_one_revision() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (original, source) = fixture()?;
    let video = serde_json::to_value(&original.clips)?;
    let mut store = Store::create(&directory.path().join("project.lumeryn"), &original).await?;
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    edit(&mut store, add(first, source, 10, 60), 0, true).await?;
    assert!(store.inspect().await?.audio_clips.is_empty());
    edit(&mut store, add(first, source, 10, 60), 0, false).await?;
    edit(&mut store, add(second, source, 20, 40), 1, false).await?;
    edit(
        &mut store,
        Edit::MoveAudioClip {
            clip_id: first,
            at_frame: 30,
        },
        2,
        false,
    )
    .await?;
    edit(
        &mut store,
        Edit::TrimAudioClip {
            clip_id: first,
            source_in_sample: 42,
            duration_frames: 50,
        },
        3,
        false,
    )
    .await?;
    edit(
        &mut store,
        Edit::SetAudioProperties {
            clip_id: first,
            gain_millidecibels: 12_000,
            muted: true,
        },
        4,
        false,
    )
    .await?;
    let before_delete = store.inspect().await?;
    assert_eq!(serde_json::to_value(&before_delete.clips)?, video);
    assert_eq!(before_delete.audio_clips[0].source_in_sample, 42);
    assert!(before_delete.audio_clips[0].muted);
    edit(
        &mut store,
        Edit::DeleteAudioClip { clip_id: first },
        5,
        false,
    )
    .await?;
    assert_eq!(store.inspect().await?.audio_clips[0].id, second);
    edit(&mut store, Edit::Undo {}, 6, false).await?;
    let restored = store.inspect().await?;
    assert_eq!(
        serde_json::to_value(&restored.audio_clips)?,
        serde_json::to_value(&before_delete.audio_clips)?
    );
    edit(&mut store, Edit::Redo {}, 7, false).await?;
    assert_eq!(store.inspect().await?.audio_clips.len(), 1);
    assert_eq!(store.integrity_check().await?["history"], "ok");
    Ok(())
}

#[tokio::test]
async fn invalid_audio_writes_roll_back_and_used_source_removal_fails() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (original, source) = fixture()?;
    let video = original.clips[0].id;
    let mut store = Store::create(&directory.path().join("project.lumeryn"), &original).await?;
    let id = Uuid::new_v4();
    edit(&mut store, add(id, source, 100, 50), 0, false).await?;
    let snapshot = serde_json::to_value(store.inspect().await?)?;
    for operation in [
        Edit::MoveAudioClip {
            clip_id: id,
            at_frame: 101,
        },
        Edit::TrimAudioClip {
            clip_id: id,
            source_in_sample: 480_000,
            duration_frames: 1,
        },
        Edit::SetAudioProperties {
            clip_id: id,
            gain_millidecibels: 12_001,
            muted: false,
        },
        Edit::TrimClip {
            clip_id: video,
            source_in_frame: 0,
            duration_frames: 149,
        },
        Edit::RemoveMedia { media_id: source },
        add(id, source, 0, 1),
    ] {
        assert!(edit(&mut store, operation, 1, false).await.is_err());
        assert_eq!(serde_json::to_value(store.inspect().await?)?, snapshot);
    }
    assert!(matches!(
        edit(&mut store, Edit::DeleteAudioClip { clip_id: id }, 0, false).await,
        Err(Error::Conflict { .. })
    ));
    store
        .execute(
            &Batch {
                operations: vec![
                    Edit::DeleteAudioClip { clip_id: id },
                    Edit::RemoveMedia { media_id: source },
                ],
            },
            Some(1),
            Some("remove-audio"),
            false,
            &MediaTools::default(),
        )
        .await?;
    edit(&mut store, Edit::Undo {}, 2, false).await?;
    assert_eq!(store.inspect().await?.audio_clips.len(), 1);
    assert_eq!(store.inspect().await?.media.len(), 2);
    let replay = store
        .execute(
            &Batch {
                operations: vec![
                    Edit::DeleteAudioClip { clip_id: id },
                    Edit::RemoveMedia { media_id: source },
                ],
            },
            Some(0),
            Some("remove-audio"),
            false,
            &MediaTools::default(),
        )
        .await?;
    assert!(replay.replayed);
    assert_eq!(replay.revision, 2);
    Ok(())
}

#[test]
fn audio_validation_uses_absolute_sample_boundaries_and_strict_commands() -> Result<()> {
    let (mut project, source) = fixture()?;
    let track = project
        .audio_track_id
        .ok_or_else(|| Error::Invalid("missing track".into()))?;
    let length = lumeryn_core::audio_plan::sample_boundary(project.frame_rate, 2)?
        - lumeryn_core::audio_plan::sample_boundary(project.frame_rate, 1)?;
    project.media[1]
        .metadata
        .audio
        .as_mut()
        .ok_or_else(|| Error::Invalid("missing audio".into()))?
        .canonical_samples_48k = Some(length);
    project.audio_clips.push(AudioClip {
        id: Uuid::new_v4(),
        media_id: source,
        track_id: track,
        at_frame: 1,
        source_in_sample: 0,
        duration_frames: 1,
        gain_millidecibels: -60_000,
        muted: false,
    });
    project.validate()?;
    project.audio_clips[0].source_in_sample = 1;
    assert!(project.validate().is_err());
    project.audio_clips[0].source_in_sample = 0;
    project.audio_clips[0].id = project.video_track_id;
    assert!(project.validate().is_err());
    assert!(serde_json::from_value::<Edit>(serde_json::json!({"operation":"set_audio_properties","clip_id":Uuid::new_v4(),"gain_millidecibels":0})).is_err());
    assert!(serde_json::from_value::<Edit>(serde_json::json!({"operation":"move_audio_clip","clip_id":Uuid::new_v4(),"at_frame":0,"ripple":true})).is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "requires configured FFmpeg and ffprobe"]
async fn lazy_sample_proof_is_transactional_and_bound_to_source_identity() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let tools = MediaTools::default();
    let first_path = directory.path().join("first.wav");
    let second_path = directory.path().join("second.wav");
    for (path, frequency) in [(&first_path, 440), (&second_path, 880)] {
        tools
            .run(
                &tools.ffmpeg,
                &[
                    "-nostdin".into(),
                    "-v".into(),
                    "error".into(),
                    "-f".into(),
                    "lavfi".into(),
                    "-i".into(),
                    format!("sine=frequency={frequency}:sample_rate=44100:duration=1").into(),
                    "-c:a".into(),
                    "pcm_s16le".into(),
                    path.as_os_str().to_owned(),
                ],
            )
            .await?;
    }
    let (mut original, source) = fixture()?;
    original.media[1].path = "first.wav".into();
    original.media[1].metadata = tools.probe(&first_path).await?;
    original.media[1]
        .metadata
        .audio
        .as_mut()
        .ok_or_else(|| Error::Invalid("missing audio".into()))?
        .canonical_samples_48k = None;
    let path = directory.path().join("project.lumeryn");
    let mut store = Store::create(&path, &original).await?;
    let batch = Batch {
        operations: vec![add(Uuid::new_v4(), source, 0, 15)],
    };
    store.execute(&batch, Some(0), None, true, &tools).await?;
    let unchanged = store.inspect().await?;
    assert!(unchanged.audio_clips.is_empty());
    assert!(
        unchanged.media[1]
            .metadata
            .audio
            .as_ref()
            .is_some_and(|audio| audio.canonical_samples_48k.is_none())
    );
    let prepared = lumeryn_core::storage::prepare_imports(&path, &batch, &tools).await?;
    store
        .execute(
            &Batch {
                operations: vec![
                    Edit::RemoveMedia { media_id: source },
                    Edit::Import {
                        id: source,
                        path: second_path,
                    },
                ],
            },
            Some(0),
            None,
            false,
            &tools,
        )
        .await?;
    assert!(
        store
            .execute_prepared(&batch, Some(1), None, false, prepared)
            .await
            .is_err()
    );
    assert!(store.inspect().await?.audio_clips.is_empty());
    let final_batch = Batch {
        operations: vec![add(Uuid::new_v4(), source, 0, 15)],
    };
    store
        .execute(&final_batch, Some(1), None, false, &tools)
        .await?;
    let final_project = store.inspect().await?;
    assert_eq!(final_project.audio_clips.len(), 1);
    assert_eq!(
        final_project.media[1]
            .metadata
            .audio
            .as_ref()
            .and_then(|audio| audio.canonical_samples_48k),
        Some(48_000)
    );
    store.integrity_check().await?;
    Ok(())
}

#[tokio::test]
async fn combined_audio_range_validates_final_position_and_duration_atomically() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (project, source) = fixture()?;
    let mut store = Store::create(&directory.path().join("project.lumeryn"), &project).await?;
    let id = Uuid::new_v4();
    edit(&mut store, add(id, source, 100, 50), 0, false).await?;
    // Lengthening at the old late position would exceed visual duration.
    edit(
        &mut store,
        Edit::SetAudioRange {
            clip_id: id,
            at_frame: 0,
            source_in_sample: 0,
            duration_frames: 150,
        },
        1,
        false,
    )
    .await?;
    assert_eq!(store.inspect().await?.audio_clips[0].duration_frames, 150);
    // Moving the long clip first would exceed duration; shorten and move together.
    edit(
        &mut store,
        Edit::SetAudioRange {
            clip_id: id,
            at_frame: 100,
            source_in_sample: 100,
            duration_frames: 50,
        },
        2,
        false,
    )
    .await?;
    let current = serde_json::to_value(store.inspect().await?)?;
    assert!(
        edit(
            &mut store,
            Edit::SetAudioRange {
                clip_id: id,
                at_frame: 100,
                source_in_sample: 0,
                duration_frames: 51
            },
            3,
            false
        )
        .await
        .is_err()
    );
    assert_eq!(serde_json::to_value(store.inspect().await?)?, current);
    store.integrity_check().await?;
    Ok(())
}
