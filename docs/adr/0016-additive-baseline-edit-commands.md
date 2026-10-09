# ADR 0016 — Transactional baseline edits without rewriting projects

Date: 2026-10-09. Status: accepted for the existing single-track graph.

The reference editor requires basic clip operations and canvas choices. Existing schema 1 stores stable IDs, source ranges and integer timeline positions; these operations need no new snapshot fields or destructive migration.

Add shared typed SplitClip, DuplicateClip, DeleteClip, ReorderClip and SetCanvas commands. An interior split retains the left ID and creates an explicitly supplied right ID, preserving half-open source ranges. Duplication inserts after its source and repositions clips through checked arithmetic. Ripple deletion and reordering require a contiguous sequence beginning at zero; they reject gaps/overlaps rather than silently changing unsupported layouts. Non-ripple deletion preserves later positions; existing preview/export reject unsupported gaps. Reorder uses a stable target ID or appends to the end. Canvas edits use validated even dimensions and preserve project FPS.

All commands execute through the existing Rust service and Store transaction, with expected revisions, dry-run, idempotent receipts, immutable sources and one new committed revision per batch. Existing schema-1 snapshots and history remain readable. CLI and Studio share semantics; schema discovery comes from the Rust types.

Five Store tests cover cut boundaries/source preservation, duplicate/reorder/ripple behavior, rollback/collisions/missing IDs, dry-run/conflicts/replay/reopen, seven canvas ratios and overflow. Drag placement, snapping, multi-track evaluation and undo need subsequent parity tests. A GUI-only undo graph was rejected because external-agent changes must remain authoritative.
