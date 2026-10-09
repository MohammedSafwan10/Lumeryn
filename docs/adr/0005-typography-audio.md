# ADR 0005: Shared native typography and sample-based audio pipelines

Date: 2026-10-09

Status: Provisional architecture direction. No new dependencies or implementation; layout, rasterization and device choices require compatibility prototypes and benchmarks.

## Context

Lumeryn requires editable text/vector layers, captions, motion graphics, audio mixing and synchronized preview/export. CLI and Studio must share Rust semantics. Browser text rendering, script-only title generation or an unrelated playback mixer would fragment the project model.

## Decision

Prefer Parley for native creative text layout, with cosmic-text tested as the alternative. Use explicit fonts and shaping/style data in project layers; cache layout and resolved font hashes. Rasterize a validated scene subset using vello_cpu as the first reference/fallback candidate, then benchmark vello_gpu acceleration. Keep shared glyph geometry/time evaluation across preview/export. Do not select compute-centric experimental Vello as mandatory production rendering. [Parley API](https://docs.rs/parley/0.11.1/parley/), [cosmic-text API](https://docs.rs/cosmic-text/0.19.0/cosmic_text/), [Vello family](https://github.com/linebender/vello/blob/main/README.md).

Use resvg for bounded static SVG import/raster caching, not animation. Keep libass optional for ASS/SSA compatibility; native caption text may use the common layer pipeline, with explicit fidelity warnings on conversion. Fonts, emoji and SVG asset rights are reviewed separately from renderer code. [resvg scope](https://github.com/linebender/resvg), [libass](https://github.com/libass/libass).

Reuse FFmpeg decoding behind the existing media adapter. Rust owns a deterministic sample-frame mixer, placement/range conversion, channel mapping, gains/fades and waveform cache. Source PCM is converted to the explicit project sample rate; render ranges use checked rational mappings and half-open boundaries. Retain source timestamps/origin, encoder delay and resampler delay. Raw PCM transport must not discard required timing information. First scope: mono/stereo, gain/fades, waveform inspect and lossless WAV verification.

Prefer Rubato for quality fixed source-rate conversion and optional playback-device drift correction. Compare quality/delay behavior using generated fixtures, keep buffers preallocated and dependency adapter versions coherent. Use dasp only for selected low-level modules when needed; it does not replace the mixer or validate real-time concurrency. [Rubato API](https://docs.rs/rubato/5.0.1/rubato/), [dasp](https://docs.rs/dasp/0.11.0/dasp/).

Add CPAL WASAPI as the Windows audio I/O boundary after offline PCM behavior passes. Producer threads decode/mix/resample ahead into bounded buffers; callbacks perform no allocation, locks, disk/IPC/logging or decode. Playback follows measured device timestamps and sample mapping; video follows the audio presentation clock. Offline export has no device/wall-clock dependency. Shared PCM mix semantics precede device-rate conversion; compare preview-producer PCM and offline PCM before claiming parity. CPAL timestamps describe predicted playback separately from callback execution. [CPAL](https://github.com/RustAudio/cpal), [timestamp semantics](https://docs.rs/cpal/0.18.2/cpal/struct.OutputStreamTimestamp.html).

## Alternatives

| Alternative | Assessment |
| --- | --- |
| cosmic-text + Swash cached titles | Strong integrated fallback; choose if comparative corpus/prototype is better; do not run competing authoritative layouts |
| Browser DOM/canvas output titles | Convenient Studio UI, but would duplicate Rust output semantics; excluded as project renderer |
| FFmpeg drawtext/libass for every title | Good narrow baseline/ASS interoperability, inadequate editable native motion-graphics model |
| Rodio or browser audio as complete engine | Convenience playback does not settle deterministic project mixing/clock control; not primary architecture |
| Separate Rust media decoder stack | Duplicates codec integration/audit; reuse FFmpeg until a measured requirement justifies change |
| FFmpeg-only audio filter graph | Useful reference/first-export implementation; requires explicit parity before interchangeable native preview/export |
| Unbounded predecode/full PCM in RAM | Cannot support long media; use chunked disk/memory caches with quotas |

## Licensing, compatibility and risks

Parley, cosmic-text, Vello, resvg, Rubato and dasp main code use MIT OR Apache-2.0; CPAL uses Apache-2.0 and libass uses ISC. Preserve notices/required Apache terms and audit locked graphs, optional SDKs and fonts individually. These licenses do not authorize fonts/media redistribution or change Lumeryn ownership. Parley emoji/test assets have exceptions. [Parley license exceptions](https://github.com/linebender/parley), [CPAL license](https://github.com/RustAudio/cpal), [Rubato manifest](https://github.com/HEnquist/rubato/blob/master/Cargo.toml), [dasp manifest](https://github.com/RustAudio/dasp/blob/master/dasp/Cargo.toml), [libass license](https://github.com/libass/libass/blob/master/COPYING).

Vello CPU0.3.0 warns about unsupported filter graphs and experimental glyph caches; validate subsets before library calls. Its RGBA8 pixmap is not an HDR pipeline. Upstream Vello uses wgpu30 with Rust1.89, and Rubato5.0.1 requires Rust1.87 with current audioadapter dependencies. These observations do not prove a compatible locked Windows build. CPAL requested buffer size is not measured end-to-end latency; device loss, callback variation and scheduler contention need tests. Audio resampling is not pitch-preserving time stretching. [CPU renderer limits](https://docs.rs/vello_cpu/0.3.0/vello_cpu/), [Vello manifest](https://github.com/linebender/vello/blob/main/Cargo.toml), [Rubato compatibility](https://docs.rs/rubato/5.0.1/rubato/).

## Required staged acceptance

Execute TA-01..TA-05 in `docs/research/TYPOGRAPHY_AUDIO.md`: first sample-exact lossless mix/waveform CLI slice, then source-rate conversion, native text/SVG frames, WASAPI clocks and GPU/ASS compatibility. Include multilingual golden frames, fixed licensed fonts, hostile resources, impulse/short-clip/delay fixtures, channel/gain math, no callback allocations, device removal and ten-minute AV drift. Record actual latency/memory measurements. All these capabilities remain planned until their implementation and acceptance evidence are recorded.
