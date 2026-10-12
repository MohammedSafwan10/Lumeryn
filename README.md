# Lumeryn

**Local video editing for people and external agents.** A Rust engine, a discoverable JSON CLI and a native desktop Studio edit the same durable project.

Private, proprietary software under development. No public source license is granted. Third-party dependencies, fonts and catalogue assets retain their own licenses and notices. NEXDARK LABS is a pending business brand.

## Current capabilities

- Native Windows Studio using egui/eframe and wgpu, with media drawers, real thumbnails, preview, waveforms and timeline dragging.
- Shared Rust project service with SQLite/SQLx, transactions, revision-aware writes, durable undo/redo and live CLI/Studio synchronization.
- Staged background imports with immediate pending cards, cancellation, verified source identity and bounded preparation workers.
- Explicit rational timeline FPS and optional first-video matching. Source, timeline and export rates are separate; the timeline rate stays locked while clips exist.
- Bounded ordered video/PNG layers, gaps, contain rectangles and free movement; video insertion, reorder, trim, split, duplicate and delete; independent mono/stereo audio placement, mixing, trim, gain and mute.
- A reviewed offline catalogue of 352 audio, graphic and font files, discoverable through both CLI and Studio.
- Unrestricted local landscape and portrait UHD export at 24, 25, 30 and 60 FPS for the supported graph. H.264/H.265 depend on usable installed encoders; automatic hardware discovery has software fallbacks. Exports provide progress, cancellation and decoded-output verification.

The sequential renderer supports64 contiguous matching-CFR SDR primary clips. The schema3 compositor supports three total visual placements (primary plus overlays), up to two extra tracks, two simultaneous videos/one PNG and UHD-bounded intermediates, with linked source and independent audio. Mixed source rates, editable text/vector scenes, masks, broader compositing, transitions, filters, recording, audio detach/fades and optional AI remain roadmap work. This is an editing foundation, not professional-editor parity. See [tested progress](PROGRESS.md) and the [feature inventory](docs/roadmap/CLIPCHAMP_FEATURE_INVENTORY.md).

## Build and run

Windows x64 is the currently verified desktop/service target. Install the pinned Rust toolchain from [rust-toolchain.toml](rust-toolchain.toml), MSVC build tools, and trusted local FFmpeg/ffprobe binaries available on `PATH`. FFmpeg, AI models and optional runtimes are not bundled.

```powershell
cargo build --workspace --locked
cargo install --path crates/lumeryn-cli --locked
lumeryn doctor
lumeryn capabilities

# Create a new project in a local, ignored directory.
New-Item -ItemType Directory -Force projects/my-video
lumeryn project create --project projects/my-video/project.lumeryn --name "My video" --preset landscape --fps 30
lumeryn studio open --project projects/my-video/project.lumeryn --executable D:/Dev/freela/Lumeryn/target/debug/lumeryn-studio.exe
```

Double-click `launch-studio.cmd` for Welcome/Open/New, or create an opt-in desktop shortcut with `scripts/create-studio-shortcut.ps1`. Studio is a GUI in debug and release; startup failures are visible. Agent launch uses `studio open` and reports observed native-window state; `project open` stays headless. Keep the built CLI and Studio binaries together. The authoritative service starts automatically in a hidden process; the CLI also works headlessly. There is no Node.js, WebView, mandatory cloud database or embedded agent runtime. Installers and automatic updates are not shipped yet.

## Agent workflow

```powershell
lumeryn --help
lumeryn capabilities
lumeryn schema
lumeryn schema --kind render
lumeryn project inspect --project projects/my-video/project.lumeryn
lumeryn media import-start --help
lumeryn media import-status --help
lumeryn project batch --help
```

Select every project explicitly with `--project`. Inspect the current revision before writing and pass `--expected-revision`; dry-run atomic batches before applying them. Only verified media can be placed. Sources are never modified. Export to a new path and check the reported verification. Follow the [CLI workflows](docs/cli/WORKFLOWS.md) and [agent handoff](docs/cli/AGENT_QUICKSTART.md) for commands and errors.

Media stays on disk, outside the project database. Use `project backup` for consistent backups; copying a live WAL database alone is unsafe. Legacy schema-1/2 projects require explicit backup/migration before new edits.

## Verification

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
.\scripts\check.ps1 -Media -Deny
.\scripts\native-smoke.ps1
```

Media checks require configured FFmpeg/ffprobe; native smoke opens an actual window on disposable generated footage. Executed evidence and outstanding physical/device checks are recorded in [native acceptance](docs/testing/NATIVE_ACCEPTANCE.md), [import/FPS acceptance](docs/testing/NATIVE_IMPORT_FPS.md) and [audio composition acceptance](docs/testing/AUDIO_COMPOSITION.md). A successful launch alone is not acceptance.

## Project documentation

- [Product requirements](docs/PRODUCT_SPEC.md), [architecture](ARCHITECTURE.md) and [architecture decisions](docs/adr/).
- [Task board](TASKS.md), [progress](PROGRESS.md) and [engineering instructions](AGENTS.md).
- [Security boundaries](SECURITY.md), [dependency licensing](docs/licensing/DEPENDENCIES.md), [media licensing](docs/licensing/MEDIA.md) and [asset provenance](docs/licensing/ASSET_LIBRARY.md).

Do not commit credentials, customer media, project databases, generated renders or build outputs. Reviewed catalogue files and historical synthetic design/test evidence are intentionally tracked. FFmpeg distribution, codec patents and native/model licensing require separate review before commercial distribution.
