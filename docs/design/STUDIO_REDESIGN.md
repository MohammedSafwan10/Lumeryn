# Studio redesign handoff

Date: 2026-10-09. Status: **B / Studio approved by the owner; native implementation and recorded100% DPI visual acceptance complete; physical/manual checks staged**. The owner rejected the existing dense technical UI, reviewed four alternatives and selected B. Preserve the engine/service/project schema and all supported editing behavior while replacing the ordinary Studio workflow with the selected composition. Completed baseline evidence remains historical; it does not verify this redesign.

Workflow: owner-invoked [mockup-to-product skill](C:/Users/Thumbeja/.codex/skills/mockup-to-product/SKILL.md), with the built-in image-generation skill for raster concept references. No external paid API, customer footage, source upload or publication. Four independent generation calls; exact augmented prompts are in studio-redesign-v1/PROMPTS.md. Generated images were copied unchanged into the workspace; originals retained. ASSETS.json records dimensions/hashes/provenance. These are internal proprietary project references, not shipping UI assets or a public license grant.

## Audience, scope and diagnosis

Audience: beginners briefing external agents, with professional manual editing available on the same Rust project graph. The representative screen is the active desktop editing/review workspace at1600x1000 logical pixels. Compare the same60second neural-network explainer at00:24; scene art, clip values and thumbnails are illustrative concept content.

Current issues observed directly in the supplied screenshot/source: prominent filesystem path, many competing dock tabs, small text, generic controls, permanently exposed raw revision JSON/UUIDs, technical audio status and little task hierarchy. Related empty, active, compact Inspector, export and tools states are implemented and captured in the same approved family. Missing/error/loading copy is implemented; pointer transition/dialog acceptance remains manual.

## Direction choices

| Choice | Reference | Composition and audience | Implementation implications |
|---|---|---|---|
|A / Focus|studio-redesign-v1/A-focus.png|Preview dominates; narrow navigation rail; contextual trim; simplified clip strip. Strong default for beginners.|Most direct redesign of existing open/play/seek/select/trim/export behavior. Asset thumbnails and simplified clip cards need actual indexed data; detailed tools remain available on demand.|
|B / Studio|studio-redesign-v1/B-studio.png|Media grid, central preview, focused contextual Inspector, substantial timeline. Manual editor workflow.|More controls visible, organized by task. Thumbnail/waveform/advanced effect entries are concept targets; do not ship decorative controls or fabricate waveform data.|
|C / Storyboard|studio-redesign-v1/C-storyboard.png|Narrative scene list and visual sequence, with preview and plain-language scene notes. Explainer workflow.|Future scene semantics/notes require the approved creative roadmap; selection of the look does not silently authorize a new backend/schema. Left list and bottom strip should have distinct purposes or one become collapsible.|
|D / Review|studio-redesign-v1/D-review.png|Large viewer, human-readable recent changes, small chapter strip; detailed timeline on demand. Agent-edited video review.|Translate actual committed commands into user-facing change summaries. Native edit path remains reachable; no embedded LLM/chat is introduced. Warm palette is a direction to choose, not an approved brand replacement.|

Selection: **B / Studio**. A/C/D remain comparison references and are not alternate production applications. Logo variations in generated concepts are not an approved identity change.

## Visual review and future design contract

All four images were inspected at full-screen scale: clear major hierarchy, labeled playback/export, readable primary copy, meaningful composition differences and recognizable sample content. They are static mockups; actual responsiveness, keyboard usability, DPI scaling, focus, contrast measurements and native performance are not tested by image generation. Generated microcopy, clock values, durations and icon details must be normalized to real data during implementation; do not blindly reproduce AI-drawn text.

Common rules: compact header with project name and quiet save state; prominent preview and one primary export action; readable14–16px body text; at least36px interactive controls; subtle separators and restrained surfaces; technical diagnostics in an advanced drawer. No raw JSON, object IDs, full paths or codec status in the ordinary editing flow. Plain-language history with expandable technical detail. Browser/mockup tooling does not change the native egui/eframe/egui_dock/wgpu architecture.

At compact desktop widths(~980px), collapse secondary drawers, prioritize preview/transport/primary actions and intentionally scroll necessary controls. Validate1440x900,1280x800 and980x640 plus100%/150%/200%DPI after implementation. Do not shrink essential text to keep every panel permanently open.

## Approved B implementation contract

Reference: [B-studio.png](studio-redesign-v1/B-studio.png), independently inspected at its actual 1586×992 raster size. Composition, spacing and interaction hierarchy are approved; neural-network artwork, titles, timings and generated thumbnails are illustrative. The image is an internal reference, never a flattened shipping UI background.

| Region | Native behavior and visual contract |
| --- | --- |
| Header | One compact project-name row and quiet connection/save state. Open project and mint Export are obvious; full paths/UUIDs/revision JSON live in advanced diagnostics. |
| Media left | Clearly labeled Import media, readable asset names, real decoded thumbnails and true duration. Selecting media remains actionable; unsupported assets and missing media have explicit status. |
| Preview center | Dominant aspect-correct video image, stable transport/scrub slot, play/pause, frame step and preview quality. Preserve actual decoded GPU frames and service revision identity. |
| Clip right | Contextual selected-clip name, source range/end/length expressed as readable time; explicit Apply trim. Validate before dispatch and retain frame-exact rational editing. |
| Timeline bottom | A substantial custom-painted ruler/clip track/playhead, selected clip outlined mint, zoom/scroll and reachable fit. Real thumbnail strips/waveforms derive from source data. They do not imply implemented independent audio tracks or effect/transition semantics. |
| Advanced detail | History, activity, render queue and diagnostics are available on demand. Raw graph/log content stays off the ordinary editing surface. |

