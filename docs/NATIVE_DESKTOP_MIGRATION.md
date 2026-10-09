# LUMERYN — MANDATORY NATIVE RUST DESKTOP MIGRATION

## CRITICAL ARCHITECTURE DECISION — EXECUTE NOW

You are the Lead Architect and Engineering Orchestrator for **Lumeryn**, a private, proprietary, professional, AI-agent-native video editor developed under the NEXDARK LABS brand.

This is a mandatory architecture update to our existing project, not a request to start a new project.

**Our final selected desktop technology stack is:**

- **Rust** — primary implementation language.
- **egui** — native immediate-mode GUI.
- **eframe** — native desktop application framework.
- **egui_dock** — professional docking and resizable workspaces.
- **egui-wgpu** — GUI integration with GPU rendering.
- **wgpu + WGSL** — GPU video composition, visual effects and custom rendering.

**Tauri, Svelte, Vite and the previous WebView-based Studio architecture are no longer part of the target product.**

This decision supersedes every earlier instruction suggesting Tauri 2, Svelte 5, TypeScript-based desktop UI or other browser-based desktop frameworks for Lumeryn Studio.

Do not continue building new features on top of Tauri or Svelte.

Do not preserve the old UI architecture as a permanent fallback.

We want a true Rust-native professional desktop editor.

However, preserve all valid and functioning engine code, CLI features, editing logic, project data, media processing, and other useful components.

**Migrate safely, verify continuously, and completely remove obsolete frontend infrastructure after successfully replacing its functionality.**

---

# 1. IMMEDIATE ACTIONS

First inspect the actual repository and determine the current state of implementation.

Identify:

- Existing Cargo workspace and crates.
- Existing Tauri application structure.
- Svelte components and stores.
- JavaScript/TypeScript editing UI.
- Frontend/backend command bridges.
- Project state management.
- Rust media processing.
- GPU renderer.
- CLI commands.
- IPC implementation.
- Existing tests.
- Background jobs.
- Build scripts.
- Installer configuration.
- Relevant documentation.
- Current Git changes.

Do not assume the repository matches an earlier specification. Inspect what actually exists.

Preserve uncommitted changes and avoid destructive Git commands.

Create a safe migration checkpoint using the existing Git workflow where appropriate.

Do not overwrite independent work from other active agents.

Before editing shared files, coordinate or stop in-progress tasks that still target Tauri/Svelte.

Update `TASKS.md` with a native desktop migration plan.

Start implementation immediately after understanding the repository.

Do not stop at architecture documents or a comparison report.

---

# 2. NEW OFFICIAL APPLICATION ARCHITECTURE

Lumeryn must have three independent architectural layers.

## A. Lumeryn Engine

The core video editing engine remains written in Rust.

Responsibilities:

- Project management.
- Non-destructive timeline.
- Clip operations.
- Effects.
- Keyframes.
- Animation.
- Compositing.
- Video and audio processing.
- Media indexing.
- Database access.
- Resource scheduling.
- Rendering.
- Exporting.
- AI and Blender integration.
- Job management.
- Revision history.

The engine must not depend on egui, eframe, or desktop window APIs.

Keep the core reusable by both the CLI and Studio.

## B. Lumeryn CLI

Retain the standalone Rust executable `lumeryn`.

Use Clap and structured JSON output.

Codex and other external agents must continue to use it to control Lumeryn.

The CLI must work without launching the desktop application.

It must support headless media operations and rendering.

No MCP integration is required.

Preserve all previously implemented CLI functionality.

## C. Lumeryn Studio

Replace the previous Tauri/Svelte application with a native Rust desktop application.

Use:

- egui for application widgets.
- eframe for window and application lifecycle management.
- egui_dock for professional panel layouts.
- egui-wgpu for GUI/GPU integration.
- wgpu for video frame composition and preview rendering.
- Rust application services for project interaction.

Do not use HTML, CSS, JavaScript, TypeScript, React, Svelte, Electron, Dioxus WebView or Tauri for the Studio.

The native desktop application must connect to the same shared project service used by the CLI.

There must be only one authoritative project state.

