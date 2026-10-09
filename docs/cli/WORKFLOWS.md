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

Import/add/trim receipts contain revision and stable object IDs; inspect retrieves the committed state. In/out ranges are half-open integer source frames, clip positions are project frames, and compatible source/project CFR is required. The same core commands will serve Studio after the authenticated service is implemented. `project open` is headless validation, not a Studio launch.

Export is the tested baseline MPEG-4 Part2/AAC MP4, not H.264. It requires 1..64 contiguous nonoverlapping clips starting at zero on one video track, matching verified CFR, zero-origin embedded streams, square pixels and baseline SDR. Audio is stereo48kHz from embedded audio or generated silence; no independent audio-track mix/transitions/effects/HDR/VFR support is claimed. Source BLAKE3 hashes are checked before/after processing. A full decoded output check plus exact frame count/rate/dimensions occurs before publishing. Existing output paths are never overwritten. This narrow preset does not establish professional-format compatibility.

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

Preview, jobs, resource estimates, native scenes/text/vector/animations, captions, AI voice, Blender, Studio and live events are discoverable as planned. Do not invoke invented syntax or present unsupported features as available. External agents may plan briefs/storyboards and licensed assets now, but the creative acceptance demos require the roadmap's implemented scene/audio/preview capabilities. Verify every final render's actual visuals/audio rather than inferring artistic quality from JSON success.

`./scripts/demo.ps1` generates legally usable synthetic fixtures and executes the real baseline; it is a technical acceptance demonstration, not one of the creative product demos.

