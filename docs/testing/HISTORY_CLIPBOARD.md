# Durable history and clip clipboard acceptance

Verification date: 2026-10-09. Scope: CC-03 shared Rust undo/redo and same-project clipboard, two actual clients of the secure Windows project service, and an export taken from an undone snapshot. These checks do not certify physical keyboard/OS clipboard interaction or complete the other composition features.

## Semantics and independent review

`Edit::Undo {}` and `Edit::Redo {}` must be the only operation in their batch. Ordinary atomic batches form one logical undo step; undo/redo restore a validated historical snapshot as a **new increasing physical revision**. New edits after undo discard the logical redo branch while keeping all durable history rows. Expected revision, idempotency receipts, dry-run rollback, metadata search rebuild and service commit events use the existing transaction path. No new tables, graph schema change or destructive history rewrite is required.

The transaction reads current state, reconstructs the logical cursor and selects the target inside the same writer transaction. Target snapshots are bounded, validated and checked against their original project ID/revision before restoration. The restored project receives the next revision and its search index/history/receipt are committed together. Existing source files are never rewritten or deleted by undo. A deterministic idempotent replay returns its original receipt even after subsequent undo; it does not apply the operation again or rewind current state.

`clipboard::Clipboard` version 1 is a bounded 16 KiB payload containing project ID, imported media ID, BLAKE3 source identity, rational frame rate and exact source-in/duration frames. It contains no source paths, scripts, cloud credentials or runtime. Copy and strict parsing are read-only. Authoritative `Edit::PasteClip { id, at_frame, clipboard }` binds the full payload to the transaction and idempotency key; the core reuses validated clip creation. Cross-project transfer, absent media, mismatched imported identity/rate, invalid ranges/IDs and unknown fields fail explicitly. A new paste allocates a new clip ID while retaining the original imported source ID and range. File identity refers to verified import metadata; preview/export perform their existing actual source-change checks.

