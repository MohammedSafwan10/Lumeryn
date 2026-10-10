# ADR 0022 — staged verified imports and explicit timeline FPS

Date: 2026-10-10. Status: accepted; executed evidence is recorded in IMPORT_BASELINE and NATIVE_IMPORT_FPS after integrated verification.

## Decision

Keep the existing Rust graph, SQLite transactions, service, media tools and native renderer. Add a small service-owned import job alongside existing render jobs; do not introduce another database, scheduler framework, backend or dependency. CLI synchronous import remains supported.

`media import-start` returns a job immediately. Native Studio creates provisional filename cards before awaiting that response. A single job admits 1–32 sources, prepares them outside the authoritative writer lock, and commits all sources and aliases in one revision. Two preparation permits and a 16-job admission limit bound resource use; 32 retained status records support reconnects. Watch events and explicit import-status expose queued, inspection, hashing, validation, commit and terminal stages. Pending headers have a distinct advisory type without verified frame count/CFR and cannot enter the project or be dragged onto the timeline. Existing committed clips remain editable. Thumbnails and waveform preparation continue through the existing bounded presentation worker after verified publication.

Cancellation drops preparation futures and kill-on-drop tool processes. It cannot roll back an already-started transaction; late requests fail explicitly. Jobs retain the submitted expected revision. Another edit during preparation produces a conflict instead of silently rebasing. Native Retry submits against the latest observed revision; cached proofs avoid repeated decoding when current source identity still matches. A multi-file failure publishes no partial import. Jobs and cache are ephemeral: completed project receipts persist, but unfinished imports do not survive service process termination.

Cache at most 16 non-deserializable, source-bound verification proofs. A cache hit requires fresh complete BLAKE3 hashes and size, plus the same configured and resolved tool identity/policy. Binary identity includes path/size/modification time; replace or update tools with a service restart for a clean policy boundary. Metadata from before the first hash is advisory; authoritative headers are re-read inside the hash window. Existing pre/post hash checks do not protect against a hostile changed-and-restored race. No mtime-only source trust or database blobs.

Candidate CFR validation now counts frames during the existing every-frame timestamp pass, removing a redundant full decode. Non-candidate rates retain complete counting and remain non-CFR; empty or corrupt output fails or follows the explicit fallback. Output-size, dimension, stream, timeout and process bounds remain. See measured fixture scope and correctness limits in [IMPORT_BASELINE](../research/IMPORT_BASELINE.md).

Add strict shared `SetFrameRate` and exact-rational CLI `timeline fps`. A different rate is allowed only before video/audio clips are placed; identical rational rates remain valid. It uses ordinary revision checks, transactions, dry runs, idempotency and undo. No migration or automatic retiming of existing clips.

Offer Match first video for an empty project with no moving-video media and no placed clips. It uses the first verified moving video's exact CFR, excluding static PNGs, and commits Import + FPS in the same undo step. Native new revision-zero projects default to matching; reopening a previously edited empty project requires selecting the checkbox, avoiding an implicit override of an earlier manual choice. CLI matching is explicit per request. Source FPS, timeline FPS and export FPS are separate values. Preview quality does not change export FPS. Export omitting its FPS override preserves the exact timeline rational rate; requested output conversion can duplicate/drop frames, not invent source motion. Mixed-rate clip placement and motion interpolation remain unsupported.

## Alternatives and obligations

Moving the old synchronous call onto a UI thread helper alone would improve apparent responsiveness but retain redundant decoding and coarse status. Trusting stream headers or file timestamps would remove validation guarantees. A durable distributed job system, persistent global cache and speculative hardware decode introduce overhead without this measured need; schedule them only when evidence warrants it.

[Official FFprobe options](https://ffmpeg.org/ffprobe.html#Main-options) separate stream inspection, frame output and frame counting. [Official FFmpeg fps filter](https://ffmpeg.org/ffmpeg-filters.html#fps) documents output duplication/dropping. Neither establishes Lumeryn performance or export correctness; generated acceptance does. Existing Rust dependency audits and external FFmpeg distribution/codec obligations remain unchanged. No library is added, source media is never modified and Lumeryn stays proprietary.
