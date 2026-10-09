# ADR 0015 — Unrestricted local UHD export on the Rust timeline

Date: 2026-10-09. Status: accepted for the existing supported baseline; advanced graph semantics remain staged.

## Context

The owner authorizes local UHD export with selectable orientation, FPS, codec, bitrate, quality and safe hardware fallback, without subscription restrictions or watermarks. The native editor and external CLI must use one Rust project graph and service; preview performance settings must not reduce final-source quality.

## Decision

Add a backward-compatible shared `render::RenderOptions` type, CLI flags/generated options schema and defaulted service request field. Existing requests preserve MPEG-4/AAC CPU/project dimensions/rational FPS. Support 720p, 1080p and UHD presets, project aspect-preserving fit, and 24/25/30/60 output FPS. Evaluate source footage once at a final-resolution virtual canvas; never upscale an intermediate editor-resolution image when full-resolution source detail exists.

Retain controlled FFmpeg subprocesses. Native MPEG-4 is the baseline. H.264/H.265 use available, verified x264/x265 CPU encoders, with optional Auto probing NVENC/QSV/AMF at actual requested dimensions/rate. CPU fallback covers failed initialization and failed real hardware encoding; unavailable encoders fail explicitly. Preserve rational cuts, source identity, immutable job revisions, no-clobber publication, decoded verification, cancellation and resource bounds. Expose real stage events instead of invented percentages.

## Evidence and consequences

The [executed research report](../research/UHD_EXPORT.md) records eight actual UHD orientation/FPS files, four CPU/Auto H.264/H.265 files, decoded audio/sample timing, pixel checks, final-source detail parity from a smaller editing canvas and injected hardware failure coverage. Short fixture results establish functionality rather than general throughput or physical A/V synchronization. Hardware decode, zero-copy, device-specific fastest selection, advanced compositor/effects, professional color/HDR and 8K are not implemented by this decision.

The graph remains the current contiguous one-track baseline and rejects unsupported semantics. Later Rust evaluator work must feed this same final render/options boundary; no parallel GUI-only graph or silent approximate export is permitted. Resolution controls select actual output dimensions; they do not recover missing source detail. Preview settings/volume remain monitor-only.

The locally installed exact FFmpeg build enables GPL/version3 and x264/x265. It is not bundled or linked into proprietary Lumeryn. Any future distribution requires exact-build/source/notice and codec rights review. A reviewed non-GPL build with native MPEG-4/AAC is the current alternative; platform/commercial H.264/H.265 backends require independent compatibility and licensing work. Sources: [FFmpeg codecs](https://ffmpeg.org/ffmpeg-codecs.html), [FPS filter behavior](https://ffmpeg.org/ffmpeg-filters.html#fps), [FFmpeg legal guidance](https://ffmpeg.org/legal.html), [NVIDIA FFmpeg integration](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/ffmpeg-with-nvidia-gpu/index.html), checked 2026-10-09.

## Rejected alternatives

Reject paywalled UHD, mandatory cloud encoding, hardware-only export, changing the Rust graph to a second editor backend, metadata-only export validation, GPU availability inferred from compiled names, and scaling source footage through preview/editor resolution before final UHD. Benchmark-driven hardware decode/interop and broader codec backends can be added after measured benefit and parity tests.
