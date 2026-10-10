//! Project-local names/removal: real SQLite transactions, synthetic source metadata.
//! These tests deliberately require no probing process or desktop window.
use lumeryn_core::{
    Batch, Edit, Error, MediaTools, Project, Result, Store,
    clipboard::Clipboard,
    media::{Audio, Metadata, Video},
    model::{Clip, Media},
    storage::Receipt,
};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{fs, path::Path, time::Instant};
use uuid::Uuid;

const SOURCE: &[u8] = b"unchanged source bytes";

fn fixture() -> Result<Project> {
    let mut project = Project::new("Media management".into(), 320, 180, "30".parse()?)?;
    let video = Video {
        codec: "mpeg4".into(),
        width: 320,
        height: 180,
        frame_rate: project.frame_rate,
        frames: 600,
        start_time_micros: 0,
        sample_aspect_ratio: "1:1".into(),
        color_transfer: None,
        constant_rate: true,
    };
    let original = Media {
        id: Uuid::new_v4(),
        path: "opening_shot.mp4".into(),
        alias: None,
        metadata: Metadata {
            size_bytes: SOURCE.len() as u64,
            blake3: Some(blake3::hash(SOURCE).to_hex().to_string()),
            video: Some(video.clone()),
            audio: None,
        },
    };
    let mut duplicate = original.clone();
    duplicate.id = Uuid::new_v4();
    project.clips.push(Clip {
        id: Uuid::new_v4(),
        media_id: original.id,
        track_id: project.video_track_id,
        at_frame: 0,
        source_in_frame: 10,
        duration_frames: 60,
    });
    project.media = vec![
        original,
        duplicate,
        Media {
            id: Uuid::new_v4(),
            path: "music.ogg".into(),
            alias: None,
            metadata: Metadata {
                size_bytes: 1,
                blake3: None,
                video: None,
                audio: Some(Audio {
                    codec: "vorbis".into(),
                    sample_rate: 48_000,
                    channels: 2,
                    start_time_micros: 0,
                    canonical_samples_48k: None,
                }),
            },
        },
        Media {
            id: Uuid::new_v4(),
            path: "illustration.png".into(),
            alias: None,
            metadata: Metadata {
                size_bytes: 1,
                blake3: None,
                video: Some(Video {
                    codec: "png".into(),
                    frames: 1,
                    ..video
                }),
                audio: None,
            },
        },
    ];
    project.validate()?;
    Ok(project)
}

fn batch(operation: Edit) -> Batch {
    Batch {
        operations: vec![operation],
    }
}

async fn execute(store: &mut Store, operation: Edit, revision: u64) -> Result<Receipt> {
    store
        .execute(
            &batch(operation),
            Some(revision),
            None,
            false,
            &MediaTools::default(),
        )
        .await
}

async fn connection(path: &Path) -> Result<SqliteConnection> {
    Ok(SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(path)).await?)
}

fn same_graph(mut actual: Project, mut expected: Project) -> Result<()> {
    actual.revision = 0;
    expected.revision = 0;
    assert_eq!(
        serde_json::to_value(actual)?,
        serde_json::to_value(expected)?
    );
    Ok(())
}

#[tokio::test]
#[ignore = "bounded synthetic 10000-media transaction measurement"]
async fn bounded_media_graph_transaction_measurement() -> Result<()> {
    for count in [4, 10_000] {
        let directory = tempfile::tempdir()?;
        let mut project = fixture()?;
        project.clips.clear();
        let template = project.media[0].clone();
        project.media = (0..count)
            .map(|index| Media {
                id: Uuid::new_v4(),
                path: format!("source_{index}.mp4").into(),
                ..template.clone()
            })
            .collect();
        let media_id = project.media[0].id;
        let snapshot_bytes = serde_json::to_vec(&project)?.len();
        let mut store = Store::create(&directory.path().join("project.lumeryn"), &project).await?;
        let rename_start = Instant::now();
        execute(
            &mut store,
            Edit::RenameMedia {
                media_id,
                alias: Some("Opening 🎬".into()),
            },
            0,
        )
        .await?;
        let rename_ms = rename_start.elapsed().as_secs_f64() * 1000.0;
        let remove_start = Instant::now();
        execute(&mut store, Edit::RemoveMedia { media_id }, 1).await?;
        let remove_ms = remove_start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(store.inspect().await?.media.len(), count - 1);
        assert!(store.search("Opening", 10).await?.is_empty());
        store.integrity_check().await?;
        println!(
            "synthetic_media_count={count} snapshot_bytes={snapshot_bytes} rename_ms={rename_ms:.3} remove_ms={remove_ms:.3}"
        );
    }
    Ok(())
}

