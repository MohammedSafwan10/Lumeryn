//! Actual shared graph PCM and decoded AAC acceptance; no device or customer media.
use lumeryn_core::{
    MediaTools, Project,
    audio_plan::{AudioPlan, CANONICAL_DECODE_FILTER, MIX_SAMPLE_RATE, sample_boundary},
    media::content_hash,
    model::{AudioClip, Clip, Media},
    render::{self, Acceleration, RenderOptions, RenderResolution, VideoCodec},
    time::FrameRate,
};
use std::{
    ffi::OsString,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn tools() -> MediaTools {
    MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG").map_or_else(|| "ffmpeg".into(), PathBuf::from),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map_or_else(|| "ffprobe".into(), PathBuf::from),
        timeout: Duration::from_secs(30),
    }
}

fn wav(path: &Path, frames: usize, frequency: f32, amplitude: f32) -> TestResult {
    let bytes = u32::try_from(frames)? * 8;
    let mut file = File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&3_u16.to_le_bytes())?;
    file.write_all(&2_u16.to_le_bytes())?;
    file.write_all(&MIX_SAMPLE_RATE.to_le_bytes())?;
    file.write_all(&(MIX_SAMPLE_RATE * 8).to_le_bytes())?;
    file.write_all(&8_u16.to_le_bytes())?;
    file.write_all(&32_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&bytes.to_le_bytes())?;
    for frame in 0..frames {
        let value = amplitude
            * (frame as f32 * frequency * std::f32::consts::TAU / MIX_SAMPLE_RATE as f32).sin();
        file.write_all(&value.to_le_bytes())?;
        file.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}

async fn fixture(directory: &Path) -> TestResult<Project> {
    let embedded = directory.join("embedded.wav");
    let music = directory.join("music'%;.wav");
    let video = directory.join("video.mkv");
    wav(&embedded, 24_000, 440.0, 0.2)?;
    wav(&music, 96_000, 880.0, 0.4)?;
    tools()
        .run(
            &tools().ffmpeg,
            &[
                "-hide_banner".into(),
                "-v".into(),
                "error".into(),
                "-nostdin".into(),
                "-n".into(),
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                "testsrc2=size=128x72:rate=30:duration=2".into(),
                "-i".into(),
                embedded.as_os_str().to_owned(),
                "-c:v".into(),
                "ffv1".into(),
                "-threads".into(),
                "2".into(),
                "-c:a".into(),
                "pcm_f32le".into(),
                video.as_os_str().to_owned(),
            ],
        )
        .await?;
    let mut project = Project::new(
        "Independent audio acceptance".into(),
        128,
        72,
        FrameRate::new(30, 1)?,
    )?;
    let video_id = Uuid::new_v4();
    let music_id = Uuid::new_v4();
    let audio_track = Uuid::new_v4();
    let video_metadata = tools().probe(&video).await?;
    // Old embedded metadata legitimately has no decoded source proof.
    assert_eq!(
        video_metadata
            .audio
            .as_ref()
            .and_then(|audio| audio.canonical_samples_48k),
        None
    );
    let mut music_metadata = tools().probe(&music).await?;
    let proof = tools()
        .run(
            &tools().ffmpeg,
            &[
                "-v".into(),
                "error".into(),
                "-nostdin".into(),
                "-protocol_whitelist".into(),
                "file".into(),
                "-i".into(),
                music.as_os_str().to_owned(),
                "-map".into(),
                "0:a:0".into(),
                "-vn".into(),
                "-af".into(),
                CANONICAL_DECODE_FILTER.into(),
                "-f".into(),
                "f32le".into(),
                "pipe:1".into(),
            ],
        )
        .await?;
    assert_eq!(proof.len(), 96_000 * 8);
    music_metadata
        .audio
        .as_mut()
        .ok_or("fixture audio missing")?
        .canonical_samples_48k = Some(proof.len() as u64 / 8);
    project.media = vec![
        Media {
            id: video_id,
            path: video,
            alias: None,
            metadata: video_metadata,
        },
        Media {
            id: music_id,
            path: music,
            alias: None,
            metadata: music_metadata,
        },
    ];
    project.clips.push(Clip {
        id: Uuid::new_v4(),
        media_id: video_id,
        track_id: project.video_track_id,
        at_frame: 0,
        source_in_frame: 0,
        duration_frames: 60,
    });
    project.audio_track_id = Some(audio_track);
    project.audio_clips = vec![
        AudioClip {
            id: Uuid::new_v4(),
            media_id: music_id,
            track_id: audio_track,
            at_frame: 15,
            source_in_sample: 8_000,
            duration_frames: 15,
            gain_millidecibels: -6_000,
            muted: false,
        },
        AudioClip {
            id: Uuid::new_v4(),
            media_id: music_id,
            track_id: audio_track,
            at_frame: 30,
            source_in_sample: 0,
            duration_frames: 15,
            gain_millidecibels: 0,
            muted: true,
        },
    ];
    project.validate()?;
    Ok(project)
}

