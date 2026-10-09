# ADR 0010: Native Rust Studio supersedes the WebView architecture

Date: 2026-10-09. Status: accepted mandatory product direction; implementation and test evidence are recorded separately.

## Decision and context

The owner explicitly replaced Tauri/Svelte/Vite/WebView Studio with native Rust: egui, eframe, egui_dock, egui-wgpu and wgpu/WGSL. This supersedes the desktop selection in ADR 0003 and Tauri-dependent distribution guidance in ADR 0008; their engine-independent single-writer, security, resource, signing and storage requirements remain useful. Tauri is not a fallback. Repository inspection found a Rust core/CLI with no existing frontend implementation, npm lockfile or Tauri assets to delete. Preserve the working project schema and engine.

Studio is `apps/lumeryn-studio`; core remains headless. `lumeryn-service` owns the authoritative writer and revision/job events; CLI and Studio use its typed requests. `lumeryn-preview` runs bounded decode/audio work without GUI dependencies. Optional external backends may independently need Python/Node, but native Studio build/launch does not.

## Compatible pins

Verified current package metadata and downloaded manifests establish egui/eframe/egui-wgpu **0.36.2**, egui_dock **0.21.1**, wgpu **30.0.1**, image **0.25.10** (PNG only) and CPAL **0.18.2** in the preview subsystem. egui_dock 0.21.1 depends on egui 0.36; egui-wgpu 0.36.2 depends on wgpu 30.0. Installed Rust 1.97 meets GUI crates' Rust 1.95 requirement. Workspace exact pins and a reviewed Cargo.lock prevent accidental family duplication. Confirm `cargo tree -i egui` and `cargo tree -i wgpu` when updating; do not independently upgrade a GUI family member.

eframe/egui/egui-wgpu/wgpu/image use MIT or Apache-2.0; egui_dock is MIT. Fonts and transitive libraries have separate obligations, including egui_dock's paste maintenance advisory. Record reviewed exceptions in the dependency audit rather than licensing proprietary Lumeryn publicly. [eframe official API](https://docs.rs/eframe/0.36.2/eframe/), [egui_dock official API](https://docs.rs/egui_dock/0.21.1/egui_dock/), [egui-wgpu official API](https://docs.rs/egui-wgpu/0.36.2/egui_wgpu/), [wgpu](https://docs.rs/wgpu/30.0.1/wgpu/).

## Integration and performance boundaries

eframe owns the native window and one wgpu device/queue. Studio accesses `CreationContext::wgpu_render_state`, creates an RGBA8 texture on that device and registers its view with egui-wgpu. Bounded decoded frames upload only when their generation/frame key changes; resizing preserves video aspect ratio and reuses the texture until source dimensions change. This is CPU decoding followed by GPU upload/presentation, not zero-copy hardware decoding. No preview GPU readback is used; smoke screenshot readback is an explicit test operation.

Supported `CallbackTrait` prepare/paint APIs expose the same device, queue and renderer resources for future native WGSL composition. This first source-frame presentation uses native texture registration; shader effects, multi-layer composition, hardware-decoder interop, device-loss recreation and measured buffering strategies remain additional milestones. Never infer preview/export parity for effects that neither path implements.

Studio owns a display snapshot with revision provenance, not a second writable project. A background Tokio worker handles IPC, history/import/trim/export requests and long-poll events; bounded channels wake the UI. Switching projects tags messages by session epoch so old snapshots cannot replace the new project. Render runs in the service and is separate from the event watcher. UI commands carry expected revision and idempotency IDs. Project media remains immutable.

Timeline uses custom painting and clip hit regions, with horizontally clipped visible blocks. The current implementation scans clip metadata to identify visible clips; it does not claim an indexed O(visible) query. Future interval indexing, cached thumbnails/waveforms and richer tracks must be measured. No decode/database/export work runs in the UI update; per-frame CPU upload submission is measured separately from GPU completion.

## Working scope and gates

Functional panels are Media Library (import/place), Preview (real decode/play/pause/seek/frame step/quality), Timeline (selection/ruler scrub/right-edge trim/zoom/scroll), Inspector (validated trim), Project Settings (actual read-only format), Render Queue (background export/events), Agent Activity (real events), Revision History (real command records), and Diagnostics. Beginner/professional dock layouts are persisted and resettable. Unsupported effects, motion graphics, models, undo/redo, track controls or resource scheduling do not get decorative functional-looking panels.

Native smoke mode takes an explicit disposable project and new artifact directory, scripts commands through the same paths, invokes the actual CLI for a live edit, checks event/snapshot convergence, exports to a new path, captures the native viewport and closes. Its report must be inspected alongside decoded output and screenshot. Unit tests cover viewport bounds/layout persistence and a 10,000-clip CPU painting benchmark. Workspace fmt/clippy/tests, existing headless media acceptance, security tests and dependency review remain required. Native launch, A/V tolerance, UI responsiveness and visual review are separately measured; a build or window screenshot alone does not complete all fourteen requested migration acceptance checks.

## Consequences

The engine stays reusable and the minimal Studio needs no frontend toolchain. Immediate-mode UI offers explicit painting/control but requires disciplined worker boundaries and measured layout/render costs. A polished editor and explainer engine still require native composition, typography, keyframes, audio robustness, accessibility/usability testing and platform-specific interop work. See [native implementation/testing notes](../research/NATIVE_STUDIO.md) for exact reproducible commands, current evidence and limits.
