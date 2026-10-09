# Native Studio implementation and verification notes

Date: 2026-10-09. The mandatory native migration replaces earlier Tauri/Svelte plans. There was no implemented WebView frontend to remove. Native Rust Studio is being implemented in `apps/lumeryn-studio`; core/CLI project data and operations are preserved.

## Verified compatibility and official references

Use exact workspace pins egui/eframe/egui-wgpu 0.36.2, egui_dock 0.21.1 and wgpu 30.0.1. Downloaded egui_dock manifest requires egui 0.36; downloaded egui-wgpu manifest requires wgpu 30.0. Current APIs differ from older tutorials: eframe `App::ui(&mut Ui, &mut Frame)`, egui `Context::set_global_style` and `egui_wants_keyboard_input`. Rust 1.95 is required by the GUI family, met by local 1.97. Cargo tree confirmed a single egui and wgpu version after root lock generation; upgrades must reverify that fact. [eframe](https://docs.rs/eframe/0.36.2/eframe/), [egui_dock](https://docs.rs/egui_dock/0.21.1/egui_dock/), [egui-wgpu](https://docs.rs/egui-wgpu/0.36.2/egui_wgpu/), [wgpu](https://docs.rs/wgpu/30.0.1/wgpu/).

eframe uses winit native windows, AccessKit accessibility, bundled default fonts and the wgpu renderer. Persistence serializes a versioned dock layout; smoke overrides its storage location under the artifact directory. No browser/Node runtime is required. Optional Motion Canvas remains a separate future worker.

`egui_wgpu::RenderState` exposes device/queue/renderer. `register_native_texture` requires RGBA8Unorm, and `update_egui_texture_from_wgpu_texture` permits texture ID reuse. Current Studio uses those supported APIs for source-video presentation on the same device. `CallbackTrait::prepare/paint` provides an integration point for future WGSL composition with shared command submission; no additional GPU device is needed. Hardware decoded texture interop is not implemented, benchmarked or described as zero-copy. [Renderer API](https://docs.rs/egui-wgpu/0.36.2/egui_wgpu/struct.Renderer.html), [callback API](https://docs.rs/egui-wgpu/0.36.2/egui_wgpu/trait.CallbackTrait.html), [creation context](https://docs.rs/eframe/0.36.2/eframe/struct.CreationContext.html).

## Architecture and actual supported interactions

The service is the sole writer; CLI and Studio send the same typed edits with revision checks. UI snapshots are display caches bound to a project/session epoch. Bounded background channels avoid UI waits for project loading, SQLx, media inspection, rendering or event polling. Service disconnections enter a reconnect state and revision events refresh project/history automatically. Export runs separately from the watcher and UI thread.

The preview subsystem validates and decodes on its own worker, returns bounded timestamped RGBA frames and exposes device/audio errors explicitly. Studio preserves aspect ratio, presents fresh frames through a registered GPU texture and supports play/pause, seeking, frame step and 360p/720p/1080p decode limits. Frame indices and rational project rates remain authoritative; UI floating-point positioning affects display only. Audio support/tolerances must be established from the preview subsystem's measured acceptance results, not merely its ready-device status.

Custom timeline rendering draws only viewport-intersecting clip blocks/ruler ticks and creates interactions for those visible clips. It supports clip selection, ruler scrub, zoom/scroll and right-edge trim. The inspector dispatches the same trim command. Media import/add commands, read-only format inspection, actual revision history, real agent/job events and background export are functional. The current core supports one video track with source audio. Multi-track audio/video, drag placement, split/snapping, transitions/keyframes, thumbnails/waveforms, proxies, effects/graphics and undo/redo remain staged; there are no decorative empty capability panels.

## Reproducible Windows verification

Run from the repository using an authorized local FFmpeg/ffprobe. First build with bounded compilation jobs to keep the workstation responsive:

```powershell
cargo build --workspace --locked -j 4
cargo fmt --check
cargo clippy --workspace --all-targets --locked -j 4 -- -D warnings
cargo test --workspace --locked -j 4
./scripts/check.ps1 -Media -Deny
```

Use a newly generated single-clip fixture with at least eight frames and square pixels, zero-origin timestamps and a matching CFR rate. Smoke deliberately commits a trim and invokes the actual CLI to restore the duration, leaving auditable revisions; use a disposable fixture. Create/export paths must be new. For example, with a project created/imported/placed through documented CLI commands:

```powershell
./target/debug/lumeryn-studio.exe --smoke --project D:/Dev/freela/Lumeryn/.lumeryn-scratch/native/project.lumeryn --artifact-dir D:/Dev/freela/Lumeryn/.lumeryn-scratch/native/smoke-new --ffmpeg C:/path/to/ffmpeg.exe --ffprobe C:/path/to/ffprobe.exe
Get-Content .lumeryn-scratch/native/smoke-new/studio-smoke.json
```

The harness waits for a real decoded/GPU-presented frame, measures playback advancement, pauses/seeks exactly, steps one frame, performs a UI trim through shared commands, invokes the actual sibling `lumeryn` CLI to restore it, waits for its live event and authoritative snapshot, performs background service export while counting UI updates, requests an eframe native viewport screenshot, writes PNG/JSON asynchronously and closes. Screenshot/report writes use create-new semantics. CLI must be built beside Studio for authenticated autostart and smoke CLI integration. A passing process exit requires a passing smoke report. If a stage fails, inspect the JSON/error logs; do not call later checks passed.

Native screenshot uses the supported `ViewportCommand::Screenshot` → `Event::Screenshot` API, not a placeholder screenshot. Review the saved actual window with an image viewer. Background export success is checked using the service result/full-decode evidence and job-completed event. External frame/audio decode verification remains required for export correctness.

The 10,000-clip headless egui benchmark runs as a unit test. To save its actual observed results to a new file:

```powershell
$env:LUMERYN_TIMELINE_BENCHMARK = 'D:/Dev/freela/Lumeryn/.lumeryn-scratch/native/timeline-new.json'
cargo test -p lumeryn-studio --locked -j 4 benchmark_ten_thousand_clips_visible_paint -- --nocapture
Remove-Item Env:/LUMERYN_TIMELINE_BENCHMARK
```

This validates a 10,000-clip graph, warms ten CPU UI passes and samples one hundred more at a 1000×300 viewport. It measures complete egui layout/custom paint CPU time, reports p50/p95 and visible clip count, and checks that only a small visible subset is painted. It scans all clip metadata; no O(visible) query or GPU presentation rate is claimed.

## Evidence and remaining acceptance

Locked Studio check, all-target clippy with `-D warnings`, all three Studio tests and the native binary build passed on 2026-10-09. The initial 10,000-clip benchmark measured p50 **0.4429 ms**, p95 **0.5479 ms**, with **15** visible clips painted and 100 samples on this host's debug build. This measures CPU egui layout/painting with a metadata scan, not GPU presentation. Native launch/smoke screenshot, external hardware measurements and workspace-wide final gates are recorded by the lead after actual execution in PROGRESS and testing/benchmarks documents.

The lead executed the first Windows native smoke successfully on NVIDIA GeForce RTX 3050 / Vulkan. Actual artifacts are `.lumeryn-scratch/studio-c3f21f155cda480aa9329488083b2046/studio-smoke.json` and `studio-native.png`; both agents reviewed the actual PNG, which shows the real red source-video frame, dark docked panels, inspector and selected timeline clip. The report records revisions 3→4 (Studio trim)→5 (CLI restoration/live event), exact seek to frame 15, step to 16, six new playback frames, 12 UI updates during export, 11 presented/uploads and no decoder frame drops. First frame from `main` was **1111.6 ms**; CPU UI update p50 **1.464 ms**, p95 **6.071 ms**, maximum **82.225 ms**, latest upload submission **0.099 ms**. Audio device reported 48 kHz stereo; this is not an end-to-end A/V measurement. This short 320×180 fixture does not prove smooth 1080p playback or 60 FPS. The first smoke preceded a pointer-trim correction and stricter failure-path handling; the final rebuilt binary requires the lead's repeat smoke for final integration evidence.

Smoke reports first-frame latency measured from the beginning of Rust `main` (excludes pre-main process loading), total script elapsed, CPU UI update p50/p95/maximum over up to 600 samples, presented/decoded/dropped frames, adapter/backend, latest CPU texture submission time, decoder/seek timing, audio device status and exact revision checks. Full process startup and CPU/RAM counters require an external process monitor. Texture submission time is not GPU completion or display latency. VRAM remains unknown until telemetry is implemented. No 60 FPS or 1080p smoothness claim follows from CPU frame counts. Paused idle UI requests no repeated repaint; only pending decode/playback and real worker/input events wake it.

Manual Windows acceptance still checks docking/restore presets, resize/high-DPI, keyboard and pointer trims, accessible contrast/focus, drag/drop media/project handling, missing media/error recovery, service restart, actual audible playback and A/V tolerance on suitable hardware. GPU device-loss recovery and multi-monitor/fullscreen/application icon completion are staged. Test errors must preserve projects and original media. Document actual limitations against all fourteen migration acceptance requirements rather than calling the entire professional-editor roadmap complete.
