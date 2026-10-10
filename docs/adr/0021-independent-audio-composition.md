# ADR 0021: Independent audio through the shared Rust graph

Date: 2026-10-10. Status: accepted implementation; executed acceptance is recorded separately.

## Decision

Extend the authoritative project graph to schema 2 with one stable independent audio-track UUID and typed AudioClip objects. Each clip stores stable source/track IDs, integer project-frame position/duration, canonical 48 kHz sample-frame source offset, integer millidecibel gain and mute. Visual duration remains authoritative. Overlap mixes; video rearrangement keeps independent sound fixed. Operations which would leave audio outside the shortened visual range fail atomically. Audio-only projects/export, detach/link groups, fades, speed and additional track topology remain later slices.

Expose AddAudioClip, MoveAudioClip, TrimAudioClip, SetAudioRange, SetAudioProperties and DeleteAudioClip through the existing batch/revision/idempotency/history service. SetAudioRange validates a position/source-range change together, avoiding invalid intermediate states. Both Studio and `timeline audio` CLI commands use these semantics; no Studio database writer or second project graph exists. Gain is -60000..12000 millidecibels; zero means unity. Required fields and unknown-field rejection remain strict. Independent source references count toward media-removal protection and integrity checks.

Before independent placement, verify actual decoded stereo 48 kHz sample frames using the same FFmpeg conversion contract used for playback/export. A sample frame contains both channels. Container duration is insufficient source-range evidence. Audio-only mono/stereo zero-origin import streams and counts decoded bytes without retaining the whole PCM output. Imported video avoids an eager full audio decode; AddAudioClip can lazily prove an existing source outside the service writer lock. Bind source ID/path/digest/size to the transactional candidate after preparation. Native first placement uses proven audio sources; unproven sources receive an explicit reimport instruction. Delayed, unknown-origin and multichannel inputs are not silently guessed or mixed under unsupported semantics.

For an absolute frame boundary f, B(f)=floor(f*fps.denominator*48000/fps.numerator). A clip occupies B(end)-B(start) samples. Validate the checked source endpoint against the proof. Moving at a fractional frame rate can change the required length by one sample; the service rejects any resulting overrun. Decode/resample to canonical stereo float, sample trim, reset timestamps, apply gain/mute, sample delay, mix with normalization/dropout gain changes disabled, hard-clamp to [-1,1], pad silence and trim the exact final duration. Preview and export consume the same headless AudioPlan. Device sample rate/channel conversion follows the canonical mix; monitor volume remains preview-only. Existing projects with no independent audio retain the previous embedded-audio execution path.

FFmpeg's documented [atrim](https://ffmpeg.org/ffmpeg-filters.html#atrim), [adelay](https://ffmpeg.org/ffmpeg-filters.html#adelay), [amix](https://ffmpeg.org/ffmpeg-filters.html#amix) and [aresample](https://ffmpeg.org/ffmpeg-filters.html#aresample) provide the adapter primitives. Only generated labels/indices/numeric values enter filter programs; filesystem paths remain separate process arguments. Existing bounded workers, cancellation, protocol restrictions and source checks remain in force. No new native decoder/mixer dependency or unsafe optimization is introduced.

## Explicit migration and compatibility

Graph schema 2 and SQLite storage/index version 1 are separate. Opening/checking/exporting/backing up schema 1 remains read-only compatible. New non-replayed edits require explicit `project migrate`. Studio offers **Back up & upgrade project**, producing a new consistent backup before migration; opening never upgrades automatically. Older builds cannot edit schema 2. Preserve the backup when rollback to an old binary is needed.

Migration runs in one BEGIN IMMEDIATE transaction. Allocate one collision-free audio-track ID across all historical project/media/video IDs, upgrade current and every historical snapshot with that same ID, retain revisions/video/source IDs, retain command/receipt bytes, validate undo targets and rebuild FTS. No decoded count is fabricated. Reject malformed/mixed/noncontiguous history and roll back the complete migration. Bounds are 100000 history records, 128 MiB snapshot text and 64 MiB command text; larger histories need staged migration. Same-revision schema changes notify clients and refresh Studio snapshots/cancel gestures.

## Native interaction

Retain the selected mint B style and native creation/property rails. Audio 1 displays decoded waveforms and a drop target; imported audio places at an integer frame, clip bodies move with their grabbed offset, right edges trim, and the Inspector edits position/source range/length/gain/mute atomically. One successful primary release commits once. Focus loss, Escape, overlays, edit-busy states and changed project/revision/epoch cancel gestures. Linked source audio still moves with its video and is distinct from independent music/voiceover. A double-click/Add action provides an alternative to dragging.

## Alternatives, limits and licensing

Reuse reviewed FFmpeg decoding and CPAL device output. Adding MLT, libav bindings, Rubato or another audio engine would add licensing/FFI/maintenance work without a measured benefit for this slice. Exact FFmpeg build distribution and codec obligations remain under [official legal guidance](https://ffmpeg.org/legal.html); availability of installed GPL-enabled encoders is not redistribution approval. Lumeryn remains proprietary. Catalogue sounds retain their reviewed CC0 provenance/notices.

The 64 independent-clip validation cap and 64 combined embedded-plus-independent contribution cap are safety bounds, not a measured professional playback capacity. Export permits at most 128 total source inputs; repeated clips currently decode repeated inputs. Seeking can decode the preceding mix before trimming, preserving sample parity at a known computational cost. The PCM ring stays bounded; this does not establish long-program latency, zero-copy GPU media or physical speaker/display synchronization. Hard clamping is deterministic, not a professional limiter or loudness meter. One painted independent lane supports overlap mixing but does not provide all professional multi-track interaction.

## Acceptance

Core graph/migration/probe, actual PCM/AAC/UHD output, CLI/service/live-watch, native RawInput gestures and real-window workflow are required. See [audio acceptance](../testing/AUDIO_COMPOSITION.md), [integration review](../research/AUDIO_INTEGRATION_REVIEW.md) and historical [interaction research](../research/DRAG_AUDIO_INTERACTIONS.md). Do not infer physical pointer or speaker/display timing from scripted actions, device status or a process launch.
