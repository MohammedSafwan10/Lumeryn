# Task board

## Approved B Studio redesign

The owner selected B / Studio. Implementation preserves the authoritative Rust service and project graph. Design contract: `docs/design/STUDIO_REDESIGN.md`; executed evidence: `docs/testing/STUDIO_REDESIGN.md`.

| ID | Priority / dependencies | Owner and scope | Acceptance | Status / evidence |
|---|---|---|---|---|
| UX-001 | P0 / none | Lead / four visual concepts | Distinct references and provenance | DONE; A Focus, B Studio, C Storyboard, D Review |
| UX-002 | P0 /001 | Owner / direction selection | Approved reference and responsive contract | DONE; B / Studio selected |
| UX-003 | P0 /002 | Lead + asset specialist / native B slice | Responsive shell, readable clocks, native icons/dialogs, real thumbnails/waveforms, monitor volume, contextual trim; preserve service semantics | DONE; final fmt, strict all-target Clippy,62distinct tests/cargo-deny and actual native workflows/captures passed |
| UX-004 | P0 /003 | Verification + Lead / native acceptance | Actual three viewport workflow and five related-state captures; source integrity and unchanged readonly revisions; compare B | DONE; final3size/default workflows,5readonly states/revision2/source8unchanged, all8PNG visual review; retained evidence in STUDIO_REDESIGN |
| UX-005 | P1 /004 | Future desktop QA / physical usability | Dialog select/cancel/focus, pointer, 150%/200% DPI, keyboard resize and physical A/V checks | TODO; scripted actions and 100% captures do not establish these checks |

## Native desktop migration — active owner decision

Checkpoint: local commit `0c0efed` preserves verified CLI/core/media/database and all specs. No Tauri/Svelte/frontend/installer/Node project exists, so no frontend deletion is necessary. Preserve independent source/test work. Native selection supersedes old future-Tauri tasks; do not recreate them.

| ID | Phase / priority | Description; dependencies | Owner / scope | Acceptance / tests | Status | Blockers / evidence |
|---|---|---|---|---|---|---|
| NAT-001 | A / P0 | Inspect/save working checkpoint; none | Lead / Git,spec,rootdocs | Preserve all current features and changes; local commit; inspect actual frontend | DONE | `0c0efed`;31verified CLI/media/storage tests; no frontend exists |
| NAT-002 | B / P0 | Native Rust app+docking;001 | architecture_research / apps/lumeryn-studio,ADR0010,NATIVE_STUDIO | Compatible exactpins, real Windows launch, layout persistence/dark design, no WebView | DONE | egui/eframe0.36.2,egui_dock0.21.1,wgpu30.0.1 actual Windows/Vulkan launch and layout tests passed |
| NAT-003 | C / P0 | Shared authoritative secure Rust service;001 | verification / crates/lumeryn-service,NATIVE_SERVICE | Authenticated/currentuser IPC, bounded commands, one Store writer, revision events/replay/conflicts, no blocked UI | DONE | 7 executed service tests: ACL/exclusive endpoint, two clients/conflicts, replay/restart, render/cancel |
| NAT-004 | D / P0 | Real preview/audio worker;001,002 | media_research / crates/lumeryn-preview,NATIVE_PREVIEW | Actual decoded frames/playpause/seek/step, bounded buffers, clock, real audio evidence/limits | DONE | 4unit+4explicit tests passed; actual720p decode/rationalseek/nativeaudio/EOS; physical A/V unmeasured |
| NAT-005 | C/E/F / P0 | Integrate service/CLI/UI editing/export;002–004 | Lead+GUI owner / CLI,workspace,UI | Existing project opens; CLI edits live; functional timeline/trim; headless export preserved; responsive background work | DONE | Actual Studio trim/liveCLI revision3-to4-to5/export29UIupdates; captured latency fixed; headless disconnect/cancel tested |
| NAT-006 | G/H / P0 | Verify/audit docs/dependencies/packaging;005 | Lead / tests,docs,deny,Git | Workspace gates, native smoke, real preview/export, explicit14-test matrix, currentdocs nativeonly | DONE | No obsolete code to delete; historical Tauri ADRs marked superseded; native distribution ADR0011 reviewed; narrow native font/Boost/paste exceptions documented; full gate52distinct tests plus native moving720p and pre-migration project smokes; partial/manual acceptance documented in NATIVE_ACCEPTANCE |

## Existing foundation and staged product work

All task records include phase, priority, dependencies, owner, scope, acceptance/tests, status, blockers and evidence. Product roadmap remains authoritative; DONE means implemented and verified, or a specifically scoped research deliverable reviewed.

