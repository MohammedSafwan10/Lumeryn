# MLT reuse and performance research

Research date: 2026-10-09. MLT is a research/reference candidate and optional future backend; it is not installed, linked or bundled by this milestone. Rust, egui/eframe/wgpu, FFmpeg, SQLite and the authoritative project service remain Lumeryn's selected architecture. Original code stays proprietary. Decision: [ADR 0012](../adr/0012-optional-media-backend-performance.md).

## Current upstream and useful components

Official upstream lists **MLT 7.42.0**, released **2026-10-03**. The development branch reports 7.43.0, so any later experiment must use a pinned release/tag and archived build manifest. The new release includes a libplacebo GPU module and changed default audio sample format; those are upstream capabilities, not tested Lumeryn capabilities. Recent fixes include Windows playback and memory/security issues, reinforcing the need to pin/review actual builds. [Official release history](https://www.mltframework.org/), [7.42.0 build source](https://raw.githubusercontent.com/mltframework/mlt/v7.42.0/CMakeLists.txt).

MLT's producers provide frames, consumers pull them, filters process them, playlists sequence clips, and multitracks/tractors combine tracks with transitions. Image/audio extraction is lazy and consumer implementations control execution threads. This is useful reference material for composition evaluation, scheduling boundaries and avoiding decode work in widget paint code. [Framework design](https://www.mltframework.org/docs/framework/).

An optional worker could translate a validated committed Lumeryn render plan into an allowlisted MLT composition, render independently and return validated output. MLT XML models playlists, multitracks and filters but its parser is non-validating; never treat arbitrary imported MLT XML/plugin properties as a safe command contract. The Rust graph remains authoritative and serializes only supported operations. No separate project semantics or second writer is introduced. [Official XML format](https://mltframework.org/docs/mltxml/).

Potential reuse candidates include multi-track mixing, transitions, resampling, effects and frame extraction. Their availability depends on the chosen plugin build and licenses. MLT's OpenGL facilities do not establish compatibility with eframe's wgpu device/queue, D3D texture sharing or zero-copy decoder output. Device lifetime, fences, pixel/color formats and any extra copies need an actual Windows proof. [GPU/OpenGL documentation](https://www.mltframework.org/docs/opengl/).

## Windows build and adapter options

The pinned CMake configuration uses C11/C++20, supports Windows deployment and contains MSVC paths. MSVC builds require PThreads4W/Dirent; Windows also requires iconv and dynamic loading support. Modules bring their own FFmpeg/SDL/other dependencies and install data/plugin files beside libraries. A Rust MSVC application must verify ABI/import libraries, DLL search locations, redistributable runtime and plugin discovery rather than reuse an arbitrary MinGW binary. [Pinned build configuration](https://raw.githubusercontent.com/mltframework/mlt/v7.42.0/CMakeLists.txt), [installation layout](https://www.mltframework.org/docs/install/).

| Approach | Value and required proof |
| --- | --- |
| Architecture reference only | Learn composition/scheduling techniques; implement original Rust logic and attribute actual source reuse. Adds no runtime dependency. |
| Optional headless subprocess | Isolates C/C++ crashes and different build toolchains. Requires a trusted executable, no shell, bounded inputs/outputs, cancellation and verified preview/export agreement. IPC and raw-frame copies may cost more than current backend. |
| Reviewed dynamically linked C API | May avoid subprocess copies but adds audited FFI ownership/threading/ABI and LGPL replacement obligations. Keep unsafe only in a small reviewed binding boundary. |

The first option is accepted now. A backend experiment should begin in a separate developer tool/worker with synthetic assets, not modify default dependencies or existing persisted project formats. No MLT Windows binary, C ABI binding, render result or performance benchmark has been executed in this repository.

## Commercial licensing and build review

MLT's framework/header is LGPL-2.1-or-later. Commercial proprietary use can be possible under LGPL obligations; it does not permit ignoring notices, library source/modifications, replacement/relinking or reverse-engineering rights for debugging modifications. A suitable replaceable shared-library mechanism can satisfy a section-6 route; static linking typically requires relinkable application objects/materials. Archive exact sources/build scripts, notices and applicable offer/delivery method before distribution. This research is not legal approval. [Pinned framework header](https://raw.githubusercontent.com/mltframework/mlt/v7.42.0/src/framework/mlt.h), [LGPL 2.1 terms, especially sections 4–6](https://raw.githubusercontent.com/mltframework/mlt/v7.42.0/COPYING).

The release build defaults **GPL=ON and GPL3=ON**. Start an isolated commercial compatibility experiment with both OFF and disable unneeded modules explicitly; review the actual resulting feature summary and every linked dependency/plugin, including FFmpeg codec flags. The configuration disables several GPL modules when GPL is OFF; LADSPA/LV2/VST2 selections also need attention. These switches are a starting point, not a blanket compliance certificate. Do not ship a Shotcut/Kdenlive binary as a presumed LGPL-only backend. [Pinned build options](https://raw.githubusercontent.com/mltframework/mlt/v7.42.0/CMakeLists.txt).

A separately installed optional process still needs review for interface/derivative-work and any redistribution terms. Keep GPL modules, third-party plugins and commercially restricted components out of an automatically bundled proprietary product until individually reviewed. Alternatives are the current explicit FFmpeg worker, reviewed direct FFmpeg bindings, original Rust/wgpu effects, or another measured permissive library; assess the exact license/build for each. MLT adoption must never change Lumeryn's proprietary license to make tooling pass.

## Measured admission gates

Do not rewrite the engine or add unsafe optimization from performance speculation. Identify an actual bottleneck using timers/profilers first. Compare identical synthetic fixtures on the same machine/toolchain, fixed codecs/quality, dimensions, rational timestamps and cold/warm cache states. Record at least ten runs and p50/p95 for startup, first frame, seeking, IPC edits, preview frame processing/upload, CPU, working set and available GPU counters. Report counters unavailable rather than inventing VRAM.

Verify decoded pixels/PCM, frame counts, trim boundaries, A/V alignment and cancellation alongside speed. Include 1080p single-track playback, several short clips, multiple audio/video layers once supported and responsive CLI edits during export. Specify target thresholds before the experiment; reject faster output with wrong semantics or material resource regressions. Any adoption proposal needs before/after evidence, audited enabled modules, Windows reliability, supported color/precision limits and a fallback in the existing media adapter.

Existing service evidence currently demonstrates a concurrent edit+inspect in 11.8829 ms during a real generated-media export; it does not benchmark MLT or establish a 60-FPS native UI. See [service evidence](../testing/NATIVE_SERVICE.md) for actual measurements and scope. MLT capability lists alone provide no speed, safety or compatibility guarantees.
