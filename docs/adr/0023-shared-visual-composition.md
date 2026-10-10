# ADR0023: Shared visual composition and staged editable features

Date: 2026-10-10. Status: accepted delivery direction; reference integration, schema3 and new product capabilities are not implemented.

## Context

Owner authorizes multiple layers/gaps/images/transforms, editable text/vectors/animation/templates, colour/effects/fades and transitions/speed/freeze. Current schema2 and preview/export only implement a contiguous single-video-track subset. Appending Inspector buttons or export-only filters would break agent/manual parity. Existing audio, history and source proofs must survive.

## Decision

1. Keep the authoritative Rust graph/service, native egui/eframe/wgpu and external FFmpeg. Add a checked headless VisualPlan in the existing core; preview, inspection and export consume the same time/order/source/transform/colour semantics. No window dependency or second editing graph.
2. Prove offgraph fixtures first. Initial new picture composition uses one generated CPU reference lowering shared by preview/export. Retain the existing sequential path after conformance testing. GPU acceleration is an alternative implementation of the same stored policy only after total-cost and pixel tests.
3. Make colour/alpha/source metadata policy explicit and versioned. Do not call ordinary FFmpeg overlay linear-light colour management. Establish tested SDR formats/range/matrix/transfer/sampling/alpha before new writes. Unknown metadata is handled explicitly. Future colour-policy changes require appearance conversion; a fallback cannot silently change pixel math.
4. Propose schema3 typed ordered tracks and video/image/scene/freeze variants. First integrate version-aware validation and composed bounded all-history migrations, preserving IDs/revisions/commands/receipts/undo and explicit service-backed backup. No schema constant change alone; no automatic upgrade on open.
5. Use rational evaluation time, integer/checked spans and distinct time units. Source/transform/effect order and half-open endpoints are versioned. Freeze is a distinct hold; speed is a positive rational separate from FPS. Transitions link explicit cuts and verified source handles, with explicit audio/ripple policies.
6. Ship CLI/schema/capabilities/preflight, native controls, live notifications and decoded output in each slice. Transient native drafts use the same evaluator and commit once; conflicts/cancellation never silently overwrite an external edit. No placeholder capability or ignored unsupported object.
7. Prioritize images/layers and static editable titles, then deterministic animation/original explainers before broad effects. Use one measured creative shaping stack, not competing Parley/cosmic authoritative layouts. Existing font UI integration is not exported typography. Restrict Vello/resvg to validated nonpanicking/data-only subsets; exact dependency selection has its own compatibility/licence experiment.

## Alternatives and consequences

- Independent Studio shaders plus FFmpeg effects are quicker demos but create appearance/timing divergence; not the default architecture.
- New Rust CPU/GPU decoders or broad libav FFI are unnecessary to prove the slice; measure process/transfer bottlenecks first. A later optimized native adapter may be justified without replacing the graph.
- MLT remains a researched optional backend/reference, not a new authoritative timeline. No existing source/proof/history is discarded.
- One shared reference can have CPU copy costs and a small initial source/layer limit. Expose those measured limits and iterate; do not claim unrestricted composition because graph storage admits many objects.
- Full colour management, more blends, GPU readback/interop, persistent caches and bigger packs remain benchmark/conformance-gated. A complete rendered frame queue is bounded; assets stay on disk.

## Licensing and security

Review exact FFmpeg build and every required filter/encoder for distribution; GPL `eq` is not a mandatory bundled convenience. User-installed availability does not grant redistribution rights. Permissive Rust library licences require notices; fonts, SVG payloads and templates are audited separately. No Clipchamp proprietary assets, project scripts/raw filters/URLs, paid runtime or public licence for Lumeryn.

References: [composition research](../research/COMPOSITION_NEXT.md), [typography research](../research/TYPOGRAPHY_ANIMATION_NEXT.md), [effects/time research](../research/EFFECTS_TIMING_NEXT.md), [FFmpeg filters](https://ffmpeg.org/ffmpeg-filters.html), [FFmpeg licensing](https://ffmpeg.org/legal.html), [Microsoft editing UI reference](https://support.microsoft.com/en-us/clipchamp/how-to-edit-a-video-in-clipchamp).

## Verification and status

[Delivery plan](../roadmap/COMPOSITION_DELIVERY_PLAN.md) and [UI contract](../design/COMPOSITION_UX.md) define stage ownership and exit gates. [External fixture](../testing/COMPOSITION_SPIKE.md) passed filter feasibility; it is not Lumeryn composition acceptance. Rust frame-plan integration is the next executable work. No production Rust/dependency/schema change is made by this ADR.
