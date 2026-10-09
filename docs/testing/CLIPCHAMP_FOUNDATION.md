# Native rails, catalogue, editing and UHD acceptance

Date: 2026-10-09. This is an incremental proprietary native-editor milestone, not completion of all eighteen reference flows. Executed reports, exact binary hashes, retained screenshot hashes, source hashes and actual probes are in [clipchamp-foundation-evidence.json](clipchamp-foundation-evidence.json).

## Source and dependency gates

`./scripts/check.ps1 -Media -Deny` passed on the final source: formatting, locked all-target Clippy with warnings denied, 68 default tests plus 19 additional explicit media tests = 87 distinct Rust tests. Cargo-deny advisory, bans, licence and source checks passed with the documented paste/font/Boost exceptions and platform duplicate warnings. Final log: `.lumeryn-scratch/clipchamp-verified-gate.log`; build: `clipchamp-verified-build.log`. No publishing, public remote, paid download, model runtime or code upload occurred.

Five actual Store tests exercise the new split/duplicate/delete/reorder/canvas operations, source cuts, rollback, dry-run, expected revision, idempotency, reopen/history and overflow. CLI schema tests cover all eight edit variants and shared RenderOptions; catalogue pagination enumerates all 352 unique asset IDs with bounded pages. Existing project/preview/service/media regressions passed.

## Native UI

Actual same-app baseline workflows passed at 1440×900, 1280×800 and 980×640: playback, exact seek/step, Studio trim, live real CLI change, background decoded export, source/history checks and native screenshots. These workflows used the integrated editing/export implementation before the final small gallery row/icon/copy polish; final nine-state captures use the final rebuilt binary.

Nine readonly captures cover active, compact Clip, export settings, advanced tools, empty, image library, audio library, both drawers collapsed and fullscreen. The lead inspected all nine final native PNGs, retained under `docs/design/native-rails-v1`. Revision 3 and all eight imported sources stayed unchanged. Ordinary captures matched requested bounds at 100% DPI. Actual fullscreen reported the OS flag true and 1920×1080 display dimensions; its requested 1440×900 window bounds intentionally do not match OS fullscreen. Enter/exit keyboard and pointer interaction still need physical testing.

Native library rendering verifies SHA256 before decoding thumbnails on a worker, with four queued requests/results, at most 64 textures and 128×80 thumbnails. User search/category changes inspect bounded metadata only. Image rows preserve aspect. Standalone audio audition reuses FFmpeg/CPAL, uses a quarter-second buffer, caps previews at 30 seconds, rejects changed sources and prevents stale play requests after stop/replacement. Seventeen preview tests including real native audio/decoder acceptance passed; physical listening quality is a separate check. Source import of actual CC0 OGG and PNG through the authoritative service passed and preserved hashes. Independent audio/image timeline placement remains absent and is stated in the gallery.

## UHD files

Four explicit UHD media tests cover eight genuine landscape/portrait source exports at 24/25/30/60 FPS, H.264/H.265 CPU and actual NVIDIA automatic selection, simulated initialization/runtime GPU failures with software fallback, full decoding, codec/dimensions/rate/frame counts, immutable sources, create-new publication, audio markers and visual detail. The full-resolution UHD-source/1080-canvas regression matched all 8,294,400 decoded grayscale pixels with the native-UHD-canvas reference; an intentionally reduced intermediate changed 584,683 pixels. Audio marker centre was approximately 0.055003 seconds for an expected 0.055; AAC padding tolerance is explicit. These short generated fixtures do not establish long-program or physical-device drift.

The real native Studio also exported and independently probed 3840×2160 and 2160×3840 H.264/AAC files at 60 FPS: each contains 120 frames, two seconds, stereo 48kHz audio and verified full decoding. NVIDIA h264_nvenc initialized in both. Native workflow wall times were about 6.01/5.53 seconds including playback/edits/capture; UI CPU p50/p95 were 1.50/4.28 and 1.49/4.40ms. These are debug CPU timings, not display FPS. Those native fixtures use upscaled synthetic 720p sources; the genuine source-detail acceptance is the separate test above. Reports: `.lumeryn-scratch/studio-064cecd9afbe41ff8a6417884d88e4fb` and `studio-61f2f49b6c3b4f4eb793dbd5c0f943a1`.

Reproduce native UHD using `./scripts/native-smoke.ps1 -Uhd` and `-Uhd -Portrait`; they generate fresh disposable projects. Actual render stages, revision-pinned jobs, cancellation/no-output, event ordering and edit responsiveness passed the service media test. A reviewed late-cancel race was fixed so successful publication remains job_completed. Percent progress is not fabricated; current progress reports real processing stages.

## Remaining acceptance and features

Physical pointer/dialog focus/cancellation, keyboard fullscreen return, 150%/200% DPI, speaker/display drift, device loss, Intel/AMD hardware, sustained long UHD exports and final installed-pack copying remain unexecuted. QSV/AMF paths are implemented but not validated on those physical devices. H.264/H.265 depend on the installed FFmpeg build; codec patents/exact-build redistribution remain under review.

The supported export graph is still contiguous single-track CFR SDR video plus embedded audio/silence. Full editable text/vector/scene/mask/compositing/effect export parity must be tested as those families are implemented. Undo/redo, tracks/mixing/fades, image holds/transforms, typography/scenes/templates, filters/transitions/speed/freeze, captions, recording/TTS and optional smart jobs remain in TASKS. Audio-only/480p and 8K/HDR/mastering are not claimed complete. The library has 352 audited CC0/OFL files, not a full professional photographic-stock or narrated-music catalogue.
