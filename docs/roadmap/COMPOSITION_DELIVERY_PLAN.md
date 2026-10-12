# Composition delivery plan

Date: 2026-10-10. Owner authorizes planning and implementation of layers/gaps/images/transforms, editable text/vectors/animation/templates, colour/effects/fades, and transitions/speed/freeze. Status: V0 and bounded V1 implemented and verified;225distinct tests and21actual native checks passed. V2–V7 remain staged. This extends the current editor, not a restart. [TASKS](../../TASKS.md) tracks execution; [ADR0023](../adr/0023-shared-visual-composition.md) records architecture.

## Product outcome and boundaries

An external agent creates an editable composition through documented CLI operations; a beginner can open the same project, drag layers and adjust properties in the approved native Studio, then export the complete result locally at UHD. Objects, styles and keyframes remain editable after saving/reopening. No flattened demonstration, private Studio graph, embedded agent, resolution paywall, arbitrary project scripts or new mandatory cloud service.

Preserve Rust + SQLite/SQLx + egui/eframe + wgpu + external FFmpeg. Keep the existing sequential fast path, import jobs, exact FPS, independent AudioPlan, durable history, source verification, encoder discovery and verified create-new publication. No dependency upgrade is justified by feature names alone. This plan does not include recording, AI, HDR,8K, mastering, reverse/optical-flow speed or general animated SVG/Lottie support.

## Research and feasibility

- [Composition](../research/COMPOSITION_NEXT.md): actual single-track/64-clip preview/export bounds, topology/migration risks, FFmpeg framesync/alpha, decoder memory, native gestures and source metadata gaps.
- [Typography/animation](../research/TYPOGRAPHY_ANIMATION_NEXT.md): compare Parley/cosmic-text; measured CPU raster first; Vello GPU compatibility remains experimental; resvg is bounded static input. One creative layout engine, not two competing stacks.
- [Effects/timing](../research/EFFECTS_TIMING_NEXT.md): versioned colour formulas/order, exact fades, fixed-cut transitions with proven handles, rational speed/source mapping, explicit pitch and freeze policies. GPL `eq` is not a mandatory redistributed dependency.
- [Executed external spike](../testing/COMPOSITION_SPIKE.md): generated720p/30 fixture with timed moving video, cropped/rotated translucent PNG and a final gap; full decode and actual frames inspected. This proves a filter combination, not Lumeryn support, GPU performance, UHD parity or colour management.

## Architecture contract

Introduce one checked window-independent VisualPlan inside the existing core. It carries revision, exact evaluation time, canvas, active sources, transforms, ordering, colour/alpha policy and resource requirements. One generated CPU reference lowering feeds preview, headless frame inspection and export; Studio presents the completed composite. Keep decode, evaluation, GUI drawing, audio and render jobs separate. A future GPU backend must conform to stored semantics; it cannot silently change appearance or discard unsupported items.

Persist typed source variants: video range, explicit image hold, later editable scene and exact-frame freeze. Stable visual-track IDs define bottom-to-top order. Initial tracks reject ordinary same-track overlap; different tracks may overlap. Typed transitions are the later controlled exception. Gaps reveal lower layers, then opaque project background. Visual duration remains the greatest visual endpoint, including still/scene holds; independent audio retains its current extent rules. Existing insert/reorder operations keep their documented ripple semantics; free movement uses a separate explicit operation/mode.

Time spans are checked half-open intervals. Source frames, project frames and audio sample frames are distinct units. Export evaluates at each output sample's exact rational project time; it must not round every sample to an integer project frame before evaluating animation. Initial video retains matching CFR; mixed-rate sampling is a separately validated extension. Image holds have duration, no invented FPS. Keyframes are evaluated from time, not accumulated UI deltas.

Before schema3, replace current-version-only validation with explicit schema1/2/3 shapes. Compose migrations in order; upgrade every history snapshot atomically, retain command/receipt bytes and revisions, allocate IDs collision-free across history, preserve undo equivalence and notify same-revision schema changes. Opening never upgrades automatically. Back up through the shared service; old binaries use that backup. Unsupported stored versions or incomplete renderer subsets fail explicitly.

Version the initial SDR reference policy and specify input matrix/range/transfer, sampling, alpha mode and output conversion before exposing composition writes. Current metadata has only transfer information; extend verified metadata and handle historical unknowns with explicit assumptions/reinspection. An ordinary FFmpeg overlay is not linear-light colour management. Linear-light premultiplied composition is a later conformance/feasibility decision; changing policy requires explicit appearance conversion. Cache identity includes this policy. No HDR input silently enters the SDR path.

## Delivery order and exit gates

V0 is an offgraph engineering gate with no new editing capability. Each subsequent persisted stage ends in a usable CLI + native + verified-export slice. Testing is continuous, not deferred to the last row. V4 first ships keyframes and five starters, then completes the larger editable explainer checkpoint.

