# CLI workflows and contracts

Install: `cargo install --path crates/lumeryn-cli --locked`. Development: `cargo run -p lumeryn-cli -- <arguments>`. Binary: `target/debug/lumeryn.exe` after `cargo build --locked` on Windows. Inspect `lumeryn capabilities`, `lumeryn schema`, and command-specific `--help`. `doctor` reports tool versions/config and compiled codecs/hardware APIs, explicitly not verified GPU usability. `hardware scan` reports OS/architecture/logical cores with unavailable RAM/VRAM/GPU telemetry marked unknown.

Use trusted user-installed FFmpeg/ffprobe. Configure `--ffmpeg`, `--ffprobe`, `LUMERYN_FFMPEG`, or `LUMERYN_FFPROBE`. A worker deadline is `--timeout-seconds` (1..3600, default 300). The local GPL FFmpeg8.1.1 build is external, not bundled; use generated/trusted fixtures until reviewed patching. No automatic downloads/cloud/models.

```powershell
lumeryn project create --project project.lumeryn --name "My video" --preset landscape --fps 30000/1001
# Also --preset portrait (1080x1920), square (1080x1080), or explicit even --width/--height.
lumeryn media probe --path source.mp4
lumeryn media import --project project.lumeryn --path source.mp4 --expected-revision 0
lumeryn project inspect --project project.lumeryn
lumeryn timeline clip add --project project.lumeryn --media-id MEDIA_UUID --duration-frames 120 --expected-revision 1
lumeryn timeline clip trim --project project.lumeryn --clip-id CLIP_UUID --source-in-frame 15 --duration-frames 60 --expected-revision 2
lumeryn project check --project project.lumeryn --assets
lumeryn render --project project.lumeryn --output finished.mp4 --expected-revision 3
lumeryn project open --project project.lumeryn
lumeryn project history --project project.lumeryn
lumeryn project search --project project.lumeryn --query source --limit 20
lumeryn project backup --project project.lumeryn --output backup.lumeryn
```

Edit receipts contain revision and stable object IDs; inspect retrieves the committed state. In/out ranges are half-open integer source frames, clip positions are project frames, and compatible source/project CFR is required. The same core commands serve native Studio through the authoritative service. `project open` is headless validation, not a Studio launch.

For responsive staged preparation, keep the project selected explicitly and inspect the revision first:

```powershell
lumeryn media import-start --project project.lumeryn --path source.mp4 --expected-revision 0 --match-first-video
lumeryn media import-status --project project.lumeryn --job-id JOB_UUID
# Cancel preparation; once committing begins, cancellation fails explicitly.
lumeryn jobs cancel --project project.lumeryn --job-id JOB_UUID
# Or choose an exact timeline rate before placing any video/audio clips.
lumeryn timeline fps --project project.lumeryn --fps 60000/1001 --expected-revision 0 --dry-run
```

The returned job's quick headers are unverified. Wait for `completed`, inspect the committed revision and media, then place clips; terminal errors are in the status. A concurrent edit can cause a revision conflict; inspect before retrying. `--match-first-video` atomically adopts the first verified moving video's CFR only when no moving-video media or clips exist. Static PNG does not define timeline FPS. Import + match is one undo step. Different FPS edits are rejected after clips exist; source metadata is preserved. Timeline/media inspectors label source and timeline rates separately. Rendering with no FPS override retains the exact rational timeline rate; an override changes output cadence, not source motion. Synchronous `media import` is unchanged. For keyed background import retries, provide a stable `--id` as required by CLI help.

Export defaults to the tested MPEG-4 Part2/AAC CPU preset at project resolution/rational FPS. Select actual local UHD without an account, watermark or subscription:

```powershell
lumeryn schema --kind render
lumeryn render --project project.lumeryn --output finished-4k.mp4 --expected-revision 3 --resolution uhd2160 --fps 30 --codec h264 --quality high --acceleration auto
# Optional --bitrate-kbps 25000 overrides quality rate control. CPU is the default acceleration.
```

