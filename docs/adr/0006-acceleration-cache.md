# ADR 0006: Verified acceleration and content-derived cache

Date: 2026-10-09. Status: architecture accepted; implement BLAKE3 source identity now. Hardware selection, derived cache, watchers and application benchmark service are staged capabilities and require executable evidence before activation.

## Context

Lumeryn needs hardware-aware media processing and efficient cache reuse without sacrificing source preservation, determinism or CPU-only editing. FFmpeg's compiled accelerator list does not establish runtime capability. File size/mtime and watcher notifications do not reliably identify unchanged source bytes; the first renderer's size-only check could accept unrelated same-size media. CLI and Studio must share these decisions through the Rust core.

## Decision

Persist full BLAKE3 identities for imported media and reject changed bytes before rendering. Check stability across probing and rendering before committing metadata or publishing output. Use algorithm/schema versions; migrate or explicitly reject old identity-less records. Pin the officially verified candidate `blake3 = "=1.8.7"` and select Apache-2.0 among its licensing alternatives, with dependency build/audit gates. Default single-threaded streaming and scheduler-controlled blocking work avoid large allocations and uncontrolled hashing parallelism. Immutable input snapshots or tested OS guards remain necessary for hostile concurrent modifications.

Use a shared media capability registry distinguishing detected adapter, compiled backend, successful initialization and verified operation configuration. Choose decode/transfer/composite/encode stages independently. Hardware-required requests fail explicitly when unavailable; auto fallback preserves the requested export contract and records the actual backend. CPU editing/export remains available for implemented presets. No automatic codec substitution or assumption that wgpu implies NVENC/QSV/AMF support.

Use a versioned BLAKE3-addressed DAG for derived artifacts. Keys include content/recipe inputs and every behavior-changing parameter/version; unrelated revisions and file paths do not invalidate reusable source artifacts. Validate cache files; publish atomically; quota/pin/eviction policies apply only to owned derived files. Cleanup cannot escape the verified cache root or affect source/project/export paths. Selective compression follows application measurements, with bounded decode sizes.

Use notify-rs as an optional responsiveness aid, backed by reconciliation and mandatory digest validation at job boundaries. Events mark references/dependencies dirty rather than implicitly replacing imported media or generating user edits. Polling covers unsupported/unreliable notification environments. These events enter the same Rust service consumed by CLI and Studio.

Benchmarks are opt-in Lumeryn operations using generated/trusted fixtures and monotonic stage timings. Store correctness, selected backend, quality settings, memory/telemetry limitations and cold/warm-cache conditions with every result. Do not import vendor charts or standalone smoke numbers as Lumeryn performance evidence.

## Alternatives and tradeoffs

| Alternative | Reason not selected as the sole strategy |
| --- | --- |
| Path + size + mtime identity | Can miss same-size replacement and preserved timestamps; useful hints, not authoritative identity. |
| SHA-256 content identity | Suitable alternative; BLAKE3 selected for official Rust streaming implementation and future content-addressed workload, with local performance still to be measured. |
| Watchers alone | Can miss/coalesce events or fail across filesystems/restarts; do not provide correctness. |
| Cache keyed by whole project revision | Simple but discards unaffected expensive artifacts after local edits. |
| Always prefer GPU | Backend may be unavailable/unsupported; transfers or quality requirements may make software preferable. |
| Compress every artifact | Encoded video often benefits little; decompression and memory expansion can harm interactive latency. |

## Consequences and validation

Full identity reads add I/O to import/export but close the observed size-only gap. Content-based sharing improves locality while DAG/index/lease recovery adds implementation work. Hardware/driver/vendor SDK licensing and codec patents need separate review; nothing changes Lumeryn's proprietary status. Source identity integration must pass same-size replacement and source-stability tests plus Rust gates; cache, watcher and GPU behavior have separate acceptance gates.

See [current research and exact references](../research/ACCELERATION_CACHE.md) for official NVIDIA/Intel/AMD/FFmpeg documentation, local compiled-component observations, BLAKE3 version/license sources, notify reliability, quotas, cleanup security, compression alternatives and the detailed benchmark contract. No hardware acceleration, watcher or cache functionality is claimed by this decision alone.