#[test]
fn rename_command_requires_alias_presence_and_schema_preserves_null_reset() -> Result<()> {
    let media_id = Uuid::new_v4();
    let missing = serde_json::json!({"operation":"rename_media","media_id":media_id});
    assert!(serde_json::from_value::<Edit>(missing.clone()).is_err());
    assert!(serde_json::from_value::<Batch>(serde_json::json!({"operations":[missing]})).is_err());
    for alias in [serde_json::Value::Null, serde_json::json!("Opening shot")] {
        let value =
            serde_json::json!({"operation":"rename_media","media_id":media_id,"alias":alias});
        let decoded: Edit = serde_json::from_value(value.clone())?;
        assert!(matches!(
            &decoded,
            Edit::RenameMedia { media_id: decoded_id, alias: decoded_alias }
                if *decoded_id == media_id && serde_json::to_value(decoded_alias)? == alias
        ));
        assert_eq!(serde_json::to_value(decoded)?, value);
        serde_json::from_value::<Batch>(serde_json::json!({"operations":[value]}))?;
    }
    let schema = serde_json::to_value(schemars::schema_for!(Batch))?;
    let rename = schema["$defs"]["Edit"]["oneOf"]
        .as_array()
        .and_then(|variants| {
            variants
                .iter()
                .find(|variant| variant["properties"]["operation"]["const"] == "rename_media")
        })
        .ok_or_else(|| Error::Invalid("schema omits rename_media command".into()))?;
    assert!(
        rename["required"]
            .as_array()
            .is_some_and(|required| { required.contains(&serde_json::json!("alias")) })
    );
    assert_eq!(
        rename["properties"]["alias"]["type"],
        serde_json::json!(["string", "null"])
    );
    Ok(())
}

#[test]
fn aliases_are_bounded_display_text_and_absent_fields_keep_legacy_shape() -> Result<()> {
    let mut project = fixture()?;
    project.schema_version = 1;
    project.audio_track_id = None;
    project.audio_clips.clear();
    assert_eq!(project.media[0].display_name(), "opening shot");
    let legacy = serde_json::to_value(&project)?;
    assert!(legacy["media"][0].get("alias").is_none());
    assert!(legacy.get("audio_track_id").is_none());
    assert!(legacy.get("audio_clips").is_none());
    assert!(
        legacy["media"][2]["metadata"]["audio"]
            .get("canonical_samples_48k")
            .is_none()
    );
    let roundtrip: Project = serde_json::from_value(legacy.clone())?;
    assert!(roundtrip.media.iter().all(|media| media.alias.is_none()));
    assert_eq!(serde_json::to_value(roundtrip)?, legacy);
    for invalid in [
        "".to_owned(),
        " \u{3000} ".to_owned(),
        "a\nname".to_owned(),
        "a\tname".to_owned(),
        "a\0name".to_owned(),
        "a\u{7f}name".to_owned(),
        "a\u{85}name".to_owned(),
        "x".repeat(257),
        "é".repeat(129),
    ] {
        project.media[0].alias = Some(invalid);
        assert!(matches!(project.validate(), Err(Error::Invalid(_))));
    }
    for valid in [
        "  अध्याय – 開始 🎬  ".to_owned(),
        "../still only a label; \"OR\" * : / \\".to_owned(),
        "é".repeat(128),
    ] {
        project.media[0].alias = Some(valid.clone());
        project.media[1].alias = Some(valid.clone());
        project.validate()?;
        assert_eq!(project.media[0].display_name(), valid);
    }
    project.media[0].alias = None;
    project.media[0].path = "/".into();
    assert_eq!(project.media[0].display_name(), "Media");
    let reset = serde_json::to_value(Edit::RenameMedia {
        media_id: project.media[0].id,
        alias: None,
    })?;
    assert!(reset["alias"].is_null());
    let mut unknown = legacy;
    unknown["media"][0]["not_a_media_field"] = true.into();
    assert!(serde_json::from_value::<Project>(unknown).is_err());
    Ok(())
}