Resolution choices are `project`, `hd720`, `fhd1080`, `uhd2160`; orientation follows the project (UHD3840×2160 landscape,2160×3840 portrait,2160×2160 square). Other aspects fit without stretching. Sources are evaluated directly at final resolution; preview quality and monitor volume never affect final rendering. FPS24/25/30/60 is optional; omit to preserve the project rational rate. Codecs `mpeg4`, `h264`, `h265` depend on usable installed encoders; H.264/H.265 CPU use x264/x265. `auto` actually probes hardware at the selected dimensions/rate and falls back to CPU on probe or encode failure, with the encoder/reason reported. Draft/balanced/high are encoder-specific quality/effort choices, not equivalent visual scores or guaranteed exact bitrate. Beyond-UHD/HDR/mastering remain staged. See [executed UHD evidence](../research/UHD_EXPORT.md).

The supported graph requires 1..64 contiguous nonoverlapping clips starting at zero on one video track, matching verified CFR, zero-origin embedded streams, square pixels and baseline SDR. Audio is stereo48kHz from embedded audio or generated silence; independent audio-track mix/transitions/effects/HDR/VFR remain unsupported. Source BLAKE3 hashes are checked before/after processing. A full decoded output check plus exact codec/frame count/rate/dimensions occurs before publishing. Existing output paths are never overwritten. Output duration rounds up once into the selected FPS, with less than one frame of last-picture/silence extension; AAC decoding can include codec-frame padding.

Read the licensed local asset catalog with bounded pages. These commands do not mutate projects:

```powershell
lumeryn assets list --offset 0 --limit 128
# Follow next_offset until null to enumerate the complete catalog.
lumeryn assets search --query camera --limit 20
lumeryn assets inspect --id ASSET_ID
lumeryn assets check
# Every assets command accepts --library <directory containing catalog.json>.
```

`inspect` verifies the selected file's SHA256 and license notice before returning its canonical path. `check` verifies all catalog files/notices. Asset catalog entries retain source/license/attribution metadata; they are not proof that every future use clears all trademark/personality rights.

The same shared revision-checked edit API supports `timeline canvas --preset landscape|portrait|square` (or both `--width`/`--height`), `timeline clip split --clip-id <id> --right-clip-id <new-id> --at-frame <absolute interior timeline frame>`, `duplicate --clip-id <id> --id <new-id>`, `delete --clip-id <id>` and `reorder --clip-id <id> --before-clip-id <id>`. All writes take explicit `--project` and revision/idempotency options. Split preserves the original left ID; duplication inserts/ripples after the source; reorder preserves ranges and repacks a contiguous sequence (omit before ID to append). Delete ripples by default; `--leave-gap` preserves later positions and makes baseline export fail on that gap. ID-generating operations require explicit new IDs with idempotency keys. Inspect the new revision after each write.

## Undo, redo and same-project clipboard

```powershell
lumeryn project history-status --project project.lumeryn
lumeryn project undo --project project.lumeryn --expected-revision 3 --dry-run
lumeryn project undo --project project.lumeryn --expected-revision 3 --idempotency-key undo-001
lumeryn project redo --project project.lumeryn --expected-revision 4
lumeryn schema --kind clipboard
$copied = lumeryn timeline clip copy --project project.lumeryn --clip-id CLIP_UUID | ConvertFrom-Json
$copied.result.clipboard | ConvertTo-Json -Depth 8 | Set-Content clipboard.json -Encoding utf8NoBOM
lumeryn timeline clip paste --project project.lumeryn --file clipboard.json --id NEW_CLIP_UUID --at-frame 120 --expected-revision 5 --dry-run
lumeryn timeline clip paste --project project.lumeryn --file clipboard.json --id NEW_CLIP_UUID --at-frame 120 --expected-revision 5 --idempotency-key paste-001
```

