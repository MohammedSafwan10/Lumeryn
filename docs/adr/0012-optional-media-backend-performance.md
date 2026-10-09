# ADR 0012: Optional media backend evaluation and measured optimization

Date: 2026-10-09. Status: accepted research policy; MLT integration deferred. Research: [MLT](../research/MLT.md).

## Context

The owner reaffirmed Rust, native egui/wgpu, FFmpeg and SQLite, and requested reusable media capabilities and performance work without another engine rewrite. Lumeryn needs exact shared editing semantics and responsive preview/CLI/Studio behavior. Adding a mature framework may supply effects/mixing but introduces Windows ABI, plugin licensing, resource and synchronization costs.

## Decision

Retain the current stack and authoritative Rust project service. Use MLT as architectural reference now; evaluate a pinned optional backend only after an actual missing capability or measured bottleneck justifies it. MLT 7.42.0 is the researched release; it is not a current dependency and no MLT speed claim is made.

Prefer an isolated headless prototype before an in-process binding. Generate an allowlisted execution plan from committed Rust project state and preserve revision/time/color semantics. Never let MLT XML or plugins become a second project authority. Shared wgpu/decoder textures and zero-copy claims require measured platform-specific evidence.

MLT's LGPL framework may support commercial proprietary use with applicable source/notices/relinking/replacement obligations. Defaults enable GPL modules, so any proposed build must explicitly disable GPL/GPL3 and review all enabled modules and their exact dependencies. An owner/legal distribution review is required before bundling; no public license is applied to Lumeryn.

## Performance gate

Profile first. Set fixture/quality/resource and correctness thresholds before comparing backends. Capture cold/warm p50/p95 latency, CPU/RAM, actual available GPU counters, responsiveness under concurrent rendering, and cancellation behavior. Verify decoded image/audio/timestamps and identical edits. Accept a change only when reproducible measurements show its benefit and failures remain explicit. Safe Rust and bounded queues remain default; unsafe code is confined to necessary reviewed FFI boundaries, never added speculatively to engine hot paths.

## Alternatives and consequences

Continue the explicit FFmpeg process adapter, add reviewed FFmpeg bindings for a specific bottleneck, or implement original Rust/wgpu composition when justified. These alternatives preserve headless capability and avoid coupling a UI rewrite to media optimization. Optional integrations increase packaging/compliance and testing work; absence of MLT keeps the minimal install unchanged.

Research is complete for this decision. No MLT build, Rust binding, GPU interop or benchmark is executed. Implementation requires a separate scoped milestone with Windows evidence, dependency review and meaningful decoded-media regression gates.
