# Drag placement and independent audio: implementation contract

Research date: 2026-10-10. Inspected baseline: `a2cf04e`. This is a source inspection and proposed contract, not implemented audio acceptance. No Rust, project database, media or dependency changes were made by this research. The active owner request asks for actual media dragging, clip rearrangement and independently draggable audio in the existing native Studio.

The more extensive [composition foundation](COMPOSITION_FOUNDATION.md) remains applicable. This document narrows the first audio delivery and identifies concrete boundaries so specialists can implement without a second editing authority.

## Interaction observed in the reference

Microsoft documents importing audio into My media, then either dragging it to the timeline or pressing its add button. Audio objects can subsequently move, split and trim; the properties panel changes their volume. Lumeryn should retain a visible add alternative and contextual properties beside drag controls. Source import and timeline placement are distinct actions. [Microsoft audio workflow](https://support.microsoft.com/en-us/clipchamp/how-to-add-audio-or-music-to-videos), [editing workflow](https://support.microsoft.com/en-us/clipchamp/how-to-edit-a-video-in-clipchamp).

Reference layering means independently placed sound contributions can overlap. Lumeryn's existing blue Source audio row is only a view of embedded sound tied to the visual clips. Making that waveform draggable without graph, preview and export changes would promise a capability the engine cannot execute. [Microsoft timeline behavior](https://support.microsoft.com/en-us/clipchamp/how-to-work-with-the-timeline-in-clipchamp).

For the existing contiguous visual lane, video insertion should show a boundary and commit one shared InsertClip or atomic AddClip/Reorder batch on pointer release. Existing clip rearrangement uses authoritative ReorderClip. Intermediate pointer movement paints a ghost; it never sends repeated database writes. Capture project identity, revision and source identity at drag start; stale releases fail or cancel. Trimming handles and dragging a clip body must have separate hit regions. Escape, project switching and dropping outside a valid lane cancel without edits. Independent sound uses absolute placement rather than the visual insertion boundary policy.

## Actual source boundaries

| Existing file | Audited limitation | Required change |
| --- | --- | --- |
| `crates/lumeryn-core/src/model.rs` | Project schema1 contains one video-track ID and video Clips; validate_clip explicitly rejects audio-only placement. RemoveMedia checks visual references. | Add a narrow typed AudioClip collection, stable audio-track identity, strict validation and audio commands. Include audio references in removal protection and unique-ID checks. |
| `crates/lumeryn-core/src/media.rs` | Audio metadata records codec/rate/channels/origin, with no source length evidence. | Add verified canonical sample-frame count and its format/probe contract. Count actual decoded samples with bounded streaming rather than inferring duration from bytes or container bitrate. |
| `crates/lumeryn-core/src/storage.rs` | checked_project insists on the current graph constant; historical_project directly parses Project. Store.migrate begins inspect and only migrates storage indexes0→1. | Explicit version-aware graph migration must convert current and all historical snapshots transactionally. Keep commands/receipts byte-for-byte. |
| `crates/lumeryn-core/src/render.rs` | Video and its embedded audio are trimmed and concatenated in paired segments; independent contributions are impossible. | Preserve verified video adapter; replace paired-audio semantics with the shared audio plan. Final export maps the same canonical mix used by preview. |
| `crates/lumeryn-preview/src/plan.rs` | Private Plan validates supported video segments and immutable source hashes independently of export. | Attach the headless core plan's audio contributions and total samples; avoid a separate preview interpretation. |
| `crates/lumeryn-preview/src/audio.rs` | Timeline Source becomes sequential embedded-audio Input objects. Each launches an individual FFmpeg decoder; audition is a separate bounded source path. | Timeline launches the shared audio program for the desired seek range, then adapts to device rate/channels. Keep audition separate. Keep bounded PCM queue and callback behavior. |
| `crates/lumeryn-service/src/server.rs` | Migrate already emits storage_migrated with an unchanged project revision. | Preserve sole-writer migration and emit an explicit graph/schema invalidation event or resync marker. |
| `apps/lumeryn-studio/src/backend.rs` | Watch snapshots are sent only on changed revision or batch.resync. | Force snapshot refresh on migration/schema invalidation even if physical revision is unchanged; cancel stale preview/drag state. |
| `crates/lumeryn-cli/src/main.rs` | Video commands and discovery report schema1/audio placement staged. | Discover and expose only implemented audio commands/limits; schema discovery must describe their exact sample/time units. |
| Native `app.rs`, `timeline.rs`, `assets.rs` | Selection/editing and waveform display assume visual Clip IDs. Library can audition/import sound but cannot place it. | Typed selection and payload target; separate music/audio lane(s), source waveforms, move/trim/properties, ordinary library import→placement flow. |

Tests and fixture constructors using Project/Audio literals also need explicit new fields or versioned helpers. Shared service receipts must return newly created audio IDs and affected stable IDs. Backup rebase/search/integrity paths must include the new graph consistently.

## Smallest coherent first audio slice

Retain one contiguous visual lane, with up to16 independent mono/stereo sound contributions as a conservative provisional playback limit. This limit is a proposed safety budget, not a measured maximum. Overlapping sound is legal. Visual duration remains authoritative in this narrow slice; sound beyond it produces an actionable error to shorten/move sound or extend video. No MP4 without a visual sequence is promised. Audio-only export, multiple visual layers, detach/link groups, fades and speed can follow independently tested changes; no decorative disabled controls are required now.

Proposed additive schema2 fields:

```text
Project.audio_track_id: stable UUID
Project.audio_clips: Vec<AudioClip>
AudioClip: id, media_id, track_id, at_frame, source_in_sample,
           duration_frames, gain_millidecibels, muted
Audio.verified_canonical_samples: optional u64 evidence
```

Source sample coordinates refer to decoded stereo48kHz sample frames after one fixed conversion policy. A sample frame contains both channel values; the count is not interleaved scalar samples. Canonical coordinates simplify deterministic seeks and source ranges for the first implementation. Decoder/resampler configuration must match count validation, playback and export. Legacy sources retain absent evidence and cannot silently acquire fabricated counts during migration; actual audio placement requires verified inspection/reimport. Consider storing the decoding contract version alongside the count if changing converter policy later would alter bounds.

Use an integer millidecibel gain with reviewed bounds, for example −60000..12000, avoiding NaN or infinite persisted floats. Unity is0 and muted is a distinct boolean. Do not treat zero gain in decibels as silence. Apply a documented deterministic final hard clamp of float sums to −1..1; report clipping rather than silently normalizing every sound as another contribution ends. Monitor volume is still outside persisted mixing.

Commands are AddAudioClip, MoveAudioClip, TrimAudioClip, DeleteAudioClip and SetAudioGain/SetAudioMute (or one tightly typed settings command). Source bounds, globally unique IDs, declared audio-track ID, checked frame arithmetic and immutable-media evidence validate before commit. All use existing Batch/revision/idempotency/undo paths. Native dragging emits a single MoveAudioClip on release; right-edge trim emits one TrimAudioClip. GUI coordinates are proposals; authoritative Rust validation determines legality.

Video shuffle/reorder does not move independent sound. A ripple video deletion that would shorten the visual sequence below an audio endpoint fails atomically unless the same explicit batch trims/removes the sound. Define that interaction before adding controls; silently discarding music is unacceptable. Source audio remains embedded at unity initially. Detach is separately implemented as one transaction creating an AudioClip and suppressing its original embedded contribution, otherwise it doubles sound.

## One headless audio plan

Add `crates/lumeryn-core/src/composition.rs` (or narrowly `audio_plan.rs`) exported from lib.rs. It owns supported graph evaluation, canonical boundaries, source contributions, source identity and allowlisted adapter generation. Preview and export consume it rather than constructing separate meaning from AudioClip fields.

For a project frame boundary f, at48kHz:

```text
B(f) = floor(f × fps.denominator × 48000 / fps.numerator)
start = B(at_frame)
length = B(at_frame + duration_frames) − B(at_frame)
source_end = source_in_sample + length
```

Use checked u128 arithmetic and bounded conversion. Validate source_end against actual verified canonical source samples. Mapping absolute endpoints prevents cumulative rounding drift at30000/1001 and24000/1001. Splitting audio must partition the exact source range, so a right child starts at the original source start plus the original absolute left sample length. Moving a clip at fractional FPS can change its one-sample required length; choose explicit validation, never read beyond verified source bounds.

FFmpeg supports sample-count atrim, exact-sample adelay with the S suffix, resampling and float mixing. atrim does not reset timestamps. amix defaults include normalization and a dropout transition; explicitly disable those for stable additive gains. Those primitives support the proposed adapter but do not establish Lumeryn parity. [atrim](https://ffmpeg.org/ffmpeg-filters.html#atrim), [adelay](https://ffmpeg.org/ffmpeg-filters.html#adelay), [amix](https://ffmpeg.org/ffmpeg-filters.html#amix), [aresample](https://ffmpeg.org/ffmpeg-filters.html#aresample).

Generated program shape: input decode→fixed resample/stereo float→sample trim→timestamp reset→gain/mute→sample delay→stable-order mix with normalization disabled→explicit clamp→silence pad→exact final sample trim. A finite silence bed may simplify empty/silent cases. The same generator accepts an adapter namespace/input offset to coexist with video export inputs. Project strings never become arbitrary filter instructions; filenames remain argument-array values.

For the first correct random seek, evaluate the same canonical mix then trim to [B(seek),B(total)), reset timestamps and adapt to CPAL device format. This can decode unnecessary earlier source audio and is a known performance cost; optimize source seeking only after sequential-versus-seek sample comparison proves parity. Never place seek flags before decoding merely for apparent speed when priming/resampler states differ. Stream bounded chunks; do not materialize a ten-minute230MB stereo float mix in memory. Keep source pre/post hash validation, cancellation generations, worker diagnostics, quarter-second PCM ring and no I/O/allocation in the device callback.

## Explicit migration and regression gates

Changing only SCHEMA_VERSION or adding Serde defaults is insufficient. The current graph column, every history snapshot, inspect, integrity, undo/redo, backup/rebase and service capture must agree. Recommend eager all-snapshot conversion in one checked transaction, using a strict legacy DTO and deterministic audio-track ID derived from the project ID without adding an unreviewed dependency. Retain all source/media/video IDs, logical/physical revisions, original commands and receipts. Empty audio collection, unity embedded audio and absent unprobed source count preserve old meaning. A consistent new-path backup precedes explicit owner-invoked migration; ordinary opening never mutates the database.

Required migration tests: old project/history migration and reopen; undo/redo across migration; old receipt replay after undo; current and historical reference rebasing; malformed late historical row rolls back the entire conversion; unknown/future schema rejects; unchanged-revision migration refreshes connected Studio and cancels drag payloads.

Required sound tests: generated mono/stereo tones/impulses/silence; zero/nonzero placement, nonzero source offset, overlap summation, unity/reduced gain/mute/clamp, 44.1→48kHz, exact decoded sample counts, short clips and rational frame rates. Lossless canonical PCM comparisons precede AAC tolerance checks. Compare seek PCM to the same sequential range. Test video rearrangement preserving independent placement, delete shortening conflicts, source replacement, dry-run, atomic failures and undo/redo. Real native drag/release tests and live CLI changes must prove one committed revision per successful drop, zero on cancellation/conflict and preview updates. Physical speaker/display synchronization remains a separately measured device check.

Final decoded export validates dimensions/FPS/frame count/duration plus independent-audio marker placement and actual heard/seen correctness. Existing video-only UHD acceptance cannot certify new audio composition. Required fmt/strict Clippy/locked tests/full media+deny gates and actual-window evidence remain unchanged.

## Dependency and licensing decision

This slice needs no new mixer/decoder dependency: use existing FFmpeg workers, CPAL output and reviewed Rust planning/types. A direct libav/MLT/Rubato integration is unnecessary until a measured bottleneck justifies it. FFmpeg distribution remains dependent on exact enabled libraries/build flags and codec rights; the installed GPL-enabled tool is not bundled or permission to distribute it. [Official FFmpeg legal guidance](https://ffmpeg.org/legal.html). Reviewed CC0 catalogue sound can be placed under its existing notices; never download or redistribute Clipchamp stock merely because it is playable in that application.

No benchmark or actual audio render was performed for this document. The canonical coordinate choice,16-contribution budget, gain bounds and migration implementation are proposals needing an ADR and tests before capability discovery advertises them.

## Isolated canonical planning implementation

A subsequent owner-authorized foundation task added `crates/lumeryn-core/src/audio_plan.rs`, without changing Project, storage, service, preview, export or GUI. Its validated data inputs are independent of the current graph: absolute local source paths/digests and verified canonical sample counts, plus scheduled frame/sample ranges, bounded integer gain and mute. It supports at most64 sources/contributions (a validation cap, not measured playback capacity). All paths stay in argument-array values; only generated indices/labels/numbers enter filters. The exported canonical conversion constant ensures source-count inspection and later decoding use the same policy. Source checks remain explicit before/after adapter execution.

An ignored scratch harness includes the source module without editing lib.rs. Rustfmt check and `cargo clippy --offline --locked --manifest-path .lumeryn-scratch/audio-plan-harness/Cargo.toml --target-dir target/audio-plan-harness --all-targets -- -D warnings` passed. `cargo test` with the same manifest/target/offline/locked flags and `-- --include-ignored` executed5tests:3unit and2actual configured-FFmpeg tests; all passed. The final fixture execution took0.72seconds; this is a small test duration, not a performance benchmark.

Actual tests decoded and counted canonical samples before planning; verified overlap, nonzero placement/source trim, negative gain/mute, deterministic positive/negative hard clamp, contribution dropout without gain pumping, exact silence padding, unchanged source hashes and changed-source rejection. A44100Hz tone was resampled to48000Hz and scheduled at30000/1001; sought PCM equalled the sequential slice byte-for-byte. Unit coverage includes10000-boundary rational partitioning, bad range/gain/source/cap/seek inputs and filename characters excluded from generated filters.

This does not implement an editable independent-audio project, migration, GUI dragging, speaker/display synchronization, AAC output acceptance, large-project playback or source-count streaming for long assets. Root integration and the complete workspace/native/media gates remain required before shipping the new capability. The existing installed application remains unchanged by this isolated source module.