async fn pcm(plan: &AudioPlan, start: u64) -> TestResult<Vec<f32>> {
    plan.verify_sources().await?;
    let bytes = tools().run(&tools().ffmpeg, &plan.pcm_args(start)?).await?;
    plan.verify_sources().await?;
    assert!(bytes.len().is_multiple_of(8));
    Ok(bytes
        .chunks_exact(4)
        .map(|value| f32::from_le_bytes([value[0], value[1], value[2], value[3]]))
        .collect())
}

fn rms(samples: &[f32], begin: usize, end: usize) -> f64 {
    let window = &samples[begin * 2..end * 2];
    (window
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        / window.len() as f64)
        .sqrt()
}

#[tokio::test]
#[ignore = "requires configured FFmpeg; graph mix, exact seek and immutable source acceptance"]
async fn graph_pcm_mixes_embedded_padding_delayed_gain_and_mute() -> TestResult {
    let directory = tempfile::tempdir()?;
    let project = fixture(directory.path()).await?;
    let database = directory.path().join("project.lumeryn");
    let plan = AudioPlan::from_project(&project, &database).await?;
    let samples = pcm(&plan, 0).await?;
    assert_eq!(samples.len(), 96_000 * 2);
    assert!((rms(&samples, 8_000, 12_800) - 0.2 / 2_f64.sqrt()).abs() < 0.0001);
    let gain = 10_f64.powf(-6.0 / 20.0);
    assert!((rms(&samples, 32_000, 36_800) - 0.4 * gain / 2_f64.sqrt()).abs() < 0.0001);
    assert!(samples[48_000 * 2..].iter().all(|sample| *sample == 0.0));
    assert_eq!(
        pcm(&plan, 20).await?,
        samples[sample_boundary(project.frame_rate, 20)? as usize * 2..]
    );
    let source_frame = 8_000 + (32_000 - 24_000);
    let expected = 0.4_f32
        * (source_frame as f32 * 880.0 * std::f32::consts::TAU / 48_000.0).sin()
        * gain as f32;
    assert!((samples[32_000 * 2] - expected).abs() < 0.000002);
    let music = &project.media[1];
    assert_eq!(
        content_hash(&music.path).await?,
        music.metadata.blake3.as_deref().ok_or("missing digest")?
    );
    let mut unproven = project.clone();
    unproven.media[1]
        .metadata
        .audio
        .as_mut()
        .ok_or("missing audio")?
        .canonical_samples_48k = None;
    assert!(AudioPlan::from_project(&unproven, &database).await.is_err());
    wav(&music.path, 96_000, 880.0, 0.3)?;
    assert!(AudioPlan::from_project(&project, &database).await.is_err());
    assert!(plan.verify_sources().await.is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "requires configured FFmpeg; actual final FPS, AAC timing and combined-input limits"]
async fn exports_full_audio_graph_at_requested_fps_and_rejects_excess_contributions() -> TestResult
{
    let directory = tempfile::tempdir()?;
    let project = fixture(directory.path()).await?;
    let database = directory.path().join("project.lumeryn");
    for fps in [24, 60] {
        let output = directory.path().join(format!("audio-{fps}.mp4"));
        let options = RenderOptions {
            fps: Some(fps),
            ..Default::default()
        };
        let rendered =
            render::export_with_options(&project, &database, &output, &tools(), &options).await?;
        assert!(rendered.verified_full_decode);
        assert_eq!(rendered.frames, fps as u64 * 2);
        assert_eq!(rendered.frame_rate, format!("{fps}/1"));
        let arguments = ["-v", "error", "-nostdin", "-i"]
            .into_iter()
            .map(OsString::from)
            .chain([
                output.as_os_str().to_owned(),
                "-map".into(),
                "0:a:0".into(),
                "-f".into(),
                "f32le".into(),
                "pipe:1".into(),
            ])
            .collect::<Vec<_>>();
        let bytes = tools().run(&tools().ffmpeg, &arguments).await?;
        let decoded = bytes
            .chunks_exact(4)
            .map(|value| f32::from_le_bytes([value[0], value[1], value[2], value[3]]))
            .collect::<Vec<_>>();
        assert!(decoded.len() >= 96_000 * 2 && decoded.len() <= (96_000 + 1024) * 2);
        assert!((rms(&decoded, 8_000, 12_800) - 0.2 / 2_f64.sqrt()).abs() < 0.005);
        assert!(
            (rms(&decoded, 32_000, 36_800) - 0.4 * 10_f64.powf(-6.0 / 20.0) / 2_f64.sqrt()).abs()
                < 0.005
        );
        assert!(rms(&decoded, 60_000, 72_000) < 0.0001);
    }
    let mut crowded = project.clone();
    crowded.audio_clips = (0..64)
        .map(|_| AudioClip {
            id: Uuid::new_v4(),
            ..project.audio_clips[0].clone()
        })
        .collect();
    crowded.validate()?;
    assert!(
        matches!(AudioPlan::from_project(&crowded, &database).await, Err(lumeryn_core::Error::Invalid(message)) if message.contains("at most 64"))
    );
    for media in &project.media {
        assert_eq!(
            content_hash(&media.path).await?,
            media.metadata.blake3.as_deref().ok_or("missing digest")?
        );
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires configured FFmpeg; actual UHD landscape/portrait composition and decoded audio markers"]
async fn independent_audio_exports_uhd_landscape_and_portrait() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut project = fixture(directory.path()).await?;
    let database = directory.path().join("project.lumeryn");
    for (portrait, fps) in [(false, 24), (true, 60)] {
        if portrait {
            project.width = 72;
            project.height = 128;
        }
        let output = directory.path().join(format!("uhd-audio-{fps}.mp4"));
        let options = RenderOptions {
            resolution: RenderResolution::Uhd2160,
            fps: Some(fps),
            codec: VideoCodec::H264,
            acceleration: Acceleration::Auto,
            ..Default::default()
        };
        let rendered =
            render::export_with_options(&project, &database, &output, &tools(), &options).await?;
        assert!(rendered.verified_full_decode);
        assert!(rendered.upscaled); // This fixture verifies composition, not native UHD detail.
        assert_eq!(
            (rendered.width, rendered.height),
            if portrait { (2160, 3840) } else { (3840, 2160) }
        );
        assert_eq!(rendered.frames, fps as u64 * 2);
        let decoded = tools()
            .run(
                &tools().ffmpeg,
                &[
                    "-v".into(),
                    "error".into(),
                    "-nostdin".into(),
                    "-i".into(),
                    output.as_os_str().to_owned(),
                    "-map".into(),
                    "0:a:0".into(),
                    "-f".into(),
                    "f32le".into(),
                    "pipe:1".into(),
                ],
            )
            .await?;
        let samples = decoded
            .chunks_exact(4)
            .map(|value| f32::from_le_bytes([value[0], value[1], value[2], value[3]]))
            .collect::<Vec<_>>();
        assert!((rms(&samples, 8_000, 12_800) - 0.2 / 2_f64.sqrt()).abs() < 0.005);
        assert!(
            (rms(&samples, 32_000, 36_800) - 0.4 * 10_f64.powf(-6.0 / 20.0) / 2_f64.sqrt()).abs()
                < 0.005
        );
        assert!(rms(&samples, 60_000, 72_000) < 0.0001);
    }
    for media in &project.media {
        assert_eq!(
            content_hash(&media.path).await?,
            media.metadata.blake3.as_deref().ok_or("missing digest")?
        );
    }
    Ok(())
}