---

# 3. NATIVE GPU RENDERING ARCHITECTURE

Build Lumeryn Studio around a reliable wgpu-based rendering pipeline.

Research the latest compatible stable versions of egui, eframe, egui-wgpu, egui_dock and wgpu.

Pin verified compatible versions.

Avoid accidentally including incompatible duplicate versions of egui or wgpu.

Use the wgpu renderer provided by eframe where practical.

Investigate sharing the underlying wgpu device, queue, textures and command submission resources with the Studio's video renderer.

Use supported egui-wgpu APIs and custom paint callbacks.

Avoid unnecessarily creating multiple GPU devices.

Avoid wasteful GPU-to-CPU-to-GPU round trips.

Do not promise true zero-copy video decoding without verifying backend support.

For hardware-decoded video, investigate appropriate texture interop, synchronization and copying strategies for each backend.

Measure the actual latency and cost.

## Rendering responsibilities

**egui**

Handles user-facing widgets, menus, timelines, inspectors, shortcuts, controls and interaction.

**wgpu**

Handles GPU compositing, image processing, GPU textures, shader effects, masks and video frame presentation.

**FFmpeg and media subsystem**

Handle media demuxing, decoding, audio processing and encoding.

**Lumeryn Core**

Controls authoritative project state and timeline evaluation.

Keep these responsibilities separated.

Do not decode a video file or execute expensive effects directly inside the egui update function.

---

# 4. PROFESSIONAL VIDEO PREVIEW

The video preview is one of the highest-priority systems.

Develop a native preview monitor capable of:

- Displaying real decoded video frames.
- Play and pause.
- Accurate seeking and scrubbing.
- Frame stepping.
- Aspect-ratio preservation.
- Timeline-synchronized playback.
- Preview quality settings.
- Proxy switching.
- Viewing effects.
- Viewing text and graphics.
- Displaying multiple composited layers.
- Resize-aware rendering.
- High-DPI scaling.
- Handling GPU device loss.
- Responsive interaction during background rendering.

Use proper media timestamps and an explicit audio/video clock strategy.

Avoid tying video decoding speed to UI frame rate.

Avoid unnecessary repeated texture uploads.

Design double-buffering, triple-buffering or another measured frame-presentation strategy where appropriate.

Do not present static images or placeholder video frames as a completed playback implementation.

The exported video and preview must follow the same effect and timeline evaluation rules.

---

# 5. CUSTOM HIGH-PERFORMANCE TIMELINE

Do not build the editing timeline using hundreds or thousands of ordinary generic widgets.

Create a dedicated optimized timeline component.

Use egui custom painting, pointer interaction and virtualized rendering where appropriate.

Requirements:

- Multiple video tracks.
- Multiple audio tracks.
- Clip blocks.
- Thumbnail strips.
- Waveforms.
- Track headers.
- Playhead.
- Time ruler.
- Zoom.
- Scroll.
- Selection.
- Dragging.
- Trimming.
- Splitting.
- Snapping.
- Transitions.
- Markers.
- Keyframes.
- Effect indicators.
- Agent edit indicators.
- Track locking.
- Track visibility.

Only process and draw visible timeline regions where feasible.

Cache expensive timeline visuals.

Ensure scrolling and dragging stay responsive with large projects.

Use rational timestamps and correct frame-rate conversion.

Do not directly mutate authoritative SQLite project state from UI paint code.

The UI must dispatch validated commands to the core.

---

# 6. PROFESSIONAL NATIVE STUDIO INTERFACE

Make Lumeryn Studio look and feel like premium professional creative software.

Our quality target is the responsiveness and polish associated with applications such as Zed, DaVinci Resolve and other serious desktop creative tools.

This is a design and performance target, not a claim that adopting egui automatically gives us their performance.

Use an elegant dark cinematic visual design.

Keep the interface clean, consistent and accessible.

Use thoughtful typography, spacing, iconography, contrast and panel hierarchy.

Do not settle for the default appearance of an egui demo application.

Implement a coherent design system with shared tokens for:

