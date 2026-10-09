# Proprietary ownership and dependency register

Research date: 2026-10-09. Lumeryn original source is proprietary. Third-party license choices below do not license Lumeryn source to the public. NEXDARK LABS is the intended publisher brand; registration is pending. No remote, package publication or distribution is authorized by this register.

## Initial Rust selection

The following current stable versions and declared MSRVs were queried from official crates.io metadata. They fit installed Rust 1.97.1. This table is a direct-dependency recommendation; the committed manifests and lockfile are authoritative for what is actually used.

| Component | Version | Declared minimum Rust | Declared license | Source |
| --- | --- | --- | --- | --- |
| SQLx | 0.9.0 | 1.94.0 | MIT OR Apache-2.0 | [package](https://crates.io/api/v1/crates/sqlx) |
| libsqlite3-sys | 0.37.0 | Not declared | MIT | [package](https://crates.io/api/v1/crates/libsqlite3-sys/0.37.0) |
| SQLite bundled core | 3.51.3 | C toolchain | Public domain | [copyright](https://www.sqlite.org/copyright.html) |
| Clap | 4.6.7 | 1.85 | MIT OR Apache-2.0 | [package](https://crates.io/api/v1/crates/clap) |
| Serde | 1.0.229 | 1.56 | MIT OR Apache-2.0 | [package](https://crates.io/api/v1/crates/serde) |
| serde_json | 1.0.151 | 1.71 | MIT OR Apache-2.0 | [package](https://crates.io/api/v1/crates/serde_json) |
| Tokio | 1.53.2 | 1.71 | MIT | [package](https://crates.io/api/v1/crates/tokio) |
| UUID | 1.27.0 | 1.89.0 | Apache-2.0 OR MIT | [package](https://crates.io/api/v1/crates/uuid) |
| thiserror | 2.0.21 | 1.77 | MIT OR Apache-2.0 | [package](https://crates.io/api/v1/crates/thiserror) |
| tracing | 0.1.44 | 1.65.0 | MIT | [package](https://crates.io/api/v1/crates/tracing) |
| tracing-subscriber | 0.3.23 | 1.65.0 | MIT | [package](https://crates.io/api/v1/crates/tracing-subscriber) |
| tempfile (tests) | 3.27.0 | 1.63 | MIT OR Apache-2.0 | [package](https://crates.io/api/v1/crates/tempfile) |
| Schemars | 1.2.2 | 1.74 | MIT | [package](https://crates.io/api/v1/crates/schemars/1.2.2) |
| BLAKE3 | 1.8.7 | Not declared | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | [package](https://crates.io/api/v1/crates/blake3/1.8.7) |

Select the MIT option for dual MIT/Apache components where their actual license permits it; preserve the upstream license and copyright notices when redistributing. Apache-2.0-selected components require the license, applicable notices, modification notices, and attention to the patent terms. BSD components require their applicable copyright/disclaimer notices. Actual source license files take priority over registry labels. [MIT text](https://opensource.org/license/mit), [Apache-2.0 terms](https://www.apache.org/licenses/LICENSE-2.0).

SQLite is dedicated to the public domain, but the Rust binding is MIT and other bundled extensions may have independent terms. Do not infer that all SQLx dependencies are public domain from SQLite's status.

## Audit procedure

1. Generate the full resolved dependency graph from `cargo metadata --locked --format-version 1` after the manifests settle; include runtime, build, proc-macro, target-specific and test crates.
2. Collect SPDX expressions, license files, copyright holders, versions, enabled features and redistribution purpose. Review ambiguous expressions and components with missing metadata individually.
3. Check the locked graph against current RustSec advisories with `cargo audit` if available; report unavailable tooling instead of declaring the graph vulnerability-free. Pinning and a permissive license do not establish security.
4. Preserve `Cargo.lock`, graph and verification evidence for each milestone. Reassess after dependency changes.
5. Before distribution, generate third-party notices and an SBOM, review native libraries/executables and model/data assets, and obtain owner review for publisher identity and intellectual-property ownership.

SQLx 0.9.0 with SQLite only avoids unrelated database/TLS dependencies, and its compatible libsqlite3-sys 0.37.0 includes the SQLite WAL-reset fix. More detail is in [storage research](../research/RUST_STORAGE.md). This is a targeted known-issue check, not a full vulnerability audit.

## Restricted or individually reviewed components

| Category | Review required and suitable alternatives |
| --- | --- |
| LGPL native/media libraries | Confirm exact version/build, linking and relinking obligations, notices and source provision. A user-installed tool can reduce bundling scope but does not settle all legal questions. Prefer a verified compatible build or permissive alternative where practical. |
| GPL / AGPL | Assess interaction, linking, modifications, distribution and network-use obligations separately. Do not link or bundle automatically. Isolated external tools require their own review; proprietary status must not be changed as a workaround. |
| FFmpeg | Exact build flags, codecs and linked libraries determine license. A GPL-enabled executable is development-only until redistribution review; prefer a vetted LGPL build and appropriate codec choices. Follow media ADR for installed-tool evidence. |
| Blender | Treat as separately installed optional GPL software with a headless process adapter; review scripts/assets/plugins and any future redistribution independently. |
| Source-available / noncommercial models | Read code and weight licenses separately; reject noncommercial weights for commercial distribution unless permission is obtained. Prefer permissively licensed weights or keep capability unavailable. |
| Fonts, music, media, LUTs and presets | Record provenance, creator, license, attribution and redistribution limits. Generated deterministic fixtures avoid committing customer media. |

FFmpeg, Blender, AI weights, preview dependencies and other future integrations need component-specific records. No inference of commercial rights follows from a GitHub repository being public. License obligations and alternatives are recorded without applying an open-source license to Lumeryn.

## Current evidence status

Direct versions, declared MSRVs and registry license expressions: researched. The complete locked graph has been inspected and the lead's final automated advisory/license/bans/source scan passed as recorded below. Full license-file archive, third-party notices, SBOM and distribution legal review remain pending.

## Initial locked-graph inspection

`cargo metadata --locked --format-version 1` was rerun after BLAKE3 and structured-log integration on 2026-10-09: **170 packages, comprising 168 third-party packages and 2 proprietary Lumeryn workspace packages**. This cross-target graph includes dependencies that are not built on Windows. Final lockfile SHA-256 at inspection: `7C5DE2AD423A2D7B3B479DC3217E1BEC126D040AA8559C043C99420DFBA2574C`. The previous 165-package graph had hash `9ACE43B19506F1104088D362B761BF2EFFDDA541269FDEE6A7E0B030F2D7E8FE`; intermediate BLAKE3 graph had 169 packages/hash `F31DD4344F5B1A99C57B0E8156526A0469126245026626A91F8E03ADE869691F`. Re-run this inspection if the lockfile changes.

No mandatory GPL, AGPL, LGPL, noncommercial or source-available expression was found in the resolved metadata. The two local packages intentionally have no public SPDX license. This is a metadata and selected-license-file review, not a legal certification.

- `schemars 1.2.2` and its derive crate are MIT; the local published LICENSE was inspected. Schemars supplies machine-readable schemas from shared Rust types. Keep exact version/lock consistency and validate generated schema against real CLI examples. [Maintainer documentation](https://docs.rs/schemars/1.2.2/schemars/).
- `libsqlite3-sys 0.37.0` resolves with bundled SQLite enabled; its local `sqlite3.h` declares SQLite 3.51.3. The independent CLI regression's runtime query confirms compiled SQLite **3.51.3**; see [verification evidence](../testing/VERIFICATION.md).
- 18 packages declare `Unicode-3.0`; `unicode-ident` additionally declares `(MIT OR Apache-2.0) AND Unicode-3.0`. MIT selection does **not** remove Unicode notice obligations. The local `unicode-ident` and `icu_provider` license files were inspected; preserve each package's copyright/permission notice, including additional upstream ICU notices. [Unicode license](https://www.unicode.org/license.txt).
- `foldhash 0.2.0` uses Zlib. Its local LICENSE permits commercial use, requires accurate origin and marks altered source versions, and preserves the notice in source distributions.
- `r-efi 6.0.0` offers `MIT OR Apache-2.0 OR LGPL-2.1-or-later`; select its MIT alternative and preserve the upstream notice. It is a target-specific UEFI dependency, not an unavoidable LGPL obligation. Packaged README/AUTHORS contain licensing evidence rather than a root LICENSE file; archive the authoritative upstream license before distribution.
- `rustix`, `linux-raw-sys` and `wasi` offer an MIT alternative to Apache/LLVM-exception options. `aho-corasick` and `memchr` offer MIT alongside Unlicense. Select MIT and retain notices.
- `flume` declares the legacy `Apache-2.0/MIT` expression; `vcpkg` and `version_check` declare `MIT/Apache-2.0`. Both license texts were present in the published packages and inspected. Treat these slash expressions as dual license alternatives only after this source review, rather than trusting ambiguous automated SPDX parsing.
- BLAKE3 adds `blake3 1.8.7`, `arrayvec 0.7.8`, `constant_time_eq 0.4.2` and `cpufeatures 0.3.1`. Select **Apache-2.0** for BLAKE3 and constant_time_eq, whose local published Apache license files were inspected; select MIT for arrayvec/cpufeatures. No CC0, MIT-0 or LLVM-exception blanket allowance is required. The normal Apache notice/patent obligations remain. BLAKE3 is used for content identity/source-change detection, not password hashing. [Official implementation and licensing](https://github.com/BLAKE3-team/BLAKE3), [Apache license option](https://github.com/BLAKE3-team/BLAKE3/blob/master/LICENSE_A2).
- Structured tracing adds `tracing-serde 0.2.0`, MIT, declared MSRV 1.63.0. It is included in the final graph/table and remains covered by the normal upstream MIT notice obligation.

The lead ran locally installed **cargo-deny 0.20.2** on **2026-10-09**, using committed `deny.toml` and the final lockfile hash above: `.lumeryn-scratch/tools/bin/cargo-deny.exe --locked check --hide-inclusion-graph`. **Advisories, bans, licenses and sources all passed.** RustSec database commit `550efd3d587a29b2e2c2b21b17a440da4fede999`, dated **2026-10-08**, was fetched during this scan. Duplicate-version warnings remain for transitive `cpufeatures`, `hashbrown` and `syn`; they are documented rather than blanket-ignored. Private unpublished workspace packages are excluded from public-license enforcement; third-party runtime, build and development dependencies remain checked. [Cargo-deny policy](../research/TEST_TOOLING.md), [RustSec tooling](https://rustsec.org/).

This is the lead's reported final scan evidence; the verification specialist independently inspected the resolved metadata and selected license files. At the initial checkpoint `cargo audit --version` returned “no such command: audit”; no standalone cargo-audit run is claimed. Passing the configured checks means no policy-blocking known finding at this scan, not freedom from undisclosed vulnerabilities. External FFmpeg/Blender builds, codec patents, model weights and other media/assets remain outside this Cargo graph's audit scope. Before distribution, refresh scan evidence and complete the full license-source archive, notice/SBOM generation, native-binary audit and owner/legal review.

Final resolved license-expression counts (including 2 private workspace packages):

| Declared expression | Packages |
| --- | --- |
| MIT OR Apache-2.0 | 99 |
| MIT | 27 |
| Apache-2.0 OR MIT | 11 |
| Unicode-3.0 | 18 |
| (MIT OR Apache-2.0) AND Unicode-3.0 | 1 |
| Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | 3 |
| Apache-2.0/MIT | 1 |
| MIT/Apache-2.0 | 2 |
| MIT OR Apache-2.0 OR LGPL-2.1-or-later | 1 |
| Unlicense OR MIT | 2 |
| Zlib | 1 |
| CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | 1 |
| CC0-1.0 OR MIT-0 OR Apache-2.0 | 1 |
| No public SPDX expression (proprietary workspace) | 2 |

### Complete resolved third-party metadata

Entries below retain the exact declared expressions; explicit selections and obligations above control review. Native SQLite core and external FFmpeg are separately recorded because Cargo metadata alone does not enumerate all embedded or subprocess software.

| Package | Locked version | Declared license |
| --- | --- | --- |
| aho-corasick | 1.1.5 | Unlicense OR MIT |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 |
| anstream | 1.0.0 | MIT OR Apache-2.0 |
| anstyle | 1.0.14 | MIT OR Apache-2.0 |
| anstyle-parse | 1.0.0 | MIT OR Apache-2.0 |
| anstyle-query | 1.1.5 | MIT OR Apache-2.0 |
| anstyle-wincon | 3.0.11 | MIT OR Apache-2.0 |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 |
| atoi | 2.0.0 | MIT |
| autocfg | 1.5.1 | Apache-2.0 OR MIT |
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| blake3 | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 |
| bytes | 1.12.1 | MIT |
| cc | 1.6.0 | MIT OR Apache-2.0 |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| clap | 4.6.7 | MIT OR Apache-2.0 |
| clap_builder | 4.6.7 | MIT OR Apache-2.0 |
| clap_derive | 4.6.7 | MIT OR Apache-2.0 |
| clap_lex | 1.1.1 | MIT OR Apache-2.0 |
| colorchoice | 1.0.5 | MIT OR Apache-2.0 |
| constant_time_eq | 0.4.2 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 |
| crc | 3.4.0 | MIT OR Apache-2.0 |
| crc-catalog | 2.5.0 | MIT OR Apache-2.0 |
| crossbeam-queue | 0.3.14 | MIT OR Apache-2.0 |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 |
| digest | 0.10.7 | MIT OR Apache-2.0 |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 |
| dotenvy | 0.15.7 | MIT |
| dyn-clone | 1.0.20 | MIT OR Apache-2.0 |
| either | 1.19.0 | MIT OR Apache-2.0 |
| equivalent | 1.0.2 | Apache-2.0 OR MIT |
| errno | 0.3.14 | MIT OR Apache-2.0 |
| event-listener | 5.4.2 | Apache-2.0 OR MIT |
| fastrand | 2.5.0 | Apache-2.0 OR MIT |
| find-msvc-tools | 0.1.14 | MIT OR Apache-2.0 |
| flume | 0.12.0 | Apache-2.0/MIT |
| foldhash | 0.2.0 | Zlib |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 |
| futures-core | 0.3.34 | MIT OR Apache-2.0 |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 |
| futures-intrusive | 0.5.0 | MIT OR Apache-2.0 |
| futures-io | 0.3.34 | MIT OR Apache-2.0 |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 |
| futures-task | 0.3.34 | MIT OR Apache-2.0 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 |
| generic-array | 0.14.7 | MIT |
| getrandom | 0.4.3 | MIT OR Apache-2.0 |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| hashlink | 0.11.1 | MIT OR Apache-2.0 |
| heck | 0.5.0 | MIT OR Apache-2.0 |
| hex | 0.4.3 | MIT OR Apache-2.0 |
| icu_collections | 2.3.0 | Unicode-3.0 |
| icu_locale_core | 2.3.0 | Unicode-3.0 |
| icu_normalizer | 2.3.0 | Unicode-3.0 |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 |
| icu_properties | 2.3.0 | Unicode-3.0 |
| icu_properties_data | 2.3.0 | Unicode-3.0 |
| icu_provider | 2.3.1 | Unicode-3.0 |
| idna | 1.1.0 | MIT OR Apache-2.0 |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT |
| indexmap | 2.14.2 | Apache-2.0 OR MIT |
| is_terminal_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| js-sys | 0.3.106 | MIT OR Apache-2.0 |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 |
| libc | 0.2.190 | MIT OR Apache-2.0 |
| libsqlite3-sys | 0.37.0 | MIT |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| litemap | 0.8.3 | Unicode-3.0 |
| lock_api | 0.4.14 | MIT OR Apache-2.0 |
| log | 0.4.34 | MIT OR Apache-2.0 |
| matchers | 0.2.0 | MIT |
| memchr | 2.8.3 | Unlicense OR MIT |
| mio | 1.2.4 | MIT |
| nu-ansi-term | 0.50.3 | MIT |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| once_cell_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| parking | 2.2.1 | Apache-2.0 OR MIT |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 |
| potential_utf | 0.1.6 | Unicode-3.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| redox_syscall | 0.5.18 | MIT |
| ref-cast | 1.0.27 | MIT OR Apache-2.0 |
| ref-cast-impl | 1.0.27 | MIT OR Apache-2.0 |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| rustversion | 1.0.23 | MIT OR Apache-2.0 |
| schemars | 1.2.2 | MIT |
| schemars_derive | 1.2.2 | MIT |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive_internals | 0.30.0 | MIT OR Apache-2.0 |
| serde_json | 1.0.151 | MIT OR Apache-2.0 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| sharded-slab | 0.1.7 | MIT |
| shlex | 2.0.1 | MIT OR Apache-2.0 |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 |
| slab | 0.4.12 | MIT |
| smallvec | 1.16.2 | MIT OR Apache-2.0 |
| socket2 | 0.6.5 | MIT OR Apache-2.0 |
| spin | 0.9.9 | MIT |
| sqlx | 0.9.0 | MIT OR Apache-2.0 |
| sqlx-core | 0.9.0 | MIT OR Apache-2.0 |
| sqlx-macros | 0.9.0 | MIT OR Apache-2.0 |
| sqlx-macros-core | 0.9.0 | MIT OR Apache-2.0 |
| sqlx-sqlite | 0.9.0 | MIT OR Apache-2.0 |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 |
| strsim | 0.11.1 | MIT |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| syn | 3.0.6 | MIT OR Apache-2.0 |
| synstructure | 0.14.0 | MIT |
| tempfile | 3.27.0 | MIT OR Apache-2.0 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 |
| thread_local | 1.1.10 | MIT OR Apache-2.0 |
| tinystr | 0.8.4 | Unicode-3.0 |
| tokio | 1.53.2 | MIT |
| tokio-macros | 2.7.2 | MIT |
| tokio-stream | 0.1.19 | MIT |
| tracing | 0.1.44 | MIT |
| tracing-attributes | 0.1.31 | MIT |
| tracing-core | 0.1.36 | MIT |
| tracing-log | 0.2.0 | MIT |
| tracing-serde | 0.2.0 | MIT |
| tracing-subscriber | 0.3.23 | MIT |
| typenum | 1.20.1 | MIT OR Apache-2.0 |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| url | 2.5.8 | MIT OR Apache-2.0 |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT |
| utf8parse | 0.2.2 | Apache-2.0 OR MIT |
| uuid | 1.27.0 | Apache-2.0 OR MIT |
| valuable | 0.1.1 | MIT |
| vcpkg | 0.2.15 | MIT/Apache-2.0 |
| version_check | 0.9.5 | MIT/Apache-2.0 |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasm-bindgen | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-macro | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-macro-support | 0.2.129 | MIT OR Apache-2.0 |
| wasm-bindgen-shared | 0.2.129 | MIT OR Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 |
| writeable | 0.6.4 | Unicode-3.0 |
| yoke | 0.8.3 | Unicode-3.0 |
| yoke-derive | 0.8.4 | Unicode-3.0 |
| zerofrom | 0.1.8 | Unicode-3.0 |
| zerofrom-derive | 0.1.8 | Unicode-3.0 |
| zerotrie | 0.2.5 | Unicode-3.0 |
| zerovec | 0.11.8 | Unicode-3.0 |
| zerovec-derive | 0.11.6 | Unicode-3.0 |
| zmij | 1.0.23 | MIT |
