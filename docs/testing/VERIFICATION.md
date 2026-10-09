# Independent first-slice verification

Date: 2026-10-09. Scope: verification specialist's independent checks, followed by explicitly attributed lead-reported final workspace/media/supply-chain gates. The independent checks below ran in this specialist's session.

## Executed CLI checks

```powershell
rustfmt --edition 2024 crates/lumeryn-cli/tests/adversarial.rs
cargo test -p lumeryn-cli --locked --test adversarial
cargo clippy -p lumeryn-cli --locked --test adversarial -- -D warnings
```

Final rerun after parser/storage/log-envelope/preset corrections: **10 passed, 0 failed, 0 ignored**, reported test execution 0.60 seconds; targeted Clippy completed with zero warnings. All spawned CLIs have unavailable FFmpeg/ffprobe paths; these tests require no media tools or network. Environment: Windows, Rust/Cargo 1.97.1, debug test build.

| Test | Verified behavior |
| --- | --- |
| Discovery and reopening | Capabilities and basic hardware report one valid JSON stdout envelope; unknown GPU/VRAM stays explicit. A created database reports runtime SQLite **3.51.3**. `60000/2002` normalizes to `30000/1001`; independent inspect/open processes preserve exact project state and revision-zero history. |
| Existing content | Creating a project over sentinel bytes returns `already_exists` and preserves bytes; no sidecar files are created. |
| Domain validation | Empty names, odd/zero/excessive dimensions fail before file creation, with `invalid_input` JSON only on stderr and exit 2. |
| Parsing validation | Zero-denominator, zero/excessive/malformed/overflowing rates, unknown commands/subcommands/flags fail exit 2, produce `invalid_arguments` JSON only on stderr and create no files. |
| Help and version | Root/command help and version succeed with human-readable stdout, no stderr and no files. |
| Missing project | Inspect/open/history/timeline inspection and a write command fail with structured stderr without creating the project or sidecars. |
| Malformed batch | Malformed JSON, unknown fields/operations, empty and oversized batches return structured errors; reopening shows unchanged project state and history. |
| Schema correspondence | Generated draft 2020-12 batch schema rejects extra fields structurally and lists all three edit variants and required fields. Shared Rust deserialization/serialization round-trips representative commands and rejects unexpected fields. This is structural correspondence, not a complete general JSON Schema validator. |
| Presets | Landscape/portrait/square defaults and explicit dimensions override persist correctly across independent CLI reopen. |
| Maintenance | Project check/search/migrate/backup work with unavailable external media tools; search finds the project, repeat migration stays unchanged and cross-directory backup reopens the same empty project. |

Parser and domain/runtime failures now provide structured JSON stderr with stable codes. Help/version remain human-readable stdout. Success stdout is JSON for executable semantic commands; every successful semantic command in this suite also verifies a parseable UUID request ID and integer elapsed milliseconds. Parser-specific assertions independently verify the corrected envelope and channel separation.

## Read-only code review

The rational rate constructor validates positive bounded numerator/denominator before normalization; arithmetic uses u128 at the FFmpeg decimal boundary. Source/timeline range additions are checked. Clip references, track IDs, project dimensions and object-ID uniqueness are validated. SQLite writer transactions and same-transaction revision/history/receipts provide an appropriate first concurrency foundation. These are reviewed properties, not a crash-recovery experiment.

Findings sent to the lead and corrected in implementation:

1. Opening an existing database originally configured WAL before schema validation. That can persistently change an unrelated or unsupported SQLite database before rejecting it. Validate existing schema without changing journal mode first; test rejection preserves the database's bytes/journal mode.
2. `Store::create` originally accepted a supplied project with nonzero revision while storing row/history revision zero. This can create an inconsistent database through the public core API. Require a new revision-zero project or persist a deliberately defined consistent initial state.
3. `Store::execute` originally loaded only the snapshot after obtaining the writer lock. Verify database schema and revision columns in the same locked transaction to reject changes since `Store::open` and prevent overwriting inconsistent metadata.

The lead corrected all three findings. Independent re-review confirmed read-only project opening, revision-zero creation through private temporary SQLite plus checkpoint/close and no-clobber publication, and schema/revision checks inside the edit transaction.

