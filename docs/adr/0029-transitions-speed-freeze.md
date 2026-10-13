# ADR0029 — Exact source-time effects and fixed-cut transitions

Date: 2026-10-12. Status: accepted bounded evaluator; product integration/real media acceptance pending.

## Decision

Rust remains authoritative. The isolated `time_effects` module provides exact rational source-time sampling, three speed choices (0.5x/1x/2x), explicit preserve-pitch/mute audio policy, exact-source silent freeze holds and original cross-dissolve/directional-wipe reference functions. No new dependency, embedded script, optional runtime or backend is introduced.

A moving interval consumes `duration * speed` source frames. Source ranges must be proven against verified CFR counts, not inferred duration. Keeping a selected source range at2x requires an even source duration; odd durations fail explicitly rather than silently changing handles. Speed placement/ripple is a separate atomic project operation. Freeze duration is explicit and its selected source frame must exist; freeze contributes silence. A rational output sample maps directly to rational source time without rounding to integer timeline frames.

Transitions are anchored to an existing cut. Duration2..300project frames has floor(duration/2) before and the remaining frames after the cut. Both adjacent normal-speed video inputs need verified before/after handles. The cut, clip positions and project length remain unchanged. No source file changes and no automatic repeated edge frames masquerading as handles. The caller must validate adjacency, no overlapping transition windows, normal speed, matching verified frame rates, source identity and graph membership. Unsupported speed-plus-transition/PNG combinations fail until explicitly integrated and tested.

The pixel reference blends straight-alpha RGBA through premultiplied alpha under the existing encoded SDR policy, not linear-light/HDR. Wipes use pixel centres, a deterministic direction, exact0/1endpoints and half-open temporal intervals. These functions define original Lumeryn presets without copying Clipchamp assets or artwork.

## Backend alternatives and research

Existing FFmpeg filtering is mature and maintained and works with the configured Windows subprocess abstraction. Its atempo/setpts/tpad/xfade tools are relevant lowering candidates; atempo changes tempo, while xfade requires matching CFR inputs/resolution/pixel format/timebase. These documented constraints cannot be delegated to unvalidated project expressions. [Official maintainer filter source](https://raw.githubusercontent.com/FFmpeg/FFmpeg/master/doc/filters.texi).

An xfade-generated whole sequence can change timing if treated as concat overlap; it is not automatically our fixed-cut contract. A compiler must supply explicit source handles and normalised surfaces. CPU reference evaluation offers predictable endpoint/alpha semantics with bounded memory; GPU acceleration is deferred until equivalent pixels and total cost are measured. Rubber Band is a maintained C++ alternative with Windows support; its maintainer requires a purchased commercial licence for proprietary distribution. No purchase is authorized and no measured quality need justifies it for the initial three speeds. [Maintainer licensing/platform page](https://breakfastquay.com/rubberband/). MLT would add another editing backend without a demonstrated benefit. Neither is added.

FFmpeg is LGPL2.1-or-later in its basic configuration; optional GPL components change the applicable licence. Codec patent rights are a separate concern. Existing external executable use does not grant redistribution rights. No FFmpeg binary, model or GPL tool is bundled by this decision. [Official legal documentation](https://ffmpeg.org/legal.html).

## Evidence and limits

Seven isolated public tests passed on Windows: fractional29.97/59.94output sampling/backward seeks, source exhaustion, odd speed length, silent freeze, public arithmetic overflow, odd transition/source-handle endpoints, directional pixel/alpha goldens and strict JSON rejection. Focused locked core Clippy passed with warnings denied; module/test rustfmt passed. No full workspace acceptance is claimed for this module alone.

Actual decoded speed/freeze/transition footage, tempo tone/pulse samples, click/flash alignment, duration tails, native sidebar interactions, service/history/CLI mutation parity and end-to-end export remain integration gates. At evaluation time work is O(1), with no full frame allocation; pixel blending is O(width*height). Measured wall-clock/RAM/UHD throughput is not yet available. Workspace disk review paused further media/build fixtures. Discovery must not advertise shipped time effects before these gates pass.