| Stage | Concrete delivery | Exit evidence |
|---|---|---|
| V0 — checked frame plan | Pure VisualPlan/time/source/order evaluation and generated reference lowering against offgraph fixtures. No new persisted edits yet. | Known gap/alpha/crop/order frames; exact rational mapping; rejected malformed/unsupported plans; bounded active decoders and headless operation. Rust reference integrated; executed gates recorded in COMPOSITION_ACCEPTANCE. |
| V1 — layers and stills | Explicit schema3 migration, two moving visual layers plus a PNG hold within measured limits, free placement/gaps, named ordered tracks. CLI inspect/add/move/range/reorder and native drag/drop. | Schema1/2 migration/rollback/history/receipts, old-reader rejection, source immutability, two clients, live updates, genuine layered preview and UHD/portrait export with independent audio. |
| V2 — direct composition | Contain/cover, source crop, position/size/pivot, flip, quarter-turns, tested arbitrary rotation and Normal opacity. Native bounds/handles with precise Inspector alternatives. | No alpha fringes, correct crop/rotation corners, output-space scaling and portrait safe areas; pointer/keyboard/DPI/cancel/conflict tests and matching headless/export frames. |
| V3 — editable titles | Select one measured layout/raster stack. Text, rectangle/ellipse/connector nodes, pinned fonts and wrapping; editable lower-third/title overlays. | Fixed-font cluster/bidi/fallback fixtures, missing-font failures, no DPI-dependent wrapping, transparent glyph edges, save/reopen/edit and full composition UHD output. |
| V4 — animation and original presets | Typed keyframes for position/scale/rotation/opacity, Hold/Linear and a documented easing family. Five original editable starters: clean title, lower third, callout, chapter card and picture-in-picture. Then neural-network explainer components. | Deterministic first/mid/end/backward seeks, rational output-rate interpolation, atomic preset expansion with returned IDs/provenance and actual editable explainer review/export. |
| V5 — colour and fades | Versioned SDR exposure/contrast/saturation/temperature, reset, inspectable original looks/intensity, opacity and separate visual/audio fades. Begin Normal; other blends only with goldens. | Identity/reset, operation order, colour/alpha patches, exact fade endpoints and sample envelopes; actual-footage thumbnails and one commit per gesture; full decoded parity. |
| V6 — transitions and time | Fixed-cut cross dissolve first, then directional wipes. Proven source handles. Rational0.5x/1x/2x with explicit pitch/ripple policy; exact-source silent freeze hold. | No hidden length/handle changes; chained transition endpoints, odd lengths, source exhaustion, rational mapping, audible tempo review and flash/click/tone A/V checks. |
| V7 — measured expansion | GPU compositor/effects, larger scenes/layers, persistent caches/proxies and broader original packs only after reference parity/baselines. | Quantified total-cost improvement, CPU fallback and device-loss recovery, quotas/invalidation, sustained1080p/4K workloads and bounded concurrent jobs. |

V3–V4 remain an early product priority: do not postpone editable explainers until every filter/transition exists. Bounded colour/audio-fade research can proceed alongside typography, but owners merge through the stable composition contract. Every stage retains relevant full regression, licence and actual native gates.

## Native interaction contract

Preserve the approved mint/Inter two-rail layout. Left creation drawers: Media/Library, Text/Shapes and Transitions as they become usable. Right context drawers: Clip/Transform, Text/Animation, Colour/Filters, Fade, Speed or Transition. Show only the selected task's controls; Advanced contains technical values. [Detailed screen/state checklist](../design/COMPOSITION_UX.md) defines the extension.

Direct gestures show a transient draft over a captured project/revision/epoch. The same evaluator draws the draft; one accepted release produces one undoable transaction. Escape/focus loss/overlay/project switch/stale revision discards it. External agent changes surface an explicit conflict/retry, never silent rebasing. Keep committed and draft state distinguishable; Saving appears only for the commit. Pending preparation stays on its asset/job, and unrelated editing remains usable.

Drag uses grabbed offsets, visible target rows, snapping to nearby endpoints/playhead, keyboard alternatives and accepted-only auto-scroll. Timeline draws visible rows/intervals rather than every object. Preview rendering/decoding never runs in the UI callback. Discard old generations, coalesce seek/drag requests, and show a quiet updating state instead of presenting stale frames as current. Bind prepared plans/drafts/cache identity to graph schema, colour policy and epoch as well as revision; a same-revision migration invalidates them. Persisted export-visible track enable is distinct from local monitor isolation; expose only implemented typed semantics.

## CLI and service parity

Visual tracks, video/PNG placement, range, contain rectangles and free primary movement are available through `timeline visual` / `timeline clip move`. Scene/node edits, full transforms, keyframes, template expansion, effects/fades, transitions, speed/freeze and frame/contact-sheet inspection remain design contracts. Final names follow the existing schema/CLI conventions.

Every shipped group includes strict JSON schemas, capability ID/status/limits/prerequisites, inspect results, examples, expected revision, idempotency, atomic dry-run/batch, undo/redo and service notifications. Discovery distinguishes stored graph limits from admitted render limits and reports unsupported combinations. No Studio-only action or semantic-only CLI success while export omits it.