- Colors.
- Typography.
- Spacing.
- Panel backgrounds.
- Borders.
- Rounded corners.
- Interactive states.
- Icons.
- Selection highlights.
- Hover states.
- Focus states.
- Alerts.
- Progress indicators.

Use consistent sizing across the application.

Support high-DPI Windows displays.

Maintain keyboard navigation where feasible.

## Required desktop panels

Create a professional docking workspace supporting:

1. Media Library.
2. Preview Monitor.
3. Video/Audio Timeline.
4. Inspector.
5. Effects Browser.
6. Motion Graphics.
7. Animation/Keyframes.
8. Project Settings.
9. Render Queue.
10. AI Models.
11. Hardware Resources.
12. Agent Activity.
13. History/Undo.
14. Logs and Diagnostics.

Do not implement empty decorative panels and claim they are functional.

Prioritize panels according to development phase.

Use egui_dock for movable, resizable and rearrangeable panels.

Persist workspace layouts.

Allow users to restore default layouts.

Support beginner-friendly and professional workspace presets.

---

# 7. BEGINNER-FIRST USER EXPERIENCE

Remember Lumeryn's primary product purpose.

The majority of our target users may have little or no editing experience.

They should be able to tell an external AI agent such as Codex what video they want, provide optional files, and let the agent perform the editing.

Do not force beginner users to understand every timeline control.

The native Studio must provide an easy way to:

- Open a project.
- See current progress.
- Watch the edited result.
- Inspect generated scenes.
- Review changes.
- Play and pause.
- Request or apply revisions through the external agent workflow.
- Export completed videos.

Do not introduce an unnecessary built-in LLM agent.

Lumeryn's CLI is already the primary agent interface.

The Studio is the visual editing and review application.

Professional editors must also be able to work manually.

Provide beginner and professional layout presets without maintaining two independent editing engines.

---

# 8. CODEX LIVE EDITING SYNCHRONIZATION

Preserve and strengthen our central agent-native architecture.

The workflow must remain:

External Codex agent → Lumeryn CLI → Shared Rust project service → Lumeryn Engine → Native Studio.

If Codex adds a clip, the native timeline must update.

If Codex modifies a title, the Inspector and preview must reflect the committed changes.

If Codex generates a 3D scene, the UI must show progress and then display the resulting asset.

If Codex trims a clip, the timeline must show the new range.

The CLI must work while Studio is closed.

Studio must reconnect to the latest project state when opened.

Use secure local IPC, revision-aware events and a single authoritative project writer.

Do not introduce independent copies of the project state that can silently diverge.

Prevent concurrent edits from overwriting one another.

Do not block the native UI thread waiting for CLI jobs or external processes.

Use background tasks and bounded communication channels.

Handle service disconnection and recovery gracefully.

---

# 9. BACKGROUND JOBS AND RESOURCE MANAGEMENT

Keep existing Rust scheduling and resource-management systems.

The native UI must stay responsive while:

- FFmpeg encodes video.
- Blender renders 3D.
- Whisper transcribes speech.
- Kokoro generates narration.
- Other TTS backends execute.
- SAM generates masks.
- Upscaling or interpolation models process frames.
- Semantic footage indexing runs.

Heavy work must run outside the UI thread.

Use Tokio or other appropriate Rust execution primitives.

Coordinate worker lifetimes and cancellation.

Avoid unbounded channels, uncontrolled task spawning, and blocking synchronous operations inside the GUI event loop.

Preserve CPU and GPU resource reservations.

Prioritize interactive preview appropriately.

Support graceful application shutdown and long-running headless jobs.

---

# 10. KEEP ALL EXISTING FUNCTIONAL CORE FEATURES

The UI migration must not change our core technology decisions.

Preserve:

- Rust editing engine.
- Rust CLI.
- Clap.
- Serde.
- Tokio.
- SQLite.
- SQLx.
- SQLite WAL.
- FFmpeg.
- wgpu/WGSL rendering architecture.
- Project revision history.
- Atomic editing transactions.
- Hardware resource detection.
- CPU/GPU job scheduling.
- AI model manager.
- Optional Kokoro.
- Optional Chatterbox.
- Optional Qwen3-TTS.
- Whisper.cpp integration.
- Scene detection.
- Semantic footage search.
- Optional SAM.
- Optional RIFE.
- Optional upscaling.
- Blender headless integration.
- Animation and motion-graphics roadmap.
- Educational explainer functionality.
- All existing valid project formats and assets.

