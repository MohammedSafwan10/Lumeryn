# ADR 0003: Shared Rust service, Studio shell and measured preview transport

Date: 2026-10-09

Status: Accepted architecture direction; Studio, service and preview implementation deferred. Preview transport selection remains provisional until prototypes pass.

## Context

The authoritative specification requires headless CLI editing, Tauri 2/Svelte 5 Studio, one project state, concurrent human/agent changes, rational timing and responsive playback. A browser-owned editing engine or UI automation cannot satisfy this contract. Windows preview integration is a technical risk that must be measured early.

## Decision

Use typed Rust commands for both clients. Establish one project writer, authenticated local IPC and revision-aware subscriptions before enabling live multi-client writes. Prefer named pipes on Windows and Unix-domain sockets elsewhere behind a shared transport contract. Use an explicit current-user security descriptor, remote-client rejection and protected service credentials. Default pipe ACLs are insufficient because Microsoft documents broad read access. [Pipe permissions](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights), [pipe creation flags](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea).

Proposed command envelope: protocol version, request ID, project ID, optional expected revision, optional idempotency key and typed command. Reject unsupported protocol versions, oversized messages and stale revisions explicitly. Mutations, resulting revisions, idempotency outcomes and durable events commit together. Snapshot subscriptions establish a consistent cursor, replay on reconnect, and require a snapshot when retained history is unavailable. An acknowledgement means the edit committed; job admission does not mean job output completed.

Studio uses Tauri 2, Svelte 5 and TypeScript for display, selection and interaction; its Rust bridge forwards the shared command contract. Restrict capabilities to required windows and commands, enforce validation inside Rust, load local assets, and apply a CSP. Registered custom commands must be scoped explicitly rather than assuming default protection. [Tauri capabilities](https://v2.tauri.app/security/capabilities/), [runtime authority](https://v2.tauri.app/security/runtime-authority/).

Prototype a native wgpu preview window first, then evaluate child viewport integration and a binary proxy-frame fallback. Rust owns evaluation/effects, cache identity and playback/audio clocks. A separate preview window is a fallback if embedded native presentation cannot meet layout/security/performance needs. Do not claim that an HWND/WebView or shared-texture bridge works before building it. wgpu's Windows D3D12/Vulkan support establishes backend availability, not Tauri interoperability. [wgpu](https://wgpu.rs/), [Tauri architecture](https://v2.tauri.app/concept/architecture/).

Project events carry small typed changes. Frame buffers travel over a separate bounded binary/native path; never use JSON/base64 event broadcasts as the 1080p playback path. Tauri recommends channels for streaming and notes events lack strong typing and granular capabilities. Transport policy must preserve edit ordering while allowing obsolete preview frames to drop. [Frontend delivery](https://v2.tauri.app/develop/calling-frontend/).

## Alternatives

| Alternative | Why not primary |
| --- | --- |
| Independent CLI and JavaScript project writers | Divergent semantics and lost edits; violates shared core requirement |
| Continuous full-project polling | No authenticated command authority or durable missed-event recovery; inefficient |
| Loopback HTTP/WebSocket | Feasible with authentication, origin enforcement and port hardening, but exposes additional network/browser surfaces; retain only if pipes fail portability needs |
| Encoded browser video preview | Useful candidate, but added latency, seek/audio/color behavior must be tested; no substitute for shared evaluator |
| Native GUI framework replacing Tauri | Avoids WebView integration risk but changes an important product preference; reconsider only with measured failure and owner decision |

## Costs, licensing and risks

Tauri and wgpu are MIT OR Apache-2.0; Svelte is MIT. Lumeryn remains proprietary with required third-party notices. Windows needs WebView2/MSVC compatibility validation. Native preview adds window/driver/DPI work and possible unsafe FFI requiring narrow audited wrappers. IPC ACLs do not isolate compromised same-user code. Raw 1080p RGBA8 at 60 fps requires ~474.6 MiB/s before copies; bounded queues and proxy quality are essential. Exact dependencies remain unpinned until the compatibility experiment. [Tauri license](https://v2.tauri.app/concept/architecture/), [wgpu license](https://github.com/gfx-rs/wgpu), [Svelte license](https://github.com/sveltejs/svelte/blob/main/LICENSE.md).

## Validation gate

Run P-01 through P-04 in `docs/research/PLATFORM.md`: production shell, alternate preview transports, AV clocks, concurrent clients, stale edits, restart/replay, rejected unauthorized peers and backpressure. Record latency/cadence/memory on actual hardware. No such prototype is claimed complete by this ADR. Until the gate passes, CLI-only transactions may work but Studio synchronization and native GPU preview remain unavailable.
