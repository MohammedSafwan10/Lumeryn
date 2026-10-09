# Native project service verification

Date: 2026-10-09. Windows, stable Rust/Cargo 1.97.1. The service crate has no egui/window dependency; Windows APIs are isolated in its transport module. CLI and Studio clients share the same canonical project endpoint and authoritative Store. No TCP listener, shell invocation, public repository or upload is involved.

## Implemented contract

`Client::connect(path)` starts a hidden current/sibling `lumeryn.exe service run --project` when needed. `Client::existing(path)` connects only to an already managed service. Clients are cloneable and each call opens an independent pipe; long watches and render requests can overlap edits. Typed requests cover create, inspect/history, execute, check, migrate, backup, metadata search, render, cancel and watch. The shared request schema derives from the actual Rust request types.

Protocol version 1 uses a little-endian 32-bit length followed by bounded JSON (1 byte–8 MiB), a UUID correlation ID, strict payload validation and structured core-error codes. An unsupported protocol fails explicitly. Read/write transport deadlines are five seconds; watch waits are capped at 30 seconds. At most 32 active connection workers, two import preparations, 16 queued/running render jobs and one actual render are admitted. Small edits bypass import admission. Store mutation remains serialized; import probing and source hash checks happen outside the state lock. Rendering captures a committed snapshot/revision before releasing the lock and invoking media workers.

Current-user SID is read from the process token. A protected explicit DACL allows only that SID; remote pipe clients are rejected and first-instance creation prevents a second authoritative endpoint. Client security quality of service uses Tokio's identification-only default. The endpoint derives from user SID and canonical project path. Event replay retains 256 in-memory events; expired or restarted cursors resynchronize from a current persisted snapshot. Event sequence is transient and distinct from durable project revision. Watch always includes the current snapshot if the project exists. Events describe commits, migration and render lifecycle/cancellation.

The service exits after ten idle seconds when no connection/job/watch worker remains. Windows launch uses `CreateProcessW` with handle inheritance **FALSE** and `CREATE_NO_WINDOW`; no parent's capture pipes survive in the service. This fixed a measured regression where captured CLI calls waited about 10.1 seconds for idle service exit despite fast request completion. After rebuilding, captured PowerShell create measured 0.630 seconds cold (JSON elapsed 84 ms), inspect 0.0457 seconds (JSON elapsed 28 ms). These individual debug measurements are regression evidence, not release latency guarantees. [Microsoft process creation](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw).

## Executed tests

```powershell
cargo clippy -p lumeryn-service --locked --all-targets -- -D warnings
cargo test -p lumeryn-service --locked
cargo test -p lumeryn-service --locked -- --include-ignored --nocapture
```

The independent service run executed **3 unit/security tests and 3 ordinary IPC integration tests**, all passing, with one media test ignored by default. Explicit generated-media execution passed that additional test, for **7 passed, 0 failed**. Initial integration suite execution was 1.75 seconds; concurrent edit plus inspect during active rendering measured **11.8829 ms**. After the process-launch correction, all seven tests passed again (integration execution **1.87 seconds**) and targeted all-target Clippy passed with warnings denied.

The existing CLI media regression suite was also rerun unchanged after rebuilding the CLI: `cargo test -p lumeryn-cli --locked --test media_regressions -- --include-ignored` reported **4 passed, 0 failed, 0 ignored**, test execution **1.76 seconds**. This includes the malformed-import-under-five-seconds assertion that exposed the inherited-handle regression; it was not relaxed. Build-lock waiting/compilation consumed separate time and is not counted as test execution.

New CLI contract tests in `crates/lumeryn-cli/tests/live_service.rs` passed targeted Clippy and `cargo test -p lumeryn-cli --locked --test live_service -- --include-ignored`: **2 passed, 0 failed**, execution **1.33 seconds**. The ordinary test validates captured create/watch latency below five seconds, current snapshot/cursor replay and JSON-only `not_found` stderr for unknown cancellation. The explicitly enabled media test launches the actual CLI with `render --job-id`, waits for job events, kills only that CLI request process, then cancels the still-running service-owned job from another CLI. It verifies `job_cancelled`, no output publication and unchanged authoritative revision. Default execution ignores this generated-media test; the lead's media gate must enable it explicitly.

| Check | Actual assertion |
| --- | --- |
| Bounded framing | Valid JSON round-trips; zero, oversized and u32-max lengths fail before allocating payload; extra fields in payload-free commands are rejected. |
| Version/correlation | Unsupported version returns structured `unsupported` and preserves UUID correlation. |
| Kernel security | Actual created pipe has a protected DACL with exactly one allow ACE matching the current user SID; a second first-instance creation is denied. |
| Concurrent clients | Two actual named-pipe clients edit the same revision; exactly one succeeds and one receives expected/current conflict. Snapshot/history are consistent. |
| Retry/live events | Idempotent retry replays without extra commit event/history; a concurrent watcher receives the committed revision. |
| Recovery | 260 commits exceed bounded replay; expired cursor requests resync. Restart retains durable revision 260 and returns its persisted snapshot. |
| Validation | Missing/invalid creation leaves no database; duplicate create never replaces existing state; invalid watch/tool bounds leave revision/history unchanged. |
| Background render | Real generated CFR media exports a fully decoded 1800-frame revision-zero snapshot while another client edits authoritative revision one. Read/edit calls remain responsive while the render task is still running. |
| Cancellation | Active second render accepts cancellation, produces `job_cancelled`, and publishes no output. Media child uses the existing core's kill-on-drop lifetime. |

## Evidence limits

Windows user-ACL structure and same-user IPC are executed tests; different-account and actual remote-network connection attempts are not exercised here. Admin/debug privileges and a malicious process running as the same user are outside this account isolation boundary. Clients must treat the local user as trusted. Future Linux/macOS transports currently return unsupported, rather than opening an unauthenticated network fallback.

Event buffers are bounded but not persisted; durable history plus snapshot performs recovery. Migration, SQLite integrity checks and backups currently hold the Store lock for their SQLite work; large-project durations remain unbenchmarked. Render cancellation is cooperative across the async job and direct media child; a cancellation after no-clobber publication cannot retract a completed export. CLI process disconnection does not automatically cancel a headless render; callers can cancel by explicit job ID. Resource admission is a conservative concurrency limit, not hardware-aware CPU/GPU telemetry or a completed scheduler.

Native window responsiveness/live GUI smoke belongs to the Studio test report. This service evidence does not establish full product migration, zero-copy preview, GPU interop, secure updater or distribution compliance. Full workspace formatting/Clippy/tests and complete supply-chain gates are coordinated by the lead.

## Official implementation references and licensing

Tokio 1.53.2 remains the locked async runtime; transport enables existing `net`/`sync` features. windows-sys 0.61.2 is MIT OR Apache-2.0 and was already in the resolved graph; select MIT and retain upstream notices. Its direct Win32 features expose token/security/process APIs, confined to service transport. No native media integration was added by this service. [Tokio server options](https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/struct.ServerOptions.html), [Microsoft pipe security](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights), [security descriptor conversion](https://learn.microsoft.com/en-us/windows/win32/api/sddl/nf-sddl-convertstringsecuritydescriptortosecuritydescriptorw), [windows-sys published package](https://crates.io/crates/windows-sys/0.61.2).