Preserve functioning functionality even when it was previously called through Tauri commands.

Move reusable Rust application logic out of Tauri-specific wrappers into independent Rust services.

Expose those services through the new native UI.

Do not rewrite functioning business logic simply because its original entry point was Tauri.

---

# 11. COMPLETELY REMOVE OLD TAURI/SVELTE INFRASTRUCTURE

This requirement is mandatory.

After replacement and verification, remove obsolete desktop frontend infrastructure, including applicable:

- Tauri Rust dependencies.
- Tauri plugins.
- Tauri runtime commands and adapters no longer needed.
- Tauri configuration.
- Old `src-tauri` application scaffolding.
- Svelte components.
- SvelteKit scaffolding, if present.
- Vite configuration.
- TypeScript-only Studio UI code.
- JavaScript-only state management.
- WebView bridges.
- Browser DOM dependencies.
- Old frontend build scripts.
- Obsolete UI assets.
- Tauri-specific installers and updater configuration.
- Unused frontend npm dependencies.
- Obsolete frontend lockfiles.
- Redundant documentation.
- Unused workspace entries.
- Old CI steps that only build the discarded frontend.

Inspect each file before deletion.

Do not blindly delete a directory containing useful Rust core code, project assets, unrelated tooling, or work from another agent.

Move reusable assets and logic to the correct native modules.

Remove old application components only after ensuring their supported functionality has been replaced or explicitly accounted for.

Do not leave two competing Studio implementations in the production workspace.

Do not keep Tauri as a hidden fallback.

Do not require Node.js or npm merely to compile or launch the finished native Lumeryn Studio.

If any independent optional external backend legitimately requires Node.js, keep it isolated and document why. That does not justify keeping the old Studio frontend.

Historical ADRs may mention Tauri as a rejected architecture, but current build instructions and product architecture must describe the new native Studio accurately.

---

# 12. RUST WORKSPACE STRUCTURE

Integrate the native desktop application into the existing Cargo workspace.

A possible structure is:

- `crates/lumeryn-core`
- `crates/lumeryn-project`
- `crates/lumeryn-timeline`
- `crates/lumeryn-media`
- `crates/lumeryn-render`
- `crates/lumeryn-preview`
- `crates/lumeryn-audio`
- `crates/lumeryn-ipc`
- `crates/lumeryn-scheduler`
- `crates/lumeryn-models`
- `crates/lumeryn-cli`
- `apps/lumeryn-studio`

Do not unnecessarily create these crates if suitable existing modules already exist.

Prefer clean migration over mass renaming.

Avoid circular dependencies.

Keep egui and eframe dependencies confined to the desktop UI and related presentation modules.

The standalone CLI and shared headless core must not depend on desktop window initialization.

---

# 13. WINDOWS AND FUTURE PLATFORM SUPPORT

Windows is our initial target.

Design for future Linux and macOS support where reasonable.

Support:

- Native Windows window creation.
- High-DPI displays.
- Window resizing.
- Fullscreen preview.
- Keyboard shortcuts.
- File drag and drop.
- Multiple monitors.
- Application icons.
- Error dialogs.
- Native filesystem access.
- Persistent settings.
- Secure local IPC.
- Proper resource cleanup.

Do not use Windows-only code in portable engine crates unless it is isolated behind platform abstractions.

For release packaging, replace Tauri-dependent installer infrastructure.

Research `cargo-packager`, NSIS, WiX/MSI and other appropriate commercial redistribution options.

Plan code signing and secure application updates separately.

Do not implement unsigned automatic code execution as an updater.

---

# 14. MULTI-SUBAGENT EXECUTION

Use Codex native subagents wherever available.

You are the Lead Orchestrator.

Delegate independent work to specialist subagents.

Suggested responsibilities:

**Agent A — GUI Architecture**