Independent regression file: `crates/lumeryn-core/tests/storage_review.rs`.

```powershell
rustfmt --edition 2024 crates/lumeryn-core/tests/storage_review.rs
cargo test -p lumeryn-core --locked --test storage_review
cargo clippy -p lumeryn-core --locked --test storage_review -- -D warnings
```

Final rerun after migration/search/backup integration: **10 passed, 0 failed, 0 ignored**, reported execution 0.12 seconds; targeted Clippy passed without warnings. The original storage regressions independently verify:

- Read-only unrelated SQLite and unsupported schema-99 databases reject opening while preserving exact original bytes, DELETE journal mode and absence of WAL/shared-memory sidecars.
- A supplied nonzero initial revision is rejected without creating any project, temporary or sidecar file.
- External schema/revision tampering after the store opens rejects an attempted edit, leaving the snapshot, tampered metadata, history count and receipt count unchanged. Both schema and revision cases run through actual SQLite transactions.

## Migration, check, backup and metadata-search extension

The same independent storage test file now adds seven tests using actual temporary SQLite files and synthetic metadata. External media workers do not run.

| Scenario | Independent assertion |
| --- | --- |
| Legacy migration 0→1 | Explicit migration creates/populates FTS5, preserves graph schema/revision/history and returns unchanged on repeat; search works only after migration. |
| Failed migration | A colliding existing table causes migration failure; `user_version` stays zero, graph/history and sentinel data are preserved, and no partial schema objects remain. |
| Index/current history tamper | Integrity check detects a forged search-index value and forged current history snapshot; graph remains unchanged. |
| Historical continuity | Missing intermediate revisions and wrong historical snapshot revision are rejected, including corruption outside the current revision. |
| Live-WAL snapshot | Backup includes an edit still present in a live source WAL; a new Store reopens the backup with matching graph/history/integrity. Source graph and synthetic asset bytes stay unchanged; existing backup bytes cannot be overwritten. |
| Cross-directory references | Backup into another directory rebases both existing and missing relative asset references to the original absolute locations, including history snapshots. Source database bytes and graph stay unchanged; assets are referenced, not copied. |
| Bound literal search | FTS operators/quotes and SQL-like input remain literal data. Phrase matching, query/limit bounds and post-query integrity are tested. |

Review found and the lead corrected two extension defects before the final passing rerun: cross-directory snapshots originally changed the meaning of relative asset references, and integrity checking originally omitted earlier history continuity. Regression tests cover both corrections. Backups remain project snapshots with external media references; they are not self-contained media archives.

## Lead-reported final gates

The lead reported a successful final `./scripts/check.ps1 -Media -Deny` run on the unchanged 170-package graph on 2026-10-09. The script executes:

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --test workflow --test media_regressions -- --include-ignored
.lumeryn-scratch/tools/bin/cargo-deny.exe --locked check --hide-inclusion-graph
```

Formatting and all-target Clippy passed; default tests reported **25 passed**, and explicitly enabled generated-media tests reported **6 passed**. Cargo-deny **0.20.2** passed advisories, bans, licenses and sources. Its fetched RustSec database was commit `550efd3d587a29b2e2c2b21b17a440da4fede999`, dated 2026-10-08; scan date was 2026-10-09. Duplicate-version warnings for transitive cpufeatures/hashbrown/syn were retained without blanket ignores. Lockfile SHA-256: `7C5DE2AD423A2D7B3B479DC3217E1BEC126D040AA8559C043C99420DFBA2574C`. The [dependency register](../licensing/DEPENDENCIES.md) records graph/license review and remaining distribution obligations.

These full gates were executed by the lead, rather than independently rerun here. Advisory success covers configured known findings in the locked Cargo graph at the recorded database revision; external executables, native build licenses, models and media assets require separate review.

## Remaining verification scope

This specialist did not independently run media import/trim/export, visual frame inspection, read-only-source media tests, crash injection, full workspace Clippy/tests or dependency scanning; the lead's executed gates are attributed above. Studio/IPC/GPU tests, crash-injection coverage, scheduler benchmarks and broad decoded-pixel/PCM golden comparisons remain later-phase work. The separate [acceptance plan](PLAN.md) retains the broader adversarial cases and later-phase gates.
