# Studio redesign handoff

Date: 2026-10-09. Status: four directions created and visually reviewed; owner selection pending. The owner rejected the existing dense technical UI and requested three or four mockups before implementation. This task delivers four design concepts only. Rust source, service, database schema and running Studio are unchanged.

Workflow: owner-invoked [mockup-to-product skill](C:/Users/Thumbeja/.codex/skills/mockup-to-product/SKILL.md), with the built-in image-generation skill for raster concept references. No external paid API, customer footage, source upload or publication. Four independent generation calls; exact augmented prompts are in studio-redesign-v1/PROMPTS.md. Generated images were copied unchanged into the workspace; originals retained. ASSETS.json records dimensions/hashes/provenance. These are internal proprietary project references, not shipping UI assets or a public license grant.

## Audience, scope and diagnosis

Audience: beginners briefing external agents, with professional manual editing available on the same Rust project graph. The representative screen is the active desktop editing/review workspace at1600x1000 logical pixels. Compare the same60second neural-network explainer at00:24; scene art, clip values and thumbnails are illustrative concept content.

Current issues observed directly in the supplied screenshot/source: prominent filesystem path, many competing dock tabs, small text, generic controls, permanently exposed raw revision JSON/UUIDs, technical audio status and little task hierarchy. Empty/loading/project-open/export/error states are not separate mockups yet. They will be designed in the chosen family before a complete visual slice is implemented.

## Direction choices

| Choice | Reference | Composition and audience | Implementation implications |
|---|---|---|---|
|A / Focus|studio-redesign-v1/A-focus.png|Preview dominates; narrow navigation rail; contextual trim; simplified clip strip. Strong default for beginners.|Most direct redesign of existing open/play/seek/select/trim/export behavior. Asset thumbnails and simplified clip cards need actual indexed data; detailed tools remain available on demand.|
|B / Studio|studio-redesign-v1/B-studio.png|Media grid, central preview, focused contextual Inspector, substantial timeline. Manual editor workflow.|More controls visible, organized by task. Thumbnail/waveform/advanced effect entries are concept targets; do not ship decorative controls or fabricate waveform data.|
|C / Storyboard|studio-redesign-v1/C-storyboard.png|Narrative scene list and visual sequence, with preview and plain-language scene notes. Explainer workflow.|Future scene semantics/notes require the approved creative roadmap; selection of the look does not silently authorize a new backend/schema. Left list and bottom strip should have distinct purposes or one become collapsible.|
|D / Review|studio-redesign-v1/D-review.png|Large viewer, human-readable recent changes, small chapter strip; detailed timeline on demand. Agent-edited video review.|Translate actual committed commands into user-facing change summaries. Native edit path remains reachable; no embedded LLM/chat is introduced. Warm palette is a direction to choose, not an approved brand replacement.|

Recommendation: A as the primary beginner workspace, with B's detailed editing organization available when needed. The owner may choose any direction; no choice has been assumed. Logo variations in generated concepts are not an approved identity change.

## Visual review and future design contract

All four images were inspected at full-screen scale: clear major hierarchy, labeled playback/export, readable primary copy, meaningful composition differences and recognizable sample content. They are static mockups; actual responsiveness, keyboard usability, DPI scaling, focus, contrast measurements and native performance are not tested by image generation. Generated microcopy, clock values, durations and icon details must be normalized to real data during implementation; do not blindly reproduce AI-drawn text.

Common rules: compact header with project name and quiet save state; prominent preview and one primary export action; readable14–16px body text; at least36px interactive controls; subtle separators and restrained surfaces; technical diagnostics in an advanced drawer. No raw JSON, object IDs, full paths or codec status in the ordinary editing flow. Plain-language history with expandable technical detail. Browser/mockup tooling does not change the native egui/eframe/egui_dock/wgpu architecture.

At compact desktop widths(~980px), collapse secondary drawers, prioritize preview/transport/primary actions and intentionally scroll necessary controls. Validate1440x900,1280x800 and980x640 plus100%/150%/200%DPI after implementation. Do not shrink essential text to keep every panel permanently open.

## Checklist and next task

- Four active-workspace concepts: created, saved, visually inspected; all unapproved.
- Project-open/empty/loading/error, media import, trim, history and export dialog states: pending selected family.
- Production design tokens/assets: pending selection; concept screenshots must not become flattened interactive UI.
- Native Rust implementation and functional/visual regression gates: pending selection.
- Existing verified engine/service/52-test evidence: retained; no new Rust code was changed, so no additional Rust test gate was run for these mockups.

Next: owner chooses A/B/C/D or a specific combination. Extend only that direction into related states, define proportions/tokens/navigation/accessibility, then implement one complete native open→preview→trim→live edit→export flow. Preserve revision-aware commands and background jobs; compare actual native screenshots with the chosen reference. Source/API/state changes beyond the visual task require separately authorized scope.
