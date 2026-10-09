# Acceleration, source identity and cache research

Reviewed 2026-10-09 (Asia/Calcutta). This document specifies architecture and the next implementation steps. It does not claim that hardware export, watchers, a disk cache or a benchmark command already exists. Benchmarks must exercise Lumeryn operations using generated or explicitly trusted fixtures; no paid services or general system stress benchmarks are required.

## Hardware media paths

Hardware detection has four separate results: adapter detected, backend compiled in, operation initialized successfully, and operation verified/benchmarked for a particular codec/profile/format. Each can fail independently. Persist `unknown`, `unavailable(reason)`, or `verified(configuration)` rather than one optimistic GPU Boolean. Device capabilities are separate from available RAM/VRAM and current utilization.

| Backend | Intended role | Official reference and constraints |
| --- | --- | --- |
| NVIDIA NVENC / NVDEC | Encode / decode; independent from wgpu composition | [Current SDK 13.1 FFmpeg guide](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.1/ffmpeg-with-nvidia-gpu/index.html). Query hardware codec/profile/bit-depth support and runtime driver compatibility. Keep surfaces on device where possible; CPU-only filters can force transfers. |
| Intel QSV / VPL | Intel decode/encode/media processing | [FFmpeg QSV options](https://ffmpeg.org/ffmpeg-codecs.html#QSV-Encoders), [Intel VPL dispatcher](https://github.com/intel/libvpl). Dispatcher presence is not a usable implementation; check adapters and requested operation. |
| AMD AMF | AMD encode and supported decode/conversion paths | [AMD SDK](https://github.com/GPUOpen-LibrariesAndSDKs/AMF). Drivers/runtime and codec capabilities vary; evaluate actual Windows/Linux runtime. |
| D3D11VA / D3D12VA / DXVA2 | Windows hardware decoding | [FFmpeg hardware options](https://ffmpeg.org/ffmpeg.html#Advanced-Video-options). Verify surface/download/format paths separately from encoding. |
| VAAPI | Linux decode/encode candidate | [FFmpeg VAAPI encoding](https://ffmpeg.org/ffmpeg-codecs.html#VAAPI-encoders). Not a Windows fallback; requires an appropriate driver/device. |
| VideoToolbox | macOS native acceleration candidate | [Apple VideoToolbox](https://developer.apple.com/documentation/videotoolbox). Platform-specific capability and output checks required. |
| CPU software | Always maintain a supported ordinary-editing path | Existing native MPEG-4/AAC baseline is tested; hardware encode is optional. Software decode and media scheduling still need explicit resource bounds. |

FFmpeg explicitly documents that `-hwaccels` lists compiled components, while runtime availability depends on hardware/driver; GPU downloads can eliminate acceleration gains. QSV modes, decoder selection and filter topology require care. [Official FFmpeg hardware documentation](https://ffmpeg.org/ffmpeg.html#Advanced-Video-options)

Licensing is evaluated at the actual integration boundary: Intel's VPL dispatcher is [MIT](https://raw.githubusercontent.com/intel/libvpl/main/LICENSE); AMD's AMF SDK uses [MIT with a separate standards/patent notice](https://raw.githubusercontent.com/GPUOpen-LibrariesAndSDKs/AMF/master/LICENSE.txt). Those grants do not establish redistribution rights for every driver/runtime or patent clearance for encoded formats. Review NVIDIA SDK/header/runtime terms before bundling or directly linking any component. The installed FFmpeg remains the GPL build recorded in [MEDIA licensing](../licensing/MEDIA.md); enabling hardware does not relicense that executable.

Read-only local observations from FFmpeg 8.1.1: `-hwaccels` lists cuda, vaapi, dxva2, qsv, d3d11va, d3d12va, amf. `-encoders` lists H.264/HEVC/AV1 NVENC, QSV and AMF variants; `-decoders` lists CUVID and QSV variants. These are **compiled-in observations only**. No hardware encoder initialization, adapter capability test or GPU throughput benchmark was performed for this review. Windows does not imply every listed backend is usable.

A future selector must choose per operation: decode, upload, scale/color conversion, composition, download and encode. A shader-capable wgpu adapter does not establish media-engine support. Budget decoder surfaces, upload/readback pools, composition textures and encoder queues together. Prefer a compatible pipeline with fewer transfers when its measured quality and end-to-end time justify it; retain CPU paths for unsupported input formats.

Auto mode may retry a failed hardware job once using a verified software implementation of the **same requested export format and codec contract**. Keep the failed attempt's diagnostic and actual fallback in provenance, discard its private temporary output, then verify the replacement normally. Explicit hardware-required mode should return an availability error. Do not silently turn an H.264 request into MPEG-4 Part 2; if no reviewed software H.264 encoder exists, fail that preset clearly. The currently tested MPEG-4/AAC baseline remains a separate supported preset.

## Benchmark and detection contract

Implement opt-in application-level fixture runs behind the shared media/job service, accessible through future CLI and Studio commands. Record monotonic stage timings for hash/probe/decode/transfer/filter/composite/encode/verification and complete job wall time. Record startup separately, actual backend selected, encoder settings, fixture digest, dimensions, rational FPS, frame/sample counts, output quality checks, peak process memory, available telemetry and instrumentation overhead. Unknown VRAM measurements remain unknown.

Compare CPU and eligible GPU paths using the same decoded content and comparable quality/bitrate settings; a fast low-quality preset is not a fair performance result. Use warm-up plus bounded repeated runs with median and high-percentile latency; separate cold cache and warm cache; reserve playback capacity and allow cancellation. Verify frame ranges and A/V alignment before interpreting speed. Never use production/customer media by default. Fixed color sequences establish frame selection; moving detailed patterns establish encode behavior. A small smoke fixture cannot establish 4K playback or production throughput.

The earlier standalone FFmpeg measurements in [MEDIA.md](MEDIA.md) are backend feasibility evidence. They are not Lumeryn benchmark results and must not appear as application performance claims. Until application fixture instrumentation is implemented and executed, record hardware performance as unmeasured.

## Immediate source-identity correction

The initial export checked only stored byte length. A same-size replacement can pass that guard while supplying unrelated frames or invalidating the stored metadata. Implement full-byte BLAKE3 identity now, independently of the future cache:

1. Stream-hash the canonical source on import; persist a versioned algorithm tag and complete 32-byte digest alongside byte length.
2. Probe only a stable input. Hash again after probing and refuse to commit if identity changed. Metadata must correspond to the committed source digest.
3. Before export, compare the full digest with the imported identity. Reject changed media with a structured reimport/relink message; do not silently replace the imported identity.
4. Recheck identity after rendering and before publishing. On mismatch, discard the private output and fail; preserve project revision/source files. Check both same-size changes and changes between probe/render stages.
5. Make old schema behavior explicit. Migrate or reject older records without identity; never synthesize a valid digest from a path/size/mtime.

Candidate pin: **`blake3 = "=1.8.7"`**, published 2026-08-20, verified from [package version metadata](https://docs.rs/crate/blake3/1.8.7) and [tagged Cargo manifest](https://raw.githubusercontent.com/BLAKE3-team/BLAKE3/1.8.7/Cargo.toml). License alternatives are CC0-1.0, Apache-2.0, or Apache-2.0 with LLVM exception; select Apache-2.0 for the dependency register and retain required notices. The manifest uses Rust edition 2024, compatible in principle with this project's Rust 1.97 toolchain; an actual dependency build, lockfile audit and Rust gates are required before claiming local integration works.

Start with default `std`, no Rayon or mmap feature. Incremental `Hasher::update_reader` is available without a file-sized allocation. For cancellation, a bounded read/update loop can check job cancellation between chunks. Execute blocking file I/O on a scheduler-controlled worker; dropping an async timeout does not stop a started blocking hash task. Bound concurrent hash jobs and bytes/operation duration; prevent hashing from starving preview. [BLAKE3 Hasher API](https://docs.rs/blake3/1.8.7/blake3/struct.Hasher.html)

Pre/post hashes detect ordinary changes but do not guarantee an immutable input while FFmpeg opens paths independently; an adversarial change-and-restore can evade observations. Before hostile concurrent-write support, render from an immutable owned snapshot whose full hash matches the imported identity, or implement tested Windows write/delete-denial guards held through all reads. Opening shared-read-only handles can exclude concurrent write/delete access, but sharing rules and existing handles must be tested. [Windows CreateFile sharing semantics](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)

## Content-addressed derived cache

Use separate keys for source bytes and derivation recipes. A derivation key hashes a deterministic, versioned, typed encoding of artifact kind, upstream content/derivation digests, source stream and exact range, normalized rational timebase, dimensions, pixel/color/sample format, complete effect parameters, relevant asset/model/plugin digests, evaluator/backend versions and output-quality settings. Include actual selected backend/driver identity where it can alter bytes. Domain-separate source identity and derivation keys. UUIDs, filesystem paths and unrelated project revisions belong in provenance, not every reusable decode/thumbnail key.

Represent dependencies as a DAG. A trim invalidates the affected timeline node and downstream composites/mixes; unchanged source thumbnails and unaffected clips remain reusable. Source replacement invalidates all artifacts descended from the old content reference. Missing/corrupt artifacts become cache misses. Read artifacts only after digest/schema/length validation. Cache contents are private application data, never source-authoritative project state.

Publish cache artifacts from a unique temporary file using no-clobber atomic promotion after validation; concurrently produced identical keys can reuse the validated winner. Track logical and stored byte sizes, creation/access time, provenance, rebuild cost and active-job leases in an index. Recover abandoned temporary files and reconcile index/files after a crash. Never report an incomplete artifact as a hit.

Keep independent configurable quotas for disk artifacts, RAM frame buffers and GPU textures, with free-space headroom and per-job reservations. Maintain playback-critical entries and active-job pins; evict eligible derived entries by recency/rebuild cost under quota pressure. Admission can decline caching an oversized artifact. Disk caches/proxies do not authorize overwriting or deleting original media, project databases, exports or user files.

Cleanup must target indexed owned regular files inside the resolved configured cache root. Verify the absolute root and each target immediately before deletion; refuse traversal, symlink/junction/reparse-point escapes or suspicious replacement, and coordinate cleanup with writers/job leases. Use one OS-native filesystem API or shell end-to-end, never turn project paths into deletion command text. Offer dry-run accounting; a purge command must describe its exact scope and follow the user's approval requirements. No cleanup operation was executed during this review.

Already encoded video/images generally stay in their original derived encoding. Consider low-cost Zstandard for measured compressible metadata, waveforms and cold raw-frame blocks; leave hot GPU/RAM frames uncompressed when decompression increases latency. Cache manifests carry compression format/version, encoded size and bounded decoded size; enforce limits to prevent decompression bombs. Zstandard's reference implementation offers BSD or GPLv2 alternatives; select the BSD option and separately audit the Rust wrapper/transitive build before adoption. No compression library was added or benchmarked. [Official Zstandard repository](https://github.com/facebook/zstd)

## External changes: notify as a hint

Current candidate **notify 8.2.0** is documented at [versioned crate metadata](https://docs.rs/crate/notify/8.2.0). The notify crate is CC0-1.0; companion notify-types/debouncers/file-id are MIT or Apache-2.0. Windows uses ReadDirectoryChangesW; cross-platform polling is available. Confirm the selected version's lockfile/toolchain and backend build before adoption. [Maintainer repository](https://github.com/notify-rs/notify)

Watch source parent directories, not only file handles: editors may replace a file during save. Debounce bursts into dirty hints; reconcile rename/delete/recreate, network shares, watcher errors, overflow and restarts. Watcher events never prove unchanged content. The official docs identify missing network-filesystem events and incomplete event delivery on large trees, with polling as a fallback. [Notify known problems](https://docs.rs/notify/8.2.0/notify/#known-problems)

On any dirty hint, mark affected source references and derived descendants stale in the shared service; rehash on next use and on bounded background reconciliation. Revalidate mandatory job inputs even when no event arrived. Do not silently mutate project history or accept a new source as the old import. Future CLI/Studio events should report missing/changed/relink-needed status and preserve stable IDs through an explicit typed relink/import revision.

## Acceptance before capability claims

- Source identity: same-size content replacement, missing/renamed file, mutation during probe/render, schema migration, cancellation and bounded hashing; no published output on mismatch.
- Cache: identical content at different paths, precise dependency invalidation, concurrent writers, interrupted promotion, corrupt artifact, quota/low disk, active-job pins, escaped cleanup targets and bounded decompression.
- Watchers: atomic-save replacement, parent deletion/recreation, missed event, overflow, network/polling fallback and service restart/reconciliation.
- Hardware: compiled-but-unavailable backend, unsupported profile/bit depth, runtime/device loss, CPU fallback preserving preset, frame/sample/content equivalence and real Lumeryn stage timings.

All hardware/cache/watcher/compression benchmarks remain pending. BLAKE3 source identity is the recommended immediate implementation, with its status and evidence maintained by the lead in TASKS/PROGRESS.