#[tokio::test]
async fn rename_is_revision_checked_atomic_dry_run_and_exactly_replayed() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let original = fixture()?;
    let media_id = original.media[0].id;
    let mut store = Store::create(&path, &original).await?;
    let rename = batch(Edit::RenameMedia {
        media_id,
        alias: Some("  Opening 🎬  ".into()),
    });
    let dry = store
        .execute(
            &rename,
            Some(0),
            Some("rename"),
            true,
            &MediaTools::default(),
        )
        .await?;
    assert!(dry.dry_run && !dry.replayed);
    assert_eq!(dry.revision, 0);
    assert_eq!(dry.object_ids, vec![media_id]);
    same_graph(store.inspect().await?, original.clone())?;
    let committed = store
        .execute(
            &rename,
            Some(0),
            Some("rename"),
            false,
            &MediaTools::default(),
        )
        .await?;
    assert_eq!(committed.revision, 1);
    assert_eq!(committed.object_ids, vec![media_id]);
    assert!(!committed.replayed);
    let renamed = store.inspect().await?;
    assert_eq!(renamed.media[0].alias.as_deref(), Some("  Opening 🎬  "));
    assert_eq!(renamed.media[0].path, original.media[0].path);
    assert_eq!(
        serde_json::to_value(&renamed.media[0].metadata)?,
        serde_json::to_value(&original.media[0].metadata)?
    );
    let replay = store
        .execute(
            &rename,
            Some(0),
            Some("rename"),
            false,
            &MediaTools::default(),
        )
        .await?;
    assert!(replay.replayed);
    assert_eq!(replay.revision, 1);
    assert_eq!(replay.object_ids, vec![media_id]);
    assert!(matches!(
        execute(
            &mut store,
            Edit::RenameMedia {
                media_id,
                alias: None
            },
            0
        )
        .await,
        Err(Error::Conflict {
            expected: 0,
            current: 1
        })
    ));
    let changed = batch(Edit::RenameMedia {
        media_id,
        alias: None,
    });
    assert!(matches!(
        store
            .execute(
                &changed,
                Some(1),
                Some("rename"),
                false,
                &MediaTools::default()
            )
            .await,
        Err(Error::Invalid(_))
    ));
    for operation in [
        Edit::RenameMedia {
            media_id: Uuid::new_v4(),
            alias: Some("missing".into()),
        },
        Edit::RemoveMedia {
            media_id: Uuid::new_v4(),
        },
    ] {
        assert!(matches!(
            execute(&mut store, operation, 1).await,
            Err(Error::NotFound(_))
        ));
    }
    let failing = Batch {
        operations: vec![
            Edit::RenameMedia {
                media_id,
                alias: Some("rollback".into()),
            },
            Edit::RenameMedia {
                media_id: original.media[1].id,
                alias: Some("\n".into()),
            },
        ],
    };
    assert!(matches!(
        store
            .execute(
                &failing,
                Some(1),
                Some("failed-batch"),
                false,
                &MediaTools::default()
            )
            .await,
        Err(Error::Invalid(_))
    ));
    same_graph(store.inspect().await?, renamed)?;
    assert_eq!(store.history().await?.len(), 2);
    let mut conn = connection(&path).await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM receipts WHERE key='failed-batch'")
            .fetch_one(&mut conn)
            .await?,
        0
    );
    conn.close().await?;
    store.integrity_check().await?;
    Ok(())
}