Undo/redo restores a logical editing state while committing a new monotonic physical revision. One ordinary atomic batch is one undo step. Revision history remains append-only; a new edit after undo discards the logical redo branch without deleting its durable records. `history-status` derives `can_undo`, `can_redo`, `current_logical_revision`, `undo_depth` and `redo_depth` from the shared storage helper. Empty undo/redo fails explicitly. These commands support the normal revision/idempotency flags and `--dry-run`; JSON `undo`/`redo` must be the sole operation in a batch. Legacy schema-1 projects remain readable; explicit graph migration is required before new undo/redo or other edits.

Copy is read-only and emits `result.clipboard`, a strict version-1 payload containing the project/media UUIDs, imported BLAKE3 identity, rational frame rate and exact source in/duration. Save that payload object, not the whole CLI envelope. Paste requires an explicit timeline `--at-frame`, a new clip ID (generated if omitted without a retry key), already-imported media and the same project/source/rate identity. Its service-side typed `PasteClip` validates against the authoritative current graph and preserves the original source range. It does not move other clips automatically: choose a contiguous append position for the current baseline renderer; overlapping/gapped export remains unsupported.

Clipboard JSON is read with a 16 KiB bound, rejects unknown fields, malformed digests, unsupported versions/rates, missing media, source-range overflow and ID collisions. It contains no source path or executable text; cross-project asset transfer remains staged. If a paste omits `--expected-revision`, the CLI captures the current revision before submitting it. Exact keyed retries bind the entire clipboard payload and return the original receipt even after later undo removes that media; changed payloads with the same key fail. Source files remain immutable references, and actual file identity is checked by project asset checks/rendering.

## Media display names and project-only removal

```powershell
lumeryn media list --project project.lumeryn
lumeryn media inspect --project project.lumeryn --media-id MEDIA_UUID
lumeryn media rename --project project.lumeryn --media-id MEDIA_UUID --name "Opening shot" --expected-revision 5 --dry-run
lumeryn media rename --project project.lumeryn --media-id MEDIA_UUID --name "Opening shot" --expected-revision 5 --idempotency-key alias-001
lumeryn media rename --project project.lumeryn --media-id MEDIA_UUID --reset --expected-revision 6
lumeryn media remove --project project.lumeryn --media-id UNUSED_MEDIA_UUID --expected-revision 7 --dry-run
lumeryn media remove --project project.lumeryn --media-id UNUSED_MEDIA_UUID --expected-revision 7 --idempotency-key remove-001
```

`media list` returns the committed revision and imported media; `media inspect` returns one entry. Entries preserve `id`, original `path` and source `metadata`, and add `alias` (null when unset), `display_name` and timeline `usage_count`. These reads do not probe source files. Display names use the alias when present, otherwise the source filename stem with underscores displayed as spaces. Source paths, IDs, byte identity and cache identity do not change when renamed. Metadata search includes aliases while retaining original paths.

Rename requires exactly one of `--name` or `--reset`. Aliases preserve accepted text exactly, including multilingual text and punctuation; blank/whitespace-only names, control characters and names longer than 256 UTF-8 bytes fail. Duplicate display names are allowed; use stable media IDs to select references. Reset clears the alias. Rename and remove support revision/idempotency flags, `--dry-run`, and shared undo/redo. The optional alias retains older serialized media meaning; new writes to schema-1 graphs require explicit migration to schema 2.

Remove changes the project reference only and never deletes source or cache files. Referenced media fails with its clip count; it never cascades to timeline clips. To remove clips and their media together, dry-run an atomic batch with explicit `delete_clip` operations before `remove_media`, then commit at the same expected revision. A failure rolls back the entire batch. JSON operations are `{"operation":"rename_media","media_id":"MEDIA_UUID","alias":"Opening shot"}` (null resets) and `{"operation":"remove_media","media_id":"MEDIA_UUID"}`. Undo can restore a removed reference, but sources remain external files and must still be available for playback/rendering.

## Independent music and voiceover

