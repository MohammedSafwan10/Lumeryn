# ADR 0002: Controlled FFmpeg executables for the first media slice

Date: 2026-10-09. Status: accepted for first vertical slice; interactive preview backend and distribution remain open decisions.

## Context

Lumeryn needs verified media metadata and real decoded-range export while retaining a Rust-owned, non-destructive project graph. Windows is the initial host; Studio, headless CLI, and later workers must share the same media service boundary. A working externally installed FFmpeg pair exists, but its GPL build must not be bundled by default.

## Decision

Use a Rust media adapter around separately installed `ffprobe` and `ffmpeg`. Accept semantic validated operations, use argument arrays directly, and parse structured metadata/progress. No shell scripts become the product's timeline implementation. Keep project state, revision validation, stable IDs, and timing conversions in Rust.

The initial reference export uses native MPEG-4 Part 2/AAC MP4; lossless FFV1/PCM fixtures verify selected content. Do not silently select x264, x265, or nonfree components. Capability detection must describe available behavior rather than promise features solely because an encoder appears in a list.

Decode/filter/re-encode for arbitrary trim boundaries. Treat keyframe-dependent stream-copy export as a separately specified future optimization. Require explicit output verification and preservation of source files. Unsupported layering, effects, stream layouts, or timing cases must fail before rendering.

## Alternatives

| Option | Assessment |
| --- | --- |
| Controlled executable | Tested locally; easiest independent headless worker and Windows deployment; startup and pipe copies make it a limited interactive decoder. |
| Direct libav DLL bindings | Potentially lower interactive overhead; exposes unsafe FFI/ABI and linking obligations; defer until preview measurements require it. |
| Windows Media Foundation | Useful optional Windows integration; platform dependence prevents selection as sole cross-platform backend. |
| Implement codecs in Rust | Insufficient first-slice ecosystem coverage and disproportionate engineering/security maintenance burden. |

## Evidence and consequences

The [media research](../research/MEDIA.md) records a real 3-second synthetic source, verified 1-second MP4 output with 30 frames, complete decode, lossless frame checksums, and 48,000 PCM samples. It includes a small startup-inclusive benchmark and a denied HTTP protocol test. Only these cases are established.

Bound CPU threads, output capture, wall-clock time, input complexity, and concurrency; move these controls behind shared scheduler/job abstractions as those subsystems mature. Process separation reduces Rust FFI complexity but does not sandbox the decoder. Patched executables and future Windows containment are required for untrusted-media production use. GPU, HDR, VFR, and cross-platform performance remain unverified.

The locally installed 8.1.1 build is GPL v3+ even when native encoders are selected. It is an external development/runtime tool, not a linked Cargo dependency or distributed Lumeryn asset. Any future packaging needs owner-reviewed license/source/notice and patent compliance. Lumeryn remains proprietary. See [media licensing](../licensing/MEDIA.md).

Revisit the decision when frame-seek latency or GPU upload measurements justify direct decoding; preserve the adapter interface. Current official references were checked before this decision: [CLI](https://ffmpeg.org/ffmpeg.html), [filters](https://ffmpeg.org/ffmpeg-filters.html), [ffprobe](https://ffmpeg.org/ffprobe.html), [licensing](https://ffmpeg.org/legal.html), [release downloads](https://ffmpeg.org/download.html).
