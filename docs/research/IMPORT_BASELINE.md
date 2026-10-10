# Import stages and measured baseline

Measured on 2026-10-10 on the local Windows development machine, using generated footage only. No owner's recordings or project databases were read or altered by this benchmark. FFmpeg/ffprobe: `8.1.1-essentials_build-www.gyan.dev`; developer-installed tools, not bundled software. The shared Rust service remains authoritative.

## Finding and small change

The previous candidate-CFR video probe launched a complete `-count_frames` decode, then another complete `-show_frames` decode to validate every timestamp. Both enumerated the same first video stream. The updated candidate-CFR path counts the frames in its existing bounded timestamp output and applies exactly the same integer-PTS, monotonicity and container-tick CFR validation. Non-candidate rates retain the count-only path and `constant_rate=false`; unknown/invalid rates are not made editable. Corrupt/missing timestamp text fails explicitly. The existing 16 MiB worker-output, 1 MiB stderr, timeout, dimensions/stream limits and kill-on-drop safeguards remain.

[Official FFprobe documentation](https://ffmpeg.org/ffprobe.html#Main-options) distinguishes header/stream output, frame output and decoded frame counting. It supports the choice of separate advisory header inspection and complete validation; it does not establish Lumeryn's measured performance or safety on its own.

The added `QuickMetadata` cannot carry a verified frame count or CFR claim. Headers are delivered first for progress presentation. On fresh validation they are read again **inside** the pre/post content-hash window: quick headers from before the first digest must never become authoritative if a source was replaced. `VerifiedMetadata` is a non-deserializable proof with a private canonical source-path binding. Only a completed validator creates it. `MediaTools::probe` retains its original verified `Metadata` interface; staged service callers use progress callbacks and the proof wrapper.

The service's bounded in-memory proof cache can avoid repeated full decoding for a matching BLAKE3 digest and size under the same configured tool/policy identity. Every hit still reads and hashes the entire current source before and after reuse, and rebinds its proof to the current canonical path. It never trusts mtime or size alone. Changed-and-restored hostile file races remain outside the existing pre/post-hash guarantee. No dependency, persistent global cache, new backend or unsafe code was added.

## Executed generated-fixture timings

Fixture: 8 seconds, 1920×1080, 60/1 FPS, 480 frames, MPEG-4 in MKV, 25,058,730 bytes. Generated with `testsrc2` and two decoder/encoder threads. BLAKE3: `fcbd3f6ecfde3a0ffb4869d57a66d7750e4a975b3f1ede43cd3f107555f1a0c8`.

| Measurement | Run 1 | Run 2 | Run 3 | Scope |
| --- | ---: | ---: | ---: | --- |
| Previous CLI `media probe` wall time | 719.95 ms | 714.56 ms | 730.73 ms | Includes CLI process startup |
| Updated staged probe | 415.99 ms | 407.11 ms | 423.76 ms | In-process Rust test, uncached |
| Updated proof-cache reuse | 52.50 ms | 55.42 ms | 52.23 ms | In-process Rust test, two full source hashes |

Quick headers arrived at 24–30 ms in these updated runs. Candidate video timing validation took 328–349 ms after its stage entry; the final source hash took approximately 13 ms. These are warm local runs of a small generated fixture, not throughput guarantees for long footage, slow disks, other codecs or simultaneous jobs. The old CLI and updated in-process numbers are **not a matched process-overhead comparison**, so a precise overall speedup is not claimed. The lead will record a latest-CLI run on the same retained bytes once integration builds.

The integrated matched CLI comparison used the same `media probe --path` invocation and identical retained bytes. Old installed debug CLI wall times: 780.264 / 763.137 / 801.249 ms. Updated debug CLI: 524.695 / 446.081 / 421.382 ms. Median decreased from 780.264 to 446.081 ms, approximately 43% on this fixture. All six returned byte-equivalent serialized verified metadata, including digest, 480 frames and exact 60/1 CFR. Runs were sequential/warm, three per binary, not a statistical throughput study or end-to-end Studio import benchmark. JSON evidence is ignored `.lumeryn-scratch/import-cli-matched.json`; provisional-card response, transaction and preview work have separate acceptance.

Retained ignored scratch evidence: `.lumeryn-scratch/import-baseline-44a4900c340847339e4944d83a689746/baseline.json`, its `probe-1.json` through `probe-3.json`, and `.lumeryn-scratch/import-benchmark-after.log`. The old CLI result also reported its internal elapsed time (699 ms on run 1), verified 480 frames, CFR and the same digest. Scratch media/binaries are not committed.

Reproduce the updated stages after generating equivalent authorized fixture bytes:

```powershell
$env:LUMERYN_IMPORT_BENCH_SOURCE = '<generated fixture path>'
cargo test -p lumeryn-core --locked --test import_stages measured_import_stages -- --include-ignored --nocapture
```

## Executed correctness checks and limits

`import_stages` generated acceptance passed: 60 FPS MPEG-4 MKV, exact 30000/1001 FFV1 MKV, 25 FPS MPEG-4 MP4 and one-frame PNG all match independent FFprobe `-count_frames` results. Cache hits omit full video validation and return identical verified metadata. A VFR fixture remains non-CFR and retains the complete frame count. A same-size WAV byte replacement causes a cache miss and new full canonical audio validation. Replacing video bytes inside the validation window is rejected. Source hashes/bytes are unchanged by all successful probe operations. Advisory headers serialize without `frames` or `constant_rate` fields.

The focused ignored suite passed four tests; three execute the real fixture checks, while the fourth is the explicit optional timing harness. Strict locked all-target core Clippy passed. The lead's integrated formatting/tests/service/native acceptance remains necessary before calling the complete import experience verified.

This slice improves duplicate decode cost and responsiveness through staged service integration. It does not add mixed-rate editing, proxies, hardware decoding, persistent thumbnail/waveform storage or unlimited import throughput. Audio-only imports still decode the complete canonical source before placement, preserving deterministic sample bounds. Long timestamp output still has the existing explicit output limit; streaming unlimited timestamp validation is a separate measured need, not introduced speculatively here.