SQLite documents a single simultaneous writer and the writer admission of `BEGIN IMMEDIATE`, while a read transaction observes a stable database snapshot. This supports the existing sole-writer service transaction design rather than introducing a second history writer. [SQLite transactions](https://www.sqlite.org/lang_transaction.html). Consistent backup continues using `VACUUM INTO` and private publication; SQLite describes it as producing a consistent snapshot of the original database. [SQLite VACUUM INTO](https://www.sqlite.org/lang_vacuum.html).

## Independently executed service tests

Owned file: `crates/lumeryn-service/tests/undo_live.rs`. Final focused command:

```powershell
cargo test --locked -p lumeryn-service --test undo_live -- --include-ignored --nocapture --test-threads=1
cargo clippy --locked -p lumeryn-service --test undo_live -- -D warnings
rustfmt --edition 2024 --check crates/lumeryn-service/tests/undo_live.rs
```

All five tests passed, including the explicitly enabled media test; final test execution 2.22s excluding compilation. Targeted strict Clippy and file formatting passed. Four tests belong to the ordinary gate; one is ignored by default and requires trusted user-installed FFmpeg/ffprobe. Root owns the full workspace/media/cargo-deny and native acceptance gate.

The lead's complete `./scripts/check.ps1 -Media -Deny` run is now confirmed by `.lumeryn-scratch/history-clipboard-gate.log`: formatting and locked all-target Clippy passed; 82 default tests plus 23 additionally enabled media tests give 105 distinct passing tests. Some default tests run again in the media groups and are counted once. Cargo-deny ended with `advisories ok, bans ok, licenses ok, sources ok`, retaining the reviewed existing exceptions and transitive platform-duplicate warnings. This is inspected full-gate evidence from the lead's run, separate from the independent focused executions above.

| Test | Actual assertions |
|---|---|
| Concurrent live undo/redo | Two real named-pipe clients race undo at revision 1: exactly one commits revision 2 and the other gets `Conflict { expected: 1, current: 2 }`. Dry-run preserves state/history/event sequence. Exact keyed retry returns the old receipt without another event. Redo, undo and a branch edit emit increasing revisions/sequences; branch redo and compound undo batches fail atomically. |
| Durable reconnect and backup | Service restarts with the same logical state. Undo/redo remain possible after restart. Cross-directory live backup keeps schema/storage 1, checks integrity, rebases historical external source references consistently and supports redo independently; original revision stays unchanged by backup. |
| Corrupted restoration target | A separate SQLite connection deliberately replaces revision 0's snapshot with a different project identity. Undo rejects it, leaving revision 1, trim range, history count and live event sequence unchanged. |
| Authoritative clipboard paste | Copy/parse preserve project state. A stale two-client paste conflicts; a forged digest is rejected. Valid paste retains exact source range/ID with a new clip ID. Undo removes that new clip; exact keyed retry remains a replay and does not recreate it; redo restores the same committed clip ID. |
| Pre-Undo schema 1 compatibility | An independently constructed SQLite fixture uses the old project/history/receipts/FTS layout and only pre-Undo trim records. Initial integrity passes, then inspect/undo/check/backup pass without migration or new tables. Backup storage version remains 1. This is a constructed compatibility fixture, not a claim to have reopened every historical customer file. |
| Historical render and cancellation | Generated 640x360 CFR footage is trimmed, undone and exported at revision 2/1800 frames while another client redoes the trim at revision 3/600 frames. Export remains pinned to revision 2 and passes full decoded verification. A subsequent UHD job cancels with the correct job/revision event, no output publication and unchanged project/history/source BLAKE3. |

The first two rows are exercised by one combined service test, giving five total tests rather than six. Fixtures are disposable generated/constructed local data. No customer media, external service, charge or upload was used.

## Other specialist evidence

The core specialist reports nine focused history tests passing, including imported-source removal after several undos, exact paste replay after that removal, logical branch invalidation, two-Store conflicts, corruption/integrity and backup rebasing. The CLI/clipboard specialist reports two core clipboard unit tests and two actual CLI workflow tests passing (one default plus one explicit generated-media test), including strict version/unknown fields, 16 KiB input bound, clipboard payload key collisions, dry-run and replay after imported media is undone. These are specialist-reported evidence, distinct from the independently executed service tests above. Reproduce CLI workflow using:

```powershell
cargo test --locked -p lumeryn-cli --test history_workflow -- --include-ignored --nocapture --test-threads=1
```

## Executed native history and UHD workflow

The lead's real native-window run passed all 17 checks in `.lumeryn-scratch/studio-203c4b1779ea48fabcf5bcb3e7913b7a/studio-smoke.json`. The report has no error, confirms history/clipboard acceptance, and records actual GPU-presented frames after each increasing revision:

| Revision | Same Studio command path | Actual state |
|---:|---|---|
| 6 |Undo|1 clip, 59 frames|
| 7 |Redo|1 clip, 60 frames|
| 8 |Paste clip|2 clips, 120 frames|
| 9 |Undo paste|1 clip, 60 frames|
| 10 |Redo paste|2 clips, 120 frames|
| 11 |Delete pasted clip|1 clip, 60 frames|
| 12 |Undo delete|2 clips, 120 frames|
| 13 |Redo delete|1 clip, 60 frames|

The same window also passed load, playback, exact seek, frame step, Studio trim and a real external CLI edit before those steps. It exported revision 13 in the background with 237 UI updates during export. The shared renderer reported full decoded verification; `uhd-file-verified.json` independently records H.264, 120 frames at 60 FPS, 3840x2160, two seconds, zero-origin stereo 48 kHz AAC. Actual encoder: `h264_nvenc` on the RTX 3050/Vulkan host, with no fallback reason. The synthetic 720p fixture was conventionally upscaled; this run does not establish genuine source-detail recovery or long-program/physical A/V accuracy. Export/read-only service tests above cover revision-pinned rendering independently.

Recorded UI update CPU p50/p95 was 1.55/3.55ms; those are debug update timings, not display FPS or physical input latency. The window report records a 1920x1009 logical viewport at 100% DPI. Report SHA256: `181f67bb29fd0d93e536459657477167437a1dd089d163aed6584bb2e5474e53`. The lead owns retained screenshot/binary/source provenance and final milestone documentation.

The harness calls `Session::undo`, `redo`, `paste_clip_at_end` and the ordinary edit path; it constructs the copied payload through the shared core. It does **not** simulate physical Ctrl+Z/Y/C/V or prove OS clipboard read/write. Native copy-menu/keyboard operation, text-field shortcut precedence, project-switch paste and pointer behavior remain manual acceptance. The historical report's generic limitations array still says undo/redo are staged; its 17 successful checks contradict that stale wording. The source/report wording issue was reported to the lead, and that string is not used as a capability assertion here.

## Performance bounds and remaining acceptance

Undo reconstruction currently scans ordered command metadata, then loads one target snapshot. It checks 100,000 records/64 MiB command data before materializing that reconstruction; individual snapshots have a 16 MiB bound. This is O(history) work inside writer admission. The core specialist's reported 10,000-record metadata status derivation was about 40ms on this host; it is not an independent writer-lock/IPC latency or sustained-project memory benchmark.

The existing full `Store::history()` API and integrity/backup snapshot scan remain linear and use complete result collections. The IPC frame bound is 8 MiB, so a very large history response may hit that envelope limit before the reconstruction command quota. No constant-time undo, unbounded history support or large-project response-time guarantee is claimed. Benchmark representative snapshot/command sizes, writer admission latency and memory before selecting pagination, checkpoints or indexed logical history; that requires a separately tested migration rather than a speculative rewrite in this slice.

Physical native keyboard shortcuts, OS clipboard integration, text-field shortcut precedence, selection when the selected clip itself disappears, project-switch clipboard behavior and manual cancellation remain untested. Root's live UI implementation continues taking expected revision from its current snapshot and treating history-status availability as advisory; the service remains authoritative. Restoring references to missing/changed source files is not restoration of their bytes. Preview/export should show their existing explicit source failures; no history operation may remove or reconstruct original media.

Dependencies, external FFmpeg ownership and asset licences are unchanged by these semantics. No public licence is applied to proprietary Lumeryn, and no executable/model/assets are newly bundled by this history milestone.
