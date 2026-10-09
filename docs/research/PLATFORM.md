# Platform feasibility and optional backend research

Research date: 2026-10-09. Owner: Architecture research agent. Scope: Studio, preview, local IPC, hardware scheduling, optional backends, interchange and color. This is design research, not evidence that any of these subsystems has been implemented or benchmarked. The CLI milestone's actual evidence belongs in `PROGRESS.md`.

## Studio and preview

Tauri 2 supplies a Rust host and web UI; its upstream architecture uses TAO for windowing and WRY for platform WebViews. Keep Svelte 5/TypeScript as presentation and interaction code, with all editing commands and evaluation in Rust. Tauri is MIT OR Apache-2.0; Svelte is MIT. Preserve upstream notices and audit the eventual locked transitive graph. [Tauri architecture](https://v2.tauri.app/concept/architecture/), [Svelte license](https://github.com/sveltejs/svelte/blob/main/LICENSE.md), [Svelte 5 guide](https://svelte.dev/docs/svelte/v5-migration-guide).

Windows deployment must account for WebView2 and MSVC build prerequisites. A Windows-only successful build does not validate Linux/macOS WebView differences. Exact Studio dependency versions will be pinned after an actual compatibility build; no Studio package lock is claimed here. Prefer a static Svelte/Vite application; no server-side SvelteKit runtime is needed. [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/), [Windows installer](https://v2.tauri.app/distribute/windows-installer/).

wgpu supports native D3D12 and Vulkan on Windows. The current documentation fetched reports wgpu 30.0.1, MSRV 1.87, with WGSL enabled by default. This observation is not a tested selection: verify the release, local compiler, driver, raw-window-handle compatibility and Windows examples before pinning. wgpu is MIT OR Apache-2.0. [Maintainer API documentation](https://docs.rs/wgpu/latest/wgpu/), [upstream repository](https://github.com/gfx-rs/wgpu), [backend overview](https://wgpu.rs/).

Nothing reviewed establishes a stable, portable way to insert a native wgpu texture directly into a Tauri DOM element. Tauri's child-WebView creation API itself is documented as unstable, and is not a native GPU viewport API. Treat native viewport composition as a feasibility gate. [Tauri webview API](https://v2.tauri.app/reference/javascript/api/namespacewebview/).

| Approach | Benefit | Cost and unresolved risk | Recommendation |
| --- | --- | --- | --- |
| Native preview window/surface | Rust renderer owns presentation; avoids JavaScript pixel serialization | Separate window is easy to prototype; child-window layout, input, z-order, DPI and ownership need Windows work | First accelerated experiment; retain detachable preview fallback |
| CPU-decoded binary frames to WebView canvas | Transparent cross-platform fallback; easy golden-frame capture | CPU copy, GPU upload, buffer lifetime and transport overhead | Test at proxy resolution with bounded buffers |
| Encoded preview stream/browser playback | Browser performs decode/presentation | Encode latency, seek, audio clocks, color fidelity and exact-frame stepping | Comparative experiment only; not authoritative timeline evaluator |
| D3D shared-texture/DirectComposition bridge | Potential reduction of GPU copies | Platform-specific unsafe interoperability; unsupported assumption until demonstrated | Deferred experiment if simpler approaches miss targets |

Calculated lower bound: 1920 x 1080 x 4 x 60 = 497,664,000 bytes/s (~474.6 MiB/s) for uncompressed RGBA8 at 60 fps, before copies; 4K is four times larger. One 1080p RGBA8 frame is ~7.91 MiB. This is arithmetic, not measured throughput. Streaming raw frames through JSON/base64 events would add avoidable allocation and encoding costs.

Tauri's event documentation explicitly reserves its event mechanism for ordinary messages, with channels recommended for ordered streaming. Use typed Rust commands for edits, bounded channels/control notifications, and a separate measured binary/native frame transport. Neither channels nor events prove zero-copy behavior. UI revision streams and video frame queues need separate backpressure policies. [Tauri frontend delivery](https://v2.tauri.app/develop/calling-frontend/).

## Secure synchronized service

Use one authoritative writer per open project. The headless CLI and Studio share the Rust command layer; before introducing simultaneous clients, route writes through a project service. Short-lived CLI transactions may form the initial slice, but are not a live synchronization implementation.

Prefer Windows named pipes and Unix-domain sockets behind one transport interface. Do not substitute an unauthenticated localhost HTTP API. Windows default named-pipe permissions grant read access to Everyone and anonymous users, so create an explicit restrictive security descriptor rather than relying on defaults. Reject remote pipe clients, prevent duplicate first-instance servers, and verify peer identity where supported. Protect service discovery and session credentials with current-user OS permissions. [Named-pipe security](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights), [CreateNamedPipe flags](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea).

Application policy: a versioned handshake authenticates the session before any project operation; frame messages with length limits and deadlines; separate read, edit and job permissions; never log authentication secrets. Same-user malicious processes and administrator access are not eliminated by a user ACL. Test the boundary explicitly instead of describing the pipe as a sandbox.

Persist edits, revision increment, idempotency response and durable change event in one SQLite transaction. Notify clients only after commit. Each event carries project ID, persistent session identity, service epoch, revision, event sequence and typed payload. Resubscribe from last seen sequence; replay retained events or return a consistent snapshot/cursor if retention expired. Slow clients must resynchronize rather than silently lose state. Jobs may publish ephemeral progress but must persist meaningful state transitions. These are Lumeryn design requirements, not guarantees supplied by Tauri events.

## Hardware scheduling

GPU enumeration identifies an adapter and supported APIs; it does not prove a decoder, encoder or inference provider works. Probe those backends separately with small actual operations. Distinguish total dedicated VRAM, current OS budget and process usage. Windows DXGI exposes a process memory budget/usage API; it warns that exceeding assigned budgets can cause paging and stutter. Its budget is not a universal whole-system free-VRAM measurement. [DXGI QueryVideoMemoryInfo](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_4/nf-dxgi1_4-idxgiadapter3-queryvideomemoryinfo).

Application policy: capability/telemetry values include source, timestamp and explicit unknown status. Use optional NVML only for NVIDIA-specific metrics after driver/library availability checks; never make it necessary for editing. A resource plan combines measured peak RAM/VRAM, runtime workspace, cache reservations, CPU slots, storage and playback headroom. Do not infer compatibility from parameter count or marketing GPU memory. Serialize unprofiled heavy GPU jobs by default; offer an explicit profiling job. Re-evaluate reservations when budgets change.

Balanced, Smooth Editing and Fastest Processing alter priorities, not correctness. Playback has protected resources; background analysis yields, exports and Blender may require exclusive GPU access. Cancellation is bounded and releases reservations only after worker exit; partially generated assets remain unregistered. Cross-process workers need measured external memory estimates because renderer-process DXGI usage does not include every worker.

## Optional Blender and AI boundaries

Blender remains a separately installed optional program. Detect its path/version and validate compatibility; a Rust adapter accepts a schema, then invokes maintained trusted code using an argument array. Disable automatic project scripts, skip user startup state, and set a nonzero Python-exception exit code. Use isolated working/output directories and sanitized environment; validate expected frames, alpha/depth metadata and completion before asset registration. Disable-autoexec does not sandbox the trusted adapter or malformed file parsers. [Blender command-line arguments](https://docs.blender.org/manual/en/5.1/advanced/command_line/arguments.html).

Blender is GPL; its license page distinguishes Blender code from artwork output. Keeping Blender external is a design choice, not a categorical legal exemption: review adapter scripts/add-ons, combined-work questions and any binary redistribution separately. Do not bundle Blender or promise proprietary rights to Blender-derived adapter code before that review. Ordinary Lumeryn code remains proprietary. Generated media still needs independent asset rights. Alternative: native simple 3D titles for limited features or another user-installed renderer with separately reviewed terms. [Blender license](https://www.blender.org/about/license/).

ONNX Runtime is MIT and supports execution-provider selection, but each provider has hardware, package, driver and operator constraints. Rust bindings are listed as community bindings. Prefer native runtime adapters when reliable; isolate Python-only backends without global environment mutation. Confirm operators, output quality, CPU fallback, cancellation and peak memory for each exact model/runtime/device combination. [ORT license](https://github.com/Microsoft/onnxruntime/blob/main/LICENSE), [execution providers](https://onnxruntime.ai/docs/execution-providers/), [API support](https://onnxruntime.ai/docs/api/).

Runtime licensing does not license checkpoints, voice packs, datasets or converted weights. Kokoro, Chatterbox Turbo, Qwen3-TTS Base/CustomVoice, Whisper, SAM 2.1, Depth Anything V2 Small, RIFE, Real-ESRGAN and semantic-search models remain candidates requiring individual pinned artifact audits. A model manifest must record source, revision, digest, weight/code/voice licenses, commercial and redistribution conditions, download size, privacy behavior, device requirements and benchmark evidence. Unknown/restricted artifacts stay unavailable for automatic installation. No model download or provider charge is authorized by this research.

ORT's current official privacy documentation describes telemetry in official builds and controls that differ by platform/build. Prefer a verified telemetry-free build; otherwise configure documented controls before initialization and test network behavior. Calling a disable API after initialization must not be represented as proof that no initialization event occurred. [ORT privacy](https://github.com/microsoft/onnxruntime/blob/main/docs/Privacy.md).

## Interchange and color

OTIO is a C++ editorial interchange library with official Python bindings; it references media and is not a media container. Its code is Apache-2.0 and adapters beyond native OTIO formats are separate packages/repositories. Lumeryn's rational timeline and SQLite revisions remain authoritative. Build an optional adapter with a documented representable subset, warnings for unsupported constructs, provenance and round-trip tests. Audit each imported adapter and disable implicit hook/script execution. Avoid requiring Python for normal edits. [OTIO upstream](https://github.com/AcademySoftwareFoundation/OpenTimelineIO).

OCIO is BSD-3-Clause; retain notices and disclaimer and avoid implied endorsement. It supplies CPU/GPU color processors, with official GPU examples based on OpenGL/GLSL. Do not assume generated GLSL is directly usable as WGSL. Investigate a narrow C++ bridge and shader/LUT conversion or validated baking, keeping CPU processing as a reference path. Store explicit source/working/display/output spaces and pinned config/LUT hashes. Audit config and LUT assets independently. [OCIO license](https://github.com/AcademySoftwareFoundation/OpenColorIO/blob/main/LICENSE), [OCIO development API](https://opencolorio.readthedocs.io/en/latest/guides/developing/developing.html).

## Required experiments and measurement ledger

All items below are **planned, not run** in this research task. Benchmark actual host details first; the specified i7/RTX 3050 6GB/32GB Windows profile is a target, not a detected fact. Record exact toolchain/build hashes, OS/driver, fixture seed, resolution/rate, warm/cold cache, CPU/RAM/VRAM, repetitions and p50/p95/p99. Initial budgets are hypotheses to revise after evidence.

| ID | Experiment | Evidence required before adoption |
| --- | --- | --- |
| P-01 | Tauri2/Svelte5 Windows compatibility shell | MSVC + WebView2 versions, production build, secure command invocation, offline launch |
| P-02 | Native wgpu separate/child viewport vs binary proxy frames | 1080p24/30/60; 4K source with proxy; seek latency, dropped frames, copies, CPU/VRAM; DPI resize and multimonitor tests; device loss fallback |
| P-03 | Playback clocks | Ten-minute generated AV sync fixture; pause, scrub, frame-step, resume; measured drift and exact displayed frame |
| P-04 | Authenticated IPC writer | Two clients, revision conflicts, replay/idempotency, service kill before/after commit, bounded slow subscriber, denied wrong user/remote connection |
| P-05 | Scheduler and hardware reporting | Actual adapter/telemetry report; unavailable API shown unknown; memory pressure; cancellation; playback plus CPU job and GPU-heavy job |
| P-06 | Blender adapter | Pinned supported release, trusted fixture, alpha/depth outputs, bad scene, timeout/cancel/process tree, disabled autoexec, no stray registered partial assets |
| P-07 | Optional model profiling | Per-artifact license approval, checksummed install, operator/device capability, peak RAM/VRAM, CPU fallback, network audit, output-quality fixtures |
| P-08 | OTIO subset | 24000/1001 and 30000/1001 edit round-trips; missing media; unsupported effect warnings; bounded parser/plugin isolation |
| P-09 | OCIO reference path | Known reference patches, alpha conventions, CPU/GPU parity, explicit transforms/config hash, preview/export matching; HDR separately gated |

Provisional preview targets: sustain fixture cadence for ten minutes with <1% unexplained frame drops, warm proxy seek p95 <100 ms, no unbounded buffers, and AV drift below one project frame. These are acceptance targets, not achieved results. Expose degraded quality/unsupported status when targets cannot be met. Codec, external binary and model release audits remain separate release gates.
