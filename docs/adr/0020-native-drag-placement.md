# ADR 0020: Native drag placement through authoritative edits

Date: 2026-10-10. Status: accepted for the contiguous video slice.

## Decision

Use the existing pinned egui 0.36.2 typed drag payload plugin and custom timeline paint. Capture project UUID, Studio epoch, committed revision and a tagged media/clip UUID only on primary drag start. Media cards insert at a snapped sequence boundary; clip bodies and linked source-audio bodies reorder the same video clip. Keep right-edge trim disjoint. One primary release submits one revision-checked service edit. Escape, outside release, focus loss and overlays cancel; stale gestures are rejected rather than silently retried. Hover/edge scrolling never write the database or launch media workers.

`InsertClip` is shared core semantics, discoverable through JSON schema, CLI `timeline clip insert`, and Studio. It validates a contiguous sequence before ripple placement, retaining existing source ranges and IDs. Existing explicit AddClip positioning remains available. Insert/reorder history is one atomic undo step. Double-click/Add remains an alternative to dragging.

No graph/storage migration, new dependency, unsafe code, cloud runtime or media mutation is necessary for this slice. Old binaries cannot parse the new edit history command; forward compatibility is not promised. Commercial distribution uses the already audited MIT/Apache egui dependency and unchanged proprietary workspace status.

## Alternatives and limits

Browser drag libraries/Tauri conflict with the selected native architecture. Windows OLE is unnecessary for internal dragging; existing filesystem drops still import sources. Free visual positioning, gaps/overlays and independent audio require shared evaluator changes rather than new painted lanes. Existing gapped/overlapping sequences fail explicitly instead of being silently packed.

The core canonical audio-plan foundation generates bounded stereo48k mixing, exact absolute frame/sample boundaries, gain/mute, overlap summation without auto-normalization, deterministic clipping and seek slicing. Its decoded fixture tests do not mean persisted independent audio, Studio music placement, detach/fades or preview/export integration are complete. Those require graph/history migration and verified source sample evidence first.

## Evidence

Official interaction/native API research: [NATIVE_DRAG_DROP](../research/NATIVE_DRAG_DROP.md). Audio integration contract: [DRAG_AUDIO_INTERACTIONS](../research/DRAG_AUDIO_INTERACTIONS.md). Executed gesture, service, native-window, output and performance evidence belongs in [NATIVE_DRAG_DROP acceptance](../testing/NATIVE_DRAG_DROP.md). Physical pointer/touch/high-DPI usability remains separate from scripted input tests.