Use charcoal surfaces with subtle cool borders, clear off-white copy and a restrained mint primary/selection accent. Aim for 14–16 logical-pixel body copy, 36-pixel primary controls, consistent native focus/hover/disabled states and quiet dividers. The selected reference is dense enough for manual editing but keeps preview, import, trim and export visibly ranked. Do not replace lower-priority controls with decorative enabled buttons: unsupported effects/transform/color entries must be absent or honestly labeled staged.

Large desktop: approximately 21% media, 58% preview and 21% Inspector width with a lower timeline of about 25–30% height; implement flexible proportions rather than image-coordinate replication. At 1280×800 reduce side-panel widths while preserving readable labels. At 980×640 prioritize preview/transport and expose secondary context through a compact drawer/tab; permit intentional scrolling instead of tiny text or clipped primary buttons. Native window/DPI constraints are measured, not inferred from requested bounds.

No backend/schema rewrite is authorized by this selection. Commands retain expected revisions/idempotency and no-overwrite export. Ordinary actions use asynchronous native dialogs on Windows; selection dispatches validated service commands. Dialog cancellation changes no project state. A picked save path does not authorize replacing an existing output.

## States and acceptance checklist

| State or flow | Approved direction | Implementation/evidence status |
| --- | --- | --- |
| Active workspace | B approved | Implemented and visually inspected; implemented-active.png retains actual native pixels. |
| Empty/open/loading/offline/error | B family, plain language and one clear next action | Empty welcome captured; explicit loading/error messages implemented; physical/manual transitions pending. |
| Native import/project/export dialogs | B actions, platform-native picker | Integrated asynchronous Windows picker; engine no-clobber retained; physical select/cancel/focus checks pending. |
| Asset thumbnail/waveform presentation | Source-derived imagery/PCM; bounded cache | Actual verified source thumbnails/PCM waveforms implemented and inspected;128-asset ephemeral bound. |
| Trim/select/seek/live CLI update | Same existing Rust semantics | Final native workflow passed at all3sizes with real trim/liveCLI events. |
| Background export/cancel/history | Preserve service-owned job behavior and readable summaries | Final decoded background export passed; export/tools native captures inspected; existing cancellation service tests pass. |
| Compact/DPI/keyboard | 1440×900,1280×800,980×640; manual100%/150%/200%DPI/focus checks | Actual3sizes plus compact drawer inspected at100%DPI; physical keyboard/focus/150%/200%DPI pending. |

Verification command: `./scripts/native-redesign-smoke.ps1`. It builds the native binaries, generates trusted moving720p fixtures, creates independent consistent project backups for large/medium/compact cases, executes the existing same-app workflow smoke and captures actual native PNGs. Each run verifies playback/seek/step, Studio trim, real CLI live update, export with UI progress and post-run source/project integrity. Reports record requested bounds, actual viewport/DPI, physical PNG dimensions and capture hashes. Windows can clamp bounds; inspect recorded actual sizes before treating them as the requested viewport. Generated artifacts stay ignored under `.lumeryn-scratch`.

Passing workflow assertions is insufficient for design acceptance: inspect every PNG next to B for panel proportions, preview crop/aspect, readable text, mint hierarchy, compact control wrapping and missing/overlapping controls. The script intentionally leaves `visual_review=pending`. File-picker pointer/cancellation, real keyboard focus/high-DPI and docking usability remain manual checks. Final Rust/dependency gates passed as recorded in the acceptance document; remaining manual checks are explicitly pending.

Before requesting a screenshot, the same-app harness waits at most eight seconds for presentation assets to settle. Asset failures remain nonfatal to the existing editing workflow; reports separately retain screenshot-time readiness/thumbnail/waveform counts and `asset_capture_wait_expired`. A workflow pass with absent assets cannot establish thumbnail/waveform visual acceptance. Review dropped multiple files, mixed project/media drops and import-picker completion during another edit: all accepted files must be batched/queued or explicitly rejected, never silently discarded by a busy flag. New-project picker/create/open and missing-file/error states require their own manual state checks; the disposable existing-project smoke does not imply they passed.

Implemented evidence: [native acceptance](../testing/STUDIO_REDESIGN.md), [actual active screen](studio-redesign-v1/implemented-active.png), [asset manifest](studio-redesign-v1/IMPLEMENTED_ASSETS.json). Final three-size/default workflows, five readonly captures and full62-test/dependency gate passed; all8screenshots inspected. Ordinary zeros, icon transport, preview gain, subdued true waveform, medium trim visibility and ruler endpoint were reviewed. No mockup raster is used as product UI. Next exact task: complete the manual dialog/pointer/DPI acceptance, then the staged semantic native scene/text/vector vertical slice. No further design choice approval is required; only a material requirements change needs the owner.
