use lumeryn_core::{
    Result,
    animation::RationalFrame,
    time::FrameRate,
    time_effects::{
        FixedCutTransition, PlaybackSpeed, SpeedAudio, TimeEffect, TransitionKind, blend_rgba,
    },
};

#[test]
fn exact_speed_sampling_at_double_export_rate_and_backward_seek() -> Result<()> {
    let speed = TimeEffect::Speed {
        speed: PlaybackSpeed::Half,
        audio: SpeedAudio::PreservePitch,
    };
    let project = FrameRate::new(30000, 1001)?;
    let output = FrameRate::new(60000, 1001)?;
    for (index, expected) in [(0, (7, 1)), (1, (29, 4)), (7, (35, 4)), (1, (29, 4))] {
        let local = RationalFrame::from_output_frame(index, project, output)?;
        assert_eq!(
            speed
                .source_at(local, 7, 8, 11)?
                .map(RationalFrame::as_ratio),
            Some(expected)
        );
    }
    assert!(
        speed
            .source_at(RationalFrame::new(8, 1)?, 7, 8, 11)?
            .is_none()
    );
    Ok(())
}
#[test]
fn source_exhaustion_and_odd_double_speed_are_explicit_errors() -> Result<()> {
    let double = TimeEffect::Speed {
        speed: PlaybackSpeed::Double,
        audio: SpeedAudio::PreservePitch,
    };
    assert!(double.validate(5, 3, 10).is_err());
    assert!(double.duration_for_source(5).is_err());
    assert_eq!(double.duration_for_source(6)?, 3);
    assert!(double.duration_for_source(u64::MAX).is_err());
    assert!(TimeEffect::Normal.validate(0, 0, 30).is_err());
    Ok(())
}
#[test]
fn freeze_is_exact_and_silent_at_every_sample() -> Result<()> {
    let freeze = TimeEffect::Freeze { source_frame: 19 };
    assert!(freeze.is_silent());
    for time in [
        RationalFrame::new(0, 1)?,
        RationalFrame::new(11, 2)?,
        RationalFrame::new(9, 1)?,
    ] {
        assert_eq!(
            freeze
                .source_at(time, 0, 10, 20)?
                .map(RationalFrame::as_ratio),
            Some((19, 1))
        );
    }
    assert!(
        TimeEffect::Freeze { source_frame: 20 }
            .validate(0, 10, 20)
            .is_err()
    );
    assert!(freeze.duration_for_source(10).is_err());
    Ok(())
}
#[test]
fn public_arithmetic_overflow_is_rejected() -> Result<()> {
    let time = RationalFrame::new(u128::MAX - 1, u128::MAX)?;
    assert!(
        TimeEffect::Speed {
            speed: PlaybackSpeed::Half,
            audio: SpeedAudio::Mute
        }
        .source_at(time, 0, 2, 2)
        .is_err()
    );
    Ok(())
}
#[test]
fn fixed_cut_odd_handles_and_half_open_endpoints() -> Result<()> {
    let transition = FixedCutTransition {
        kind: TransitionKind::CrossDissolve,
        duration_frames: 5,
    };
    let handles = transition.validate_sources(10, 10, 13, 2, 5)?;
    assert_eq!((handles.before_cut, handles.after_cut), (2, 3));
    let sources = transition.sources_at(RationalFrame::new(17, 2)?, 10, 10, 13, 2, 5)?;
    assert_eq!(
        sources.map(|(a, b)| (a.as_ratio(), b.as_ratio())),
        Some(((17, 2), (1, 2)))
    );
    assert_eq!(
        transition.progress(RationalFrame::new(8, 1)?, 10)?,
        Some(0.0)
    );
    assert_eq!(
        transition.progress(RationalFrame::new(17, 2)?, 10)?,
        Some(0.1)
    );
    assert_eq!(
        transition.progress(RationalFrame::new(10, 1)?, 10)?,
        Some(0.4)
    );
    assert_eq!(transition.progress(RationalFrame::new(13, 1)?, 10)?, None);
    assert!(transition.validate_sources(10, 10, 12, 2, 5).is_err());
    assert!(transition.validate_sources(10, 10, 13, 1, 5).is_err());
    assert!(transition.progress(RationalFrame::new(0, 1)?, 1).is_err());
    Ok(())
}
#[test]
fn dissolve_pixels_preserve_alpha_and_wipe_direction() -> Result<()> {
    assert_eq!(
        blend_rgba([255, 0, 0, 255], [0, 0, 255, 255], 0.5)?,
        [128, 0, 128, 255]
    );
    assert_eq!(
        blend_rgba([255, 0, 0, 0], [0, 0, 255, 255], 0.5)?,
        [0, 0, 255, 128]
    );
    for kind in [
        TransitionKind::CrossDissolve,
        TransitionKind::WipeLeft,
        TransitionKind::WipeRight,
        TransitionKind::WipeUp,
        TransitionKind::WipeDown,
    ] {
        let t = FixedCutTransition {
            kind,
            duration_frames: 4,
        };
        assert_eq!(t.incoming_weight(0.0, 0, 0, 4, 4)?, 0.0);
        assert_eq!(t.incoming_weight(1.0, 0, 0, 4, 4)?, 1.0);
    }
    let t = FixedCutTransition {
        kind: TransitionKind::WipeRight,
        duration_frames: 4,
    };
    assert_eq!(t.incoming_weight(0.5, 0, 0, 4, 4)?, 1.0);
    assert_eq!(t.incoming_weight(0.5, 3, 0, 4, 4)?, 0.0);
    assert!(t.incoming_weight(f64::NAN, 0, 0, 4, 4).is_err());
    assert!(t.incoming_weight(0.5, 4, 0, 4, 4).is_err());
    Ok(())
}
#[test]
fn strict_serialized_effects_reject_expression_injection() {
    assert!(
        serde_json::from_str::<TimeEffect>(
            r#"{"kind":"speed","speed":"normal","audio":"preserve_pitch","expression":"system()"}"#
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<TimeEffect>(
            r#"{"kind":"speed","speed":3,"audio":"preserve_pitch"}"#
        )
        .is_err()
    );
}
