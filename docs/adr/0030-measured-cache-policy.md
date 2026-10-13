# ADR0030 — Bounded static scene reuse before renderer expansion

Date: 2026-10-12. Status: proposed integration contract; implementation and measured acceptance pending. Extends ADR0023/0026, preserving the authoritative Rust graph.

Begin with process-local reuse of validated final-resolution static scene pixels through the existing CPU renderer. Use an explicit 64 MiB retained-pixel budget, 16-entry LRU and immutable shared buffers. Key ordered scene content, pinned font hash/face, physical output dimensions and raster/alpha/colour policy version. Validate before lookup; never cache errors or promote low-resolution preview pixels into final exports. Revision/generation checks remain delivery guards. Outstanding buffer references and cold-render allocations are outside the retained-entry accounting and must be measured with bounded caller queues.

The typography experiment's actual UHD raster p50/p95 25.103/25.364 ms motivates measuring reuse, but establishes neither a speedup nor 60 FPS product performance. Root must compare cached/uncached complete-call timings, peak memory and identical pixel hashes, followed by real native and decoded export acceptance. Detailed API and negative cases: [PERFORMANCE_EXPANSION](../research/PERFORMANCE_EXPANSION.md).

Rejected initial expansion: unrestricted disk cache (adds storage/recovery/cleanup work before a measured benefit), larger layer limits (unmeasured decoder/compositor peak costs), GPU replacement (unproven total transfer/conformance cost), more dependencies and unsafe optimization. Persistent derivatives remain a later opt-in, quota-bound, integrity-checked sole-writer slice. Original editable graph/source files are never cache entries or cleanup candidates.

Use the existing SHA256/serde/Arc facilities, reviewed fonts and pinned renderer. No new public license, downloaded asset, codec distribution or dependency exception. Coordinate one shared Cargo target and reduced development artifacts independently from runtime cache policy; build storage does not prove rendering performance. Capability discovery remains unchanged until integrated behavior and gates actually pass.