New projects use graph schema 2. For an older project, inspect/check it, create a consistent new-path backup, then explicitly `project migrate --project <path>`. Migration upgrades current and historical snapshots together without advancing the editing revision; preserve the backup if an older application build is needed. Studio's **Back up & upgrade project** performs these two operations explicitly. Opening never migrates automatically.

```powershell
lumeryn timeline inspect --project project.lumeryn
lumeryn media import --project project.lumeryn --path narration.wav --id MEDIA_UUID --expected-revision 5
lumeryn timeline audio add --project project.lumeryn --media-id MEDIA_UUID --id AUDIO_UUID --at-frame 30 --source-in-sample 0 --duration-frames 60 --gain-millidecibels -6000 --expected-revision 6 --dry-run
lumeryn timeline audio add --project project.lumeryn --media-id MEDIA_UUID --id AUDIO_UUID --at-frame 30 --source-in-sample 0 --duration-frames 60 --gain-millidecibels -6000 --expected-revision 6 --idempotency-key voice-001
lumeryn timeline audio move --project project.lumeryn --clip-id AUDIO_UUID --at-frame 15 --expected-revision 7
lumeryn timeline audio trim --project project.lumeryn --clip-id AUDIO_UUID --source-in-sample 4800 --duration-frames 45 --expected-revision 8
lumeryn timeline audio range --project project.lumeryn --clip-id AUDIO_UUID --at-frame 0 --source-in-sample 2400 --duration-frames 60 --expected-revision 9
lumeryn timeline audio properties --project project.lumeryn --clip-id AUDIO_UUID --gain-millidecibels -6000 --muted --expected-revision 10
lumeryn timeline audio delete --project project.lumeryn --clip-id AUDIO_UUID --expected-revision 11
lumeryn project undo --project project.lumeryn --expected-revision 12
```

Replace example revisions and IDs with actual inspected values. `range` updates timeline position/source offset/length together. `properties` writes both gain and mute; omit `--muted` to unmute and supply the intended gain (default 0 is unity). Source offsets count stereo **sample frames at canonical 48000 Hz**, not bytes, channel samples or source-native-rate samples. Actual decoded-length proof is acquired for supported zero-origin mono/stereo audio; video imports defer this proof until an independent audio placement needs it. An unsupported or changed source fails explicitly. Dry-run can decode/prove a source but does not persist graph changes.

Independent clips may overlap and mix with embedded source audio. They must fit inside the visual timeline. Video reorder preserves independent positions; shortening video past an audio endpoint fails. Gain bounds are -60..+12 dB. The final sum hard-clamps rather than auto-normalizing. There is one independent track and a safety cap of 64 combined audible/silent embedded-plus-independent contributions for the current mixer, not a measured playback-capacity promise. Both clients share the same plan for playback and export. Detach/fades, additional tracks, audio-only export and professional metering remain staged. JSON discovery includes all six strict audio operations. Referenced-media usage/removal counts video and independent audio.

## Atomic edits and safe retries

`lumeryn schema` generates draft2020-12 JSON Schema for `Batch` from shared Rust types; semantic validation additionally checks ranges, identities, references, rates and revision constraints. Example JSON file (replace IDs and path):

```json
{"operations":[
  {"operation":"import","id":"c6dca336-e642-46fb-b27a-19d227338e1a","path":"source.mp4"},
  {"operation":"add_clip","id":"e822d5e6-9810-4b61-b7e4-a4974696fd9b","media_id":"c6dca336-e642-46fb-b27a-19d227338e1a","at_frame":0,"source_in_frame":15,"duration_frames":60}
]}
```

```powershell
lumeryn project batch --project project.lumeryn --file edit.json --dry-run --expected-revision 0
lumeryn project batch --project project.lumeryn --file edit.json --expected-revision 0 --idempotency-key sequence-001
```

