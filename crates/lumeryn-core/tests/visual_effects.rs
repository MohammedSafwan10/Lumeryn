use lumeryn_core::{
    MediaTools,
    audio_plan::sample_boundary,
    time::FrameRate,
    visual_effects::{ColourPreset, FadeEnvelope, VisualEffects},
};
use std::{ffi::OsString, path::PathBuf, time::Duration};
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn strict_effect_contract_and_reset() -> TestResult {
    let rate = FrameRate::new(30000, 1001)?;
    assert_eq!(VisualEffects::default().lower_filters(rate, 30)?, None);
    assert!(serde_json::from_str::<VisualEffects>(r#"{"version":1,"exposure_millistops":0,"contrast_basis_points":10000,"saturation_basis_points":10000,"temperature_basis_points":0,"fades":{"in_frames":0,"out_frames":0},"expression":"evil"}"#).is_err());
    for effects in [
        VisualEffects {
            version: 2,
            ..Default::default()
        },
        VisualEffects {
            exposure_millistops: 2001,
            ..Default::default()
        },
        VisualEffects {
            temperature_basis_points: -10001,
            ..Default::default()
        },
        VisualEffects {
            contrast_basis_points: 20001,
            ..Default::default()
        },
    ] {
        assert!(effects.validate(30).is_err());
    }
    assert!(
        FadeEnvelope {
            in_frames: 16,
            out_frames: 15
        }
        .validate(30)
        .is_err()
    );
    assert!(
        FadeEnvelope {
            in_frames: u64::MAX,
            out_frames: 1
        }
        .validate(30)
        .is_err()
    );
    for preset in [
        ColourPreset::Neutral,
        ColourPreset::WarmStory,
        ColourPreset::CoolClarity,
        ColourPreset::SoftMono,
    ] {
        assert_eq!(VisualEffects::preset(preset, 0)?, VisualEffects::default());
        VisualEffects::preset(preset, 10000)?.validate(30)?;
    }
    assert!(VisualEffects::preset(ColourPreset::WarmStory, 10001).is_err());
    Ok(())
}

#[test]
fn fades_use_absolute_fractional_sample_boundaries_and_halfopen_end() -> TestResult {
    let rate = FrameRate::new(30000, 1001)?;
    let fade = FadeEnvelope {
        in_frames: 2,
        out_frames: 3,
    };
    let start = sample_boundary(rate, 1)?;
    let fade_in_end = sample_boundary(rate, 3)?;
    let fade_out_start = sample_boundary(rate, 8)?;
    let end = sample_boundary(rate, 11)?;
    assert_eq!(fade.audio_gain(rate, 1, 10, start - 1)?, 0.0);
    assert_eq!(fade.audio_gain(rate, 1, 10, start)?, 0.0);
    assert_eq!(fade.audio_gain(rate, 1, 10, fade_in_end)?, 1.0);
    assert_eq!(fade.audio_gain(rate, 1, 10, fade_out_start)?, 1.0);
    assert!(fade.audio_gain(rate, 1, 10, end - 1)? > 0.0);
    assert_eq!(fade.audio_gain(rate, 1, 10, end)?, 0.0);
    assert_eq!(
        fade.lower_audio_filters(rate, 1, 10)?,
        Some(format!(
            "afade=t=in:ss=0:ns={}:curve=tri,afade=t=out:ss={}:ns={}:curve=tri",
            fade_in_end - start,
            fade_out_start - start,
            end - fade_out_start
        ))
    );
    let midpoint = start + (fade_in_end - start) / 2;
    assert!((fade.audio_gain(rate, 1, 10, midpoint)? - 0.5).abs() < 0.001);
    assert!(fade.audio_gain(rate, u64::MAX, 10, 0).is_err());
    Ok(())
}

async fn pixels(
    effects: VisualEffects,
    input: &[[u8; 4]],
    rate: FrameRate,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let folder = tempfile::tempdir()?;
    let source = folder.path().join("source.rgba");
    std::fs::write(&source, input.iter().flatten().copied().collect::<Vec<_>>())?;
    let tools = MediaTools {
        ffmpeg: PathBuf::from(std::env::var("LUMERYN_FFMPEG")?),
        ffprobe: PathBuf::from(std::env::var("LUMERYN_FFPROBE")?),
        timeout: Duration::from_secs(30),
    };
    let filter = effects
        .lower_filters(rate, input.len() as u64)?
        .unwrap_or_else(|| "null".into());
    let mut args = [
        "-v",
        "error",
        "-nostdin",
        "-threads",
        "1",
        "-f",
        "rawvideo",
        "-pixel_format",
        "rgba",
        "-video_size",
        "1x1",
        "-framerate",
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    args.extend([
        rate.to_string().into(),
        "-i".into(),
        source.into_os_string(),
        "-vf".into(),
        filter.into(),
        "-frames:v".into(),
        input.len().to_string().into(),
        "-threads".into(),
        "1".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgba".into(),
        "pipe:1".into(),
    ]);
    Ok(tools.run(&tools.ffmpeg, &args).await?)
}

#[tokio::test]
#[ignore = "requires configured external FFmpeg"]
async fn decoded_colour_alpha_reset_and_fractional_fade_pixels() -> TestResult {
    let rate = FrameRate::new(30000, 1001)?;
    let source = [[64, 32, 16, 128]; 8];
    assert_eq!(
        pixels(VisualEffects::default(), &source, rate).await?,
        source.iter().flatten().copied().collect::<Vec<_>>()
    );
    let exposed = pixels(
        VisualEffects {
            exposure_millistops: 1000,
            ..Default::default()
        },
        &source,
        rate,
    )
    .await?;
    for pixel in exposed.chunks_exact(4) {
        for (actual, expected) in pixel.iter().zip([128, 64, 32, 128]) {
            assert!(actual.abs_diff(expected) <= 1, "{pixel:?}");
        }
    }
    let mono = pixels(
        VisualEffects::preset(ColourPreset::SoftMono, 10000)?,
        &source,
        rate,
    )
    .await?;
    assert!(
        mono.chunks_exact(4)
            .all(|pixel| pixel[0].abs_diff(pixel[1]) <= 1
                && pixel[1].abs_diff(pixel[2]) <= 1
                && pixel[3] == 128)
    );
    let warm = pixels(
        VisualEffects::preset(ColourPreset::WarmStory, 10000)?,
        &source,
        rate,
    )
    .await?;
    assert!(
        warm.chunks_exact(4)
            .all(|pixel| pixel[0] > 64 && pixel[3] == 128)
    );
    let extrema = pixels(
        VisualEffects {
            saturation_basis_points: 20000,
            temperature_basis_points: 10000,
            contrast_basis_points: 20000,
            exposure_millistops: 2000,
            ..Default::default()
        },
        &source,
        rate,
    )
    .await?;
    assert!(extrema.chunks_exact(4).all(|pixel| pixel[3] == 128));
    let faded = pixels(
        VisualEffects {
            fades: FadeEnvelope {
                in_frames: 2,
                out_frames: 2,
            },
            ..Default::default()
        },
        &source,
        rate,
    )
    .await?;
    for (pixel, alpha) in faded
        .chunks_exact(4)
        .zip([0, 64, 128, 128, 128, 128, 128, 64])
    {
        assert!(pixel[3].abs_diff(alpha) <= 1, "{pixel:?} vs {alpha}");
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires configured external FFmpeg"]
async fn decoded_audio_fade_matches_absolute_sample_envelope() -> TestResult {
    let rate = FrameRate::new(30000, 1001)?;
    let at = 1;
    let duration = 10;
    let start = sample_boundary(rate, at)?;
    let end = sample_boundary(rate, at + duration)?;
    let folder = tempfile::tempdir()?;
    let input = folder.path().join("constant.f32");
    let source = (start..end)
        .flat_map(|_| 0.25_f32.to_le_bytes())
        .collect::<Vec<_>>();
    std::fs::write(&input, &source)?;
    let tools = MediaTools {
        ffmpeg: PathBuf::from(std::env::var("LUMERYN_FFMPEG")?),
        ffprobe: PathBuf::from(std::env::var("LUMERYN_FFPROBE")?),
        timeout: Duration::from_secs(30),
    };
    let fades = FadeEnvelope {
        in_frames: 2,
        out_frames: 3,
    };
    let filter = fades
        .lower_audio_filters(rate, at, duration)?
        .ok_or("missing audio filter")?;
    let mut args = [
        "-v", "error", "-nostdin", "-f", "f32le", "-ar", "48000", "-ac", "1", "-i",
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    args.extend([
        input.into_os_string(),
        "-af".into(),
        filter.into(),
        "-f".into(),
        "f32le".into(),
        "pipe:1".into(),
    ]);
    let pcm = tools.run(&tools.ffmpeg, &args).await?;
    assert_eq!(pcm.len(), source.len());
    for (index, bytes) in pcm.chunks_exact(4).enumerate() {
        let actual = f32::from_le_bytes(bytes.try_into()?);
        let expected = 0.25 * fades.audio_gain(rate, at, duration, start + index as u64)?;
        assert!(
            (actual - expected).abs() < 0.000001,
            "sample{index}: {actual} vs{expected}"
        );
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires configured external FFmpeg; measured CPU reference experiment"]
async fn measured_colour_fade_cpu_reference() -> TestResult {
    let tools = MediaTools {
        ffmpeg: PathBuf::from(std::env::var("LUMERYN_FFMPEG")?),
        ffprobe: PathBuf::from(std::env::var("LUMERYN_FFPROBE")?),
        timeout: Duration::from_secs(60),
    };
    let rate = FrameRate::new(30, 1)?;
    let colour = VisualEffects::preset(ColourPreset::WarmStory, 10000)?;
    let faded = VisualEffects {
        fades: FadeEnvelope {
            in_frames: 10,
            out_frames: 10,
        },
        ..colour
    };
    for (name, filter) in [
        ("baseline", "format=rgba".to_owned()),
        (
            "colour",
            colour.lower_filters(rate, 30)?.ok_or("missing filter")?,
        ),
        (
            "colour_fade",
            faded.lower_filters(rate, 30)?.ok_or("missing filter")?,
        ),
    ] {
        let mut elapsed = Vec::new();
        for _ in 0..3 {
            let args = [
                "-v",
                "error",
                "-nostdin",
                "-filter_threads",
                "1",
                "-threads",
                "1",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1280x720:rate=30",
                "-vf",
                &filter,
                "-frames:v",
                "30",
                "-f",
                "null",
                "-",
            ]
            .into_iter()
            .map(OsString::from)
            .collect::<Vec<_>>();
            let started = std::time::Instant::now();
            tools.run(&tools.ffmpeg, &args).await?;
            elapsed.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        elapsed.sort_by(f64::total_cmp);
        eprintln!(
            "{name} 720p30frames total_ms median={:.3} runs={elapsed:?}",
            elapsed[1]
        );
    }
    Ok(())
}
