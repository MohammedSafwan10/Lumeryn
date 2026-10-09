# First CLI vertical-slice verification plan

Status: acceptance plan, not results. Research date: 2026-10-09. Record actual command outputs, fixture sizes, tool versions and hardware in the milestone report. Commands below are gates; concrete CLI syntax must match the implemented `--help` and CLI reference.

## Rust gate

Run from workspace root, sequentially, after final code integration:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
```

Unit tests cover rational frame math, boundaries/overflow, domain validation and serialization. Storage integration tests exercise real temporary SQLite files, constraints, revision history and reopening. CLI tests invoke the compiled executable and parse machine output. A compiling executable alone does not satisfy acceptance.

## Generated-media round trip

Generate a legally usable small synthetic video locally using the installed FFmpeg, at fixed size/frame rate with distinct frames and a deterministic audio tone. Record source SHA-256, probe metadata and FFmpeg build flags. Work only inside a disposable project fixture.

| ID | Scenario | Observable acceptance |
| --- | --- | --- |
| V01 | Create | Database exists; schema, stable project ID, frame rate, resolution and initial revision are inspectable; no source files are created unexpectedly. |
| V02 | Import | Probe reports true stream metadata; stored asset ID/path match; source hash, length and modification time remain unchanged. |
| V03 | Inspect | JSON contains supported stable fields and parseable structured values; diagnostics do not corrupt JSON stdout. |
| V04 | Add clip | Source range maps to exact rational frame positions; valid track/asset relation persists; revision increases once. |
| V05 | Trim | Source-in and duration update deterministically; invalid/negative/empty/out-of-bounds ranges fail without edit/history/revision changes. |
| V06 | Export | Actual output file has expected video dimensions, frame rate and frames; all frames decode with `ffmpeg -v error -i output -f null -`; present audio is probed/decoded. |
| V07 | Reopen | Fresh CLI process returns the same IDs, asset metadata, clip ranges, settings and revision; it can export the persisted timeline. |
| V08 | Inspect image | Decode representative output frames and inspect content; source selection and trim correspond to expected generated frames. Duration tolerance is justified by frame/sample/container quantization. |

Use FFprobe JSON for metadata and frame count, not file extension or encoder exit status alone. Export output must be nonempty, playable and timed correctly. A constant-color fixture is insufficient to detect incorrect trim selection; use a changing pattern or frame numbers. No public sample downloads are needed.

## Adversarial cases

| ID | Scenario | Required assertion |
| --- | --- | --- |
| A01 | Stale expected revision | Stable conflict error and nonzero exit; edit, history and revision unchanged. |
| A02 | Two clients submit the same expected revision | Exactly one mutation commits; loser gets conflict or documented bounded busy error; no torn state; history/revision agree. |
| A03 | Source marked read-only | Import/edit/export succeed with source hash unchanged. Restore only fixture attributes during cleanup. |
| A04 | Asset deleted or moved after import | Metadata inspection can identify missing media; export fails clearly; project rows persist; no false success or partial final output. |
| A05 | Unsupported schema/version | Existing database is rejected; schema/version and rows are not rewritten. |
| A06 | Corrupted database / non-database file | Error response remains structured and project file is not overwritten. |
| A07 | Existing project target | Create refuses overwrite; existing bytes remain identical. |
| A08 | Existing export target | Export refuses overwrite by default; original bytes identical. Any future force flag is explicit and tested separately. |
| A09 | Output equals source/project/database sidecar | Reject before spawning encoder; hashes and persisted state unchanged. Also test canonical equivalent paths. |
| A10 | Filenames with spaces, Unicode and shell punctuation | Argument-array subprocess execution works; punctuation remains data, no shell command runs. |
| A11 | Missing tool, unsupported/corrupt media | Clear tool/probe/media error; no imported partial row or incremented revision. |
| A12 | Invalid object IDs or unsupported overlap/gap/effect | Typed rejection before export/mutation; no advertised unsupported behavior silently rendered. |
| A13 | Rational timing | Exercise 30000/1001 frame rate and nonzero source-in; checked integer/rational conversion preserves expected frame count and avoids cumulative float drift. |
| A14 | Extreme integers and malformed fractions | Reject denominator zero, negative duration and checked-arithmetic overflow; no panic. |
| A15 | Interrupted encode / process failure | Kill only fixture worker; no valid-looking final file remains; retry can create final output; owned temporary files cleaned. |
| A16 | Interrupted DB write | Disposable crash fixture reopens with pre-commit or fully committed operation; integrity check passes; history agrees with revision. |
| A17 | Resource limits | Bound external process duration, stderr/stdout capture and threads; oversized probe/project input fails without excessive allocations. |

A01, A03-A05, A07-A11 and A13-A14 should be exercised in the first delivery. Others may remain recorded follow-up work, but any untested safety behavior must be explicitly identified. Tests should verify public invariants, not duplicate implementation logic.

## Limitations and future phases

The first supported render subset needs a precise contract for tracks, overlap, gaps, audio and frame-rate conversion. Unsupported states must fail explicitly. Preview/Studio synchronization, migrations, service IPC, GPU render equivalence, scheduler stress, cancellation and comprehensive fuzzing remain later gates until implemented.

Keep golden tolerances tied to encoder/color/pixel differences rather than arbitrary broad thresholds. Add multi-clip audio/video synchronization and trim-boundary golden frames before expanding render features. Measure startup/export performance using recorded actual hardware; do not extrapolate professional-editor parity from a tiny fixture.

## Results record

Store final executed gates, pass/fail counts, explicit skipped cases, FFmpeg/SQLite versions, round-trip probe data and known limitations in `PROGRESS.md` and milestone evidence. Test failure is a blocker to claiming that behavior works; unavailable test dependencies are reported, not silently skipped as success.
