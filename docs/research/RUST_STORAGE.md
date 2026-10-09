# Rust and persistent project research

Research date: 2026-10-09. Status: recommendation; build and runtime verification are separate evidence.

The inspected Windows environment has Rust 1.97.1 and Cargo 1.97.1. The current stable SQLx 0.9.0 requires Rust 1.94.0, so it fits this environment. Use a pinned Cargo workspace with `rust-version = "1.94"` or higher, commit `Cargo.lock`, and test on the declared minimum before claiming MSRV support. [SQLx package metadata](https://crates.io/api/v1/crates/sqlx), [SQLx release](https://github.com/transact-rs/sqlx/releases/tag/v0.9.0).

## Selection

Use SQLx with `default-features = false` and SQLite/Tokio features only. Avoid PostgreSQL/MySQL drivers, TLS, dynamic extension loading and SQL macros until needed. Literal SQL plus bound values keeps the first schema auditable without a build-time database; SQLx 0.9's `SqlSafeStr` accepts static literals by default. [Maintainer release explanation](https://github.com/transact-rs/sqlx/discussions/4271).

Use a single small SQLite project database and stable UUIDs. The first workflow should persist assets, clips, project settings, monotonic revisions and command history. Immutable media remains outside the database. Long-running FFmpeg work must occur outside edit transactions. A later service can own a writer connection while CLI/Studio use the same typed commands.

| Choice | Reason and alternative |
| --- | --- |
| SQLx SQLite | Matches preferred stack, async orchestration and future service. `rusqlite` is a good synchronous alternative, but introduces another persistence implementation unless fully substituted. |
| Local filesystem only | SQLite WAL uses shared memory and is unsuitable for project databases on network filesystems. Copy/export portable snapshots instead. |
| WAL + FULL | Readers can coexist with one writer; FULL preserves commit durability better than NORMAL during power failure. Actual disk guarantees still depend on the OS/device. |
| Foreign keys ON | Apply at every connection; reject dangling assets/clips. |
| Bounded busy timeout | Return a typed busy error after a finite wait. Avoid unlimited retries. |
| BEGIN IMMEDIATE | Acquire the writer before reading revision/state; avoid read-to-write snapshot escalation. Use SQLx's RAII transaction through `begin_with("BEGIN IMMEDIATE")`. |

These SQLite operating properties are documented in [WAL](https://www.sqlite.org/wal.html), [transactions](https://www.sqlite.org/lang_transaction.html) and [SQLx connection options](https://docs.rs/sqlx/0.9.0/sqlx/sqlite/struct.SqliteConnectOptions.html). They are architecture constraints, not measured Lumeryn throughput.

## Important SQLite compatibility finding

SQLx 0.9.0 permits `libsqlite3-sys >=0.30.1,<0.38.0`. Highest compatible 0.37.0 embeds SQLite **3.51.3**, verified from its published header. Pin/lock that binding with bundled SQLite rather than inheriting an unknown machine DLL. SQLite's WAL-reset race affects older releases through 3.51.2; 3.51.3 contains the fix. [SQLx SQLite dependency metadata](https://crates.io/api/v1/crates/sqlx-sqlite/0.9.0/dependencies), [published header](https://docs.rs/crate/libsqlite3-sys/0.37.0/source/sqlite3/sqlite3.h), [official issue explanation](https://www.sqlite.org/wal.html#walreset).

The bundled C build needs a compatible Windows C toolchain. Confirm native compilation, then query `sqlite_version()` in integration tests; accepting an older system SQLite is not a fallback. The fallback for toolchain trouble is to configure a verified native toolchain, not silently remove persistence safety.

## Transaction and revision contract

1. Validate typed arguments and resolve asset paths before obtaining the writer.
2. Start `BEGIN IMMEDIATE` and read schema version and current revision.
3. Compare supplied expected revision. A mismatch returns a stable revision-conflict error without state/history changes.
4. Validate operation against current state, apply its rows, and increment revision with a conditional update.
5. Insert history describing the validated operation and resulting revision in the same transaction.
6. Commit and only then report success. Failed operations roll back both edit and revision.

This is an optimistic client contract implemented within SQLite's serialized writer, not a substitute for SQLite locking. Every successful edit increments once; failed edits never increment. A later command batch needs one transaction and an explicit batch revision policy.

Inspection should use a consistent read transaction when assembling project/asset/clip snapshots. Render/export must capture that snapshot before spawning workers and report the snapshot revision; do not hold SQLite read transactions for an entire encode. [SQLx transaction API](https://docs.rs/sqlx/0.9.0/sqlx/trait.Connection.html#method.begin_with), [SQLite isolation](https://www.sqlite.org/isolation.html).

## Recovery and schema trust

Reject unknown/newer schema versions before executing schema-dependent edits. Creation must never overwrite an existing project. Migrations require explicit versioned transactions and fixtures before production support is claimed. Keep WAL sidecars with a live database; use SQLite backup/snapshot facilities rather than arbitrary copy while writers are active. Future crash tests must kill a writer in a disposable fixture, reopen and verify integrity plus atomic history. [SQLite backup API](https://www.sqlite.org/backup.html).

Treat IDs, paths, metadata and existing database content as untrusted. No SQL concatenation with user data, extension loading or project-provided code. SQLite constraints complement domain validation. Read-only media import/export must succeed without changing source hashes; `immutable(true)` is inappropriate for a live synchronizing project.

## Validation and remaining work

Required before first milestone: compile the selected dependency graph; Rust formatting, Clippy and tests; reopen persistent edits; revision rollback/conflict tests; query actual bundled SQLite version; FFmpeg export round trip. Planned later: crash injection, schema migration fixtures, read/write clients with long-reader checkpoint pressure, durability testing and timed representative workloads. No scheduler, IPC, preview or performance claim follows from the dependency selection alone.
