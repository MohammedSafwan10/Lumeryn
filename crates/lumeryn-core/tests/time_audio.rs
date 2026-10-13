use lumeryn_core::{
    MediaTools, Project, Result,
    audio_plan::AudioPlan,
    media::{Audio, Metadata, Video},
    model::{Clip, Media},
    time_effects::{PlaybackSpeed, SpeedAudio, TimeEffect},
};
use uuid::Uuid;
#[tokio::test]
async fn embedded_time_audio_filter_contract_and_silent_freeze() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("source.mkv");
    std::fs::write(&source, b"time")?;
    let mut p = Project::new("Time audio".into(), 64, 64, "30".parse()?)?;
    let media = Uuid::new_v4();
    let clip = Uuid::new_v4();
    p.media.push(Media {
        id: media,
        path: "source.mkv".into(),
        alias: None,
        metadata: Metadata {
            size_bytes: 4,
            blake3: Some(blake3::hash(b"time").to_hex().to_string()),
            video: Some(Video {
                codec: "ffv1".into(),
                width: 64,
                height: 64,
                frame_rate: p.frame_rate,
                frames: 60,
                start_time_micros: 0,
                sample_aspect_ratio: "1:1".into(),
                color_transfer: None,
                constant_rate: true,
            }),
            audio: Some(Audio {
                codec: "pcm_f32le".into(),
                sample_rate: 48000,
                channels: 2,
                start_time_micros: 0,
                canonical_samples_48k: Some(96000),
            }),
        },
    });
    p.clips.push(Clip {
        id: clip,
        media_id: media,
        track_id: p.video_track_id,
        at_frame: 0,
        source_in_frame: 0,
        duration_frames: 30,
    });
    p.time_effects.insert(
        clip,
        TimeEffect::Speed {
            speed: PlaybackSpeed::Double,
            audio: SpeedAudio::PreservePitch,
        },
    );
    let plan = AudioPlan::from_project(&p, &dir.path().join("project.lumeryn")).await?;
    let filter = plan.filter_program(0, 0)?.filters;
    assert!(filter.contains("end_sample=96000"));
    assert!(filter.contains("atempo=2"));
    assert!(filter.contains("apad=whole_len=48000,atrim=end_sample=48000"));
    assert_eq!(plan.total_samples(), 48000);
    for effect in [
        TimeEffect::Freeze { source_frame: 0 },
        TimeEffect::Speed {
            speed: PlaybackSpeed::Half,
            audio: SpeedAudio::Mute,
        },
    ] {
        p.time_effects.insert(clip, effect);
        let silent = AudioPlan::from_project(&p, &dir.path().join("project.lumeryn")).await?;
        assert!(silent.input_args().is_empty());
        assert!(silent.filter_program(0, 0)?.filters.contains("anullsrc"));
    }
    p.frame_rate = "30000/1001".parse()?;
    if let Some(video) = p.media[0].metadata.video.as_mut() {
        video.frame_rate = p.frame_rate;
    }
    p.clips[0].source_in_frame = 1;
    p.clips[0].at_frame = 1;
    p.clips[0].duration_frames = 5;
    p.time_effects.insert(
        clip,
        TimeEffect::Speed {
            speed: PlaybackSpeed::Half,
            audio: SpeedAudio::PreservePitch,
        },
    );
    let rational = AudioPlan::from_project(&p, &dir.path().join("project.lumeryn")).await?;
    assert!(
        rational
            .filter_program(0, 0)?
            .filters
            .contains("start_sample=1601:end_sample=5605")
    );
    Ok(())
}

#[tokio::test]
#[ignore = "configured FFmpeg; actual tempo PCM pitch/duration/seek/silence proof"]
async fn actual_speed_audio_pitch_exact_length_seek_and_freeze_silence() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("tone.mkv");
    let tools = MediaTools::default();
    let args = [
        "-hide_banner",
        "-v",
        "error",
        "-nostdin",
        "-f",
        "lavfi",
        "-i",
        "color=red:size=64x64:rate=30:duration=2",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000:duration=2",
        "-c:v",
        "ffv1",
        "-c:a",
        "pcm_f32le",
        "-shortest",
    ]
    .into_iter()
    .map(std::ffi::OsString::from)
    .chain([source.as_os_str().to_owned()])
    .collect::<Vec<_>>();
    tools.run(&tools.ffmpeg, &args).await?;
    let metadata = tools.probe(&source).await?;
    let mut p = Project::new("Actual time".into(), 64, 64, "30".parse()?)?;
    let mid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    p.media.push(Media {
        id: mid,
        path: "tone.mkv".into(),
        alias: None,
        metadata,
    });
    p.clips.push(Clip {
        id: cid,
        media_id: mid,
        track_id: p.video_track_id,
        at_frame: 0,
        source_in_frame: 0,
        duration_frames: 30,
    });
    for speed in [PlaybackSpeed::Half, PlaybackSpeed::Double] {
        p.time_effects.insert(
            cid,
            TimeEffect::Speed {
                speed,
                audio: SpeedAudio::PreservePitch,
            },
        );
        let plan = AudioPlan::from_project(&p, &dir.path().join("project.lumeryn")).await?;
        let bytes = tools.run(&tools.ffmpeg, &plan.pcm_args(0)?).await?;
        assert_eq!(bytes.len(), 48000 * 8);
        let samples = bytes
            .chunks_exact(8)
            .map(|v| f32::from_le_bytes([v[0], v[1], v[2], v[3]]))
            .collect::<Vec<_>>();
        // Stable interior region excludes atempo startup/tail; positive crossings
        // prove pitch remained near440Hz rather than220/880Hz.
        let crossings = samples[8000..32000]
            .windows(2)
            .filter(|p| p[0] <= 0.0 && p[1] > 0.0)
            .count();
        assert!(
            (215..=225).contains(&crossings),
            "pitch crossings {crossings} at {speed:?}"
        );
        let seek = tools.run(&tools.ffmpeg, &plan.pcm_args(15)?).await?;
        assert_eq!(seek, &bytes[24000 * 8..]);
        plan.verify_sources().await?;
    }
    p.time_effects
        .insert(cid, TimeEffect::Freeze { source_frame: 7 });
    let plan = AudioPlan::from_project(&p, &dir.path().join("project.lumeryn")).await?;
    let bytes = tools.run(&tools.ffmpeg, &plan.pcm_args(0)?).await?;
    assert_eq!(bytes.len(), 48000 * 8);
    assert!(bytes.iter().all(|b| *b == 0));
    Ok(())
}