Dry-run reports resulting spans/duration, changed IDs, ripple scope, transition handles, asset/font prerequisites and resource estimates with unknowns marked unknown. Omitted properties cannot accidentally reset settings. Errors identify the affected object/range and remedy; diagnostic logs avoid private text/path dumps. Reuse the existing error envelope and request IDs rather than inventing a second transport.

## Original prebuilt assets and templates

Reuse the existing reviewed352-file catalogue. Create small original declarative templates with previews rendered by Lumeryn itself; parameters expand into ordinary editable objects and keyframes. Catalogue entries carry stableID/version, parameter schema, content hashes, licence/provenance and resource requirements. No Clipchamp stock, proprietary preset artwork or copied LUTs. No embedded Rust/Python/JS/WGSL or unrestricted expressions. Audit any new fonts/SVG/LUT/music individually before download/bundling. Do not acquire a large pack before actual placement/rendering works.

## Acceptance matrix and performance budget

| Boundary | Mandatory checks |
|---|---|
| Storage/commands | Invalid discriminants/ranges, duplicate or dangling IDs, overflow, source changes, dry-run rollback, stale two-client writers, idempotent replay, old-schema migration and every history/undo target. |
| Pixels/layout | Known asymmetric patterns, gaps/layer order, alpha edges, crop/fit/rotation, glyph clusters/wrapping, animated endpoints, identity/reset and transition source handles. Lossless pre-encode comparisons use recorded policy-specific tolerances; lossy checks are separate. |
| Time/audio |24/25/30/60 and exact29.97/59.94 timelines, half-open endpoints, export-rate conversion, sample boundaries, speed/transition/fade clicks and flashes, long-duration drift and independent versus linked audio policies. |
| Output | Actual3840x2160 and2160x3840, complete composition, frame count/FPS/duration/audio, source hashes and new-path publication; cancellation, unavailable codecs and headless CPU path. |
| Native |1440x900,1280x800,980x640;100/150/200%DPI, both drawers/scroll/focus, pointer and keyboard gestures, stale drafts, tool/device loss, export while editing and no hidden primary controls. |
| Security/licensing | Bounded text/glyph/path/image/graph size, restricted SVG references, no scripts/URLs, decompression admission, subprocess argument isolation, pinned fonts/looks and cargo-deny/source notices. |

Record cold/warm startup, seek-to-present p50/p95, sustained playback/drop/underrun counts, frame evaluation/raster/decode/transfer cost, active processes, peak RAM/VRAM and cancellation under concurrent import/export.60FPS's16.67ms is a target, not a current measured promise. A UHD RGBA8 frame is33.18MB; a floatRGBA frame132.71MB before extra buffers. Budget active sources and one bounded composite queue, not one full queue per timeline object. Do not open10000 FFmpeg inputs, start one decoder per frame or introduce speculative unsafe optimisations.

## Ownership and next exact task

Research specialists completed separate reports; root owns this plan/ADR/UI contract. For implementation assign one graph/storage owner, one shared evaluation/backend owner, and one client/verification owner after contracts merge. Avoid parallel edits of model.rs, storage.rs, render.rs or AudioPlan. Typography/effect experiments can own isolated modules/tests and report back; no speculative crate extraction.

V0 and bounded V1 are implemented and verified; the final integrated gate passed225distinct tests and the actual native layer/UHD workflow passed21checks. Next implement V2 crop/cover/rotation/flip/opacity and direct canvas manipulation through the shared plan, with source-bound pixel fixtures and CLI/native/export parity. Then prioritize V3 editable titles/shapes and V4 keyframes/explainers. The current owner project remains explicit and is never automatically migrated or reset.

## V1 scope adjustment and next executable milestone

Ship the checked reference and explicit migration together with three total admitted visual placements (primary plus overlays), two extra tracks, maximum two active videos/one PNG and bounded UHD surfaces. CLI discovery exposes these limits and the64-clip unchanged sequential fast path separately. This delivers real layers/gaps/PIP/stills without prematurely building a decoder-window scheduler. Independent audio, revision conflicts, source proofs and history remain shared. No new mandatory library/backend is introduced.

V1 includes precise contain position/size and a corner PIP preset earlier than V2. Crop, cover, rotate/flip, explicit opacity and direct canvas handles remain V2. The native Inspector uses readable time values with exact frames available secondarily. Native draft-composite preview and larger source counts remain pending. Finish V2 with small source-bound fixtures and the same preview/export lowering, then prioritize V3 measured creative typography and V4 editable explainers. Do not expose decorative controls for the remaining families.

## Owner launch feedback checkpoint

Completed before further creative features: GUI subsystem in debug/release, agent-safe no-handle-inheritance Studio spawning with OS visibility observations, and console-free media workers across preview/import/render. The owner confirmed no extra terminals after rollout. Responsive media actions and actual PNG source-card dragging are also verified. These share existing process/service boundaries; no embedded runtime/installer/second graph is introduced. Existing zero-project Welcome/Open/New remains. Passing framebuffer captures alone cannot certify a foreground physical display; evidence and remaining manual checks are in STUDIO_LAUNCH and COMPOSITION_ACCEPTANCE.
