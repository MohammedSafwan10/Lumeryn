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

Database graph schema1 and storage index version1 are distinct. Older storage version0 can be inspected without automatic migration; use `project backup` then `project migrate` explicitly to add FTS5. Migrations retain editing revisions/history and reject unsupported versions. `project check` validates SQLite, project/history continuity and metadata index; `--assets` also checks sources. Backup uses a consistent SQLite snapshot with integrity verification; it references rather than copies media. Cross-directory backups rebase relative references to the original assets, so a database alone is not a self-contained asset package. Relinking/missing-digest upgrades and owner-mediated corruption repair are future capabilities; never delete or repair a user's database silently.

Native Studio and its preview worker share project revision events with the CLI through the authoritative Windows service. The baseline export runs as a cancellable service job. Resource estimates, native scenes/text/vector/animations, captions, AI voice, Blender, professional mixing, proxies and multi-track effects remain staged. Do not invoke invented syntax or present unsupported features as available. External agents may plan briefs/storyboards and licensed assets now, but the creative acceptance demos require the roadmap's implemented scene/audio/preview capabilities. Verify every final render's actual visuals/audio rather than inferring artistic quality from JSON success.

`./scripts/demo.ps1` generates legally usable synthetic fixtures and executes the real baseline; it is a technical acceptance demonstration, not one of the creative product demos.

## Live revision and render control

On Windows, every CLI project operation connects to the same authoritative service as native Studio. Read live notifications using `project watch --project <path> --after-sequence <n> --wait-ms 2000`; omit the cursor for the latest snapshot. Expired/restarted event cursors resynchronize from persisted project state. Cursors are transient; editing revisions/history remain durable.

Use `render --project <path> --output <new path> --expected-revision <n> --job-id <UUID>` when external cancellation is needed. Rendering captures that committed revision and runs independently; closing Studio or interrupting its CLI request does not cancel an admitted render. Observe job events and explicitly use `jobs cancel --project <path> --job-id <UUID>`. `job_progress` carries actual stages `validating`, `encoder_probe`, `encoding`, `verification`, `publication` with the same job ID/revision; percentages are not fabricated. Cancellation cannot retract output already published; inspect job events/output before retrying. The JSON command envelope and conflict/error exit codes are retained.
