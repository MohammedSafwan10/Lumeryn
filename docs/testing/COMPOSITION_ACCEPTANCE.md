# Bounded visual composition acceptance

Date: 2026-10-12. Windows x64, pinned Rust1.97.1, configured external FFmpeg/ffprobe9.0.2. Scope is V0/V1 layers/PNG/gaps/contain rectangles, not full professional-editor parity. Windows launch and media-worker fixes are included in the final full gate; detailed launch evidence is in STUDIO_LAUNCH.

## Executed engine/client gates

`./scripts/check.ps1 -Media -Deny` passed locked formatting, all-target Clippy with warnings denied, default and actual media tests plus dependency advisories/bans/licences/sources.225distinct test names passed (171default+54additional), with repeated executions deduplicated. The earlier composition-only gate passed216; seven launch/worker and two responsive-media regressions were subsequently added. No new third-party library or codec distribution was introduced by composition. The separate later launch helper reuses existing windows-sys. Source log: `.lumeryn-scratch/composition-full-gate11.log`.

- Strict30-edit schema and CLI arguments; atomic range/rectangle batches, dry-run/replay/revision conflicts/undo and media-reference protection.
- Explicit schema1/2 to3 all-history migration preserving IDs, audio identity, edit revision/history and command/receipt bytes, with failed migration rollback and old reader shape fixtures.
- Real decoded pixels establish ordering, half-open holds, alpha and gaps; a changing-frame source establishes exact nonzero source offsets. Public-plan overflow/mutated order and tiny-canvas huge-intermediate guards are tested.
- Eight actual final-resolution exports:3840×2160 and2160×3840 at24/25/30/60FPS, verified decoded output/duration/pixels/new-path publication and unchanged sources. A7680×4320 project-space default hold actually exports UHD within intermediate bounds.
- Native preview forward/backward seeks match the same CPU reference byte-for-byte, including still-only leading gap. Composition previews clamp larger configured bounds to3840 while keeping aspect ratio.
- Actual CLI export has50decoded frames at30FPS,128×72,1.666667seconds and stereo48kAAC. Decoded interior windows contain440Hz only, both440/880,880 only, then silence; visually occluding PNG does not mute underlying video audio.
- Eight actual egui event-driven tests cover primary/overlay/audio edges, captured grab offset, valid/stale drops, PNG-to-primary/audio rejection, Escape/focus/project/epoch/readonly cancellation and mixed-button orphan cleanup. The existing video/audio gesture suite stays green.

An independent reviewer reran25focused cases from compiled binaries; all passed. Findings fixed before acceptance include missing audio paths, incorrect overlay keyboard dispatch, oversized projected surfaces, public-plan arithmetic, grabbed-offset jumps, stale project identity, orphan trims and insufficient export assertions. [Review](COMPOSITION_REVIEW.md).

## Real native window and inspected pixels

Final composition-native window workflow passed21checks, including eight shared-service layer/image edit/history steps, native revision refresh and background export. Final revision19 file fully decoded3840×2160,H264NVENC,60FPS,340frames,5.666667seconds and stereo48kAAC; independent ffprobe matched. Integrity/history/three source assets verified afterward. The generated720p source was upscaled; this fixture proves final resolution/composition, not native UHD detail. Exact reports/hashes are summarized in [evidence](composition-evidence.json).

Final artifact directory: `.lumeryn-scratch/studio-02622dd60b2a435bbe5d4259dd96c270/`. Actual screenshot inspected: five timeline rows, real peaks/thumbnails, clear ruler and shared preview. Initial screenshot caught cramped rows and repeated subsecond labels; corrected row-aware height/reused rational ruler labels, then reran strict Clippy,40Studio checks and the entire native workflow. Final debug CPU update p50/p95=4.3394/8.1789ms; sampled Studio peakRAM292134912bytes. These exclude FFmpeg worker peaks and do not measure display/input latency or app-specific VRAM.

Readonly1440×900 video-overlay and980×640 image-hold captures used the same committed disposable revision19 and actual decoder. Video PIP occupies the expected upper-right contain rectangle; translucent PNG reveals the lower video. Time values are readable and exact frames collapse; compact mode keeps one context drawer and scrolling for extra rows. A subsequent compact Inspector refinement hides inactive rectangle fields rather than using screen space for disabled controls. Final980×640 capture at revision19/frame30 passed and was visually inspected at `.lumeryn-scratch/composition-final-capture-60c37117417b47c0b9250dae1112c1d7/`: Apply changes and delete remain visible, PNG alpha and lower video are correct. Launch evidence is in STUDIO_LAUNCH. No mock contents are seeded in normal Studio and no owner project is migrated/reset.

## Product boundaries and remaining checks

Composition currently admits **three total visual placements**, including inactive primary clips, at most two added ordered tracks, two simultaneous videos and one simultaneous PNG. The contiguous primary-only fast path retains64clips. Source/intermediate surfaces are bounded to3840 per dimension; project-space canvas can be8192 only when final scaling remains bounded. Normal encoded-SDR straight-alpha source-over is explicit; linear-light/HDR, managed colour, additional blend modes and broader formats remain staged.

Images are verified PNG holds with an explicit duration, not fake source FPS. Embedded overlay video audio remains linked/unity and audible beneath covering visuals; independent gain/mute mixing remains available. Per-overlay detach/mute/fades, additional audio tracks and professional metering are later typed semantics. Same-track visual overlap is rejected; uncovered time/space is black. A shortened visual extent cannot strand independent audio.

Crop/cover/rotate/flip/pivot/direct handles/full opacity, scene text/vectors/keyframes/templates, colour/effects/fades, transitions/speed/freeze remain V2–V6. Origin-based seek cost, larger decoder scheduling, persistent caches/proxies/GPU interop remain measured expansion work. Native drag markers/local edge geometry are uncommitted presentation; a live composed draft preview is not claimed. Physical pointer/dialog/DPI/IME/device/speaker-display synchronization, long media drift, sustained UHD and every GPU/encoder remain unexecuted. Passing these bounded checks is not a zero-bugs or editor-parity guarantee.

Source hashes protect ordinary source replacement; hostile change-and-restore races and indirect playlist child identities are not certified. Sources stay filesystem references rather than blobs; do not copy a live WAL database alone.
