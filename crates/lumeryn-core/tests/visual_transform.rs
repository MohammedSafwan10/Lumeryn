use lumeryn_core::{
    MediaTools,
    visual_transform::{FitMode, SourceCrop, VisualTransform},
};
use std::{ffi::OsString, path::PathBuf, time::Duration};
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn transform_admission_rejects_overflow_and_unknown_contract_fields() -> TestResult {
    let identity = VisualTransform::default();
    assert!(identity.is_identity());
    identity.validate(3840, 2160)?;
    assert!(identity.validate(3841, 1).is_err());
    for crop in [
        SourceCrop {
            x: u32::MAX,
            y: 0,
            width: 1,
            height: 1,
        },
        SourceCrop {
            x: 0,
            y: 0,
            width: 0,
            height: 1,
        },
        SourceCrop {
            x: 0,
            y: 3,
            width: 2,
            height: 2,
        },
    ] {
        assert!(
            VisualTransform {
                crop: Some(crop),
                ..identity
            }
            .validate(4, 4)
            .is_err()
        );
    }
    for transform in [
        VisualTransform {
            opacity_basis_points: 10001,
            ..identity
        },
        VisualTransform {
            pivot_x_basis_points: 10001,
            ..identity
        },
        VisualTransform {
            rotation_millidegrees: 360001,
            ..identity
        },
    ] {
        assert!(transform.validate(4, 4).is_err());
    }
    let mut value = serde_json::to_value(identity)?;
    value["expression"] = "movie(secret)".into();
    assert!(serde_json::from_value::<VisualTransform>(value).is_err());
    assert!(
        VisualTransform {
            fit: FitMode::Cover,
            ..identity
        }
        .lower_filters(3840, 1, 1, 3840)?
        .contains("crop=1:1")
    );
    assert!(identity.lower_filters(4, 4, 3841, 1).is_err());
    assert!(
        VisualTransform {
            rotation_millidegrees: 33000,
            pivot_x_basis_points: 0,
            ..identity
        }
        .lower_filters(4, 4, 721, 4)
        .is_err()
    );
    assert!(
        VisualTransform {
            rotation_millidegrees: 33000,
            ..identity
        }
        .lower_filters(4, 4, 3840, 2160)?
        .contains("rotate=")
    );
    Ok(())
}

fn tools() -> MediaTools {
    MediaTools {
        ffmpeg: std::env::var_os("LUMERYN_FFMPEG").map_or_else(|| "ffmpeg".into(), PathBuf::from),
        ffprobe: std::env::var_os("LUMERYN_FFPROBE")
            .map_or_else(|| "ffprobe".into(), PathBuf::from),
        timeout: Duration::from_secs(20),
    }
}
async fn render(
    source: &[u8],
    width: u32,
    height: u32,
    target_width: u32,
    target_height: u32,
    transform: VisualTransform,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let folder = tempfile::tempdir()?;
    let input = folder.path().join("source.rgba");
    std::fs::write(&input, source)?;
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
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    args.extend([
        format!("{width}x{height}").into(),
        "-i".into(),
        input.into_os_string(),
        "-vf".into(),
        transform
            .lower_filters(width, height, target_width, target_height)?
            .into(),
        "-frames:v".into(),
        "1".into(),
        "-threads".into(),
        "1".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgba".into(),
        "pipe:1".into(),
    ]);
    Ok(tools().run(&tools().ffmpeg, &args).await?)
}
fn pixel(data: &[u8], width: usize, x: usize, y: usize) -> &[u8] {
    let start = (y * width + x) * 4;
    &data[start..start + 4]
}
fn near(actual: &[u8], expected: [u8; 4], tolerance: u8) {
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(a, b)| a.abs_diff(b) <= tolerance),
        "{actual:?} != {expected:?}"
    );
}

