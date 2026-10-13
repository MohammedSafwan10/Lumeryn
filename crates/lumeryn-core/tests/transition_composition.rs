use lumeryn_core::{
    MediaTools, Project, Result,
    model::{Clip, Media, TransitionEdge},
    time_effects::TransitionKind,
    transition_backend,
};
use std::ffi::OsString;
use uuid::Uuid;
#[tokio::test]
#[ignore = "configured FFmpeg; actual fixed-cut dissolve/wipe endpoint pixels and unchanged duration"]
async fn fixed_cut_all_original_transitions_decode_endpoints_and_midpoints() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let tools = MediaTools::default();
    let mut p = Project::new("Transitions".into(), 64, 64, "30".parse()?)?;
    for (index, color) in ["red", "blue"].into_iter().enumerate() {
        let path = dir.path().join(format!("{index}.mkv"));
        let mut args = [
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            &format!("color={color}:s=64x64:r=30"),
            "-frames:v",
            "20",
            "-c:v",
            "ffv1",
        ]
        .map(OsString::from)
        .to_vec();
        args.push(path.as_os_str().to_owned());
        tools.run(&tools.ffmpeg, &args).await?;
        let mid = Uuid::new_v4();
        p.media.push(Media {
            id: mid,
            path: format!("{index}.mkv").into(),
            alias: None,
            metadata: tools.probe(&path).await?,
        });
        p.clips.push(Clip {
            id: Uuid::new_v4(),
            media_id: mid,
            track_id: p.video_track_id,
            at_frame: index as u64 * 6,
            source_in_frame: if index == 0 { 0 } else { 2 },
            duration_frames: 6,
        });
    }
    for kind in [
        TransitionKind::CrossDissolve,
        TransitionKind::WipeLeft,
        TransitionKind::WipeRight,
        TransitionKind::WipeUp,
        TransitionKind::WipeDown,
    ] {
        p.transitions = vec![TransitionEdge {
            id: Uuid::new_v4(),
            outgoing_clip_id: p.clips[0].id,
            incoming_clip_id: p.clips[1].id,
            kind,
            duration_frames: 4,
        }];
        p.validate()?;
        let plan = transition_backend::TransitionPlan::from_project(
            &p,
            &dir.path().join("project.lumeryn"),
        )
        .await?;
        for fps in [30, 60] {
            let program = transition_backend::lower(
                &plan,
                64,
                64,
                lumeryn_core::time::FrameRate::new(fps, 1)?,
            )?;
            let count = 12 * fps / 30;
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
            assert_eq!(bytes.len(), count as usize * 64 * 64 * 4);
            let pixel = |frame: usize, x: usize, y: usize| {
                let offset = frame * 64 * 64 * 4 + (y * 64 + x) * 4;
                [bytes[offset], bytes[offset + 1], bytes[offset + 2]]
            };
            assert!(pixel(0, 32, 32)[0] > 230);
            assert!(pixel(count as usize - 1, 32, 32)[2] > 230);
            let mid = pixel((6 * fps / 30) as usize, 16, 16);
            match kind {
                TransitionKind::CrossDissolve => {
                    assert!((110..=145).contains(&mid[0]) && (110..=145).contains(&mid[2]))
                }
                TransitionKind::WipeLeft | TransitionKind::WipeUp => {
                    assert!(mid[0] > 230, "{kind:?} midpoint{mid:?}")
                }
                TransitionKind::WipeRight | TransitionKind::WipeDown => {
                    assert!(mid[2] > 230, "{kind:?} midpoint{mid:?}")
                }
            };
            plan.verify_sources().await?;
        }
    }
    Ok(())
}