A valid batch commits once; any invalid operation rolls back the entire batch/history/index. Dry-run does not commit or reserve IDs/revisions/keys, but import probing still runs. Optional idempotency keys return the original committed receipt even when the source is later unavailable; repeat exactly the same serialized parameters and IDs. Reusing a key with changed operations fails. Single import/add requires `--id` with a key, avoiding accidentally generated new IDs on retry. Creation and output publication are create-new operations; inspect an existing result rather than retrying over it.

## Output, failures and recovery

Semantic success emits one JSON object to stdout: `ok`, request ID, elapsed milliseconds, and `result`. Errors are JSON stderr; trace JSON also stays stderr when `RUST_LOG` enables it. Help/version remain human text. Exit0 success;1 I/O/storage/existing/not-found;2 invalid input/arguments;3 stale revision (includes expected/current);4 unsupported;5 media worker/timeout;130 cancellation. On conflict inspect/replan; never blindly overwrite another edit. On cancellation inspect committed revision before retry, since a commit may have finished just before the signal.

Database graph schema2 and storage index version1 are distinct; schema1 graphs remain readable and require explicit migration before new writes. Older storage version0 can be inspected without automatic migration; use `project backup` then `project migrate` explicitly to add FTS5. Migrations retain editing revisions/history and reject unsupported versions. `project check` validates SQLite, project/history continuity and metadata index; `--assets` also checks sources. Backup uses a consistent SQLite snapshot with integrity verification; it references rather than copies media. Cross-directory backups rebase relative references to the original assets, so a database alone is not a self-contained asset package. Relinking/missing-digest upgrades and owner-mediated corruption repair are future capabilities; never delete or repair a user's database silently.

Native Studio and its preview worker share project revision events with the CLI through the authoritative Windows service. The baseline export runs as a cancellable service job. Resource estimates, native scenes/text/vector/animations, captions, AI voice, Blender, professional mixing, proxies and multi-track effects remain staged. Do not invoke invented syntax or present unsupported features as available. External agents may plan briefs/storyboards and licensed assets now, but the creative acceptance demos require the roadmap's implemented scene/audio/preview capabilities. Verify every final render's actual visuals/audio rather than inferring artistic quality from JSON success.

`./scripts/demo.ps1` generates legally usable synthetic fixtures and executes the real baseline; it is a technical acceptance demonstration, not one of the creative product demos.

## Live revision and render control

On Windows, every CLI project operation connects to the same authoritative service as native Studio. Read live notifications using `project watch --project <path> --after-sequence <n> --wait-ms 2000`; omit the cursor for the latest snapshot. Expired/restarted event cursors resynchronize from persisted project state. Cursors are transient; editing revisions/history remain durable.

Use `render --project <path> --output <new path> --expected-revision <n> --job-id <UUID>` when external cancellation is needed. Rendering captures that committed revision and runs independently; closing Studio or interrupting its CLI request does not cancel an admitted render. Observe job events and explicitly use `jobs cancel --project <path> --job-id <UUID>`. `job_progress` carries actual stages `validating`, `encoder_probe`, `encoding`, `verification`, `publication` with the same job ID/revision; percentages are not fabricated. Cancellation cannot retract output already published; inspect job events/output before retrying. The JSON command envelope and conflict/error exit codes are retained.

## Boundary insertion shared with native dragging

Use `lumeryn timeline clip insert --project <path> --expected-revision <revision> --id <new-uuid> --media-id <imported-uuid> --before-clip-id <existing-uuid> --source-in-frame 0 --duration-frames 90`. Omit `--before-clip-id` to append, including an empty sequence. Source ranges and IDs stay unchanged for existing clips; later positions ripple. Gaps/overlaps fail explicitly. JSON operation `insert_clip` has `id`, `media_id`, nullable `before_clip_id`, `source_in_frame` and `duration_frames`; use batch dry-run before larger plans. One insertion is one undo step. Studio media drops use this same operation; clip-body/linked-source-audio drags use `reorder_clip`. Independent audio placement remains staged.
