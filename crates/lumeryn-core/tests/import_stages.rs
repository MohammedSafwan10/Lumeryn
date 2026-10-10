//! Generated fixtures only. Advisory headers never substitute for a source proof.
use lumeryn_core::{
    Error, MediaTools, Result,
    media::{ProbeProgress, ProbeStage, content_hash},
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::Instant,
};

fn tools() -> MediaTools {
    MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffmpeg".into()),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffprobe".into()),
        ..MediaTools::default()
    }
}

fn generate(path: &Path, input: &str, extra: &[&str]) -> Result<()> {
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

async fn counted_frames(path: &Path) -> Result<u64> {
    let args = [
        "-v",
        "error",
        "-threads",
        "2",
        "-protocol_whitelist",
        "file",
        "-select_streams",
        "v:0",
        "-count_frames",
        "-show_entries",
        "stream=nb_read_frames",
        "-of",
        "json",
    ]
    .into_iter()
    .map(OsString::from)
    .chain([path.as_os_str().to_owned()])
    .collect::<Vec<_>>();
    let value: serde_json::Value =
        serde_json::from_slice(&tools().run(&tools().ffprobe, &args).await?)?;
    value["streams"][0]["nb_read_frames"]
        .as_str()
        .ok_or_else(|| Error::Invalid("fixture missing count".into()))?
        .parse()
        .map_err(|_| Error::Invalid("fixture invalid count".into()))
}

#[tokio::test]
#[ignore = "requires installed FFmpeg/ffprobe"]
async fn combined_validation_matches_full_decode_counts_and_cache_reuses_only_proofs() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    for (rate, frames, extension, codec) in [
        ("60", 120, "mkv", "mpeg4"),
        ("30000/1001", 60, "mkv", "ffv1"),
        ("25", 50, "mp4", "mpeg4"),
    ] {
        let source = directory
            .path()
            .join(format!("{}.{}", rate.replace('/', "-"), extension));
        generate(
            &source,
            &format!("testsrc2=s=64x36:r={rate}:d=2"),
            &["-c:v", codec, "-threads", "2"],
        )?;
        let digest = content_hash(&source).await?;
        let mut stages = Vec::new();
        let verified = tools()
            .probe_with_progress(&source, |event| stages.push(event))
            .await?;
        let metadata = verified.metadata();
        let video = metadata
            .video
            .as_ref()
            .ok_or_else(|| Error::Invalid("fixture missing video".into()))?;
        assert_eq!(video.frames, frames);
        assert_eq!(video.frames, counted_frames(&source).await?);
        assert!(video.constant_rate);
        assert_eq!(video.frame_rate, rate.parse()?);
        assert_eq!(metadata.blake3.as_deref(), Some(digest.as_str()));
        assert_eq!(
            stages.iter().map(|event| event.stage).collect::<Vec<_>>(),
            [
                ProbeStage::Inspecting,
                ProbeStage::Hashing,
                ProbeStage::ValidatingVideo,
                ProbeStage::VerifyingSource
            ]
        );
        let quick = stages[1]
            .quick
            .as_ref()
            .ok_or_else(|| Error::Invalid("quick headers missing".into()))?;
        let quick_json = serde_json::to_value(quick)?;
        assert!(quick_json["video"].get("constant_rate").is_none());
        assert!(quick_json["video"].get("frames").is_none());
        let mut cached_stages = Vec::new();
        let cached = tools()
            .probe_with_cached_progress(&source, std::slice::from_ref(&verified), |event| {
                cached_stages.push(event)
            })
            .await?;
        assert_eq!(
            serde_json::to_value(cached.metadata())?,
            serde_json::to_value(metadata)?
        );
        assert_eq!(
            cached_stages
                .iter()
                .map(|event| event.stage)
                .collect::<Vec<_>>(),
            [
                ProbeStage::Inspecting,
                ProbeStage::Hashing,
                ProbeStage::VerifyingSource
            ]
        );
        assert_eq!(content_hash(&source).await?, digest);
    }
    let image = directory.path().join("still.png");
    generate(&image, "color=c=blue:s=64x36:d=1", &["-frames:v", "1"])?;
    let metadata = tools().probe(&image).await?;
    assert_eq!(metadata.video.as_ref().map(|video| video.frames), Some(1));
    assert_eq!(counted_frames(&image).await?, 1);
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed FFmpeg/ffprobe"]
async fn noncandidate_vfr_is_counted_and_same_size_replacement_invalidates_cache() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let video = directory.path().join("vfr.mkv");
    generate(
        &video,
        "testsrc2=s=64x36:r=30:d=1",
        &[
            "-vf",
            "setpts=if(lt(N\\,15)\\,N/(30*TB)\\,(15+(N-15)*2)/(30*TB))",
            "-fps_mode",
            "vfr",
            "-c:v",
            "ffv1",
        ],
    )?;
    let metadata = tools().probe(&video).await?;
    let stream = metadata
        .video
        .as_ref()
        .ok_or_else(|| Error::Invalid("fixture missing video".into()))?;
    assert_eq!(stream.frames, counted_frames(&video).await?);
    assert!(!stream.constant_rate);
    let audio = directory.path().join("same-size.wav");
    generate(&audio, "sine=f=440:r=48000:d=1", &["-c:a", "pcm_s16le"])?;
    let verified = tools().probe_with_progress(&audio, |_| {}).await?;
    let old_digest = verified.metadata().blake3.clone();
    let mut bytes = std::fs::read(&audio)?;
    let last = bytes
        .last_mut()
        .ok_or_else(|| Error::Invalid("empty fixture".into()))?;
    *last ^= 1;
    std::fs::write(&audio, &bytes)?;
    let mut stages = Vec::new();
    let changed = tools()
        .probe_with_cached_progress(&audio, &[verified], |event| stages.push(event.stage))
        .await?;
    assert_ne!(changed.metadata().blake3, old_digest);
    assert!(stages.contains(&ProbeStage::ValidatingAudio));
    assert_eq!(changed.metadata().size_bytes, bytes.len() as u64);
    assert_eq!(std::fs::read(&audio)?, bytes);
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed FFmpeg/ffprobe"]
async fn replacement_inside_validation_is_rejected_and_quick_inspection_stays_advisory()
-> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("original.mkv");
    let replacement = directory.path().join("replacement.mkv");
    generate(&source, "color=c=blue:s=64x36:r=30:d=1", &["-c:v", "ffv1"])?;
    generate(
        &replacement,
        "color=c=red:s=64x36:r=30:d=1",
        &["-c:v", "ffv1"],
    )?;
    let quick = tools().inspect(&source).await?;
    assert_eq!(
        quick.video.as_ref().and_then(|video| video.frame_rate),
        Some("30".parse()?)
    );
    let bytes = std::fs::read(replacement)?;
    let mut write_error = None;
    let result = tools()
        .probe_with_progress(&source, |event| {
            if event.stage == ProbeStage::ValidatingVideo {
                write_error = std::fs::write(&source, &bytes).err();
            }
        })
        .await;
    if let Some(error) = write_error {
        return Err(error.into());
    }
    assert!(matches!(result, Err(Error::Invalid(_))));
    Ok(())
}

#[tokio::test]
#[ignore = "manual generated-fixture timing; LUMERYN_IMPORT_BENCH_SOURCE required"]
async fn measured_import_stages() -> Result<()> {
    let Some(source) = std::env::var_os("LUMERYN_IMPORT_BENCH_SOURCE") else {
        return Ok(());
    };
    let source = PathBuf::from(source);
    let mut cached = Vec::new();
    for run in 0..6 {
        let started = Instant::now();
        let mut stages: Vec<ProbeProgress> = Vec::new();
        let metadata = tools()
            .probe_with_cached_progress(&source, if run < 3 { &[] } else { &cached }, |event| {
                stages.push(event)
            })
            .await?;
        println!(
            "{}",
            serde_json::json!({"run":run,"cached":run>=3,"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"stages":stages,"digest":metadata.metadata().blake3})
        );
        if cached.is_empty() {
            cached.push(metadata)
        }
    }
    Ok(())
}
