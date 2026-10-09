# Approved B native Studio acceptance

The owner selected B / Studio. The native layout contract is in `docs/design/STUDIO_REDESIGN.md`; this document records how to verify the implementation. Workflow success, captured pixels and human visual acceptance are separate results. The concept image is a design reference, not a screenshot of implemented features.

Final source and native captures passed on 2026-10-09 after the ruler-label, medium Inspector and empty-state corrections. Machine evidence is retained in [studio-redesign-evidence.json](studio-redesign-evidence.json); five visually inspected native state PNGs and hashes are in [IMPLEMENTED_ASSETS.json](../design/studio-redesign-v1/IMPLEMENTED_ASSETS.json). Physical dialog/device/high-DPI checks remain pending.

## Execution status

| Check | Current evidence | Status |
|---|---|---|
| Four PowerShell scripts parse | PowerShell AST parser returned zero errors for demo, native smoke, redesign smoke and state capture | PASSED 2026-10-09 |
| Skip-build guard | Every Cargo invocation is inside `if (-not $SkipBuild)`; executing all four actual guard blocks with mocked Cargo produced0calls when skipped and1call when building; parent smoke scripts call demo with `-SkipBuild` | PASSED independent guard execution |
| Artifact overwrite protection | Independent disposable probe attempted a second JSON write; create-new rejected it and SHA-256 of original bytes stayed unchanged | PASSED 2026-10-09 |
| Build lock behavior | Independent disposable file held open exclusively; bounded wait rejected it after approximately10seconds (10.03s measured), then accepted it after handle release | PASSED 2026-10-09; no user processes killed |
| Redesigned native workflow at three sizes | `redesign-3ff3b9ebca114e3b83dd45d378a31f99` reports/screenshots at1440×900,1280×800,980×640; exact bounds/DPI, all9workflow checks and source checks; asset readiness complete | PASSED final binary 2026-10-09 |
| Default native workflow and process sampling | `studio-7cc4e0359a4c4bf0b9b38ac61e327856`; actual1600×1000 window, exit0, complete source/asset checks and responsive export | PASSED final binary 2026-10-09 |
| Active, compact Inspector, Export, tools and empty native scenes | `b-states-ad2833e1b4c545218659f2a46464816b`; five states, revision2 unchanged, verified8sources, active states8thumbnails/8waveforms/0failures | PASSED |
| Visual comparison with approved B | Root inspected all5state PNGs and all3workflow PNGs against B; controls readable/reachable, clocks/icons/audio lane reviewed | PASSED at recorded100% DPI |
| Dialog cancellation, focus, pointer/keyboard usability and DPI | Physical manual procedure below | PENDING |
| Full workspace/media/supply-chain gate | Final `redesign-final-gate.log`: formatting, strict all-target Clippy,49default+13additional explicit media tests, cargo-deny advisories/bans/licenses/sources; `redesign-final-build.log` binaries matching recorded hashes | PASSED final source 2026-10-09;62distinct tests |

Script probe artifacts are ignored under `.lumeryn-scratch/script-review-ab320d41c9734d2da8fd3e4804c2e730`. Parsing, build-guard review and helper probes do not establish native rendering or editing acceptance. The independent actual native runs below executed sequentially with `-SkipBuild` after the lead's final source gate/build; no rebuild or existing user-process termination occurred.

## Executed final native evidence — 2026-10-09

Three-size summary: `.lumeryn-scratch/redesign-3ff3b9ebca114e3b83dd45d378a31f99/redesign-smoke.json`. All9checks passed in each fresh disposable fixture: load, GPU frame, advancing playback, exact seek, frame step, Studio trim, real CLI/live event, decoded background export and native screenshot. Revision3-to4-to5 and source/SQLite/history checks passed. Actual logical/physical dimensions matched requests at1.0pixels/point; each reported1thumbnail/1waveform/0failed assets and no asset timeout. Root inspected all three native PNGs.

| Case | Actual viewport | First frame ms | UI CPU p50/p95 ms | UI updates during export |
|---|---|---:|---:|---:|
| large | 1440x900 | 718.37 | 1.306/4.446 | 24 |
| medium | 1280x800 | 712.23 | 1.159/4.178 | 24 |
| compact | 980x640 | 713.48 | 0.970/4.009 | 23 |

Default1600x1000 native run: `.lumeryn-scratch/studio-7cc4e0359a4c4bf0b9b38ac61e327856`, exit0, all9checks, revision5/source integrity, 797.73ms first frame, UI CPU p50/p95 1.230/4.120ms, 24UIupdates during verified export. Audio48kHz/stereo device readiness does not prove physical audibility or A/V drift. Process metrics are retained there; whole-adapter counters do not identify Studio-exclusive VRAM.

Readonly five-state run: `.lumeryn-scratch/b-states-ad2833e1b4c545218659f2a46464816b/states.json`. Active/export1440x900, compact Inspector980x640, tools/empty1280x800; actual bounds matched at1.0pixels/point. Revision2 stayed unchanged before/after every capture;8source assets verified before/after. All4project states settled8actual thumbnails/8waveforms with0errors; empty state has0assets. Root inspected all5native PNGs: proportional plain zeros, recognizable icon-only transport, real monitor volume, subdued waveform, aspect-correct previews, visible trim/export actions, unclipped ruler endpoint and no fake empty progress. Intentional compact drawer overlay and scrollable media/tools remain. Source trim fields retain source-time semantics, not the concept's illustrative timeline timestamps.