| ID | Phase / priority | Description; dependencies | Owner / files | Acceptance and required tests | Status | Blockers / evidence |
|---|---|---|---|---|---|---|
| LUM-001 | 0 / P0 | Inspect/init local repository; none | Lead / root, docs/PRODUCT_SPEC.md | Preserve full source spec; no remote; inspect toolchain | DONE | Empty workspace; Git init; Rust/Cargo 1.97.1 |
| LUM-002 | 0 / P0 | Media research and feasibility; 001 | media_research / docs/research/MEDIA.md, ADR 0002, licensing/MEDIA.md | Official sources, actual encode/decode, license boundary, timing evidence | DONE | Installed binary is GPLv3+; no bundling |
| LUM-003 | 0 / P0 | Platform/IPC/resources research; 001 | architecture_research / PLATFORM, ADRs 0003–0004 | Official sources, risks/alternatives; label untested prototypes | DONE | Native preview needs measured experiment |
| LUM-004 | 0 / P0 | Rust/SQLite/license/test research; 001 | verification / RUST_STORAGE, DEPENDENCIES, testing/PLAN | Stable versions, bundled SQLite review, adversarial acceptance plan | DONE | Initial locked dependency graph and cargo-deny audited; native extension gets a separate review |
| LUM-005 | 1 / P0 | Project/core/CLI implementation; 002,004 | Lead / Cargo, crates, schemas, CLI docs | Create/import/add/trim/reopen; JSON; atomic batches/conflicts; fmt/clippy/unit/integration | DONE | CLI/core gate passed at checkpoint 0c0efed; native integration and corrected launch regression verified under NAT-005 |
| LUM-006 | 2 / P0 | First real export; 005 | Lead / media, render, CLI tests | Generated source → trimmed playable MP4; frame/audio verification; protect originals | DONE | Generated-media real export/frame/audio acceptance passed at checkpoint 0c0efed |
| LUM-007 | 0–2 / P0 | Independent verification/local milestone; 002–006 | verification + Lead / tests, progress, Git | Full gates, explicit media acceptance, independent review; commit locally | DONE | 31 tests and cargo-deny passed at checkpoint 0c0efed |
| LUM-008 | 0 / P1 | GPU hardware and native preview prototypes; 003 | Future / renderer, resources, benchmarks | Actual adapter telemetry + measured 1080p preview/resize, no fabricated VRAM | TODO | Requires renderer experiment; not CLI blocker |
| LUM-009 | 3 / P0 | Authenticated single-writer service and events; 005 | Future / IPC, service | Two clients, missed-event recovery, ACL, conflicts, restart tests | DONE | Implemented by NAT-003; seven actual service tests passed |
| LUM-010 | 2 / P1 | Separate audio placement/transitions/export capabilities; 006 | Future / timeline, audio, render | Actual audio mix/transitions plus decoded/golden comparisons | TODO | First export baseline |
## Remaining native acceptance and measured optimization

DONE above describes the scoped verified baseline. The full professional-editor roadmap and partial checks in docs/testing/NATIVE_ACCEPTANCE.md remain open.

| ID | Phase / priority | Work; dependencies | Owner / scope | Acceptance / evidence | Status |
|---|---|---|---|---|---|
|NAT-007|H/P0|Complete manual/device acceptance;006|Next/desktop,audio,QA|Pointer/docking/resize/high-DPI, physical10minute A/V, GPUloss and actual different-account/remote denial; exact Windows procedure in NATIVE_ACCEPTANCE|TODO|
|PERF-001|0/P1|Backend baseline and MLT evaluation;006|Lead+verification/benchmarks,MLT,ADR0012|Actual NVENC/NVDEC initialization, QSV/AMF failures, CPU/RAM/UI/idle/sharedGPU measurements; official MLT licensing; no graph replacement|DONE|
|PERF-002|Next/P1|Hardware/proxy/interop/incremental optimization;PERF-001|Next/media,render,resources|Representative1080p/4K identical-quality tests, GPUsharing/fences, CPUfallback, exact proxy mapping/cache invalidation/quotas; no speculative Assembly/unsafe shortcuts|TODO|

Next executable creative slice: roadmap C1-C8 starts with a versioned semantic scene schema, then native shapes/text/animation evaluated in the headless core through the same revision service, Studio preview and decoded export parity. Research Parley/cosmic-text/Vello/resvg before actual integration. UI fonts do not implement professional export typography. Then reusable explainer components, narration cues and scene-local QA, followed by editable60second neural-network Demo A. Optional approved media/Blender support Demos B/C later.

Global SQLite model/settings/benchmarks storage, transcript FTS5, vector indexes, professional mixing and persistent waveform caching, effects, undo/redo, complete hardware scheduling, AI models, disk proxies/cache, installer/code signing and signed updates remain staged. No task implies shipped support without recorded tests.
