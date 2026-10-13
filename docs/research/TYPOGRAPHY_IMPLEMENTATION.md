# Editable typography: executed selection experiment

Date:2026-10-12. Status: isolated headless text/shape foundation implemented. Persisted scenes, CLI/Studio editing and decoded export acceptance are root-owned integration work, not complete from this module.

## Selection, maturity and maintenance

Select **Parley0.12.0**, default features disabled, `std`, with **Swash0.2.10**, default features disabled, `std/scale/render`. Parley supplies positioned glyphs; a small direct Swash grayscale-outline adapter produces CPU masks. Both released stacks compiled on Windows with Rust1.97.1. [Parley official API](https://docs.rs/parley/0.12.0/parley/), [maintainer repository and license](https://github.com/linebender/parley), [Swash official API](https://docs.rs/swash/0.2.10/swash/). Code is MIT OR Apache-2.0; preserve selected licenses/attribution and audit transitive dependencies. Fontique uses maintained Read-fonts0.41/Skrifa0.44/HarfRust0.12. None changes Lumeryn's proprietary status.

Cosmic-text0.19.0 initially compiled and passed five focused tests, with a convenient integrated raster path. Root's actual dependency audit found `fontdb0.23 -> ttf-parser0.25.1`, subject to [RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192): unmaintained with no patched version. The [maintainer issue](https://github.com/harfbuzz/ttf-parser/issues/217) also reports a suspected algorithmic-complexity problem. Reviewed font admission reduces exposure but does not eliminate maintenance risk. Switching to Parley/directSwash avoids fontdb/ttf-parser; no advisory exception is requested.

An early micro-experiment measured Parley layout alone0.017/0.028ms p50/p95 versus cosmic layout plus small72px glyph raster0.252/0.340ms. This is **not** a fastest-stack comparison: raster work differs, and the early UHD case did not grow the glyphs. Final-resolution measurements below supersede it for title rendering. DirectWrite couples appearance to Windows/platform fonts; FFmpeg drawtext introduces build-specific semantics; tiny-skia still needs a shaper; Vello CPU/GPU requires another adapter and total-cost fidelity measurements. The maintained parser boundary earned the small adapter's complexity through an actual audit, not speculation. No Vello or OS font enumeration is introduced.

## Semantic and font contract

`SceneDocument {width,height,nodes}` stores ordered `SceneNode {id:Uuid,content}`. Content is strict tagged Text/Rectangle/Ellipse/Connector. Text has UTF-8 text, `FontRef {sha256,face_index}`, project-pixel x/y/width/height, font size, line-height, Start/Center/End and RGBA fill. Shapes have editable geometry/fill or stroke. IDs must be unique/nonnil locally; root must enforce graph-wide identity. Scene placement/time spans and shared edits remain root-owned. No scripts, expressions, URLs, private UI graph or fake media reference.

`SceneRenderer::new(FontRef,bytes)` checks SHA256 and accepts one reviewed single-face font file at face0. Font collections fail explicitly. Fontique `system_fonts=false` registers only those bytes; paragraph direction comes from text, without ambient locale/system-font substitution. The caller must admit reviewed catalogue bytes; self-supplied hashes alone are not a security allowlist. Variation axes/rich spans/fallback maps/color emoji are not exposed yet.

Existing unmodified Inter4.1 regular face SHA256: `29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031`. It already has reviewed OFL1.1 catalogue rights/notices. [Official Inter](https://github.com/rsms/inter), [official OFL](https://openfontlicense.org/open-font-license-official-text/). No new fonts, subsets or copied commercial assets were downloaded. Missing hash/face/glyph and unsupported raster content fail clearly.

Layout uses logical project pixels, then positioned glyphs and Parley's normalized variation coordinates rasterize at the requested physical output scale. Preview size and UI DPI never choose wrapping. Output aspect must match the scene; portrait uses an explicit portrait scene. UTF-8 cluster ranges come directly from Parley. Limits:64nodes,4096text bytes,8192glyphs/text,3840per canvas axis,16MiBfont bytes,1024physical font size, conservative64MiBglyph estimate. These bounds do not prove arbitrary hostile font safety; initial admission remains reviewed fonts only.

Latin/combining marks/wrapping were tested. Chinese in Inter correctly fails MissingGlyph. Arabic/Indic/CJK/bidi/fallback/IME corpus acceptance remains pending audited fonts; do not infer universal support from upstream capabilities. Start/End use Parley's contextual paragraph alignment; that contract alone is not a multilingual acceptance result.

## Raster policy and measurements

Output is straight encoded-SDR RGBA8 with zero RGB at zero alpha. Coverage explicitly multiplies fill alpha; integer source-over preserves order. Grayscale outlines only; no silently missing color/subpixel glyphs. Rectangles use analytic pixel-area coverage; ellipses and round-cap connectors use four samples. General paths/gradients/arbitrary strokes/HDR/linear-light blending remain outside this foundation.

Intel i7-12700F, Windows, Rust1.97.1 release. Original1280x720 scene:1200x220 translucent rectangle, two lines at58logical font size. Raster scales glyphs1.5x/3x; portrait explicitly rearranges720x1280 with wrapping. One cold plus20warm samples, sorted p50 sample10/p95 sample19; includes layout/raster/allocation/shapes/blending, excludes FFmpeg/decode/GPU transfers.

| Output | Cold ms | p50 / p95 ms | RGBA SHA256 |
|---|---:|---:|---|
|1920x1080|7.069|6.421 /6.628|`4bdb3bda4d77db26c7114ecfbe83a45f04eeff48929ad9a8faa46903c29b952e`|
|3840x2160|24.904|25.103 /25.364|`b83cc5053fa74e005532a4129605c55c5fd36aed73243dfd6636551dfc12fa07`|
|2160x3840|30.145|29.134 /29.972|`a87c32f05db21a254704f278b3dff773f4620f07640bceca18cbe768ffbf1347`|

Parley renderer initialization489microseconds. Glyph images are short-lived; Swash ScaleContext retains its bounded internal LRU. Earlier cosmic sampling showed39,624,704bytes working set (20ms sampling can miss peaks), standalone binary2,540,544bytes; no production memory/binary-growth claim. In that rejected experiment analytic rectangle coverage improved UHD p50/p95 from40.397/50.945 to25.055/25.720ms with identical hash. The table is the maintained replacement, a separate run.

UHD raster exceeds16.67ms:60FPS titles are not established. Reuse immutable static scene rasters by full scene/font/output/policy hash; invalidate on any appearance change. Transform-only animation can reuse rasters if semantics allow. GPU adoption must compare full layout/raster/upload/readback and golden fidelity before choosing a backend.

Ignored evidence directory `.lumeryn-scratch/typography-spike/` includes source/lockfile, RGBA/PNGs and measurements. The replacement's1080p/portrait pictures were actually inspected: scaled mint letters, correct accents, transparent backgrounds and deliberate portrait wrapping. Five isolated Parley release tests and strict module Clippy passed. Earlier cosmic's five production focused tests passed before replacement. Root reruns production full/deny/native/export gates after the dependency switch.

Title completion still requires graph migration/history, editable save/reopen, CLI/schema/discovery/native controls, shared preview/export adapters, actual complete layered UHD/portrait exports with alpha-edge/A/V checks and audited multilingual/device acceptance. Experiment PNGs do not meet those exit gates.