Binaries match between final state/workflow runs: CLI `CDD0A910CA4BD0DE6CF2F92B883D0FA6291611957491371208102360B3E67E49`; Studio `77CD80C38CAE95B94A583ADE73EAFBA5E1AA9DA36053A83A2FD3B6750BF61AF4`. Only locally generated demo footage/synthetic audio was used. The graph is editable as imported clips; the neural artwork itself is raster footage, not the staged native semantic scene engine. Captures suppress tooltips only in verification mode; ordinary hover explanations remain. Screenshot exports are renderer-native, not OS chrome or fabricated mockups.

Earlier independent runtime `.lumeryn-scratch/redesign-bb9c47bdd66b4183ab7ab256c7ea404a` and default `studio-9621adba3cdd4115b0185f8bcdc6eef3` remain historical. That debug default sample observed267.52MiB peak working set and whole-adapter1297–1601MiB/6–16%GPU, with possible concurrent captures. These short synthetic720p measurements do not establish sustained FPS, exclusive VRAM, professional1080p/4K budgets, GPU completion latency or physical A/V sync.

## Reproducible workflow and capture

Run the full source gate and build current CLI/Studio binaries before using `-SkipBuild`. That flag means use existing binaries; it does not prove they correspond to current source or passed tests. Harness summaries record binary SHA-256 and whether building was skipped. Windows services or Studio instances can hold an executable open. Before builds the scripts wait approximately ten seconds for those locks to release, then fail with an explanation. They never terminate existing user sessions to rebuild.

```powershell
./scripts/check.ps1 -Media -Deny
./scripts/native-redesign-smoke.ps1
./scripts/native-smoke.ps1 -SkipBuild
./scripts/capture-studio-states.ps1 -Project 'D:\path\project.lumeryn' -SkipBuild -CaptureFrame 720
```

`native-redesign-smoke.ps1` builds once, then calls the demo generator with `-Motion -SkipBuild`. The demo creates trusted synthetic720p motion and sine audio, imports and edits through the CLI, renders, reopens and checks source hashes. Each redesign case uses `project backup` to create a consistent fresh disposable database; it never copies a live WAL file alone. The cases request1440×900,1280×800 and980×640 logical pixels. Each actual visible native window must pass load, GPU frame, playback, exact pause/seek, frame step, Studio trim, live external CLI revision update, background export with at least one UI update, screenshot saving and source-integrity checks. The script records actual viewport/DPI, physical PNG dimensions, SHA-256, asset counts/readiness and any asset-wait expiration. Window-manager constraints are recorded; requested bounds are not silently treated as actual bounds.

`native-smoke.ps1` exercises the same actual workflow at the application's default size and samples Studio CPU/RAM plus optional whole-adapter NVIDIA counters. Those counters do not measure exclusive Studio VRAM. Its freshly generated fixture is disposable. It builds the workspace once unless `-SkipBuild` is supplied; the nested demo never builds again.

`capture-studio-states.ps1` opens five actual capture-only native windows for active, compact Inspector, Export settings, project tools and empty welcome scenes. It calls no edit or render command. The capture module can seek the preview without changing the graph. The script inspects the selected project's revision before and after every scene, rejects any revision change, verifies source assets before and after the sequence, and records the exact inspected revision against captured scene metadata. An ordinary concurrent edit causes rejection and requires a fresh stable run. These checks do not defeat hostile change-and-restore races.

Every run receives a new ignored GUID directory. JSON summaries use `FileMode.CreateNew`; native PNG/report writes use the backend's create-new paths. Screenshots must exist, have a valid PNG signature, positive dimensions matching capture metadata and a recorded hash. Native captures time out; cleanup terminates only the disposable window launched by that script and preserves the shared project service. Existing artifacts are never accepted as a new pass.

## Visual and manual acceptance

Inspect each actual PNG against approved B. Record the artifact path, viewport, DPI, screenshot hash and reviewer outcome. The scripts deliberately leave `visual_review` as `pending`; a report's `passed` flag proves its stated workflow/capture checks rather than design fidelity.

- Large/medium: charcoal hierarchy, mint primary actions, readable media labels, native aspect-correct preview, context-sensitive Inspector and proportional timeline. Confirm the real source thumbnail and waveform evidence rather than assuming ready assets from a workflow pass.
- Compact: preview and transport remain usable; Inspector opens as a reachable drawer; primary buttons and timeline labels do not clip; horizontal/vertical scrolling is intentional.
- Related scenes: welcome actions are reachable; Export settings have clear valid options and a new-path flow; project tools keep technical details away from the ordinary edit flow. No unsupported feature appears as an enabled promise.
- Native dialogs: cancel Open, Import, New Project and Export selection; verify no project revision/file change. Reopen dialogs, switch focus and use keyboard navigation. Existing project/export destinations must remain protected by engine no-overwrite errors.
- Multiple imports and project switching: drop multiple media files, mixed project/media files, and complete an import picker while another edit is active. Verify explicit handling of every input and no silent loss. Start project creation, then change project selection; stale completion must not replace the chosen project.
- Display/manual checks: repeat at100%,150% and200% scaling, resize across thresholds, use keyboard focus and inspect contrast/label readability. Device loss, long physical A/V drift and professional1080p/4K budgets retain the limitations in `NATIVE_ACCEPTANCE.md`.

Native media caches are bounded; a failed asset or expired eight-second wait is reported separately from screenshot success. Source-derived caches do not imply proxies, effects, professional mixing or multi-track composition. Do not mark visual thumbnail/waveform acceptance when the capture reports absent or failed assets.
