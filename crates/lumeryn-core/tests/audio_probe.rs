//! Real source-proof acceptance; explicitly requires installed FFmpeg/ffprobe.
use lumeryn_core::{Error, MediaTools, Result, media::content_hash};
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
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

fn wav(path: &Path, rate: u32, channels: u16, seconds: u32) -> Result<()> {
    let bytes = rate * u32::from(channels) * 2 * seconds;
    let mut output = File::create_new(path)?;
    output.write_all(b"RIFF")?;
    output.write_all(&(bytes + 36).to_le_bytes())?;
    output.write_all(b"WAVEfmt ")?;
    output.write_all(&16_u32.to_le_bytes())?;
    output.write_all(&1_u16.to_le_bytes())?;
    output.write_all(&channels.to_le_bytes())?;
    output.write_all(&rate.to_le_bytes())?;
    output.write_all(&(rate * u32::from(channels) * 2).to_le_bytes())?;
    output.write_all(&(channels * 2).to_le_bytes())?;
    output.write_all(&16_u16.to_le_bytes())?;
    output.write_all(b"data")?;
    output.write_all(&bytes.to_le_bytes())?;
    let buffer = vec![0_u8; 64 * 1024];
    let mut remaining = bytes as usize;
    while remaining > 0 {
        let count = remaining.min(buffer.len());
        output.write_all(&buffer[..count])?;
        remaining -= count;
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed FFmpeg/ffprobe"]
async fn full_stream_count_exceeds_capture_limit_and_preserves_wav_sources() -> Result<()> {
    let directory = tempfile::tempdir()?;
    for (rate, channels) in [(44_100, 1), (48_000, 2)] {
        let source = directory.path().join(format!("{rate}-{channels}.wav"));
        wav(&source, rate, channels, 48)?;
        let digest = content_hash(&source).await?;
        let metadata = tools().probe(&source).await?;
        let audio = metadata
            .audio
            .ok_or_else(|| Error::Invalid("missing fixture audio".into()))?;
        assert_eq!(audio.start_time_micros, 0);
        assert_eq!(audio.canonical_samples_48k, Some(48 * 48_000));
        let samples = tools().canonical_audio_samples(&source, &digest).await?;
        assert_eq!(samples, 48 * 48_000);
        assert!(samples * 8 > 16 * 1024 * 1024);
        assert_eq!(content_hash(&source).await?, digest);
        assert_eq!(metadata.blake3.as_deref(), Some(digest.as_str()));
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed FFmpeg/ffprobe"]
async fn video_import_keeps_sample_proof_lazy() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("video.mkv");
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
            "color=s=32x18:r=30:d=1",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=48000:cl=stereo",
            "-t",
            "1",
            "-c:v",
            "ffv1",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&source)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let digest = content_hash(&source).await?;
    // ffprobe is sufficient for legacy video metadata. An unusable decoder path
    // proves this import did not invoke an expensive canonical-audio decode.
    let probe_tools = MediaTools {
        ffmpeg: "missing-lumeryn-ffmpeg".into(),
        ..tools()
    };
    let metadata = probe_tools.probe(&source).await?;
    assert!(metadata.video.is_some());
    assert_eq!(
        metadata.audio.and_then(|audio| audio.canonical_samples_48k),
        None
    );
    assert_eq!(content_hash(&source).await?, digest);
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed FFmpeg/ffprobe"]
async fn rejects_changed_malformed_multichannel_delayed_sources_and_timeouts() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("source.wav");
    wav(&source, 48_000, 2, 1)?;
    let digest = content_hash(&source).await?;
    let missing_tools = MediaTools {
        ffmpeg: "missing-lumeryn-ffmpeg".into(),
        ffprobe: "missing-lumeryn-ffprobe".into(),
        ..tools()
    };
    assert!(matches!(
        missing_tools
            .canonical_audio_samples(&source, &"0".repeat(64))
            .await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        missing_tools
            .canonical_audio_samples(&source, &digest)
            .await,
        Err(Error::Media(_))
    ));
    let malformed = directory.path().join("invalid.wav");
    std::fs::write(&malformed, b"not a media file")?;
    let malformed_digest = content_hash(&malformed).await?;
    assert!(
        tools()
            .canonical_audio_samples(&malformed, &malformed_digest)
            .await
            .is_err()
    );
    let surround = directory.path().join("surround.wav");
    wav(&surround, 48_000, 6, 1)?;
    assert!(matches!(
        tools()
            .canonical_audio_samples(&surround, &content_hash(&surround).await?)
            .await,
        Err(Error::Unsupported(_))
    ));
    assert_eq!(
        tools()
            .probe(&surround)
            .await?
            .audio
            .and_then(|audio| audio.canonical_samples_48k),
        None
    );
    let delayed = directory.path().join("delayed.mka");
    let output = std::process::Command::new(tools().ffmpeg)
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-nostdin",
            "-n",
            "-itsoffset",
            "0.25",
            "-i",
        ])
        .arg(&source)
        .args(["-c:a", "pcm_s16le"])
        .arg(&delayed)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(matches!(
        tools()
            .canonical_audio_samples(&delayed, &content_hash(&delayed).await?)
            .await,
        Err(Error::Unsupported(_))
    ));
    assert_eq!(
        tools()
            .probe(&delayed)
            .await?
            .audio
            .and_then(|audio| audio.canonical_samples_48k),
        None
    );
    let timeout_tools = MediaTools {
        timeout: Duration::ZERO,
        ..tools()
    };
    assert!(matches!(
        timeout_tools
            .canonical_audio_samples(&source, &digest)
            .await,
        Err(Error::Timeout)
    ));
    std::fs::write(&source, b"changed")?;
    assert!(matches!(
        tools().canonical_audio_samples(&source, &digest).await,
        Err(Error::Invalid(_))
    ));
    Ok(())
}
