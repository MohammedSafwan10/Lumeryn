# Media backend research

Reviewed: 2026-10-09 (Asia/Calcutta). Scope: first CLI vertical slice, with a reusable media boundary for later preview and GPU work. Experiments below ran locally; they are backend prototypes, not claims that all Lumeryn commands already exist.

## Selected baseline and compatibility

Use separately installed `ffprobe` for JSON metadata and `ffmpeg` for controlled decode/filter/encode jobs. Keep semantic edits and authoritative rational time in Rust. Do not persist command lines as the timeline model. Direct libav bindings remain a future decoder option where interactive latency and texture transfer justify their complexity.

The local WinGet executable reports `8.1.1-essentials_build-www.gyan.dev`, GCC 15.2.0, libavcodec 62.28.101, libavformat 62.12.101. Windows executables run correctly in the experiments. Linux/macOS commands are expected to be portable, but have not been executed here. Installed binary is GPL v3+; see [license review](../licensing/MEDIA.md).

The official release page currently lists 9.0.2 (2026-09-18) and maintained branch 8.1.3 (2026-09-21). The installed binary has not been upgraded. Require an explicitly reviewed executable/version/capability record in production; avoid claiming a tested version is the latest. [Official downloads](https://ffmpeg.org/download.html)

## Time and stream rules

Transcoded input seeking can decode and discard frames before the requested boundary; stream copy can preserve those frames. Therefore stream copy is not the default for accurate edits. Generate arguments directly, use explicit stream maps, disable stdin interaction, and reserve stdout for progress or probe JSON. [FFmpeg CLI options](https://ffmpeg.org/ffmpeg.html)

For the prototype, decoded `trim`/`atrim` select a half-open range; `setpts`/`asetpts` reset output timestamps. Frame and sample indices are useful for known CFR and PCM fixtures. Timestamp selection is required for VFR. Concatenated segments must share normalized stream properties and begin at zero. [Official filters](https://ffmpeg.org/ffmpeg-filters.html#trim), [audio trim](https://ffmpeg.org/ffmpeg-filters.html#atrim), [concat](https://ffmpeg.org/ffmpeg-filters.html#concat)

The Rust adapter must explicitly define timestamp origin, rounding, stream timebase conversion, and clip end behavior. Decimal seconds in these fixed examples are exact .5/1.5 values, not a replacement for rational project timing. VFR, nonzero stream start, rotation, HDR, unusual aspect ratio, audio padding, and 24000/1001 inputs need dedicated coverage before broader accuracy claims.

Native `mpeg4` plus native `aac` provides a small MP4 prototype without selecting x264/x265. FFV1 plus PCM in Matroska supplies lossless regression fixtures. AAC is an official native encoder; selecting it is distinct from `libfdk_aac`. [Official codec documentation](https://ffmpeg.org/ffmpeg-codecs.html#aac)

## Reproducible local evidence

Working directory: project root. Outputs under ignored `.lumeryn-scratch/media/`. All encode/probe/decode operations below returned exit 0, except the deliberate denied network probe. Every output was newly created with `-n`; reruns need fresh output names.

```powershell
ffmpeg -hide_banner -loglevel error -nostdin -n -f lavfi -i 'testsrc2=size=320x180:rate=30:duration=3' -f lavfi -i 'sine=frequency=440:sample_rate=48000:duration=3' -map 0:v:0 -map 1:a:0 -c:v ffv1 -threads 2 -c:a pcm_s16le '.lumeryn-scratch/media/source.mkv'

ffmpeg -hide_banner -loglevel error -nostdin -n -protocol_whitelist file -i '.lumeryn-scratch/media/source.mkv' -filter_complex '[0:v:0]trim=start=0.5:end=1.5,setpts=PTS-STARTPTS[v];[0:a:0]atrim=start=0.5:end=1.5,asetpts=PTS-STARTPTS[a]' -map '[v]' -map '[a]' -c:v mpeg4 -q:v 3 -pix_fmt yuv420p -threads 2 -c:a aac -b:a 128k -movflags +faststart '.lumeryn-scratch/media/trim.mp4'

ffprobe -v error -protocol_whitelist file -count_frames -show_entries 'stream=index,codec_name,width,height,avg_frame_rate,duration,nb_read_frames,sample_rate,channels:format=duration,size' -of json '.lumeryn-scratch/media/trim.mp4'
ffmpeg -hide_banner -loglevel error -nostdin -protocol_whitelist file -i '.lumeryn-scratch/media/trim.mp4' -f null -
```

Verified output: MPEG-4 Part 2, 320x180, 30/1 fps, exactly 30 decoded video frames; AAC mono at 48 kHz; both stream durations and container duration 1.000000 seconds; size 173,217 bytes. Full video/audio decode to null succeeded. AAC decoded packet count was 47; packet count is not sample count and does not establish sample-exact AAC output.

An additional lossless encode used `trim=start_frame=15:end_frame=45` and `atrim=start_sample=24000:end_sample=72000`, followed by timestamp reset and `-c:v ffv1 -c:a pcm_s16le`. `framemd5` from its decoded video matched every one of the 30 records from the selected source interval. Decoded mono `s16le` audio measured 96,000 bytes, exactly 48,000 samples. These checks establish correct decoded frame selection and one-second PCM sample count for this fixture.

| Artifact | SHA-256 |
| --- | --- |
| source.mkv | 31599D10D9DEDA44F2C70F4938A1B5F14CA64C1C84C45CC9B0F5AC554D9453CA |
| trim.mp4 | 4044E86535ABC9F88134C1EFB7BA43067138E275F88F4E530B194034AB54F9B2 |
| trim-lossless.mkv | 54E1741F63045281D3B82F3E63EAE76C77F8CA7AC03B29263C2ED7648640A7E0 |

Five repeated one-second MPEG-4/AAC exports with two video encoder threads took 65.7, 47.0, 47.6, 47.9, 47.3 ms wall time (mean 51.1 ms), including subprocess startup, measured with PowerShell Stopwatch on this machine. This is a tiny warm-cache smoke benchmark; it does not establish 4K throughput, memory ceilings, interactive preview latency, or comparative codec performance.

## Security and resource design

`-protocol_whitelist file` rejected `http://127.0.0.1:9/no-media` before input access: `Protocol 'http' not on whitelist 'file'!`, nonzero exit. FFmpeg permits many protocols by default; restrict each actual input. An allowlist is not an OS sandbox and local containers can reference other local files. Reject manifests/playlists and unsupported demuxers in the initial local-file profile. [Protocol documentation](https://ffmpeg.org/ffmpeg-protocols.html#Protocol-Options)

The official security register records additional fixes in 8.1.2 beyond the installed 8.1.1. Use generated/trusted fixtures for this first implementation and review/update the executable before enabling untrusted customer-media processing. The prototype did not audit the installed binary or demonstrate exploitability. [FFmpeg security register](https://ffmpeg.org/security.html)

Implement these application controls: canonical regular-file paths; explicit executable configuration; no shell invocation; typed allowlisted encoder/filter arguments; no source overwrite; bounded simultaneous jobs, threads, probe size, stderr/stdout retention, and wall-clock deadlines; cancel/kill and reap children; unique temporary outputs; post-encode probe/decode verification; atomic promotion to a newly reserved destination. Windows Job Objects and restricted tokens are future containment work, not established properties of a plain subprocess.

Scheduling must estimate decoded buffers as well as compressed file size: one 3840x2160 RGBA frame is 33,177,600 bytes; eight such frames consume about 253 MiB before decoder surfaces and other pools. First jobs should reserve conservative CPU slots and reject excessive dimensions/durations. Establish measured memory and throughput baselines before adaptive admission or GPU scheduling. Software fallback remains mandatory; detected hardware does not prove a driver/backend is usable.

## Alternatives and next validation

Direct libav DLL bindings may reduce copy/seek latency but require audited ABI/version matching, an unsafe FFI boundary, and linking compliance. Media Foundation can provide Windows-native media paths but cannot be the only backend for the cross-platform project. Reviewed LGPL-only FFmpeg builds are the future redistribution candidate. Hardware H.264/HEVC/AV1 paths require device-specific capability tests, quality benchmarks, and patent/distribution review. No alternative was benchmarked or installed in this prototype.

Before expanding capabilities: test no-audio inputs, independently offset audio/video streams, rational CFR, VFR, multiple clips, relinking, malformed metadata, restricted protocols, timeout/cancellation, output collision races, resource exhaustion, source preservation, and exact post-render frame/sample limits. Store backend identity and render settings with job provenance.
