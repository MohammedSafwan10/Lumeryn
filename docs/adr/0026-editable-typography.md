# ADR0026 — Fixed-font CPU reference for editable static scenes

Date:2026-10-12. Status: selected first implementation foundation; application integration/acceptance pending. Extends ADR0005/0009/0023, without replacing Rust's authoritative graph.

Use pinned Parley0.12.0 defaultfalse std with direct Swash0.2.10 defaultfalse std/scale/render. One explicit, content-hashed reviewed font face feeds one headless project-pixel layout and output-scaled grayscale glyph raster. Straight-alpha encoded-SDR RGBA is shared by future preview/export adapters. Add bounded editable Text/Rectangle/Ellipse/Connector data with stable UUIDs and strict serde/JSON schemas. Do not use egui's application text layout for export, OS font substitution or FFmpeg build-specific title layout.

Cosmic-text0.19 initially supplied a smaller integrated path, but its fontdb transitively requires unmaintained ttf-parser0.25.1 (RUSTSEC-2026-0192, no patched version). The maintained Parley/directSwash adapter avoids that dependency and passed the same isolated tests; no advisory exception is introduced. DirectWrite is platform-specific; tiny-skia still needs a shaper; Vello CPU/GPU would introduce an unproven additional stack. No fastest-engine or multilingual-completeness assertion accompanies selection. Revisit only for measured fidelity/maintenance/total-cost benefits.

Both candidate text stacks and Swash have MIT OR Apache-2.0 code licensing; production cargo-deny/transitive notices remain required. Existing Inter4.1 remains unmodified and OFL1.1-attributed. New fallback fonts, models, emoji assets or renderer stacks require separate review. Lumeryn original code remains proprietary.

Limits/errors and exact measured evidence are in [TYPOGRAPHY_IMPLEMENTATION](../research/TYPOGRAPHY_IMPLEMENTATION.md). Final-resolution UHD p50/p95 was25.103/25.364ms; portrait29.134/29.972ms. Use static raster reuse before making playback claims. Missing/unsupported glyphs are errors, not silent substitutions. Initial Inter corpus only proves tested Latin/combining/wrapping; audited multilingual fallbacks, bidi/IME/native editing and decoded UHD acceptance are still required.

Persisted scenes must use a real scene source referencing a checked scene object, not a fake media UUID or flattened image in place of editable semantics. Root owns graph migration, revision-aware edits, source admission, caches and service/CLI/native integration. This module alone is not advertised as a shipped title feature.
