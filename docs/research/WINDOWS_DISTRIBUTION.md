# Native Windows distribution

Updated 2026-10-09. Native Rust Studio supersedes all earlier Tauri packaging/updater plans. No installer, certificate, update endpoint or release is implemented.

Prototype per-user NSIS containing lumeryn.exe and lumeryn-studio.exe; evaluate MSI/WiX later. cargo-packager supports Cargo-only NSIS/MSI workflows but its official documentation labels it public preview. Pin a tested version and audit generated templates/tool downloads before adoption; handwritten NSIS is a controlled alternative. No packaging runtime dependency is needed now. No Node.js or WebView2 prerequisite applies. [Packager documentation](https://docs.crabnebula.dev/packager/).

Review installer tool/plugin licenses individually. NSIS primarily uses zlib/libpng; compression licenses differ and LZMA includes CPL with a linking exception. Initially prefer zlib compression. Review pinned WiX terms separately. This does not change proprietary Lumeryn ownership. [NSIS licenses](https://nsis.sourceforge.io/License), [WiX documentation](https://docs.firegiant.com/wix/).

Authenticode publisher signing and update manifest/artifact signing are separate. Use protected owner-approved keys, SHA-256 digests and trusted RFC3161 timestamps, then verify final executables/installers. A future native updater must authenticate HTTPS, platform/channel/version, artifact digest and a pinned signing key before execution; reject unsigned/replayed downloads. Key rotation requires a signed trust transition. No updater is added now; manual installation remains the first release behavior. [Microsoft SignTool](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool), [timestamp validation](https://learn.microsoft.com/en-us/windows/win32/seccrypto/time-stamping-authenticode-signatures).

Stage downloads and checkpoint active jobs before upgrades. Back up projects before irreversible migrations. Installer rollback does not restore project history or models; retain compatible old binaries and reject unsupported schemas. [MSI rollback](https://learn.microsoft.com/en-us/windows/win32/msi/rollback-installation).

Install binaries separately from projects/media/models/settings. Normal uninstall preserves these user stores and external FFmpeg/Blender/drivers. Optional data removal needs explicit choice and verified paths. Optional models/runtimes are independently pinned, checksum-verified, licensed and installed; do not download them as part of minimal installation.

Capability discovery must remain local, read-only and truthful: implemented versus planned, available versus missing, unknown hardware telemetry, precise format/effect limits, measured resources and fallback evidence. A compiled FFmpeg backend or detected GPU is not a successful codec benchmark. A later typed registry should generate CLI/Studio discovery together; retain stable JSON contracts.

Release tests remain unexecuted: clean Windows11 x64 install/repair/uninstall, Unicode/offline paths, PATH restoration, ACLs, active-job upgrades, protocol mismatch, models/settings persistence, migrations, corrupt/wrong-channel/unsigned/replayed updates, interrupted install, low disk and rollback. Audit packaged fonts/assets/codecs/models and notices beyond Cargo. Signing, charges and publication require owner authorization. See ADR0011.