#[tokio::test]
async fn alias_search_reset_and_history_restore_preserve_source_bytes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("opening_shot.mp4");
    fs::write(&source, SOURCE)?;
    let original = fixture()?;
    let media_id = original.media[0].id;
    let mut store = Store::create(&directory.path().join("project.lumeryn"), &original).await?;
    let alias = "  अध्याय \"alpha OR beta\"  ";
    execute(
        &mut store,
        Edit::RenameMedia {
            media_id,
            alias: Some(alias.into()),
        },
        0,
    )
    .await?;
    for query in ["अध्याय", "alpha OR beta", "\"alpha OR beta\""] {
        let found = store.search(query, 10).await?;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0]["object_id"], media_id.to_string());
        assert!(
            found[0]["text"]
                .as_str()
                .is_some_and(|text| text.ends_with(alias))
        );
    }
    assert!(store.search("alpha NOT beta", 10).await?.is_empty());
    assert_eq!(store.search("opening", 10).await?.len(), 2);
    execute(
        &mut store,
        Edit::RenameMedia {
            media_id,
            alias: None,
        },
        1,
    )
    .await?;
    assert!(store.search("अध्याय", 10).await?.is_empty());
    assert_eq!(
        store.inspect().await?.media[0].display_name(),
        "opening shot"
    );
    execute(&mut store, Edit::Undo {}, 2).await?;
    assert_eq!(store.inspect().await?.media[0].display_name(), alias);
    assert_eq!(store.search("alpha OR beta", 10).await?.len(), 1);
    execute(&mut store, Edit::Redo {}, 3).await?;
    assert!(store.search("alpha OR beta", 10).await?.is_empty());
    execute(&mut store, Edit::Undo {}, 4).await?;
    execute(&mut store, Edit::Undo {}, 5).await?;
    same_graph(store.inspect().await?, original)?;
    assert_eq!(store.inspect().await?.revision, 6);
    assert_eq!(store.history().await?.len(), 7);
    assert_eq!(fs::read(source)?, SOURCE);
    store.integrity_check().await?;
    Ok(())
}

#[tokio::test]
async fn removal_restricts_current_usage_and_explicit_batch_restores_same_ids() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let source = directory.path().join("opening_shot.mp4");
    fs::write(&source, SOURCE)?;
    let mut original = fixture()?;
    original.media[0].alias = Some("Opening".into());
    let media_id = original.media[0].id;
    let duplicate_id = original.media[1].id;
    let clip_id = original.clips[0].id;
    let mut store = Store::create(&path, &original).await?;
    let blocked = execute(&mut store, Edit::RemoveMedia { media_id }, 0).await;
    assert!(
        matches!(blocked, Err(Error::Invalid(message)) if message.contains("1 clip(s)") && message.contains("remove its clips first"))
    );
    same_graph(store.inspect().await?, original.clone())?;
    let request = Batch {
        operations: vec![
            Edit::DeleteClip {
                clip_id,
                ripple: false,
            },
            Edit::RemoveMedia { media_id },
        ],
    };
    let dry = store
        .execute(
            &request,
            Some(0),
            Some("remove"),
            true,
            &MediaTools::default(),
        )
        .await?;
    assert_eq!(dry.object_ids, vec![clip_id, media_id]);
    assert_eq!(dry.revision, 0);
    same_graph(store.inspect().await?, original.clone())?;
    let committed = store
        .execute(
            &request,
            Some(0),
            Some("remove"),
            false,
            &MediaTools::default(),
        )
        .await?;
    assert_eq!(committed.revision, 1);
    assert_eq!(committed.object_ids, vec![clip_id, media_id]);
    let removed = store.inspect().await?;
    assert!(removed.clips.is_empty());
    assert!(!removed.media.iter().any(|media| media.id == media_id));
    assert!(removed.media.iter().any(|media| media.id == duplicate_id));
    assert!(
        store
            .search("Opening", 10)
            .await?
            .iter()
            .all(|record| record["object_id"] != media_id.to_string())
    );
    let replay = store
        .execute(
            &request,
            Some(0),
            Some("remove"),
            false,
            &MediaTools::default(),
        )
        .await?;
    assert!(replay.replayed);
    assert_eq!(replay.revision, 1);
    execute(&mut store, Edit::Undo {}, 1).await?;
    same_graph(store.inspect().await?, original)?;
    execute(&mut store, Edit::Redo {}, 2).await?;
    same_graph(store.inspect().await?, removed)?;
    let remaining = store.inspect().await?;
    for (index, media) in remaining.media.iter().enumerate() {
        let receipt = execute(
            &mut store,
            Edit::RemoveMedia { media_id: media.id },
            3 + index as u64,
        )
        .await?;
        assert_eq!(receipt.object_ids, vec![media.id]);
    }
    assert!(store.inspect().await?.media.is_empty());
    let mut reopened = Store::open(&path).await?;
    assert!(reopened.inspect().await?.media.is_empty());
    execute(&mut reopened, Edit::Undo {}, 6).await?;
    assert_eq!(reopened.inspect().await?.media[0].id, remaining.media[2].id);
    assert_eq!(fs::read(source)?, SOURCE);
    reopened.integrity_check().await?;
    Ok(())
}

