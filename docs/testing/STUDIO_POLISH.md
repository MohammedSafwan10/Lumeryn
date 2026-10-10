# Native panel, typography and media-management acceptance

Verified 2026-10-10. The owner's approved B/mint/native rail design is retained. Export and Project were reorganized after the owner rejected their clutter, and ordinary UI now uses audited Inter Regular. This is an implemented application capture, not a generated mockup. Full18-feature or physical-input acceptance is not claimed.

Final serial commands passed:

```powershell
./scripts/check.ps1 -Media -Deny
cargo build --locked -p lumeryn-cli -p lumeryn-studio -j 4
./scripts/capture-studio-states.ps1 -Project <disposable eight-source demo> -SkipBuild
./scripts/native-smoke.ps1 -SkipBuild -History -Uhd
cargo install --path crates/lumeryn-cli --locked --force -j 4
```

Full gate:94default+25additional=119distinct Rust tests, formatting, locked all-target warning-denied Clippy and cargo-deny with existing reviewed exceptions. Additional tests include the generated CLI media workflow and the explicitly enabled bounded core media measurement. Executable feature-unification hashes differ between repeated groups; counts deduplicate test identity rather than counting the same test again. Gate/build/native logs live under ignored scratch; exact reports, hashes and retained images are in studio-polish-evidence.json. One earlier gate overlapped native captures and stopped at the executable-lock guard. No process was killed; the final gate/build/captures/native sequence ran serially and passed.

## Actual presentation

Eleven readonly native states passed capture/source/revision checks: active, compact, Project, ordinary Export, compact Export, advanced tools, empty, image library, audio library, collapsed drawers and OS fullscreen. All eleven plus the actual workflow PNG were visually inspected. The eight-source demo retained revision3 and original source hashes. Ordinary captured viewports match1440×900,1280×800 or980×640 at100% DPI; fullscreen is the actual1920×1080 desktop, not the requested ordinary bounds.

Export groups resolution on the left and frame rate/codec/quality on the right. Encoder/bitrate stay under Advanced. Filename, short destination/Browse, output summary and the mint Export button remain visible in the actual980×640 capture; only settings can scroll. Two intermediate layout attempts caught excessive height and a whole-window scroll hiding the footer. Both were corrected before final source acceptance. Normal Studio defaults to1080p/H.264/automatic hardware selection; ordinary synthetic smoke/CLI defaults remain their tested baseline policy. Native pickers, shared options, stages, cancellation and create-new verification are retained.

Project shows name/canvas/size/readable rate/duration. Paths/revisions are collapsed; manual path tools remain in the advanced workspace. Inter is registered once with existing fallbacks retained. Plain zeros, ordinary labels, clocks, media titles and small ruler/sidebar text were inspected at100% DPI. Current Latin UI readability is accepted for this slice; no universal script/RTL, glyph-atlas memory, controlled startup comparison or highDPI claim follows. research/STUDIO_TYPOGRAPHY.md and ADR0019 record official sources and licence conditions. Native scene/export typography remains a separate roadmap capability.

Retained views: [Export](../design/native-polish-v1/export.png), [compact Export](../design/native-polish-v1/export-compact.png), [Project](../design/native-polish-v1/project.png), [active](../design/native-polish-v1/active.png), [workflow](../design/native-polish-v1/workflow.png).

## Shared media management and native history

CLI list/inspect expose display name, optional alias, original path/metadata and usage count. Strict rename requires exactly one name/reset flag; typed RenameMedia requires the alias field, allowing explicit null reset. A missing alias cannot silently clear a name. RemoveMedia rejects referenced sources; explicit delete-clips/remove batches remain atomic and undoable. Source files, imported identities and catalogue notices are never renamed or removed. Cards, timeline and Inspector use one shared label helper. Native thumbnail context actions and the rename form use the revision-aware service; project changes cancel stale forms and absent media selection is cleared.

Eight default core tests plus one explicit measurement and four default/one generated CLI tests cover strict fields/schema, multilingual/byte bounds, source identity/bytes, conflicts/dry-run/replay, rollback, FTS/reset, deletion restrictions/batches, two-Store revision revalidation, reopened history, old absent-alias snapshots/index bytes, consistent/rebased backups and clipboard replay after reference removal. Old binaries reject alias-bearing graphs/new commands; this is new-reader backward compatibility, not forward compatibility. No schema/storage history rewrite or new dependency was introduced. ADR0018 records the bounds and isolated10k-object measurement; linear transaction/search rebuilding is not a constant-time claim.

The actual native smoke passed21checks: the existing load/play/seek/step/trim/live CLI workflow, eight clipboard/delete history steps and four rename/undo/redo/reset steps. GPU-presented revisions6–17 were recorded. At14 the source alias is Opening shot,15 restores no alias,16 restores Opening shot and17 resets it. Clip count/range stay1/60 during those four steps. The harness calls the same Session helpers and shared Rust commands as controls; it does not inject physical pointer or keyboard events.

Final export pinned revision17 and verified3840×2160,H.264 NVENC,60FPS,120frames,two seconds,zero-origin48kHz stereoAAC,full decode and unchanged source/history integrity. The UI updated252times during export. Debug UI CPU p50/p95=1.59/4.36ms, which is not display FPS/input latency or an isolated font comparison. The720p synthetic source is upscaled; genuineUHD detail and audio markers are separate core UHD tests.

Installed CLI list/capabilities passed after the locked local reinstall. No remote, upload, paid acquisition, public licence, model/runtime addition or customer media was used.

## Remaining checks and next work

Physical rename/removal/context menus, OS copy/paste and text-field focus, file-dialog selection/cancel, expanded Advanced scrolling, non-default DPI and speaker/display synchronization remain manual acceptance. Source missing/change failures remain explicit; undo restores references, never source bytes. Complete composition/audio/image placement, native editable text/vectors/keyframes, effects and other screenshot families remain staged. research/COMPOSITION_FOUNDATION.md identifies the next shared evaluator, absolute rational sample boundaries and explicit all-history migration needed before those capabilities. Preserve early editable explainers rather than deferring them until every traditional editor family is complete.
