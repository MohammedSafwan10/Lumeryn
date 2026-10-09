# Native preview evidence

Verified on Windows on 2026-10-09 with Rust 1.97.1, installed FFmpeg 8.1.1 and CPAL 0.18.2. This library has no GUI dependency; a worker owns a Tokio runtime, native output stream and controlled FFmpeg children. Studio receives actual bounded RGBA frames and owns upload to its existing egui-wgpu device. The library does not create a GPU device, decode on the GUI thread, or upload unchanged frames.

`PreviewPlayer::new(MediaTools, PreviewConfig)` owns its arguments. `load(Project, PathBuf)` accepts a request asynchronously; inspect `status().error` for validation or decoding errors. `play`, `pause`, `seek`, `step` and audio enable are bounded, nonblocking commands. Seek during an in-progress load is retained. `try_frame` drains to the newest current generation. Consumers retain the displayed frame when no newer frame is available. Each frame includes integer project position, revision, generation and exact rational PTS.

The supported evaluator matches baseline export: 1..64 contiguous clips on the project video track, matching source/project CFR, integer source-in and duration, square-pixel SDR, zero-origin video/audio, embedded audio or silence, aspect-preserving scale and padding. Unsupported gaps, overlaps, HDR, anamorphic pixels and source timestamp offsets fail explicitly. Local source paths are canonicalized and verified against stored BLAKE3 before/after decoding. This duplicates the export baseline's narrow evaluation rules temporarily; factor the shared evaluator into the headless core after measured parity. No effect, animated overlay, compositing, transition or professional multitrack mixer is claimed.

Video decoding streams one FFmpeg process per clip, restarted for seeks, with two decoder/filter/output threads, bounded stderr retention (64KiB) and per-read/exit timeout. Seeking currently decodes from the beginning of the relevant source to preserve exact frame trims; no seek-index cache is claimed. Backpressure bounds the command queue to eight, decoded packet queue to one, presentation queue to 1..3 plus one pending and one being decoded. Default RGBA payload is at most six 1280x720 buffers (21.1MiB), excluding the consumer's retained CPU frame, GPU texture, child decoder/filter surfaces, operating-system pipes and allocator overhead. No OS process memory limit or hardware zero-copy path exists. Status `buffered_frames` measures queued frames, not total resident memory.

The native CPAL default device supports F32/I16/U16 conversion. PCM is streamed and bounded to 250ms plus one 4KiB read block; unsupported device formats and unavailable devices are explicit. Audio filters use the same source-in/rational-duration, stereo 48k normalization, padding and silence as export before device format conversion. The callback performs no decoding, allocation or I/O and uses `try_lock`, supplying silence and counting underruns if data is unavailable. Producer backpressure waits for consumption notification; it does not poll a full paused queue. The stream pauses with playback. The actor disables its 4ms presentation timer when paused after the first frame; commands and cancellation remain active.

Audio is the presentation master when available. Sample count maps to project frames with integer/rational arithmetic; CPAL's predicted first-sample DAC timestamp compensates reported device latency. Underflow freezes sample progress rather than accumulating wall-clock drift. Video falls back to a monotonic clock when audio is disabled or unavailable. Seek creates a new audio/video generation and PCM queue. Pause/restart rebase submitted samples; queued device latency and physical speaker/display synchronization remain unmeasured. No professional, sample-accurate physical A/V guarantee is made. Device loss, unusual callback buffer sizes and non-Windows backend behavior require further measurements.

Run default and external gates:

```powershell
cargo fmt --check
cargo clippy -p lumeryn-preview --all-targets -- -D warnings
cargo test --locked -p lumeryn-preview
cargo test --locked -p lumeryn-preview --test generated_preview -- --include-ignored --test-threads=1 --nocapture
```

Executable overrides are `LUMERYN_FFMPEG` and `LUMERYN_FFPROBE`. Ignored tests generate local trusted FFV1/PCM fixtures under `.lumeryn-scratch/media` and remove their temporary directories. They do not download customer media or tooling. Four unit tests verify rational clock boundaries, paused clock, bounded dimensions/queue validation and predicted-DAC sample mapping. Four explicit integration tests verify exact RGBA against an independent selected-frame decode, contiguous clip source-in mapping, seek-before-load completion, reverse step, stable paused output, 30000/1001 PTS, actual native device play/pause/seek and audio/video EOS. Native audio smoke checks the output stream and sample-based frame progression; it is not a speaker loopback test.

The actual generated 1280x720 FFV1 CPU decode benchmark, debug build, three sequential runs measured:

| Run | Load to first frame ms | Seek to frame 25 ms | Play frame 25 through EOS ms | Max queued frames | Last pipe read ms | Dropped |
|---|---:|---:|---:|---:|---:|---:|
| 0 | 70.11 | 125.69 | 506.11 | 2 | 5.27 | 0 |
| 1 | 80.37 | 126.44 | 516.57 | 2 | 5.44 | 0 |
| 2 | 77.37 | 135.84 | 506.79 | 2 | 5.72 | 0 |

Expected playback is 15/30 = 500ms through EOS; every run reached final frame 39. First latency includes async validation and source digest; seek latency includes process startup and preceding-frame decode. `last pipe read` is raw read/availability time, not isolated codec CPU time. These are local synthetic application measurements, not general performance promises. No GPU interoperability, hardware codec, 1080p throughput or proxy cache benchmark is claimed. Idle process CPU, physical A/V loopback and different devices are separate future measurements.

Repeated native device runs used stereo 48kHz and reached project frame 15 in 492.06ms and 510.87ms against the expected 500ms sample-clock interval. Observed underrun counters were 0 and 1 respectively; the implementation reports underruns and supplies silence, rather than claiming glitch-free output. This measures scheduled project-frame progression, not audible-to-visible synchronization. Final preview gates passed four unit and four explicit generated/native integration tests, with clippy warnings denied.

Official sources reviewed: [CPAL 0.18.2 crate metadata](https://docs.rs/crate/cpal/0.18.2) (Apache-2.0, no ASIO feature enabled), [stream API](https://docs.rs/cpal/0.18.2/cpal/), [predicted output timestamps](https://docs.rs/cpal/0.18.2/cpal/struct.OutputStreamTimestamp.html), [FFmpeg filter documentation](https://ffmpeg.org/ffmpeg-filters.html) and [FFmpeg process options](https://ffmpeg.org/ffmpeg.html). The user-installed FFmpeg binary's GPL status is documented separately; no FFmpeg binary is bundled or Lumeryn source license changed.
