# Native migration acceptance — 2026-10-09

The working native baseline is verified on Windows x64. This is not full professional-editor parity or a claim that every production acceptance target has passed. Native computer interaction is unavailable to this session; the executed GUI harness runs the real window/render loop and dispatches the same command paths as controls. Manual pointer/high-DPI/device-loss and physical A/V checks remain unexecuted.

## Executed gates and artifacts

`./scripts/check.ps1 -Media -Deny` passed: formatting, locked all-target Clippy with warnings denied, 40 default tests and 12 additional explicit tests, for52 distinct tests. Repeated default tests in the explicit runs are not counted twice. Cargo-deny advisories/bans/licenses/sources passed with individually documented font/Boost/paste maintenance exceptions and platform duplicate warnings. Local evidence: `.lumeryn-scratch/full-native-gate.log`. Windows is the tested service transport; other desktop platforms are not claimed.

`./scripts/native-smoke.ps1` builds the actual native binary and generates moving720p FFV1/PCM footage. The audited final harness passed in `.lumeryn-scratch/studio-c723fa36e51445dea5b364f24e541ea4/`: native PNG, JSON report, process metrics and fully decoded MPEG4/AAC export. It opened revision3, played/paused, sought frame30, stepped31, trimmed60→59 at revision4, invoked the real CLI to restore60 at revision5, received the live event, and completed export while29 UI updates occurred. The lead inspected the actual native screenshot and six-frame moving export contact sheet. Output is60frames1280x720/30fps with audio. Source hashes/project integrity were verified after Studio closed.

A genuine pre-migration project from checkpoint0c0efed was verified and backed up through the service into `.lumeryn-scratch/legacy-native-293555ba172c470d83972f54d9849648/existing-project.lumeryn`. Native smoke against this consistent SQLite backup passed through revision5; the original remains revision3 and its media unchanged. The test did not copy a live WAL database alone.

## Mandatory fourteen-check matrix

| # | Requirement | Status and actual evidence |
|---|---|---|
|1|Rust compilation|PASS: Windows workspace build, strict Clippy, tests; compatible one-version egui/wgpu graph.|
|2|Native Studio|PASS: actual eframe/wgpu Windows window, RTX3050/Vulkan, native viewport PNG; no WebView runtime.|
|3|Existing project|PASS: genuine pre-migration checkpoint project opens through backup/native smoke with preserved metadata/source references.|
|4|Real playback|PASS for baseline: decoded moving720p frames, play/pause/exact seek/step, source/frame parity tests; no effects/proxy/multi-layer claim. Manual resize/high-DPI remains unrun.|
|5|Timeline interaction|PASS for implemented command paths: actual project clip selection/trim/ruler/zoom/scroll code, visible painting tests and Studio trim commit. Manual pointer usability is unrun; multi-track/split/undo remain staged.|
|6|CLI integration|PASS: actual sibling CLI changes project while Studio is open.|
|7|Live synchronization|PASS: native event/snapshot converges to CLI revision5 without reopening; independent IPC tests cover replay/expiry/restart/conflict.|
|8|Headless operation|PASS: original workflow/export tests and CLI job persists after the client exits, then cancels from another CLI; no window dependencies in CLI target graph.|
|9|A/V correctness|PARTIAL: exact source ranges, rational frame/sample scheduling, generated export decoded audio/frame checks and native device progression; physical speaker/display offset and sustained drift are unmeasured.|
|10|Background jobs|PASS for implemented render:29 native UI updates during export; independent actual1800-frame render permits concurrent edits/reads; cancellation tested. AI jobs are not implemented.|
|11|Dependency cleanup|PASS: inspection found no old frontend code/config; native target tree has no Tauri/Svelte/Vite/WebView, CLI tree has no egui/wgpu/CPAL/winit. No Node build step. Historical plans marked rejected.|
|12|Real export|PASS: native background export and headless generated fixtures fully decode; moving contact sheet inspected, expected60frames720p; source hashes unchanged.|
|13|Error handling|PARTIAL: malformed/missing/changed media, stale revisions, protocol bounds, restart/replay, timeout, output collisions and cancellation tested. Native GPU loss, driver failure and real different-user/remote attempts remain unrun.|
|14|Performance|MEASURED BASELINE: startup/UI work/decode/seek/process CPU/RAM and global GPU counters recorded; sustained display FPS, per-app VRAM and physical latency remain unknown. No60FPS guarantee.|

## Exact remaining Windows procedure

1. Run `./scripts/native-smoke.ps1`, check `passed:true`, exit0 and the emitted PNG/JSON/export. Do not pass a customer project directly to `--smoke`; it intentionally edits its disposable fixture. Then launch `target/debug/lumeryn-studio.exe --project <emitted project>` for manual review.
2. Play/pause with Space; step with arrows/buttons, scrub ruler/seek slider, select the clip and drag its right edge or apply Inspector trim. Read revision first; compare `lumeryn project inspect --project <path>`. Verify stale expected revisions fail, not overwrite.
3. Arrange docking panels, switch Beginner/Professional, reset, close normally and reopen. Test min/default/maximized sizes and Windows100%/150%/200% scaling; confirm aspect ratio and no clipped essential controls. These interactions have not been performed by an automated pointer driver.
4. Start `lumeryn render --project <path> --output <new file> --expected-revision <n> --job-id <UUID>`; observe events via `project watch`, edit through Studio/CLI and verify convergence. Cancel using `jobs cancel --project <path> --job-id <UUID>`. Test a sufficiently long disposable render; never overwrite output.
5. On a disposable fixture, stop only its service process and observe reconnect/latest revision; temporarily move only generated test media and confirm explicit missing-source error without data loss. Restore the fixture. Test absent/unavailable GPU on a separate test machine; do not reset or change a user's live driver.
6. Use a generated visible-flash/audio-click fixture with known timing. Measure loopback audio plus instrumented display presentation (or synchronized external recording) at start, seek and after10minutes. Define a device-specific tolerance before claiming synchronization; planned initial target is≤1project frame plus measured device/display latency, not an achieved result. Capture underruns, output device/rate/buffer and driver identity. Native clock tests alone cannot pass this check.

See NATIVE_PREVIEW, NATIVE_SERVICE, research/NATIVE_STUDIO and benchmarks/NATIVE for detailed evidence/limits. No installer, signed updater, models or hosted CI is delivered by this baseline.

## Approved B redesign follow-up — 2026-10-09

The earlier nine-panel UI and52-test counts above describe the historical baseline. Current B shell preserves those engine/service workflows and adds source-derived bounded thumbnails/waveforms, native dialogs and real monitor-only volume. Final locked gate passed49default+13additional media tests=62distinct, strict all-target Clippy/formatting and cargo-deny. Actual three-size/default native workflows and five readonly related-state captures passed; root visually inspected all8state/workflow PNGs. Retained reports, binary hashes, actual bounds and source checks are in [STUDIO_REDESIGN](STUDIO_REDESIGN.md). Source revision2 stayed unchanged during neural-demo capture. The project graph is imported raster footage; native semantic scenes remain staged. Physical pointer/dialog cancellation/highDPI and device/A/V acceptance above remain pending.