- Research compatible crate versions.
- Create native application shell.
- Integrate egui/eframe.
- Build docking layout.

**Agent B — GPU Preview**

- Investigate shared wgpu resources.
- Implement native video preview prototype.
- Benchmark texture presentation.
- Test playback and seeking.

**Agent C — Core Integration**

- Extract reusable Tauri-bound Rust logic.
- Implement native core service calls.
- Preserve IPC and project synchronization.
- Build integration tests.

**Agent D — Migration Audit**

- Identify obsolete frontend code.
- Audit dependencies.
- Plan safe cleanup.
- Review licensing and architecture consistency.

**Agent E — QA and Performance**

- Create smoke tests.
- Test native UI.
- Benchmark timeline responsiveness.
- Detect migration regressions.

Only run agents in parallel when their tasks and file ownership do not conflict.

Avoid having multiple agents modify the workspace manifest or shared application entry point simultaneously.

Prefer isolated branches/worktrees where supported.

Coordinate integration centrally.

Do not fabricate subagent work or reports.

---

# 15. MIGRATION EXECUTION PHASES

Execute in the following sequence.

## Phase A — Inspect and safeguard

- Examine current implementation.
- Preserve working changes.
- Identify frontend-to-engine boundaries.
- Create migration tasks.
- Coordinate ongoing agents.

## Phase B — Native Rust foundation

- Select compatible crate versions.
- Integrate egui, eframe, egui_dock and wgpu.
- Create the native Lumeryn Studio binary.
- Verify the application launches on Windows.
- Establish basic native window and rendering.

## Phase C — Connect to the existing engine

- Connect the native Studio to the shared Rust core.
- Open existing projects.
- Query project state.
- Display timeline data.
- Subscribe to revisions.
- Preserve CLI functionality.

## Phase D — Real GPU preview

- Decode test footage.
- Display actual video frames.
- Implement play/pause and seeking.
- Test frame timing.
- Test resizing.
- Handle audio synchronization.
- Profile frame latency and memory usage.

## Phase E — Functional editor panels

- Media Library.
- Preview Monitor.
- Timeline.
- Inspector.
- Project Settings.
- Background Jobs.
- Agent Activity.

Implement working functionality, not static decoration.

## Phase F — Professional interaction

- Clip selection.
- Drag and trim.
- Track scrolling.
- Timeline zoom.
- Undo/redo.
- Docking layout.
- Shortcuts.
- GPU preview synchronization.

## Phase G — Complete old frontend removal

Once native replacement functionality has passed tests:

- Remove Tauri.
- Remove Svelte.
- Remove Vite.
- Remove obsolete frontend bridges.
- Remove unused packages.
- Remove obsolete build and installer configuration.
- Update Cargo workspace and lockfile.
- Update relevant CI.
- Confirm no old frontend runtime remains necessary.

## Phase H — Full verification

Verify native Studio, Rust engine and CLI function together.

Fix actual regressions before considering the migration complete.

---

# 16. MANDATORY ACCEPTANCE TESTS

Do not claim migration completion until the following have been checked.

**Test 1 — Rust compilation**

The Cargo workspace builds successfully for the supported target.

**Test 2 — Native Studio**

Lumeryn Studio launches without Tauri or WebView.

**Test 3 — Existing project**

An existing Lumeryn project can be opened without corruption or undocumented data loss.

**Test 4 — Actual video playback**

A real imported video can be displayed, played, paused and scrubbed using the native GPU preview implementation.

**Test 5 — Timeline interaction**

The native timeline displays project tracks and supports implemented interactive editing operations.

**Test 6 — Codex CLI integration**

The Rust CLI can modify the project while the Studio is open.

**Test 7 — Live synchronization**

Committed CLI edits appear in the native Studio without manually reopening the project.

**Test 8 — Headless operation**

The CLI can perform supported editing and export operations while Studio is closed.

**Test 9 — Audio/video correctness**

Preview and exported media maintain expected synchronization within documented tolerances.

**Test 10 — Background jobs**

Long-running rendering or AI processing does not freeze the UI.

**Test 11 — Dependency cleanup**

