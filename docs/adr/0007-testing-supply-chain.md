# ADR 0007: Layered verification and proprietary dependency policy

Date: 2026-10-09. Status: accepted policy; staged tool adoption. Detailed compatibility/release research: [TEST_TOOLING](../research/TEST_TOOLING.md).

## Context

Lumeryn needs evidence for non-destructive edits, exact timing, persistent revision/history consistency and real media output. Compilation and snapshots alone cannot show these invariants. Development tooling must work with a stable Windows Rust workspace and commercially suitable dependencies while keeping original source proprietary.

## Decision

Keep the baseline `cargo fmt`, Clippy with warnings denied, unit/integration tests, and explicitly executed generated-media workflow. Add cargo-deny as the dependency advisory/license/bans/source gate. Do not publish proprietary workspace crates or change their license to satisfy an audit tool. Inspect external media binaries and model/assets separately.

Adopt testing mechanisms according to the risk they cover:

| Mechanism | Intended role | Adoption scope |
| --- | --- | --- |
| Explicit adversarial CLI/storage tests | Public error/output behavior, no-overwrite/no-side-effect guarantees, locked revision and schema invariants | Implemented independent suite; retain as required gate. |
| Cargo-deny | Locked dependency advisories, reviewed license alternatives and approved sources | Installed/configured; locked scan passed 2026-10-09. See [evidence](../licensing/DEPENDENCIES.md). |
| Proptest | Rational math and bounded edit-state invariants, minimized regressions | Next targeted dev-only addition; finite stable test budget. |
| Insta | Stable machine contracts, schema/capabilities snapshots with narrow redaction | Add when approved snapshots provide useful change review; semantic assertions remain. |
| Nextest | Process isolation, suite grouping and bounded job concurrency | Optional runner initially; measure Windows overhead and retain Cargo doctests. |
| Cargo-fuzz | Coverage-driven parsing/domain fault discovery | Later isolated nightly workspace; synthetic corpus and strict limits. |
| Golden audio/video checks | Actual trim content, image/audio correctness and synchronization | Baseline decode/frame/timing workflow now; expand measured content metrics before advanced effects. |

Current official package licenses are permissive: nextest/proptest/cargo-fuzz/cargo-deny offer MIT or Apache-2.0; Insta is Apache-2.0. Test tools are development-only unless explicitly distributed. Recheck their complete graph and enabled features when introducing dependencies.

## Alternatives

Using only ordinary Cargo tests is sufficient for a small runner but lacks automatic supply-chain policy and systematic input exploration. Installing every tool at once creates extra dependency/toolchain maintenance without proving additional behavior. Replacing semantic tests with snapshots could approve wrong state; replacing decoded-media checks with encoded-file hashes produces fragile comparisons. These alternatives do not satisfy the respective invariants.

## Consequences and limits

Stable Rust remains the production requirement. Nightly sanitizer tools use explicit isolated invocation; current cargo-fuzz supports native Windows with MSVC AddressSanitizer, while Linux/WSL is a practical first worker. No root default toolchain change is needed.

Randomized tests use bounded inputs and preserve failures. Snapshots require reviewed expectations and redact only nondeterministic identifiers/paths. Lossy golden media comparisons operate on decoded pixels/PCM with measured tolerances, not encoded container bytes. FFmpeg-heavy tests are resource-limited so they cannot saturate a development machine through runner parallelism.

Cargo-deny checks original third-party license expressions without licensing Lumeryn publicly. It does not replace notices, native-library audits, model-weight permissions, publisher/IP review or legal review before distribution. An advisory exception must be individually justified; a passing scan means no configured known-policy finding at that scan, not zero vulnerabilities.

## Acceptance

Evidence must name installed tool versions, exact executed commands, pass/fail/skip counts, advisory data freshness and any exceptions. Research-only tooling is labeled planned/evaluated. Existing independent CLI/storage regressions remain green after policy/tool integration; generated media still decodes with correct timeline/content observations. Changes to Cargo/deny policy require rerunning the locked dependency check and updating the register.
