# Native drag placement acceptance

Executed 2026-10-10 against the drag milestone following `a2cf04e`. Source and tests use the existing approved native rails/Inter/mint presentation.

## Passed

- `cargo fmt --check`, all-target locked Clippy with warnings denied, locked workspace tests, explicit media/preview/service/Studio tests and cargo-deny: `scripts/check.ps1 -Media -Deny`. Log `.lumeryn-scratch/drag-gate.log`; final serial run passed. Earlier runs correctly failed an outdated 13-operation schema assertion and a test's incorrect edge-scroll expectation; both were corrected before acceptance.
- Seven core insertion tests cover source preservation, start/middle/end/empty placement, ordering, unsupported gaps/overlaps, ranges/IDs/CFR/overflow, dry-run, conflicts, atomic rollback, idempotency and undo/redo/integrity. CLI advertises `timeline.clip.insert` and strict schema has14 operations.
- Multi-pass real egui `RawInput` tests exercise media insertion, clip-body reorder, trim/body separation, empty drop, Escape, outside/secondary release, primary-only initiation, focus-loss/resume, stale revision/epoch, same-slot no-op, audio-only rejection, linked audio without waveform, and accepted-only edge scrolling. At zoom20, a pointer6px from the actual viewport edge scrolls250px in30 simulated60Hz frames; a1200px scrolled drop resolves the expected second-clip boundary. Synthetic input tests establish handler behavior, not physical-device usability.
- Core canonical audio foundation: three unit tests and two actual FFmpeg decoded PCM tests verify absolute rational sample boundaries, overlap/offsets, gain/mute, deterministic clipping, no normalization pumping, silence padding,44.1→48k conversion, byte-exact seek parity and unchanged source identity. Independent persisted audio/preview/export remains staged.
- Fresh native window smoke with history and UHD passed25 checks, including four new shared drop transitions: media insertion/reorder/undo/undo at revisions18..21, actual preview refreshed at every revision. Report: `.lumeryn-scratch/studio-e77d83bd30864dd68681bae3884ea38f/studio-smoke.json`. These native actions use the same drop submission path; physical pointer gestures are separate from this command-path test.
- Final output actually probed and fully decoded:3840×2160 H.264 NVENC,60/1 FPS,120 frames,2 seconds, AAC stereo48k; project/assets/history integrity passed at revision21. This720p fixture is upscaled; source-detail and marker-sync coverage remains in existing UHD tests. Native screenshot visually inspected.448 UI updates; CPU update p50/p95=1.6919/4.3062ms debug, not display FPS or end-to-end latency.

## Isolation and limits

The owner's open editor/source project was neither killed nor reset. `CARGO_TARGET_DIR=.lumeryn-scratch/drag-target` isolates binaries, service and newly generated fixtures; acceptance scripts now honor that variable. Original source media is unchanged. No new dependencies/assets, uploads or public licensing were introduced.

Video/linked-source-audio drags rearrange their shared video clip; media drops insert at a snapped boundary and ripple a contiguous sequence. Drop feedback is an insertion marker plus inert label. Edge-scroll does not run over the fixed header/ruler or rejected payloads. Escape/focus loss/overlays cancel; service revision conflicts remain explicit. Free visual gaps/overlapping layers, music placement, audio detach/gain/fades, touch/high-DPI/physical dragging and subjective interaction polish are not accepted as complete.

Next coherent slice: explicit graph/history migration, verified streaming canonical source counts, independent audio placement/move/trim/gain/mute and one shared preview/export audio plan; see research/DRAG_AUDIO_INTERACTIONS.