The production Studio no longer depends on Tauri, Svelte, Vite or Node.js-based frontend tooling.

**Test 12 — Real export**

The core engine can export a valid playable video after migration.

**Test 13 — Error handling**

Missing media, GPU issues, service disconnection and malformed commands fail safely.

**Test 14 — Performance**

Record actual startup time, UI responsiveness, playback behavior, CPU/RAM usage and GPU/VRAM usage.

Use real test evidence.

If GUI testing cannot run in the current environment, report that limitation and provide an exact reproducible Windows test procedure. Do not call unexecuted tests passed.

---

# 17. PERFORMANCE OBJECTIVES

Target a Zed-like feeling of responsiveness while recognizing that Zed uses a different GUI framework.

Proposed engineering targets:

- Smooth 60 FPS editing UI on suitable hardware.
- Minimal idle CPU/GPU activity.
- Low-latency timeline interaction.
- Frame-accurate timeline editing.
- Smooth basic 1080p playback.
- Responsive UI during background processing.
- Efficient GPU texture management.
- Predictable memory usage.
- Fast startup.
- Graceful fallback on lower-end hardware.

These are engineering targets, not promised benchmark results.

Optimize based on measurements rather than assumptions.

Do not run expensive database, media or AI operations inside the UI update loop.

Use event-driven repaint requests where practical.

Avoid continuous rendering when the editor is idle.

Draw only visible timeline content.

Manage GPU texture lifetime carefully.

---

# 18. DOCUMENTATION UPDATES

Update all authoritative project documentation.

At minimum inspect and update:

- `AGENTS.md`
- `ARCHITECTURE.md`
- `TASKS.md`
- `PROGRESS.md`
- `README.md`
- `docs/PRODUCT_SPEC.md`
- Relevant architecture decision records.
- Desktop build instructions.
- Testing documentation.
- Distribution plans.

Clearly state that the native Rust GUI supersedes the previous Tauri/Svelte architecture.

Create an ADR documenting:

- Why we migrated.
- The selected stack.
- Technical trade-offs.
- Integration details.
- Performance observations.
- Compatibility risks.
- Final architectural structure.

Keep historical records accurate, but eliminate conflicting current instructions.

Future Codex sessions must not accidentally recreate the old Tauri frontend because outdated documentation tells them to.

---

# 19. COMPLETION DEFINITION

The migration is complete only when:

1. Lumeryn Studio is a functioning native Rust desktop application.
2. egui + eframe + egui_dock + wgpu are integrated correctly.
3. The shared Rust core remains independent.
4. Existing CLI behavior is preserved.
5. Project persistence continues working.
6. Native preview and timeline have real implemented functionality.
7. Existing supported editing operations function through the native Studio.
8. CLI/Studio synchronization works.
9. Unnecessary Tauri/Svelte/WebView code has been removed.
10. Cargo builds and applicable tests pass.
11. Documentation accurately reflects the migrated architecture.
12. Real limitations and incomplete features are clearly documented.

Do not hide missing functionality behind placeholder UI.

Do not silently drop existing features.

Do not report a migration as complete merely because the application launches.

---

# 20. FINAL INSTRUCTION

**Proceed with the migration now.**

This is an approved architectural direction, not a request to compare frameworks indefinitely.

Use independent specialist subagents when helpful.

Preserve current working functionality, then replace the Studio implementation in verified stages.

Remove the obsolete Tauri/Svelte frontend completely after its replacement is validated.

Keep the codebase private and proprietary.

Continue building toward Lumeryn's original mission:

**A powerful, native Rust video editing and motion-graphics engine that allows external AI agents such as Codex to create beautiful, professionally edited YouTube videos, Reels, animated explainers and cinematic content for users who may know nothing about video editing.**

The user supplies an idea and optional assets.

The agent handles the creative editing through Lumeryn CLI.

The native Lumeryn Studio lets the user watch, preview, refine and control the finished work.

Do not alter that vision.

**Start by inspecting the repository, coordinating current tasks, and implementing the first native Rust Studio milestone. Continue through the full migration and verification process rather than stopping at planning.**
