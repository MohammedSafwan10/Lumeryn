use lumeryn_core::scene_text::*;
use sha2::{Digest, Sha256};
use uuid::Uuid;
fn renderer() -> SceneRenderer {
    let b =
        include_bytes!("../../../assets/library/packs/inter.v4.1/Inter[opsz,wght].ttf").to_vec();
    SceneRenderer::new(
        FontRef {
            sha256: format!("{:x}", Sha256::digest(&b)),
            face_index: 0,
        },
        b,
    )
    .unwrap()
}
fn text() -> TextNode {
    let b = include_bytes!("../../../assets/library/packs/inter.v4.1/Inter[opsz,wght].ttf");
    TextNode {
        text: "Editable Lumeryn café e\u{301}".into(),
        font: FontRef {
            sha256: format!("{:x}", Sha256::digest(b)),
            face_index: 0,
        },
        x: 20.,
        y: 20.,
        width: 280.,
        height: 130.,
        font_size: 24.,
        line_height: 32.,
        color: Rgba8 {
            r: 71,
            g: 210,
            b: 160,
            a: 128,
        },
        alignment: TextAlignment::Start,
    }
}
#[test]
fn pinned_font_and_missing_glyph_fail() {
    let mut r = renderer();
    let mut t = text();
    t.font.sha256 = "0".repeat(64);
    assert_eq!(r.layout(&t), Err(SceneError::FontMismatch));
    t = text();
    t.text = "你好".into();
    assert_eq!(r.layout(&t), Err(SceneError::MissingGlyph));
}
#[test]
fn wrapping_clusters_are_utf8_and_dpi_independent() {
    let mut r = renderer();
    let mut t = text();
    t.width = 140.;
    let before = r.layout(&t).unwrap();
    assert!(before.line_count > 1);
    assert!(
        before
            .glyphs
            .iter()
            .all(|g| t.text.is_char_boundary(g.start) && t.text.is_char_boundary(g.end))
    );
    let s = SceneDocument {
        width: 320,
        height: 180,
        nodes: vec![SceneNode {
            id: Uuid::from_u128(1),
            content: SceneContent::Text(t.clone()),
        }],
    };
    r.render(&s, 640, 360).unwrap();
    assert_eq!(before, r.layout(&t).unwrap());
}
#[test]
fn transparent_glyphs_are_straight_alpha_and_deterministic() {
    let mut r = renderer();
    let s = SceneDocument {
        width: 320,
        height: 180,
        nodes: vec![SceneNode {
            id: Uuid::from_u128(1),
            content: SceneContent::Text(text()),
        }],
    };
    let a = r.render(&s, 640, 360).unwrap();
    let b = r.render(&s, 640, 360).unwrap();
    assert_eq!(a, b);
    assert!(a.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 128));
    assert!(a.chunks_exact(4).all(|p| if p[3] == 0 {
        p[..3] == [0, 0, 0]
    } else {
        p[..3] == [71, 210, 160] && p[3] <= 128
    }));
    assert_eq!(
        blake3::hash(&a).to_hex().as_str(),
        "c5ed13f2dea3420059a1c2384349f49bc078c8c625e18392b682996afe87c970"
    );
}
#[test]
fn shapes_blend_order_and_clip() {
    let mut r = renderer();
    let shape = |id, fill| SceneNode {
        id: Uuid::from_u128(id),
        content: SceneContent::Rectangle(ShapeBox {
            x: -2.,
            y: -2.,
            width: 12.,
            height: 12.,
            fill,
        }),
    };
    let s = SceneDocument {
        width: 10,
        height: 10,
        nodes: vec![
            shape(
                1,
                Rgba8 {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                },
            ),
            shape(
                2,
                Rgba8 {
                    r: 0,
                    g: 0,
                    b: 255,
                    a: 128,
                },
            ),
        ],
    };
    let p = r.render(&s, 10, 10).unwrap();
    assert_eq!(&p[..4], &[127, 0, 128, 255]);
}
#[test]
fn rejects_invalid_limits_and_overflow() {
    let mut r = renderer();
    let mut t = text();
    t.height = 1.;
    assert_eq!(r.layout(&t), Err(SceneError::TextOverflow));
    t = text();
    t.font_size = f32::NAN;
    assert!(r.layout(&t).is_err());
    let s = SceneDocument {
        width: 1,
        height: 1,
        nodes: vec![SceneNode {
            id: Uuid::from_u128(1),
            content: SceneContent::Text(text()),
        }],
    };
    assert_eq!(
        r.render(&s, 3840, 3840),
        Err(SceneError::Limit("physical font size"))
    );
}
