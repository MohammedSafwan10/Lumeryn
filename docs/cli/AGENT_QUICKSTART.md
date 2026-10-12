# External-agent test handoff

Verified locally 2026-10-10. Installed CLI: `C:/Users/Thumbeja/.cargo/bin/lumeryn.exe`. Native Studio and CLI share the authoritative project service; live edits appear in the open editor. No embedded agent runtime is required.

The owner's clean test project is `D:/Dev/freela/Lumeryn/projects/agent-test/project.lumeryn`: schema2, revision0,1920×1080,30FPS, zero media/video/audio placements. Studio does not insert demo content. This is an ignored local database, not a committed fixture. Inspect the current revision before every write; it will change when testing starts.

```powershell
lumeryn capabilities
lumeryn schema
lumeryn schema --kind render
lumeryn project inspect --project D:/Dev/freela/Lumeryn/projects/agent-test/project.lumeryn
lumeryn assets list --help
lumeryn assets search --help
lumeryn media probe --help
lumeryn timeline clip --help
lumeryn timeline audio --help
lumeryn timeline visual --help
lumeryn studio open --help
lumeryn project batch --help
lumeryn render --help
```

Use real inspected media, matching the project CFR for the current video renderer. Select the project explicitly, pass `--expected-revision`, use stable IDs and optional idempotency keys, and dry-run atomic batches before applying them. Do not modify source files. Imports and audio edits require trusted configured FFmpeg/ffprobe; `lumeryn doctor` diagnoses those tools. Export to a new path, then require reported full-decode verification and check the actual file. `project open` validates headlessly; it does not launch Studio.

Current tested operations include video placement/insertion/reorder/trim/split/duplicate/delete, undo/redo/clipboard, independent audio placement/move/trim/range/gain/mute/delete, media aliases, seven canvas ratios, live synchronization and supported-graph UHD export. The offline library has352 reviewed files and verified hashes. Schema3 adds verified PNG holds, contain rectangles, bounded ordered layers and gaps; three total visual placements and two extra tracks are the current composition limits. Audio1 allows overlap mixing within visual duration; embedded source audio remains linked/unity. Detach, audio-only export, fades/transitions/speed/filters, native editable text/vector/scenes/masks/broader compositing, recording and optionalAI are not complete. Discover status rather than treating a planned capability as implemented.

Detailed commands/errors: [WORKFLOWS](WORKFLOWS.md). Executed current evidence and remaining physical/device checks: [AUDIO_COMPOSITION](../testing/AUDIO_COMPOSITION.md). Historical mockups and generated acceptance projects are reference/testing material outside the normal product flow.

GUI launch: `lumeryn studio open --project <path> --executable <trusted GUI path>`. A successful `project open` is headless. Require the launch command's `native_window_observed` state, then distinguish OS visibility from physical/user-project-render confirmation. Early exit/timeout must never be described as an opened editor. See STUDIO_LAUNCH and ADR0024.
