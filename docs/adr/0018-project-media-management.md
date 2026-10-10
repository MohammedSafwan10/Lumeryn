# ADR 0018: Project-local media names and restricted reference removal

Date: 2026-10-10. Status: accepted for the CC-03 metadata slice; core implementation verified. Client and native acceptance are recorded separately by the integrated milestone.

## Context and decision

Beginners and external agents need recognizable imported-media labels and a way to remove unused project references. The original media path, bytes, imported identity and reviewed library catalogue must remain independent of presentation text. CLI and native Studio use the same typed Rust project service; neither owns a second media registry.

Add `Media.alias: Option<String>` and `Media::display_name()`. `Some` retains the exact accepted text; `None` uses the existing filename stem with underscores replaced by spaces, or `Media` when there is no stem. Aliases are nonblank after whitespace inspection, at most 256 UTF-8 bytes, and contain no Unicode control characters. Multilingual text, ordinary punctuation and duplicate labels are accepted. Full graph validation checks these rules, including snapshots created outside the command path. Paths and source metadata remain separately available and authoritative.

`RenameMedia { media_id, alias }` sets a name; a nullable `alias` value resets it. `RemoveMedia { media_id }` removes only that UUID's current project reference. Missing UUIDs fail explicitly. Referenced media fail with the current clip count and an instruction to remove their clips first. There is no cascade, implicit ripple, source/cache deletion, relinking or garbage collection. Another import of the same path or digest remains independent. A deliberate atomic batch may submit existing explicit `DeleteClip` operations before `RemoveMedia`; the whole batch is one undo step. Successful receipts include the affected media UUID. Equal-value rename commands retain the existing explicit-command revision policy; clients may avoid submitting an unchanged form.

Both operations reuse Store transactions, expected revisions, dry-run, durable receipts and snapshot history. Undo restores the same imported UUID, alias, reference and clips at a new increasing physical revision; redo reapplies the stored graph. Current search omits removed references. Clipboard version 1 remains label-independent: fresh paste requires the currently imported UUID/digest, while exact previously admitted keyed retries still return the original receipt before current-graph validation.

The Restrict-style reference rule lives in Rust because media/clip UUIDs are inside the serialized graph. SQLite foreign-key pragmas cannot inspect these JSON relationships. This is a semantic comparison with [SQLite's documented RESTRICT action](https://www.sqlite.org/foreignkeys.html#fk_actions), not a claim that normalized media foreign keys were added.

## Compatibility boundary

Keep graph schema 1 and storage version 1 for this additive optional metadata field. Use both `#[serde(default)]` and `#[serde(skip_serializing_if = "Option::is_none")]`. The [official Serde field documentation](https://serde.rs/field-attrs.html) describes absent-field defaulting and conditional serialization separately. Updated readers decode old absent-alias snapshots as `None`; omission preserves old graph JSON content when reserialized. Opening an old project does not rewrite its snapshots or history. This matters because Store integrity compares the current history snapshot against the typed graph, and the derived search records against their rebuilt representation.

This is backward compatibility for a new reader reading old projects. **Old binaries reject alias-bearing media objects and the new rename/remove history commands.** Strict unknown-field rejection remains intentional; old live clients can also reject new snapshots. Do not describe this decision as bidirectional compatibility. A backup and a binary supporting the new capabilities are needed when using these new edits. A reset omits the alias in the new current snapshot, but does not erase the new commands or alias-bearing historical snapshots, so it does not restore old-binary compatibility.

For `alias=None`, retain the exact previous FTS text (`path video_codec audio_codec`, including spaces); do not append the filename-derived display fallback. Append one separator and alias text only for `Some`. Existing transactional search rebuilding handles rename/reset/removal and undo/redo. Literal phrase quoting and bound SQL remain unchanged, including names containing quotes or FTS operator words. [SQLite FTS5 documentation](https://www.sqlite.org/fts5.html) describes index consistency responsibilities; no table, trigger or migration is introduced here.

Future composition/reference changes, mandatory aliases, storage restructuring, or a required formal old-reader barrier need an explicit owner-reviewed schema/storage migration after backup. Merely changing the schema constant would reject historical undo targets. That migration must preserve version-aware historical decoding, graph/reference meaning, logical/physical revisions and receipts. This label slice does not decide the composition schema.

## Alternatives, licensing and limits

Filesystem rename/deletion is rejected because it violates immutable source references and would affect unrelated projects. GUI-only labels are rejected because agents, cards, timelines and inspectors need one name. A separate alias database or FTS column is unnecessary for the current bounded JSON graph. A schema-2 bump with history migration is deferred to composition rather than introduced incidentally for optional labels.

No dependencies, optional tools/models, paid services or licensing obligations are added. Existing dependency notices remain applicable; SQLite is [public domain](https://www.sqlite.org/copyright.html). Lumeryn remains proprietary. No original or library media is uploaded or modified.

Graph admission retains the existing 10,000-media/10,000-clip limits and 16 MiB current-snapshot read bound. Validation/removal scan the current bounded graph; each commit serializes its snapshot and rebuilds all current FTS rows. Alias text is never a filesystem path, FFmpeg argument or cache key. There is no measured CPU/GPU backend change, persistent cache optimization or new memory guarantee.

An isolated Windows debug-profile sample on this development machine, `cargo test -p lumeryn-core --locked --test media_management bounded_media_graph_transaction_measurement -- --ignored --nocapture`, measured:

| Synthetic imported references | Snapshot JSON bytes | Rename commit | Remove commit |
|---|---:|---:|---:|
| 4 | 1,807 | 3.145 ms | 1.252 ms |
| 10,000 | 3,919,141 | 532.326 ms | 556.794 ms |

These are one-run local samples with synthetic metadata, no source probing, no clips, and no statistical latency/resource guarantee. The measurement validates the configured media bound and includes transaction/search rebuilding. A parallel-test run measured different values; the isolated values above are the retained sample. Real rich projects and production-profile benchmarks remain separate work. This evidence does not justify changing the Rust graph/backend.

## Executed core verification

`cargo fmt -p lumeryn-core --check`, `cargo clippy -p lumeryn-core --all-targets --locked -- -D warnings`, and `cargo test -p lumeryn-core --locked` passed: 42 default core tests. `cargo test -p lumeryn-core --locked --test media_management -- --include-ignored --nocapture` passed eight tests in the initial suite; the final integrated gate additionally runs the ninth strict nullable-field/schema test, including the explicit bounded measurement. Existing media-dependent export/history tests remain ignored in this focused default run and are part of the lead's integrated media gate.

Eight new default tests exercise exact multilingual/256-byte validation and strict graph serialization; missing UUIDs; dry-run/revision conflict/receipt replay and key mismatch; failed mixed-batch rollback without receipts; literal alias search/reset/undo/redo; used-source rejection and explicit delete-plus-remove; duplicate source imports and unused audio/image references; two-client revision revalidation; reopened removal/history; legacy absent-alias JSON and exact index text with untouched revision-zero bytes; same-directory and rebased backup restoration; clipboard failure/replay/undo restoration; and immutable temporary source bytes. Synthetic metadata tests deliberately execute no probing tool. Client discovery/CLI commands, actual admitted-render snapshot behavior and visual native card/timeline/inspector/cache acceptance require their own integrated evidence.

The command alias field must be present: a string sets it and explicit null resets it. A field-specific deserializer and required nullable schema reject accidental omission; persisted Media.alias stays optional for old snapshots. Missing-field/null/string/schema assertions passed in the final focused42-test core run.
