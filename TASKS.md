# Task board

All task records include phase, priority, dependencies, owner, scope, acceptance/tests, status, blockers and evidence. Product roadmap remains authoritative; DONE means implemented and verified, or a specifically scoped research deliverable reviewed.

| ID | Phase / priority | Description; dependencies | Owner / files | Acceptance and required tests | Status | Blockers / evidence |
|---|---|---|---|---|---|---|
| LUM-001 | 0 / P0 | Inspect/init local repository; none | Lead / root, docs/PRODUCT_SPEC.md | Preserve full source spec; no remote; inspect toolchain | DONE | Empty workspace; Git init; Rust/Cargo 1.97.1 |
| LUM-002 | 0 / P0 | Media research and feasibility; 001 | media_research / docs/research/MEDIA.md, ADR 0002, licensing/MEDIA.md | Official sources, actual encode/decode, license boundary, timing evidence | IN PROGRESS | Installed binary is GPLv3+; no bundling |
| LUM-003 | 0 / P0 | Platform/IPC/resources research; 001 | architecture_research / PLATFORM, ADRs 0003–0004 | Official sources, risks/alternatives; label untested prototypes | IN PROGRESS | Native preview needs measured experiment |
| LUM-004 | 0 / P0 | Rust/SQLite/license/test research; 001 | verification / RUST_STORAGE, DEPENDENCIES, testing/PLAN | Stable versions, bundled SQLite review, adversarial acceptance plan | IN PROGRESS | Dependency graph pending build |
| LUM-005 | 1 / P0 | Project/core/CLI implementation; 002,004 | Lead / Cargo, crates, schemas, CLI docs | Create/import/add/trim/reopen; JSON; atomic batches/conflicts; fmt/clippy/unit/integration | IN PROGRESS | No compiled application yet |
| LUM-006 | 2 / P0 | First real export; 005 | Lead / media, render, CLI tests | Generated source → trimmed playable MP4; frame/audio verification; protect originals | TODO | Requires 005 |
| LUM-007 | 0–2 / P0 | Independent verification/local milestone; 002–006 | verification + Lead / tests, progress, Git | Full gates, explicit media acceptance, independent review; commit locally | TODO | Requires passing implementation |
| LUM-008 | 0 / P1 | GPU hardware and native preview prototypes; 003 | Future / renderer, resources, benchmarks | Actual adapter telemetry + measured 1080p preview/resize, no fabricated VRAM | TODO | Requires renderer experiment; not CLI blocker |
| LUM-009 | 3 / P0 | Authenticated single-writer service and events; 005 | Future / IPC, service | Two clients, missed-event recovery, ACL, conflicts, restart tests | TODO | Secure IPC prototype first |
| LUM-010 | 2 / P1 | Separate audio placement/transitions/export capabilities; 006 | Future / timeline, audio, render | Actual audio mix/transitions plus decoded/golden comparisons | TODO | First export baseline |

