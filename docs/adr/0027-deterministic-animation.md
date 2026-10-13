# ADR0027: seek-independent bounded animation

Date: 2026-10-12. Status: proposed for integrated schema4 slice; pure implementation tested offgraph.

An editable scene must appear at the same state when preview seeks backward or export uses a different rate. Evaluate keyframes from exact local rational project time, retain fixed-point integer serialized values, and keep channel curves separate from scene object IDs. Use Hold, Linear and bounded SmoothStep only; left keys govern segments and clip intervals stay half-open. Original starter presets expand to ordinary object specifications and caller-owned IDs rather than flattening images or installing a second timeline.

This selects existing Rust/Serde instead of a tween runtime, UI-delta animation, external script renderer or arbitrary expressions. No new dependency, distribution binary or mandatory model is required. The [W3C easing principle](https://www.w3.org/TR/css-easing-1/) supports pure evaluation; the specific polynomial/units/bounds are Lumeryn's versioned contract, not CSS interchange. Platform-neutral O(keys) evaluation is initially bounded6channels×128keys. Float interpolation occurs only after exact segment selection; crossplatform bit identity is not promised and future GPU conformance requires pixel tolerances. No unsafe code is introduced.

Root owns graph/version/history migration, revision-aware operations, scene adaptation, CLI/native controls and shared rendered output. Typography owns audited fonts and glyph layout; media remains verified and immutable. Until those checks and decoded output goldens pass, capability discovery must not advertise project animation/template rendering. See [experiment and verification](../research/ANIMATION_IMPLEMENTATION.md).
