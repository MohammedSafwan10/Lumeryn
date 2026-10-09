# ADR 0008: Minimal Windows packages, separate signing and evidence-based capability discovery

Date: 2026-10-09

Status: Historical; desktop and packaging decisions superseded by ADR 0010 and ADR 0011. Shared headless service, security, capability and resource principles remain applicable. No Tauri/WebView fallback is authorized.

## Context

Windows is the first desktop target, but Rust headless editing must work without Studio, AI or Blender. Native/model dependencies have different license and hardware requirements. A proprietary release requires reliable install/update/uninstall behavior and truthful CLI discovery, without falsely representing the pending NEXDARK LABS business identity.

## Decision

First prototype an x64 NSIS per-user package when the tested CLI and Studio slices are ready; consider MSI later for managed deployment. Keep CLI/core, user data and optional runtime/model storage separate. Prefer Evergreen WebView2 with explicit online/offline installer choices. No installer/update operation silently downloads models or mutates global Python/CUDA/driver installations. Tauri's current guide supports NSIS and WiX v3 MSI and requires Windows for MSI generation. [Installer guide](https://v2.tauri.app/distribute/windows-installer/), [WebView2 distribution](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution).

Authenticode and Tauri updater signatures are separate trust mechanisms. Sign/verify binaries and installer using approved publisher identity, then produce updater signatures over final immutable artifact bytes with a separately protected key. Tauri requires updater signatures and production TLS; loss of its private key breaks continuity without a planned transition. Signing is not a guarantee of SmartScreen reputation. [Updater](https://v2.tauri.app/plugin/updater/), [code signing](https://v2.tauri.app/distribute/sign/windows/), [Microsoft reputation guidance](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).

Updates require a safe project/job checkpoint, verified artifact and schema/protocol compatibility. Use consistent project backups before migrations. A controlled signed rollback is distinct from disabling version checks; never silently replace new user edits with an old database. MSI installer rollback does not automatically undo application data migrations or model changes. Test NSIS independently. Uninstall preserves projects/source media/model stores by default and does not remove shared/user-installed external tools. [MSI rollback](https://learn.microsoft.com/en-us/windows/win32/msi/rollback-installation).

Build a shared typed Rust capability registry for CLI/help/schema and future Studio. Separate implementation maturity (`implemented`, `prototype`, `planned`) from runtime availability (`available`, `limited`, `unavailable`, `disabled`, `unknown`). Bind available scopes to exact requirements and evidence: backend/model/codec/font versions and digests, OS/device/operator limits, tests/probes, licensing review and measurement provenance. Unknown metrics remain unknown; runtime presence alone proves no inference/encode feature. Read-only discovery never downloads, installs, charges or executes arbitrary project code.

Contracts include stable ID/version, operation schema, headless/preview/export coverage, CPU/GPU fallback, cancellation, deterministic scope, required artifacts, license status, resource estimates and evidence timestamps. Shared validation rechecks at invocation because discovery can become stale. Generated assets/jobs record immutable inputs/parameters/backend identities and output provenance. Preserve current supported output contracts through versioned evolution; this ADR does not claim that the existing simple capability list already fulfills the expanded schema.

## Alternatives

| Alternative | Assessment |
| --- | --- |
| MSI first | Suitable for enterprise, but adds Windows packaging prerequisites and scope/repair complexity; defer until requirements justify |
| Fixed WebView2 bundled in every package | Offline/reproducible runtime benefit, but larger footprint and security patch responsibility; optional future distribution |
| Installer containing all models and Blender | High footprint and license surface; conflicts with optional/local-first architecture |
| One signature assumed to cover OS and updates | Incorrect trust boundary; require both mechanisms |
| Feature names without readiness/evidence | Misleads agents when codecs/models/drivers are missing; rejected |
| Automatic downgrade and cache/data deletion | Risks project incompatibility and user data loss; explicit compatible recovery policy required |

## Costs, licensing and residual risks

Audit packaging tools, WebView2 redistribution, runtime DLLs, codecs, model checkpoints, fonts and application notices independently. GPL/source-available/noncommercial artifacts need individual treatment and alternatives; Lumeryn remains proprietary. Production signing may cost money and require identity validation; no purchase/publication is performed here. Owner identity/IP/trademark review remains pending. Current updater documentation reports Rust ≥1.90; a complete locked Windows build/test is still needed. DLL loading from untrusted project directories, compromised signing credentials, migration incompatibility and destructive uninstall hooks are release risks requiring focused tests.

## Acceptance

Run DIST-01..DIST-06 from `docs/research/WINDOWS_DISTRIBUTION.md`: truthful capability contract tests first, then clean-machine installation, signature-tamper rejection, interrupted update/migration recovery, controlled rollback and preserved user/model data. Record package hashes/versions/sizes and actual Windows environment. No distribution or paid signing begins before the owner reviews a concrete verified release package and authorizes it.