#[tokio::test]
async fn removal_rolls_back_prior_edits_and_two_clients_recheck_revisions() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("project.lumeryn");
    let original = fixture()?;
    let media_id = original.media[0].id;
    let unused_id = original.media[1].id;
    let mut first = Store::create(&path, &original).await?;
    let mut second = Store::open(&path).await?;
    let failing = Batch {
        operations: vec![
            Edit::RemoveMedia {
                media_id: unused_id,
            },
            Edit::RenameMedia {
                media_id,
                alias: Some("rollback".into()),
            },
            Edit::RemoveMedia { media_id },
        ],
    };
    assert!(matches!(
        first
            .execute(
                &failing,
                Some(0),
                Some("rollback"),
                false,
                &MediaTools::default()
            )
            .await,
        Err(Error::Invalid(_))
    ));
    same_graph(second.inspect().await?, original.clone())?;
    execute(
        &mut first,
        Edit::RemoveMedia {
            media_id: unused_id,
        },
        0,
    )
    .await?;
    assert!(matches!(
        execute(
            &mut second,
            Edit::RenameMedia {
                media_id,
                alias: Some("stale".into())
            },
            0
        )
        .await,
        Err(Error::Conflict {
            expected: 0,
            current: 1
        })
    ));
    assert_eq!(second.inspect().await?.revision, 1);
    execute(
        &mut second,
        Edit::RenameMedia {
            media_id,
            alias: Some("accepted".into()),
        },
        1,
    )
    .await?;
    assert_eq!(
        first.inspect().await?.media[0].alias.as_deref(),
        Some("accepted")
    );
    first.integrity_check().await?;
    Ok(())
}