#[tokio::test]
#[ignore = "requires configured external FFmpeg"]
async fn actual_crop_cover_flip_quarter_turn_and_opacity_pixels() -> TestResult {
    let source = (0..4)
        .flat_map(|_| {
            (0..8).flat_map(|x| {
                if x < 4 {
                    [255, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                }
            })
        })
        .collect::<Vec<_>>();
    let crop = VisualTransform {
        crop: Some(SourceCrop {
            x: 4,
            y: 0,
            width: 4,
            height: 4,
        }),
        ..Default::default()
    };
    let pixels = render(&source, 8, 4, 4, 4, crop).await?;
    near(pixel(&pixels, 4, 1, 1), [0, 255, 0, 255], 2);
    let flip = VisualTransform {
        flip_horizontal: true,
        ..Default::default()
    };
    let pixels = render(&source, 8, 4, 8, 4, flip).await?;
    near(pixel(&pixels, 8, 1, 1), [0, 255, 0, 255], 2);
    near(pixel(&pixels, 8, 6, 1), [255, 0, 0, 255], 2);
    let cover = VisualTransform {
        fit: FitMode::Cover,
        ..Default::default()
    };
    let pixels = render(&source, 8, 4, 4, 4, cover).await?;
    near(pixel(&pixels, 4, 0, 1), [255, 0, 0, 255], 2);
    near(pixel(&pixels, 4, 3, 1), [0, 255, 0, 255], 2);
    let square = (0..8)
        .flat_map(|_| {
            (0..8).flat_map(|x| {
                if x < 4 {
                    [255, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                }
            })
        })
        .collect::<Vec<_>>();
    let rotate = VisualTransform {
        rotation_millidegrees: 90000,
        ..Default::default()
    };
    let pixels = render(&square, 8, 8, 8, 8, rotate).await?;
    near(pixel(&pixels, 8, 3, 1), [255, 0, 0, 255], 2);
    near(pixel(&pixels, 8, 3, 6), [0, 255, 0, 255], 2);
    let translucent = VisualTransform {
        opacity_basis_points: 2500,
        ..Default::default()
    };
    let pixels = render(&square, 8, 8, 8, 8, translucent).await?;
    near(pixel(&pixels, 8, 1, 1), [255, 0, 0, 64], 2);
    Ok(())
}

#[tokio::test]
#[ignore = "requires configured external FFmpeg"]
async fn actual_arbitrary_pivot_rotation_clips_surface_and_preserves_alpha_edges() -> TestResult {
    // Fully transparent blue must not fringe the red foreground under interpolation.
    let source = (0..16)
        .flat_map(|y| {
            (0..16).flat_map(move |x| {
                if (4..12).contains(&x) && (4..12).contains(&y) {
                    [255, 0, 0, 128]
                } else {
                    [0, 0, 255, 0]
                }
            })
        })
        .collect::<Vec<_>>();
    let transform = VisualTransform {
        rotation_millidegrees: 33000,
        ..Default::default()
    };
    let pixels = render(&source, 16, 16, 16, 16, transform).await?;
    assert_eq!(pixels.len(), 16 * 16 * 4);
    assert_eq!(pixel(&pixels, 16, 0, 0)[3], 0);
    near(pixel(&pixels, 16, 7, 7), [255, 0, 0, 128], 3);
    let edges = pixels
        .chunks_exact(4)
        .filter(|p| p[3] > 8 && p[3] < 120)
        .collect::<Vec<_>>();
    assert!(
        !edges.is_empty(),
        "fixture must exercise interpolated edges"
    );
    assert!(
        edges.iter().all(|p| p[2] <= 2 && p[0] > 240),
        "transparent blue contaminated red edge"
    );
    let non_center = VisualTransform {
        rotation_millidegrees: 33000,
        pivot_x_basis_points: 4000,
        ..Default::default()
    };
    let mapped = render(&source, 16, 16, 16, 16, non_center).await?;
    assert!(
        mapped
            .chunks_exact(4)
            .filter(|p| p[3] > 8)
            .all(|p| p[2] <= 2 && p[0] > 240)
    );
    let pivot = VisualTransform {
        rotation_millidegrees: 90000,
        pivot_x_basis_points: 0,
        pivot_y_basis_points: 0,
        ..Default::default()
    };
    let pixels = render(&source, 16, 16, 16, 16, pivot).await?;
    assert!(
        pixels.chunks_exact(4).all(|p| p[3] == 0),
        "top-left clockwise pivot should rotate inset shape outside fixed box"
    );
    Ok(())
}
