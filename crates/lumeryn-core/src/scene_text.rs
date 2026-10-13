//! Bounded headless static scene raster foundation. No filesystem or system-font lookup.
//!
//! Layout uses project pixels; physical glyphs are rasterized at output scale. The returned
//! encoded-SDR RGBA8 pixels use straight alpha, including zero RGB when alpha is zero.
use parley::{
    Alignment, AlignmentOptions, FontContext, Layout, LayoutContext, LineHeight,
    PositionedLayoutItem, StyleProperty,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use swash::{
    scale::{Render, ScaleContext, Source, image::Content},
    zeno::{Format, Vector},
};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_SCENE_NODES: usize = 64;
pub const MAX_TEXT_BYTES: usize = 4096;
pub const MAX_SCENE_GLYPHS: usize = 8192;
pub const MAX_FONT_BYTES: usize = 16 * 1024 * 1024;

/// The original reviewed OFL1.1 Inter4.1 face shipped with the application.
/// Project files cannot replace this admission identity with their own font bytes.
pub fn audited_font() -> (FontRef, Vec<u8>) {
    (
        FontRef {
            sha256: "29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031".into(),
            face_index: 0,
        },
        include_bytes!("../../../assets/library/packs/inter.v4.1/Inter[opsz,wght].ttf").to_vec(),
    )
}

/// Pure bounded semantic checks plus shaping of only admitted built-in font bytes.
pub fn validate_audited_documents<'a>(
    documents: impl IntoIterator<Item = &'a SceneDocument>,
) -> Result<(), SceneError> {
    let mut renderer = None;
    for document in documents {
        document.validate()?;
        for node in &document.nodes {
            if let SceneContent::Text(text) = &node.content {
                if renderer.is_none() {
                    let (font, bytes) = audited_font();
                    renderer = Some(SceneRenderer::new(font, bytes)?);
                }
                if let Some(renderer) = renderer.as_mut() {
                    renderer.layout(text)?;
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontRef {
    pub sha256: String,
    pub face_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextAlignment {
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextNode {
    pub text: String,
    pub font: FontRef,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
    pub line_height: f32,
    pub color: Rgba8,
    pub alignment: TextAlignment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShapeBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub fill: Rgba8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Connector {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub stroke_width: f32,
    pub color: Rgba8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "properties",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SceneContent {
    Text(TextNode),
    Rectangle(ShapeBox),
    Ellipse(ShapeBox),
    Connector(Connector),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneNode {
    pub id: Uuid,
    pub content: SceneContent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SceneDocument {
    pub width: u32,
    pub height: u32,
    pub nodes: Vec<SceneNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlyphCluster {
    pub start: usize,
    pub end: usize,
    pub glyph_id: u16,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    pub line_count: usize,
    pub glyphs: Vec<GlyphCluster>,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SceneError {
    #[error("scene limit exceeded: {0}")]
    Limit(&'static str),
    #[error("invalid scene property: {0}")]
    Invalid(&'static str),
    #[error("font bytes or face do not match the pinned font reference")]
    FontMismatch,
    #[error("pinned font has no glyph for a text cluster; install an audited matching font")]
    MissingGlyph,
    #[error("text exceeds its layout box; enlarge the box or reduce its size")]
    TextOverflow,
    #[error("glyph has no supported grayscale raster")]
    UnsupportedGlyphRaster,
}

/// One fixed face per renderer: no OS font database, implicit fallback, or family ambiguity.
pub struct SceneRenderer {
    font: FontRef,
    family: String,
    fonts: FontContext,
    layout_context: LayoutContext<()>,
    scale_context: ScaleContext,
}

impl SceneRenderer {
    pub fn new(font: FontRef, bytes: Vec<u8>) -> Result<Self, SceneError> {
        if bytes.is_empty() || bytes.len() > MAX_FONT_BYTES {
            return Err(SceneError::Limit("font bytes"));
        }
        if format!("{:x}", Sha256::digest(&bytes)) != font.sha256 {
            return Err(SceneError::FontMismatch);
        }
        // Initial reviewed single-face files only; collections/variation edits need a later contract.
        if font.face_index != 0
            || bytes.starts_with(b"ttcf")
            || swash::FontRef::from_index(&bytes, 0).is_none()
        {
            return Err(SceneError::FontMismatch);
        }
        let mut fonts = FontContext {
            collection: parley::fontique::Collection::new(parley::fontique::CollectionOptions {
                shared: false,
                system_fonts: false,
            }),
            source_cache: Default::default(),
        };
        let registrations = fonts.collection.register_fonts(bytes.into(), None);
        if registrations.len() != 1 || registrations[0].1.len() != 1 {
            return Err(SceneError::FontMismatch);
        }
        let family = fonts
            .collection
            .family_name(registrations[0].0)
            .ok_or(SceneError::FontMismatch)?
            .to_owned();
        Ok(Self {
            font,
            family,
            fonts,
            layout_context: LayoutContext::new(),
            scale_context: ScaleContext::new(),
        })
    }

    fn text_layout(&mut self, node: &TextNode) -> Result<Layout<()>, SceneError> {
        validate_text(node)?;
        if node.font != self.font {
            return Err(SceneError::FontMismatch);
        }
        let mut builder =
            self.layout_context
                .ranged_builder(&mut self.fonts, &node.text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(parley::FontFamily::Source(
            self.family.as_str().into(),
        )));
        builder.push_default(StyleProperty::FontSize(node.font_size));
        builder.push_default(StyleProperty::LineHeight(LineHeight::Absolute(
            node.line_height,
        )));
        let mut layout = builder.build(&node.text);
        layout.break_all_lines(Some(node.width));
        layout.align(
            match node.alignment {
                TextAlignment::Start => Alignment::Start,
                TextAlignment::Center => Alignment::Center,
                TextAlignment::End => Alignment::End,
            },
            AlignmentOptions::default(),
        );
        let mut glyph_count = 0;
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    for glyph in run.glyphs() {
                        if glyph.id == 0 {
                            return Err(SceneError::MissingGlyph);
                        }
                        glyph_count += 1;
                    }
                }
            }
        }
        if glyph_count > MAX_SCENE_GLYPHS {
            return Err(SceneError::Limit("glyph count"));
        }
        if layout.height() > node.height + 0.01 || layout.width() > node.width + 0.01 {
            return Err(SceneError::TextOverflow);
        }
        Ok(layout)
    }

    pub fn layout(&mut self, node: &TextNode) -> Result<TextLayout, SceneError> {
        let layout = self.text_layout(node)?;
        let mut result = TextLayout {
            line_count: 0,
            glyphs: Vec::new(),
            width: layout.width(),
            height: layout.height(),
        };
        for line in layout.lines() {
            result.line_count += 1;
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    let mut x = run.offset();
                    for cluster in run.run().visual_clusters() {
                        let range = cluster.text_range();
                        for glyph in cluster.glyphs() {
                            result.glyphs.push(GlyphCluster {
                                start: range.start,
                                end: range.end,
                                glyph_id: u16::try_from(glyph.id)
                                    .map_err(|_| SceneError::UnsupportedGlyphRaster)?,
                                x: x + glyph.x,
                                y: run.baseline() + glyph.y,
                            });
                        }
                        x += cluster.advance();
                    }
                }
            }
        }
        Ok(result)
    }

    pub fn render(
        &mut self,
        scene: &SceneDocument,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, SceneError> {
        scene.validate()?;
        validate_canvas(width, height)?;
        // Aspect-preserving output: an explicit canvas conversion must precede rasterization.
        if u64::from(scene.width) * u64::from(height) != u64::from(scene.height) * u64::from(width)
        {
            return Err(SceneError::Invalid("output aspect ratio"));
        }
        let size = (width as usize)
            .checked_mul(height as usize)
            .and_then(|p| p.checked_mul(4))
            .ok_or(SceneError::Limit("frame bytes"))?;
        let mut pixels = vec![0; size];
        let scale = width as f32 / scene.width as f32;
        for node in &scene.nodes {
            match &node.content {
                SceneContent::Text(text) => {
                    self.render_text(text, scale, width, height, &mut pixels)?
                }
                SceneContent::Rectangle(shape) => {
                    raster_shape(shape, false, scale, width, height, &mut pixels)
                }
                SceneContent::Ellipse(shape) => {
                    raster_shape(shape, true, scale, width, height, &mut pixels)
                }
                SceneContent::Connector(line) => {
                    raster_connector(line, scale, width, height, &mut pixels)
                }
            }
        }
        Ok(pixels)
    }

    fn render_text(
        &mut self,
        text: &TextNode,
        scale: f32,
        width: u32,
        height: u32,
        pixels: &mut [u8],
    ) -> Result<(), SceneError> {
        if text.font_size * scale > 1024.0 {
            return Err(SceneError::Limit("physical font size"));
        }
        let layout = self.text_layout(text)?;
        let mut glyph_count = 0;
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    glyph_count += run.glyphs().count();
                }
            }
        }
        if glyph_count as f64 * f64::from(text.font_size * scale).powi(2) > 64.0 * 1024.0 * 1024.0 {
            return Err(SceneError::Limit("glyph raster working set"));
        }
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    let data = run.run().font();
                    let font = swash::FontRef::from_index(data.data.as_ref(), data.index as usize)
                        .ok_or(SceneError::FontMismatch)?;
                    let coords: Vec<i16> = run
                        .run()
                        .normalized_coords()
                        .iter()
                        .map(|c| c.to_bits())
                        .collect();
                    let mut scaler = self
                        .scale_context
                        .builder(font)
                        .size(run.run().font_size() * scale)
                        .hint(false)
                        .normalized_coords(coords)
                        .build();
                    for glyph in run.positioned_glyphs() {
                        let x = (text.x + glyph.x) * scale;
                        let y = (text.y + glyph.y) * scale;
                        let id = u16::try_from(glyph.id)
                            .map_err(|_| SceneError::UnsupportedGlyphRaster)?;
                        let image = Render::new(&[Source::Outline])
                            .format(Format::Alpha)
                            .offset(Vector::new(x - x.floor(), y - y.floor()))
                            .render(&mut scaler, id);
                        let Some(image) = image else {
                            return Err(SceneError::UnsupportedGlyphRaster);
                        };
                        if image.content != Content::Mask {
                            return Err(SceneError::UnsupportedGlyphRaster);
                        }
                        let left = x.floor() as i32 + image.placement.left;
                        let top = y.floor() as i32 - image.placement.top;
                        for (index, coverage) in image.data.iter().copied().enumerate() {
                            let px = left + (index % image.placement.width as usize) as i32;
                            let py = top + (index / image.placement.width as usize) as i32;
                            if px as f32 >= text.x * scale
                                && py as f32 >= text.y * scale
                                && (px as f32) < (text.x + text.width) * scale
                                && (py as f32) < (text.y + text.height) * scale
                            {
                                blend(pixels, width, height, px, py, text.color, coverage);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl SceneDocument {
    pub fn validate(&self) -> Result<(), SceneError> {
        validate_canvas(self.width, self.height)?;
        if self.nodes.len() > MAX_SCENE_NODES {
            return Err(SceneError::Limit("node count"));
        }
        let mut ids = std::collections::HashSet::new();
        for node in &self.nodes {
            if node.id.is_nil() || !ids.insert(node.id) {
                return Err(SceneError::Invalid("node ID"));
            }
            match &node.content {
                SceneContent::Text(text) => validate_text(text)?,
                SceneContent::Rectangle(shape) | SceneContent::Ellipse(shape) => {
                    validate_box(shape.x, shape.y, shape.width, shape.height)?
                }
                SceneContent::Connector(line) => {
                    if ![line.x1, line.y1, line.x2, line.y2]
                        .into_iter()
                        .all(valid_coord)
                        || !valid_size(line.stroke_width)
                    {
                        return Err(SceneError::Invalid("connector"));
                    }
                    if line.x1 == line.x2 && line.y1 == line.y2 {
                        return Err(SceneError::Invalid("zero-length connector"));
                    }
                }
            }
        }
        Ok(())
    }
}

fn validate_canvas(width: u32, height: u32) -> Result<(), SceneError> {
    if width == 0 || height == 0 || width > 3840 || height > 3840 {
        Err(SceneError::Limit("canvas dimensions"))
    } else {
        Ok(())
    }
}
fn valid_coord(value: f32) -> bool {
    value.is_finite() && value.abs() <= 8192.0
}
fn valid_size(value: f32) -> bool {
    value.is_finite() && value > 0.0 && value <= 8192.0
}
fn validate_box(x: f32, y: f32, width: f32, height: f32) -> Result<(), SceneError> {
    if !valid_coord(x) || !valid_coord(y) || !valid_size(width) || !valid_size(height) {
        Err(SceneError::Invalid("box geometry"))
    } else {
        Ok(())
    }
}
fn validate_text(text: &TextNode) -> Result<(), SceneError> {
    validate_box(text.x, text.y, text.width, text.height)?;
    if text.text.len() > MAX_TEXT_BYTES {
        return Err(SceneError::Limit("text bytes"));
    }
    if text
        .text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(SceneError::Invalid("text control character"));
    }
    if !valid_size(text.font_size)
        || text.font_size > 1024.0
        || !valid_size(text.line_height)
        || text.line_height < text.font_size
    {
        return Err(SceneError::Invalid("text metrics"));
    }
    if text.font.sha256.len() != 64
        || !text
            .font
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(SceneError::Invalid("font hash"));
    }
    Ok(())
}

fn blend(pixels: &mut [u8], width: u32, height: u32, x: i32, y: i32, color: Rgba8, coverage: u8) {
    if x < 0 || y < 0 || x as u32 >= width || y as u32 >= height {
        return;
    }
    let offset = (y as usize * width as usize + x as usize) * 4;
    let dst = &mut pixels[offset..offset + 4];
    let sa = (u32::from(color.a) * u32::from(coverage) + 127) / 255;
    let da = u32::from(dst[3]);
    let alpha_numerator = sa * 255 + da * (255 - sa);
    if alpha_numerator == 0 {
        dst.fill(0);
        return;
    }
    for (channel, source) in dst[..3].iter_mut().zip([color.r, color.g, color.b]) {
        let numerator = u32::from(source) * sa * 255 + u32::from(*channel) * da * (255 - sa);
        *channel = ((numerator + alpha_numerator / 2) / alpha_numerator) as u8;
    }
    dst[3] = ((alpha_numerator + 127) / 255) as u8;
}

fn raster_shape(
    shape: &ShapeBox,
    ellipse: bool,
    scale: f32,
    width: u32,
    height: u32,
    pixels: &mut [u8],
) {
    if !ellipse {
        let left = shape.x * scale;
        let top = shape.y * scale;
        let right = (shape.x + shape.width) * scale;
        let bottom = (shape.y + shape.height) * scale;
        for py in (top.floor().max(0.0) as u32)..(bottom.ceil().clamp(0.0, height as f32) as u32) {
            for px in (left.floor().max(0.0) as u32)..(right.ceil().clamp(0.0, width as f32) as u32)
            {
                let area = (right.min(px as f32 + 1.0) - left.max(px as f32)).max(0.0)
                    * (bottom.min(py as f32 + 1.0) - top.max(py as f32)).max(0.0);
                blend(
                    pixels,
                    width,
                    height,
                    px as i32,
                    py as i32,
                    shape.fill,
                    (area * 255.0).round() as u8,
                );
            }
        }
        return;
    }
    raster_coverage(
        shape.x,
        shape.y,
        shape.width,
        shape.height,
        scale,
        width,
        height,
        pixels,
        shape.fill,
        |x, y| {
            !ellipse
                || ((x - shape.x) / shape.width * 2.0 - 1.0).powi(2)
                    + ((y - shape.y) / shape.height * 2.0 - 1.0).powi(2)
                    <= 1.0
        },
    );
}
fn raster_connector(line: &Connector, scale: f32, width: u32, height: u32, pixels: &mut [u8]) {
    let radius = line.stroke_width / 2.0;
    let x = line.x1.min(line.x2) - radius;
    let y = line.y1.min(line.y2) - radius;
    let w = (line.x2 - line.x1).abs() + line.stroke_width;
    let h = (line.y2 - line.y1).abs() + line.stroke_width;
    raster_coverage(
        x,
        y,
        w,
        h,
        scale,
        width,
        height,
        pixels,
        line.color,
        |px, py| {
            let dx = line.x2 - line.x1;
            let dy = line.y2 - line.y1;
            let t =
                (((px - line.x1) * dx + (py - line.y1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
            (px - (line.x1 + t * dx)).powi(2) + (py - (line.y1 + t * dy)).powi(2) <= radius * radius
        },
    );
}

#[allow(clippy::too_many_arguments)] // Internal bounded scan helper shares a single frame buffer.
fn raster_coverage(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    scale: f32,
    width: u32,
    height: u32,
    pixels: &mut [u8],
    color: Rgba8,
    inside: impl Fn(f32, f32) -> bool,
) {
    let left = (x * scale).floor().max(0.0) as u32;
    let top = (y * scale).floor().max(0.0) as u32;
    let right = ((x + w) * scale).ceil().clamp(0.0, width as f32) as u32;
    let bottom = ((y + h) * scale).ceil().clamp(0.0, height as f32) as u32;
    for py in top..bottom {
        for px in left..right {
            let hits = [0.25, 0.75]
                .into_iter()
                .flat_map(|oy| [0.25, 0.75].into_iter().map(move |ox| (ox, oy)))
                .filter(|(ox, oy)| {
                    let sx = (px as f32 + ox) / scale;
                    let sy = (py as f32 + oy) / scale;
                    sx >= x && sx < x + w && sy >= y && sy < y + h && inside(sx, sy)
                })
                .count();
            blend(
                pixels,
                width,
                height,
                px as i32,
                py as i32,
                color,
                ((hits * 255 + 2) / 4) as u8,
            );
        }
    }
}
