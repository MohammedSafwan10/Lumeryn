# ADR 0004: Hardware-aware scheduling and isolated optional backends

Date: 2026-10-09

Status: Accepted design direction; optional integrations and scheduler implementation deferred pending per-backend tests and licensing audits.

## Context

Ordinary editing must work without AI, Blender, CUDA or cloud subscriptions. Optional inference, rendering and analysis compete with playback for RAM, GPU memory, CPU and disk. Model weight size is an inadequate allocation estimate. Proprietary Lumeryn code, runtime libraries, external applications, model checkpoints and generated assets have distinct ownership/license boundaries.

## Decision

Keep backend interfaces in Rust. A capability registry reports installed/verified/unavailable/unknown independently; a runtime's presence never automatically proves every model or codec is usable. The scheduler admits jobs using current telemetry, conservative reservations and measured peak profiles. Unknown telemetry is explicit. Prioritize interactive playback and reserve headroom; permit CPU jobs alongside GPU preview only when measured budgets allow it. Heavy/unprofiled GPU jobs queue or run in controlled profiling mode. Balanced, Smooth Editing and Fastest Processing alter admission priorities only.

Windows resource telemetry may use DXGI for per-process budget and usage, wgpu for adapter/API capabilities, OS APIs for CPU/RAM/storage, and optional NVML for supported NVIDIA metrics. DXGI budget is not interchangeable with whole-machine free VRAM and must not be mislabeled. Workers in separate processes require separate accounting. [DXGI budget API](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_4/nf-dxgi1_4-idxgiadapter3-queryvideomemoryinfo), [wgpu backends](https://docs.rs/wgpu/latest/wgpu/).

Each job declares device preferences, measured/estimated RAM and VRAM peaks, CPU slots, scratch space, cancellation capability, expected outputs and deterministic cache inputs. Plans report measurement provenance and uncertainty. Track loading and transient allocations, release reservations after process termination, use bounded retries with causes, and persist job state. Timeout/cancellation/failure cannot register incomplete assets. Source media is immutable; cache eviction never touches it.

Blender remains user-installed and optional. Rust validates semantic scene JSON, invokes a maintained adapter by argument array in a private job directory, and validates outputs. Explicitly disable automatic scripts, skip user startup configuration, and configure Python exceptions to produce failure exits. Sanitize environment and govern process trees/timeouts. These flags reduce automatic script risk but are not a parser or process sandbox. [Blender arguments](https://docs.blender.org/manual/en/5.1/advanced/command_line/arguments.html).

Audit Blender GPL obligations separately for executable redistribution, adapter scripts/add-ons and any combined-work relationship; external invocation does not by itself establish license compatibility. Blender's license page distinguishes its code from artwork output. No Blender bundle is planned until reviewed. Lumeryn's core remains proprietary; assets used to make artwork have their own licenses. [Blender licensing](https://www.blender.org/about/license/).

Prefer ONNX Runtime/native adapters for supported inference, with isolated pinned Python workers where native support is impractical. ORT is MIT, and its execution providers have independent device/package requirements; validate each exact model/provider pair. A checkpoint manifest records immutable version/hash, code and weight licenses, voice rights, commercial/redistribution limits, dependencies, size and measured requirements. Download/install only explicitly requested artifacts from trusted sources, verify checksums and atomically promote installations. Reject partial or unaudited installs. No automatic model downloads or charges. [ORT license](https://github.com/Microsoft/onnxruntime/blob/main/LICENSE), [ORT providers](https://onnxruntime.ai/docs/execution-providers/).

Disable optional runtime telemetry using build/platform-specific documented controls and verify network behavior. A runtime API call after initialization may not prevent initialization events. Prefer telemetry-free builds for local-first behavior. [ORT privacy](https://github.com/microsoft/onnxruntime/blob/main/docs/Privacy.md).

Use OpenTimelineIO only for optional interchange with an explicitly supported subset and loss warnings; the native Rust project remains authoritative. OTIO core is Apache-2.0, has C++/Python APIs, and non-native adapters are separately packaged. Independently audit and isolate adapters/hooks. Use OpenColorIO as a future explicit color abstraction; its BSD-3-Clause code, configs and LUTs require separate notices/asset review. Prototype CPU reference processing before GPU shader integration, since official GLSL examples do not prove WGSL interoperability. [OTIO](https://github.com/AcademySoftwareFoundation/OpenTimelineIO), [OCIO license](https://github.com/AcademySoftwareFoundation/OpenColorIO/blob/main/LICENSE), [OCIO API](https://opencolorio.readthedocs.io/en/latest/guides/developing/developing.html).

## Alternatives

| Alternative | Trade-off |
| --- | --- |
| Mandatory bundled AI/Python/Blender | Large installer and audit surface; incompatible with optional feature requirement |
| Parameter-count-only resource estimates | Ignores workspace, temporary buffers and competing playback; insufficient for safe admission |
| Unbounded worker concurrency | Leads to oversubscription/OOM; rejected |
| Full native model rewrites | Strong integration but high maintenance and operator-parity cost; evaluate per model |
| Generic arbitrary-script plugin system | Grants unsafe execution from projects; excluded from normal command contract |
| Global Python/CUDA mutation | Dependency conflicts and poor recovery; use isolated reproducible workers |

## Compatibility, performance and release gates

No AI checkpoints, Blender version, OTIO binding or OCIO GPU bridge is declared verified by this ADR. Windows subprocess control, GPU telemetry coverage, multi-adapter selection, provider prerequisites, RAM/VRAM overhead and cancellation need actual tests. SDK/DLL/execution-provider notices must accompany their eventual distributions. Restrictive model licenses may require omitting that artifact, a separately licensed alternative or owner review; they never justify relicensing the Lumeryn core.

Run P-05 through P-09 in `docs/research/PLATFORM.md`. Acceptance includes unknown telemetry handling, peak-memory profiling, playback under load, cancellation without partial asset registration, exact checkpoint/license evidence, generated Blender alpha/depth output validation, fractional-rate OTIO round-trips, and OCIO CPU/GPU reference comparisons. Record actual hardware rather than treating the target RTX 3050 profile as available. These tests are pending, and advanced backend capabilities remain planned.
