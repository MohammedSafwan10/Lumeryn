# Owner architecture clarifications — 2026-10-09

These preferences supplement the complete master specification and critical vision update. They preserve working code and require research rather than blind adoption.

## Local databases

Preferred architecture is Rust + SQLx + SQLite, fully local-first; no PostgreSQL, Firebase, Appwrite or mandatory cloud database for the initial desktop. A portable per-project `project.lumeryn` contains timeline/clips/tracks/effects/keyframes/settings/revisions/edit history. A separate global application database will own installed AI-model metadata, application settings, hardware benchmark profiles and shared metadata. Use WAL, transactions, schema migrations, integrity checks and safe recovery. Evaluate FTS5 transcript/metadata search and optional local CLIP vector indexing using actual benchmarks. Large videos/images/audio/models/render caches stay as filesystem assets. CLI and Studio writes ultimately pass through the same authoritative Rust project service with revision concurrency/live events. Explain trade-offs before replacing working choices. ADR0001 defines current and staged scope.

## Research and appropriate integration

Evaluate official current documentation, licensing, maturity/maintenance, Windows compatibility, runtime overhead and alternatives for: Parley/cosmic-text shaping and international text; Vello CPU/GPU vectors; static SVG resvg; CPAL output, Rubato resampling, dasp low-level DSP; exact AV clocks, mixing/waveforms/latency/export while retaining FFmpeg decoding; NVENC/NVDEC, Intel QSV, AMD and platform media acceleration with actual codec/device benchmarks and CPU fallback; BLAKE3 content addresses, notify change hints, dependency invalidation, quotas/safe cleanup and suitable compression; native subtitle rendering and optional libass ASS compatibility; tracing JSON/correlation/job/performance diagnostics without private-content logs.

Evaluate nextest, insta, proptest, cargo-fuzz, golden visual fixtures and automated AV synchronization checks. Use supported Linux CI fuzz workers where appropriate; current tool documentation also describes qualified native Windows ASan setups. Integrate cargo-deny and dependency/license audits; code/assets/checkpoints each need proprietary-commercial redistribution review. No unselected model/asset is certified merely because a code license is permissive.

Plan Tauri NSIS/MSI Windows installation, secure Authenticode signing, independently signed application updates, clean uninstall preserving model storage/user media, and compatible rollback. Expose truthful CLI capabilities/hardware requirements/evidence while keeping optional runtimes/models out of minimal installation. Significant choices get ADRs; integrate necessary foundations now, stage advanced features in correct phases, and delegate independent research/audits with file ownership. Preserve prior implementation rather than restarting.

The full critical product vision is retained verbatim in PRODUCT_VISION_UPDATE.md and appended to PRODUCT_SPEC.md. It sets beginners/external agents as primary users, editable animated explainers as a flagship, semantic scenes/native primitives, bounded visual/audio QA, three user workflows, authorized original content, and editable Demo A/B/C. It reprioritizes creative slices after reliable foundation and live Studio synchronization, without waiting for all traditional-editor features.

