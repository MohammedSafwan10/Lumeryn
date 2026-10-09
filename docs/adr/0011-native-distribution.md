# ADR 0011: Native Windows packaging and independent update trust

Date: 2026-10-09
Status: Accepted direction; packaging, signing and updater implementation deferred until native app acceptance.

The mandatory native desktop decision removes Tauri installers/updater plugins and WebView prerequisites. Distribute native CLI and Studio together through a per-user NSIS prototype, with MSI/WiX evaluated later. Evaluate cargo-packager as a pinned build-only tool; its official public-preview status and generated templates require clean-machine testing. Handwritten NSIS is a controlled alternative. Review tool/plugin/compression licenses and retain notices without licensing proprietary Lumeryn source publicly.

Authenticode publisher signing and update manifest/artifact signing remain separate trust mechanisms. Protected keys, timestamp verification, authenticated downloads, rejection of unsigned/replayed updates, project backups and schema-compatible rollback are required before automatic updates. No paid service, endpoint or release is created. The initial binaries are locally built and manually launched.

Preserve projects, original media, model stores and settings on normal uninstall. Optional runtimes are independently installed. No Node.js or WebView2 is required to build/run Studio. See research/WINDOWS_DISTRIBUTION.md for official sources, trade-offs and the unexecuted release test matrix. This supersedes ADR 0008's Tauri packaging choice while retaining its capability introspection principles.
