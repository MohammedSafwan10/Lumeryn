# Test tooling and supply-chain evaluation

Research date: 2026-10-09. This document separates recommended tools from installed/tested tools. Cargo manifests, `deny.toml`, `PROGRESS.md` and executed verification evidence determine actual adoption. No tool here changes Lumeryn's proprietary license or permits source publication.

## Verified current releases

Official crates.io metadata was queried for stable versions, declared MSRV and licenses. All declared MSRVs below fit installed Rust 1.97.1. Version/MSRV evidence does not substitute for building the configured graph.

| Tool | Stable release | Declared MSRV | License | Initial decision |
| --- | --- | --- | --- | --- |
| [cargo-nextest](https://crates.io/api/v1/crates/cargo-nextest) | 0.9.148 | 1.91 | Apache-2.0 OR MIT | Optional test runner; benchmark locally before replacing routine runner. |
| [insta](https://crates.io/api/v1/crates/insta) | 1.49.0 | 1.66.0 | Apache-2.0 | Suitable dev-only snapshots for stable schemas/capabilities and selected JSON contracts. |
| [proptest](https://crates.io/api/v1/crates/proptest) | 1.11.0 | 1.85 | MIT OR Apache-2.0 | Suitable dev-only property checks for timing and edit invariants. |
| [cargo-fuzz](https://crates.io/api/v1/crates/cargo-fuzz) | 0.13.2 | Not declared | MIT OR Apache-2.0 | Isolated nightly fuzz stage; production workspace stays stable. |
| [cargo-deny](https://crates.io/api/v1/crates/cargo-deny) | 0.20.2 | 1.88.0 | MIT OR Apache-2.0 | Adopt dependency license/advisory/source gate now. |

Development tools run locally and do not become Lumeryn runtime dependencies merely because they are installed. Actual test-library dependencies still need full transitive review and notices where redistributed. Insta's Apache license requires its applicable license/notices; do not mislabel it MIT.

## Nextest

Nextest runs tests in separate processes and offers grouping, timeouts and detailed reporting. Windows x86-64 prebuilt binaries are officially provided and digitally signed. Source installation requires `--locked`. [Installation](https://nexte.st/docs/installation/pre-built-binaries/), [source builds](https://nexte.st/docs/installation/from-source/).

Use a fixed installed version and a repository profile with bounded timeouts, no automatic failure-hiding retries, and a resource group limiting FFmpeg-heavy tests. Doctests still need a separate `cargo test --doc` step. Windows process creation overhead can make a small suite slower, so record representative measurements before claiming a speedup. [Running tests](https://nexte.st/docs/running/), [Windows limitations](https://nexte.st/docs/installation/windows/).

Current alternative: retain ordinary `cargo test --workspace --locked`. It already runs the small suite and doctests without another required executable. Nextest becomes more valuable as independent media/job tests grow; do not weaken coverage when switching runners.

## Insta snapshots

Use narrowly scoped JSON snapshots for generated batch schema, capabilities and stable error envelopes. Normalize UUIDs and machine-specific fixture paths while retaining revision numbers, frame ranges, error codes and supported-operation names. These values express correctness and must not be redacted. Insta supports structured redaction and LF normalization. [Redactions](https://insta.rs/docs/redactions/), [snapshot files](https://insta.rs/docs/snapshot-files/).

Review snapshot diffs against intended behavior before accepting. Do not use automatic acceptance in verification/CI. Snapshot schemas cannot replace parser/domain/transaction assertions: a new incorrect snapshot is still incorrect. Prefer JSON-only features rather than pulling optional serializers into the graph. An alternative for the current small contract is the explicit structured assertions already in independent adversarial tests.

## Proptest

Recommended properties: rate normalization preserves rational identity; normalized round trips preserve exact values; frame-to-time conversion stays monotonic; checked source/timeline bounds never panic; rejected edit sequences preserve revision/history; accepted atomic batches increment once. Use bounded ranges and small projects, with an independently stated mathematical/state oracle rather than a copy of the production formula.

Start with a finite case budget for the stable Windows gate; record seeds and commit minimized regression cases. Include manual zero/overflow tests because randomized generation does not ensure hitting exact boundaries. Failure persistence replays known inputs on subsequent runs. [Introduction](https://proptest-rs.github.io/proptest/intro.html), [failure persistence](https://proptest-rs.github.io/proptest/proptest/failure-persistence.html).

Alternative: deterministic handpicked/bounded-loop cases, already used in the first slice. Proptest is justified when arithmetic/state combinations expand; do not add enormous media processes to every generated case.

## Cargo-fuzz and platform constraints

Cargo-fuzz drives libFuzzer and needs a C++ compiler plus a nightly Rust toolchain for sanitizer instrumentation. Keep it in an isolated `fuzz/` workspace with a pinned nightly, its own lockfile and strict input/time/memory limits. Do not switch the user's default toolchain or the production workspace to nightly. [Rust Fuzz Book](https://rust-fuzz.github.io/book/cargo-fuzz.html), [requirements](https://rust-fuzz.github.io/book/cargo-fuzz/setup.html).

Native Windows is supported by current official instructions through MSVC C++ AddressSanitizer, with the sanitizer DLL/tool binaries discoverable in the developer shell. Linux/WSL is a practical primary fuzz worker because sanitizer setup is simpler, not the only supported platform. Native Windows fuzz runs should be added after the local compiler/sanitizer combination is verified. [Windows setup](https://rust-fuzz.github.io/book/cargo-fuzz/windows/setup.html).

Initial targets should parse bounded batch/project JSON and frame rates, then validate without executing external tools or project scripts. Fuzz corpus contains synthetic data only; retain minimized crashes as stable regression tests. Do not fuzz arbitrary customer footage or run untrusted subprocess paths. A bounded smoke run is evidence of that run, not proof of no vulnerabilities. Nightly tooling and corpus expansion are a later milestone, not an obstacle to the stable CLI slice.

## Golden audio/video tests

Use deterministic synthetic media with changing frame patterns and known tones, selected source ranges and explicit color/pixel formats. Verify source hashes, timeline frame count, source-in selection, actual decode, output timestamps and audio presence. These baseline checks should precede expansion of render features.

For lossy output compare decoded frames and decoded PCM using documented tolerances rather than hashing encoded MP4 bytes. Candidate metrics include selected-frame pixel error or PSNR/SSIM, waveform correlation/RMS, expected tone frequency and bounded audio/video alignment. Set thresholds from known-good reference measurements and fail on wrong-frame/wrong-tone fixtures. FFmpeg provides image/audio comparison and analysis filters. [Official filters](https://ffmpeg.org/ffmpeg-filters.html).

Golden assets should be tiny and generated from checked-in parameters; no private/customer media or large binaries. Snapshot/runner/property tools do not replace media-content assertions. Full golden comparisons, color-managed output and GPU equivalence remain separate capabilities requiring their own fixtures and measurements.

## Cargo-deny policy

Run all four checks: advisories, licenses, bans and sources, with the committed lockfile. Preserve private workspace packages through `publish = false` and `licenses.private.ignore = true`, while checking third-party runtime/build/dev dependencies. Allow reviewed permissive licenses, including needed Unicode/Zlib obligations; unlisted copyleft licenses remain denied unless separately reviewed. Dual-license alternatives must be selected correctly. Do not assign MIT to Lumeryn merely to satisfy the tool. [License policy](https://embarkstudios.github.io/cargo-deny/checks/licenses/cfg.html).

Use current RustSec data, reject vulnerabilities and unknown Git/registry sources, and document any individual advisory exception with a reason and review/expiry. Avoid obsolete example fields because current cargo-deny rejects several former configuration keys. Advisory database fetching downloads public security metadata and does not publish Lumeryn source. Offline scans must report database age. [Advisories](https://embarkstudios.github.io/cargo-deny/checks/advisories/cfg.html), [sources](https://embarkstudios.github.io/cargo-deny/checks/sources/cfg.html).

This gate complements the manual [dependency register](../licensing/DEPENDENCIES.md). It cannot inspect all licenses of external FFmpeg builds, codec patents, model weights, fonts or embedded native components automatically. It also does not establish freedom from vulnerabilities absent an advisory.

## Evidence and rollout

Independent CLI and storage regression tests have executed; their exact scope and the lead-reported final workspace/media gates are in [VERIFICATION.md](../testing/VERIFICATION.md). The lead installed/configured cargo-deny 0.20.2 and its final locked advisories/license/bans/source scan passed on 2026-10-09; graph hash, advisory freshness and limitations are recorded in the [dependency register](../licensing/DEPENDENCIES.md). Nextest, Insta, Proptest and cargo-fuzz are evaluated recommendations until explicit implementation/run evidence is recorded. Keep tests passing on stable Windows; add bounded property/snapshot coverage next, then isolated fuzzing and broader golden fixtures as the supported editing model grows.