#[tokio::test]
async fn absent_alias_legacy_snapshot_and_exact_search_survive_open_undo_and_backup() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("legacy.lumeryn");
    let mut original = fixture()?;
    original.schema_version = 1;
    original.audio_track_id = None;
    original.audio_clips.clear();
    let media_id = original.media[0].id;
    let mut created = Store::create(&path, &original).await?;
    created.integrity_check().await?;
    let mut conn = connection(&path).await?;
    let saved: String = sqlx::query_scalar("SELECT snapshot FROM history WHERE revision=0")
        .fetch_one(&mut conn)
        .await?;
    assert!(!saved.contains("\"alias\""));
    assert_eq!(saved, serde_json::to_string(&original)?);
    let text: String = sqlx::query_scalar("SELECT text FROM search_index WHERE object_id=?")
        .bind(media_id.to_string())
        .fetch_one(&mut conn)
        .await?;
    assert_eq!(text, "opening_shot.mp4 mpeg4 ");
    let mut store = Store::open(&path).await?;
    same_graph(store.inspect().await?, original.clone())?;
    store.integrity_check().await?;
    assert_eq!(store.search("opening", 10).await?.len(), 2);
    let backup = directory.path().join("legacy-backup.lumeryn");
    store.backup(&backup).await?;
    let mut backup_conn = connection(&backup).await?;
    let copied: String = sqlx::query_scalar("SELECT snapshot FROM history WHERE revision=0")
        .fetch_one(&mut backup_conn)
        .await?;
    assert_eq!(copied, saved);
    backup_conn.close().await?;
    assert!(matches!(
        execute(
            &mut store,
            Edit::RenameMedia {
                media_id,
                alias: Some("must migrate".into())
            },
            0
        )
        .await,
        Err(Error::Unsupported(_))
    ));
    let migration = store.migrate().await?;
    assert_eq!(migration["graph_from"], 1);
    assert_eq!(migration["graph_to"], lumeryn_core::model::SCHEMA_VERSION);
    let migrated = store.inspect().await?;
    original.schema_version = lumeryn_core::model::SCHEMA_VERSION;
    original.audio_track_id = migrated.audio_track_id;
    same_graph(migrated, original.clone())?;
    let migrated_saved: String =
        sqlx::query_scalar("SELECT snapshot FROM history WHERE revision=0")
            .fetch_one(&mut conn)
            .await?;
    assert_eq!(migrated_saved, serde_json::to_string(&original)?);
    execute(
        &mut store,
        Edit::RenameMedia {
            media_id,
            alias: Some("legacy opened".into()),
        },
        0,
    )
    .await?;
    execute(&mut store, Edit::Undo {}, 1).await?;
    same_graph(store.inspect().await?, original)?;
    let unchanged: String = sqlx::query_scalar("SELECT snapshot FROM history WHERE revision=0")
        .fetch_one(&mut conn)
        .await?;
    assert_eq!(unchanged, migrated_saved);
    let text_after: String = sqlx::query_scalar("SELECT text FROM search_index WHERE object_id=?")
        .bind(media_id.to_string())
        .fetch_one(&mut conn)
        .await?;
    assert_eq!(text_after, text);
    conn.close().await?;
    store.integrity_check().await?;
    let backup_directory = tempfile::tempdir()?;
    let cross_backup = backup_directory.path().join("rebased.lumeryn");
    store.backup(&cross_backup).await?;
    let mut rebased = Store::open(&cross_backup).await?;
    assert_eq!(
        rebased.inspect().await?.media[0].path,
        fs::canonicalize(directory.path())?.join("opening_shot.mp4")
    );
    rebased.integrity_check().await?;
    execute(&mut rebased, Edit::Redo {}, 2).await?;
    assert_eq!(
        rebased.inspect().await?.media[0].alias.as_deref(),
        Some("legacy opened")
    );
    Ok(())
}

#[tokio::test]
async fn clipboard_fresh_paste_requires_current_media_but_exact_retry_retains_receipt() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let original = fixture()?;
    let media_id = original.media[0].id;
    let original_clip = original.clips[0].id;
    let clipboard = Clipboard::copy(&original, original_clip)?;
    let pasted_clip = Uuid::new_v4();
    let paste = batch(Edit::PasteClip {
        id: pasted_clip,
        at_frame: 60,
        clipboard: clipboard.clone(),
    });
    let mut store = Store::create(&directory.path().join("project.lumeryn"), &original).await?;
    store
        .execute(
            &paste,
            Some(0),
            Some("paste"),
            false,
            &MediaTools::default(),
        )
        .await?;
    let remove = Batch {
        operations: vec![
            Edit::DeleteClip {
                clip_id: original_clip,
                ripple: false,
            },
            Edit::DeleteClip {
                clip_id: pasted_clip,
                ripple: false,
            },
            Edit::RemoveMedia { media_id },
        ],
    };
    store
        .execute(&remove, Some(1), None, false, &MediaTools::default())
        .await?;
    let retry = store
        .execute(
            &paste,
            Some(0),
            Some("paste"),
            false,
            &MediaTools::default(),
        )
        .await?;
    assert!(retry.replayed);
    assert_eq!(retry.revision, 1);
    assert_eq!(retry.object_ids, vec![pasted_clip]);
    assert_eq!(store.inspect().await?.revision, 2);
    let fresh = Edit::PasteClip {
        id: Uuid::new_v4(),
        at_frame: 120,
        clipboard,
    };
    assert!(matches!(
        execute(&mut store, fresh.clone(), 2).await,
        Err(Error::NotFound(_))
    ));
    execute(&mut store, Edit::Undo {}, 2).await?;
    assert_eq!(store.inspect().await?.clips.len(), 2);
    execute(&mut store, Edit::Redo {}, 3).await?;
    assert!(store.inspect().await?.clips.is_empty());
    execute(&mut store, Edit::Undo {}, 4).await?;
    execute(&mut store, fresh, 5).await?;
    assert_eq!(store.inspect().await?.clips.len(), 3);
    store.integrity_check().await?;
    Ok(())
}
