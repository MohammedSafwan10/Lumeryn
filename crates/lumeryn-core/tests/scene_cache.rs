use lumeryn_core::{
    scene_cache::{DEFAULT_CACHE_BYTES, DEFAULT_CACHE_ENTRIES, StaticSceneCache},
    scene_text::{FontRef, Rgba8, SceneContent, SceneDocument, SceneError, SceneNode, ShapeBox},
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

fn cache(bytes: usize, entries: usize) -> StaticSceneCache {
    let font = include_bytes!("../../../assets/library/packs/inter.v4.1/Inter[opsz,wght].ttf");
    StaticSceneCache::new(
        FontRef {
            sha256: format!("{:x}", Sha256::digest(font)),
            face_index: 0,
        },
        font.to_vec(),
        bytes,
        entries,
    )
    .unwrap()
}

fn document(red: u8) -> SceneDocument {
    SceneDocument {
        width: 10,
        height: 10,
        nodes: vec![SceneNode {
            id: Uuid::from_u128(1),
            content: SceneContent::Rectangle(ShapeBox {
                x: 0.,
                y: 0.,
                width: 10.,
                height: 10.,
                fill: Rgba8 {
                    r: red,
                    g: 12,
                    b: 24,
                    a: 128,
                },
            }),
        }],
    }
}

#[test]
fn static_cache_reuses_identical_pixels_and_matches_disabled_renderer() {
    let mut retained = cache(DEFAULT_CACHE_BYTES, DEFAULT_CACHE_ENTRIES);
    let first = retained.render_cached(&document(55), 10, 10).unwrap();
    let second = retained.render_cached(&document(55), 10, 10).unwrap();
    assert!(!first.cache_hit);
    assert!(second.cache_hit);
    assert!(Arc::ptr_eq(&first.pixels, &second.pixels));
    let uncached = cache(0, 0).render_cached(&document(55), 10, 10).unwrap();
    assert_eq!(first.pixels, uncached.pixels);
    assert_eq!(retained.stats().retained_bytes, 400);
    assert_eq!(retained.stats().hits, 1);
}

#[test]
fn static_cache_content_output_and_node_order_invalidate() {
    let mut retained = cache(DEFAULT_CACHE_BYTES, DEFAULT_CACHE_ENTRIES);
    let first = retained.render_cached(&document(55), 10, 10).unwrap();
    let changed = retained.render_cached(&document(56), 10, 10).unwrap();
    let scaled = retained.render_cached(&document(55), 20, 20).unwrap();
    assert_ne!(first.key, changed.key);
    assert_ne!(first.key, scaled.key);
    let mut two = document(55);
    let mut another = document(56).nodes.remove(0);
    another.id = Uuid::from_u128(2);
    two.nodes.push(another);
    let forward = retained.render_cached(&two, 10, 10).unwrap();
    two.nodes.reverse();
    let reverse = retained.render_cached(&two, 10, 10).unwrap();
    assert_ne!(forward.key, reverse.key);
    assert_ne!(forward.pixels, reverse.pixels);
}

#[test]
fn static_cache_lru_eviction_keeps_outstanding_buffers_valid() {
    let mut retained = cache(800, 2);
    let first = retained.render_cached(&document(55), 10, 10).unwrap();
    retained.render_cached(&document(56), 10, 10).unwrap();
    assert!(
        retained
            .render_cached(&document(55), 10, 10)
            .unwrap()
            .cache_hit
    );
    retained.render_cached(&document(57), 10, 10).unwrap();
    assert_eq!(retained.stats().retained_bytes, 800);
    assert_eq!(retained.stats().entries, 2);
    assert_eq!(retained.stats().evictions, 1);
    assert!(
        !retained
            .render_cached(&document(56), 10, 10)
            .unwrap()
            .cache_hit
    );
    assert_eq!(&first.pixels[..4], &[55, 12, 24, 128]);
}

#[test]
fn static_cache_zero_and_undersized_budgets_do_not_retain() {
    for (bytes, entries) in [(0, 16), (400, 0), (399, 16)] {
        let mut retained = cache(bytes, entries);
        assert!(
            !retained
                .render_cached(&document(55), 10, 10)
                .unwrap()
                .cache_hit
        );
        assert!(
            !retained
                .render_cached(&document(55), 10, 10)
                .unwrap()
                .cache_hit
        );
        assert_eq!(retained.stats().retained_bytes, 0);
        assert_eq!(retained.stats().entries, 0);
    }
}

#[test]
fn static_cache_rejects_invalid_requests_after_valid_hit() {
    let mut retained = cache(400, 1);
    retained.render_cached(&document(55), 10, 10).unwrap();
    let mut invalid = document(55);
    invalid.nodes[0].id = Uuid::nil();
    assert!(retained.render_cached(&invalid, 10, 10).is_err());
    assert!(retained.render_cached(&document(55), 0, 0).is_err());
    assert!(
        retained
            .render_cached(&document(55), u32::MAX, u32::MAX)
            .is_err()
    );
    assert!(retained.render_cached(&document(55), 20, 10).is_err());
    assert!(
        retained
            .render_cached(&document(55), 10, 10)
            .unwrap()
            .cache_hit
    );
    assert_eq!(retained.stats().misses, 1);
}

#[test]
fn static_cache_errors_are_never_retained_and_font_is_verified() {
    use lumeryn_core::scene_text::{TextAlignment, TextNode};
    let font = include_bytes!("../../../assets/library/packs/inter.v4.1/Inter[opsz,wght].ttf");
    let mut retained = cache(400, 1);
    let mut missing = document(55);
    missing.nodes[0].content = SceneContent::Text(TextNode {
        text: "A".into(),
        font: FontRef {
            sha256: "0".repeat(64),
            face_index: 0,
        },
        x: 0.,
        y: 0.,
        width: 10.,
        height: 10.,
        font_size: 2.,
        line_height: 3.,
        color: Rgba8 {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
        alignment: TextAlignment::Start,
    });
    for _ in 0..2 {
        assert!(matches!(
            retained.render_cached(&missing, 10, 10),
            Err(SceneError::FontMismatch)
        ));
    }
    assert_eq!(retained.stats().misses, 2);
    assert_eq!(retained.stats().entries, 0);
    assert!(
        StaticSceneCache::new(
            FontRef {
                sha256: "0".repeat(64),
                face_index: 0
            },
            font.to_vec(),
            400,
            1,
        )
        .is_err()
    );
    assert!(
        StaticSceneCache::new(
            FontRef {
                sha256: format!("{:x}", Sha256::digest(font)),
                face_index: 0
            },
            font.to_vec(),
            DEFAULT_CACHE_BYTES + 1,
            1,
        )
        .is_err()
    );
}

#[test]
#[ignore = "root-coordinated release timing experiment; not a correctness gate"]
fn static_cache_matched_full_call_benchmark() {
    use lumeryn_core::scene_text::{TextAlignment, TextNode};
    use std::time::Instant;
    let font = include_bytes!("../../../assets/library/packs/inter.v4.1/Inter[opsz,wght].ttf");
    let font_ref = FontRef {
        sha256: format!("{:x}", Sha256::digest(font)),
        face_index: 0,
    };
    for (width, height) in [(1280, 720), (1920, 1080), (3840, 2160), (2160, 3840)] {
        let (canvas_width, canvas_height) = if width > height {
            (1280, 720)
        } else {
            (720, 1280)
        };
        let fixture = SceneDocument {
            width: canvas_width,
            height: canvas_height,
            nodes: vec![
                SceneNode {
                    id: Uuid::from_u128(1),
                    content: SceneContent::Rectangle(ShapeBox {
                        x: 20.,
                        y: 20.,
                        width: canvas_width as f32 - 40.,
                        height: 220.,
                        fill: Rgba8 {
                            r: 9,
                            g: 20,
                            b: 32,
                            a: 150,
                        },
                    }),
                },
                SceneNode {
                    id: Uuid::from_u128(2),
                    content: SceneContent::Text(TextNode {
                        text: "Editable Lumeryn\ncafé e\u{301}".into(),
                        font: font_ref.clone(),
                        x: 40.,
                        y: 40.,
                        width: canvas_width as f32 - 80.,
                        height: 180.,
                        font_size: 48.,
                        line_height: 64.,
                        color: Rgba8 {
                            r: 71,
                            g: 210,
                            b: 160,
                            a: 220,
                        },
                        alignment: TextAlignment::Start,
                    }),
                },
            ],
        };
        let mut expected_hash = None;
        for enabled in [false, true] {
            let mut renderer = cache(
                if enabled { DEFAULT_CACHE_BYTES } else { 0 },
                DEFAULT_CACHE_ENTRIES,
            );
            let cold_started = Instant::now();
            let cold = renderer.render_cached(&fixture, width, height).unwrap();
            let cold_ms = cold_started.elapsed().as_secs_f64() * 1000.;
            let hash = format!("{:x}", Sha256::digest(&cold.pixels));
            if let Some(expected) = &expected_hash {
                assert_eq!(expected, &hash);
            } else {
                expected_hash = Some(hash.clone());
            }
            drop(cold);
            let mut times = Vec::with_capacity(20);
            for _ in 0..20 {
                let started = Instant::now();
                let frame = renderer.render_cached(&fixture, width, height).unwrap();
                times.push(started.elapsed().as_secs_f64() * 1000.);
                assert_eq!(format!("{:x}", Sha256::digest(&frame.pixels)), hash);
                assert_eq!(frame.cache_hit, enabled);
            }
            times.sort_by(f64::total_cmp);
            println!(
                "{}",
                serde_json::json!({
                    "width": width, "height": height, "cache_enabled": enabled,
                    "cold_ms": cold_ms, "p50_ms": times[10], "p95_ms": times[19],
                    "rgba_sha256": hash, "font_sha256": font_ref.sha256,
                    "hits": renderer.stats().hits, "misses": renderer.stats().misses,
                    "retained_bytes": renderer.stats().retained_bytes,
                    "peak_working_set": null,
                    "scope": "complete cache call; excludes pixel hashing, FFmpeg, GPU and encoding"
                })
            );
        }
    }
}
