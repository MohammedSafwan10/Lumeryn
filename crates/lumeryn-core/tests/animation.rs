use lumeryn_core::{
    Result,
    animation::{
        Animation, AnimationChannel, AnimationProperty, Interpolation, Keyframe, RationalFrame,
    },
};

fn curve(interpolation: Interpolation) -> Animation {
    Animation {
        channels: vec![AnimationChannel {
            property: AnimationProperty::X,
            keys: vec![
                Keyframe {
                    at_frame: 0,
                    value: 0,
                    interpolation,
                },
                Keyframe {
                    at_frame: 30,
                    value: 300_000,
                    interpolation,
                },
            ],
        }],
    }
}

#[test]
fn fractional_output_seek_is_order_independent() -> Result<()> {
    let animation = curve(Interpolation::Linear);
    let project = "30000/1001".parse()?;
    let output = "60".parse()?;
    let t = RationalFrame::from_output_frame(30, project, output)?;
    let expected = 150.0 / 1.001;
    let value = animation.evaluate(31, t)?[0].1;
    assert!((value - expected).abs() < 1e-10);
    animation.evaluate(31, RationalFrame::new(29, 1)?)?;
    animation.evaluate(31, RationalFrame::new(0, 1)?)?;
    assert_eq!(animation.evaluate(31, t)?[0].1, value);
    assert_eq!(
        animation.evaluate(31, RationalFrame::new(15_000, 1001)?)?[0].1,
        value
    );
    Ok(())
}

#[test]
fn clip_is_half_open_and_exact_key_uses_new_value() -> Result<()> {
    assert_eq!(
        RationalFrame::new(10, 1)?.in_clip(10, 30)?,
        Some(RationalFrame::new(0, 1)?)
    );
    assert!(RationalFrame::new(40, 1)?.in_clip(10, 30)?.is_none());
    assert!(RationalFrame::new(9, 1)?.in_clip(10, 30)?.is_none());
    let animation = curve(Interpolation::Hold);
    assert_eq!(
        animation.evaluate(31, RationalFrame::new(599, 20)?)?[0].1,
        0.0
    );
    assert_eq!(
        animation.evaluate(31, RationalFrame::new(30, 1)?)?[0].1,
        300.0
    );
    assert!(animation.evaluate(30, RationalFrame::new(30, 1)?).is_err());
    Ok(())
}

#[test]
fn bounded_smoothstep_no_overshoot_and_identity() -> Result<()> {
    for interpolation in [Interpolation::Linear, Interpolation::SmoothStep] {
        let animation = curve(interpolation);
        let values = (0..=600)
            .map(|sample| {
                animation
                    .evaluate(31, RationalFrame::new(sample, 20)?)
                    .map(|values| values[0].1)
            })
            .collect::<Result<Vec<_>>>()?;
        assert!(values.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(values.iter().all(|value| (0.0..=300.0).contains(value)));
        assert_eq!(values[0], 0.0);
        assert_eq!(values[300], 150.0);
        assert_eq!(values[600], 300.0);
    }
    assert!(
        Animation::default()
            .evaluate(1, RationalFrame::new(0, 1)?)?
            .is_empty()
    );
    Ok(())
}

#[test]
fn rejects_invalid_order_bounds_duplicates_and_overflow() -> Result<()> {
    let mut animation = curve(Interpolation::Linear);
    animation.channels[0].keys[1].at_frame = 0;
    assert!(animation.validate(30).is_err());
    animation.channels[0].keys[1].at_frame = 31;
    assert!(animation.validate(30).is_err());
    animation.channels[0].keys[1].at_frame = 30;
    animation.channels[0].keys[1].value = i64::MAX;
    assert!(animation.validate(30).is_err());
    animation.channels[0].keys[1].value = 0;
    animation.channels.push(animation.channels[0].clone());
    assert!(animation.validate(30).is_err());
    assert!(RationalFrame::new(0, 0).is_err());
    assert!(RationalFrame::new(1, u128::MAX)?.in_clip(2, 1).is_err());
    assert!(RationalFrame::new(0, 1)?.in_clip(u64::MAX, 1).is_err());
    assert!(serde_json::from_str::<Animation>(r#"{"channels":[],"script":"evil"}"#).is_err());
    Ok(())
}

#[test]
fn starters_expand_to_repeatable_editable_objects() -> Result<()> {
    use lumeryn_core::animation::{Starter, StarterContent, expand_starter};
    let ids = [uuid::Uuid::new_v4(), uuid::Uuid::new_v4()];
    for starter in [
        Starter::CleanTitle,
        Starter::LowerThird,
        Starter::Callout,
        Starter::ChapterCard,
        Starter::PictureInPicture,
    ] {
        let objects = expand_starter(starter, 1920, 1080, 120, "Chapter one", &ids)?;
        assert_eq!(
            objects,
            expand_starter(starter, 1920, 1080, 120, "Chapter one", &ids)?
        );
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].id, ids[0]);
        assert!(
            objects
                .iter()
                .all(|object| object.x + object.width <= 1920 && object.y + object.height <= 1080)
        );
        assert_eq!(
            objects[1]
                .animation
                .evaluate(120, RationalFrame::new(8, 1)?)?[0]
                .1,
            1.0
        );
        if starter == Starter::PictureInPicture {
            assert_eq!(objects[1].content, StarterContent::MediaSlot {});
        }
    }
    assert!(expand_starter(Starter::CleanTitle, 320, 180, 1, "Title", &ids).is_err());
    assert!(
        expand_starter(
            Starter::CleanTitle,
            320,
            180,
            120,
            "Title",
            &[ids[0], ids[0]]
        )
        .is_err()
    );
    Ok(())
}
