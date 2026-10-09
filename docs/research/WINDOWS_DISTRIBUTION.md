# Windows distribution, updates and truthful capabilities

Research date: 2026-10-09. No installer, signing service, update endpoint or release was created. No paid service or publication is authorized by this document. Studio and distribution work follows the tested Rust CLI foundation. NEXDARK LABS is a pending sole-proprietorship brand; installer publisher identity, certificate identity, trademark/IP ownership and commercial notices require owner review before release.

## Installer decision

Recommend a Windows x64 NSIS per-user installer as the first consumer distribution prototype, with MSI for later managed deployment. Build/install acceptance on a clean Windows machine is required; current CLI compilation alone is not installer proof. Tauri supports NSIS setup executables and WiX v3 MSI; its current guide says MSI generation requires Windows and the VBSCRIPT optional feature. Prefer Windows MSVC builds rather than relying on less-tested cross-compilation. NSIS can expose per-user/per-machine modes; choose one consistent installation scope per channel. [Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/), [install-mode schema](https://v2.tauri.app/reference/config/#nsisconfig).

Application policy: install binaries separately from user projects, caches, model stores and credentials. Include the CLI with Studio later, with explicit optional user PATH modification and safe restoration during uninstall. The headless CLI must remain usable without WebView2. Detect conflicting processes and request a safe project/job checkpoint before replacing binaries; do not kill an active export and report successful upgrade. All packaging tool/template/plugin licenses join the release bill of materials.

WebView2 is a Studio prerequisite. Prefer Evergreen runtime discovery with an approved bootstrapper for normal installers; provide an explicit offline installer variant for disconnected users. Tauri documents bootstrapper, offline-installer, fixed-runtime and skip modes. Fixed runtimes move patch delivery to Lumeryn and increase package size, so avoid them initially. Microsoft describes Evergreen as shared and automatically updated; application restart adopts the new runtime. Do not silently install optional model/Python/CUDA dependencies during the base install. [Tauri runtime modes](https://v2.tauri.app/distribute/windows-installer/), [Microsoft runtime distribution](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution).

Actual supported Windows versions and architectures need a tested matrix for the complete product stack. A Tauri support statement does not establish that FFmpeg, wgpu, audio backends or inference SDKs support the same older systems. Start acceptance on Windows 11 x64 and record Windows 10 compatibility separately where appropriate; broader promises require separate tests and current OS support review.

## Two independent signatures

| Mechanism | Purpose | Release policy |
| --- | --- | --- |
| Windows Authenticode | OS publisher identity and executable/installer integrity | Sign application/CLI/worker binaries and final installer, validate chain/digest/timestamp; use verified legal identity |
| Tauri updater signature | Verify downloaded updater artifacts using the embedded updater public key | Sign final immutable update bytes with separate protected key; verify before installation |

These signatures are not interchangeable. Tauri updater signature verification is mandatory and production endpoints require TLS by default. Its public key is embedded, and losing the private key prevents updates to already-installed clients unless a recovery transition was arranged. Current updater documentation states Rust ≥1.90. These are documentation observations, not a verified plugin integration. [Tauri updater](https://v2.tauri.app/plugin/updater/).

Authenticode uses certificate/service provisioning, while updater artifact signing uses an application-managed key. Prefer an owner-approved nonexportable hardware/cloud signing setup after identity and cost review; do not follow obsolete exportable-PFX examples blindly. Preserve trusted timestamp evidence; Microsoft explains timestamping permits signature validation after certificate expiry. Signing does not guarantee SmartScreen reputation or eliminate warnings: Microsoft states EV no longer automatically bypasses reputation checks. [Tauri code signing](https://v2.tauri.app/distribute/sign/windows/), [Microsoft timestamping](https://learn.microsoft.com/en-us/windows/win32/seccrypto/time-stamping-authenticode-signatures), [Microsoft SmartScreen](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).

Application release sequence: build reproducibly → run gates and packaging tests → sign inner binaries → package installer → Authenticode-sign installer → generate detached updater signature over final artifact → compute digests and immutable manifest → review/promote approved release. Never modify signed bytes afterward. Private keys/passwords/tokens stay outside repository and logs. Maintain key backup, access controls, revocation and rotation/recovery drill. A test key is for private local fixtures only and must not be shipped as production trust.

## Update and rollback policy

Design requirements, not functionality supplied automatically by a plugin:

- Updates identify channel, product version, architecture, minimum compatible core/project schema, download size, artifact digest, updater signature and release provenance. Validate allowed origin/redirect policy, TLS and artifact signature before invoking installers. Check disk space and timeout; corrupted/partial downloads never appear installed.
- Preserve offline editing when update checks fail. Make update checking/downloading/install timing explicit and never restart during active editing or uncheckpointed jobs. Credentials and project/media data do not accompany update requests.
- Keep stable/preview channels distinct; preview releases require deliberate enrollment. Require a newer version by default. A controlled rollback needs a signed authorized rollback policy, known-safe artifact and explicit compatibility assessment rather than broadly disabling version checks.
- Back up the committed project database before schema migrations using a consistent SQLite backup strategy. Verify the backup and record schema/app version. A previous binary may not open a migrated database; keep old-compatible data or a reviewed migration path. Never overwrite the latest user's project with an old backup silently.
- Validate core/CLI/Studio protocol versions after upgrade and prevent mismatched concurrent writers. Retain job restart metadata; optional model manifests bind runtime compatibility independently from application version.

Tauri can customize update version comparison, which can support rollback, but that is not proof of safe database or installer rollback. Windows Installer normally rolls back a failed MSI installation; custom actions require additional rollback authoring. Neither mechanism rolls back Lumeryn project changes or model installations automatically. NSIS behavior must be tested independently. [Tauri updater rollback hook](https://v2.tauri.app/plugin/updater/), [Microsoft MSI rollback](https://learn.microsoft.com/en-us/windows/win32/msi/rollback-installation).

## Uninstall and storage preservation

Default Lumeryn policy: uninstall application-owned binaries, shortcuts and scoped registration; preserve user projects/media, model stores and application data unless a separate explicit removal action is approved. Do not remove shared WebView2, user-installed FFmpeg, Blender, CUDA, drivers or unrelated Python environments. Treat model-store relocation paths as user data; never recursively delete a computed path merely because it appeared in a config.

Store optional models in a user-controlled versioned directory outside the installation tree. Keep manifests/checksums/licenses with artifacts, with atomic installation and runtime-version separation. Caches have quotas and clearable metadata; original media is never a cache. Installer/update templates and hooks must prove preservation on repair, upgrade, downgrade attempt, cancellation and uninstall. Review the pinned generated script; do not assume an upstream default satisfies Lumeryn's retention policy.

## Capability contract recommendation

This section defines a future typed contract for `lumeryn capabilities` and `doctor`. It is not a claim that the existing simple implemented/planned lists already implement it. Capability inspection is read-only, local and must not automatically install/download models, execute arbitrary project scripts or call a paid provider. Use bounded cached probes for common discovery, with separately requested smoke tests.

Keep implementation, runtime readiness and evidence separate rather than equating a listed feature with support:

| Field | Proposed meaning |
| --- | --- |
| `capability_id`, `contract_version`, `operation_schema` | Stable namespaced ID and versioned parameters/results/CLI binding |
| `implementation` | `implemented`, `prototype`, `planned`; only implemented advertised as production scope |
| `availability` | `available`, `limited`, `unavailable`, `disabled`, `unknown`; reasons and limits are mandatory except for unconstrained available scope |
| `requirements` | Executable/runtime/model/font/codec artifacts, supported version ranges, OS/API/operator/device prerequisites, disk and measured/estimated RAM/VRAM |
| `requirements[].state` | `satisfied`, `missing`, `incompatible`, `unknown`, `requires_authorization`; expected action and evidence |
| `evidence` | Probe/test ID, method, time, exact backend/artifact version/digest, host/device/driver, scope and expiry; no fabricated benchmark |
| `license` | Reviewed status, exact license/artifact notices, commercial/redistribution limits and owner gate |
| `provenance` | Lumeryn build/version/target and backend/artifact identities; source/provider revision and digest; sanitize sensitive paths |
| `resource_plan` | Measured vs estimated vs unknown peaks and scheduler/parallel-preview status; parameters bound to the estimate |
| `behavior` | Headless/preview/export/CPU/GPU coverage, deterministic scope, cancellation, outputs, known unsupported parameters |

For example, `render.h264` may be implemented yet unavailable when no tested encoder exists; `tts.kokoro` stays planned even if weights happen to exist. A GPU adapter or FFmpeg codec list alone is not an encode/decode/inference smoke-test result. Preview and export coverage are separate fields; full effect support requires both validated paths. Software fallback is advertised only if implemented/tested. Unknown telemetry stays unknown. Model license approval does not prove hardware compatibility.

Generate command schema and capability records from one Rust registry so help, CLI and future Studio agree. Include top-level API/schema version, build, discovery timestamp and project-scoped constraints. Schema evolution requires an explicit compatibility policy; preserve current supported CLI behavior until clients migrate. Capability lists document supported contracts, while operation validation still runs immediately before execution to avoid stale discovery races.

Long-running/generated assets record input media hashes, source ranges, parameters, model/font/config/backend versions, outputs and license/provenance references. Resource-sensitive outputs include the selected device, fallback reason and job IDs. Returned errors distinguish unimplemented, missing dependency, license/authorization requirement, incompatible resource/device, stale revision and backend failure. Logs go to stderr; machine output stays on stdout. Secrets/private paths are excluded by default.

## Minimal installation and optional runtimes

| Layer | Base installation behavior | Later capability enablement |
| --- | --- | --- |
| Rust CLI/core | Small native binaries and necessary reviewed libraries | Works headlessly; installed codec capabilities queried truthfully |
| Studio | Same Rust core plus Tauri/Svelte assets; WebView2 required | Secure IPC and measured preview before shipping |
| FFmpeg/ffprobe | Initially user-configured external binaries; no GPL bundle implied | Distribution edition may bundle only after exact build/license/codec audit |
| Blender | Not bundled; detect configured user installation | Versioned trusted adapter and output tests |
| AI | No weights/Python/CUDA by default | Explicit per-artifact install, checksums/licenses/requirements and isolated runtimes |
| Fonts/OCIO/OTIO/ASS | Only required reviewed baseline assets | Optional formats/adapters/configs enabled with artifact and compatibility audits |

Native dependencies still need Windows DLL resolution hardening: resolve known application-owned libraries from controlled locations, validate expected artifact hashes, and do not load arbitrary DLLs/code from project/media directories. Never install global CUDA/Python silently. Model storage persists across application upgrades and is validated again before runtime use.

## Acceptance gates

All tests below are **planned/unrun** in this research task.

1. DIST-01: schema-backed capability registry distinguishes planned/missing/limited/unknown, reads without downloads/charges, and passes JSON contract/help parity tests with unavailable dependencies and changing environments.
2. DIST-02: clean Windows x64 per-user install/repair/uninstall fixture; CLI PATH behavior, Unicode paths, no admin requirement beyond explicit modes, WebView2 absent/present/offline and CLI independence; user models/projects remain intact.
3. DIST-03: locally signed test artifacts accepted; wrong key, tampered artifact, wrong architecture/channel and interrupted download rejected; trusted Authenticode chain/timestamp checks reserved for approved production identity.
4. DIST-04: update while service/jobs active reaches safe checkpoint; cancel/installer failure recovers; next startup validates protocol/schema; migration failure restores working state without silent edit loss.
5. DIST-05: downgrade/rollback fixture demonstrates compatible project handling and artifact authorization; failed MSI/NSIS behavior measured independently; no blanket insecure endpoint/version exceptions.
6. DIST-06: model install/update/relocation/version rollback and application uninstall retain artifact/manifests/licenses; partial install not registered; foreign paths never deleted.

Release evidence records actual installer sizes, install/update times, signatures, package dependency inventory and clean-machine results. Owner approval for publishing, paid signing and identity/legal matters remains a later concrete release gate; it does not block ongoing local CLI implementation.
