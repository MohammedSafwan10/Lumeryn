use lumeryn_core::{
    MediaTools, Result,
    media::content_hash,
    time::FrameRate,
    time_effects::{PlaybackSpeed, SpeedAudio, TimeEffect},
    visual_backend,
    visual_plan::{Picture, PictureRect, PictureSource, VisualPlan},
};
use std::ffi::OsString;
#[tokio::test]
#[ignore = "configured FFmpeg; decoded frame-index speed/freeze output samples"]
async fn decoded_speed_source_indices_and_exact_freeze() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("indices.mkv");
    let tools = MediaTools::default();
    let mut args = [
        "-v",
        "error",
        "-nostdin",
        "-f",
        "lavfi",
        "-i",
        "nullsrc=s=8x8:r=30,geq=r='N*10':g=0:b=0",
        "-frames:v",
        "20",
        "-c:v",
        "ffv1",
        "-pix_fmt",
        "gbrp",
    ]
    .map(OsString::from)
    .to_vec();
    args.push(source.as_os_str().to_owned());
    tools.run(&tools.ffmpeg, &args).await?;
    let digest = content_hash(&source).await?;
    for effect in [
        TimeEffect::Speed {
            speed: PlaybackSpeed::Half,
            audio: SpeedAudio::Mute,
        },
        TimeEffect::Speed {
            speed: PlaybackSpeed::Double,
            audio: SpeedAudio::Mute,
        },
        TimeEffect::Freeze { source_frame: 7 },
    ] {
        let plan = VisualPlan::new(
            FrameRate::new(30, 1)?,
            0,
            8,
            8,
            vec![Picture {
                time_effect: effect,
                animation: Default::default(),
                path: source.clone(),
                digest: digest.clone(),
                source: PictureSource::Video { source_in_frame: 0 },
                at_frame: 0,
                duration_frames: 8,
                layer: 0,
                rect: PictureRect {
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 8,
                },
                source_width: 8,
                source_height: 8,
                transform: Default::default(),
                effects: Default::default(),
            }],
        )?;
        for fps in [30, 60] {
            let output = FrameRate::new(fps, 1)?;
            let count = 8 * fps / 30;
            let program = visual_backend::lower_at_rate(&plan, 8, 8, output)?;
            let mut args = ["-v", "error", "-nostdin", "-filter_complex_threads", "1"]
                .map(OsString::from)
                .to_vec();
            args.extend(program.input_args);
            args.extend([
                "-filter_complex".into(),
                program.filters.into(),
                "-map".into(),
                format!("[{}]", program.output_label).into(),
                "-an".into(),
                "-frames:v".into(),
                count.to_string().into(),
                "-pix_fmt".into(),
                "rgba".into(),
                "-f".into(),
                "rawvideo".into(),
                "pipe:1".into(),
            ]);
            let bytes = tools.run(&tools.ffmpeg, &args).await?;
            assert_eq!(bytes.len(), count as usize * 8 * 8 * 4);
            for (index, frame) in bytes.chunks_exact(8 * 8 * 4).enumerate() {
                let expected = plan.active_at(index as u64, output)?[0]
                    .1
                    .ok_or_else(|| lumeryn_core::Error::Invalid("missing source sample".into()))?
                    * 10;
                assert!(
                    (i64::from(frame[0]) - expected as i64).abs() <= 2,
                    "effect{effect:?} fps{fps} sample{index}:red{} expected{expected}",
                    frame[0]
                );
            }
            plan.verify_sources().await?;
        }
    }
    assert_eq!(content_hash(&source).await?, digest);
    Ok(())
}
