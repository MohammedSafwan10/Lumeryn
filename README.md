# Lumeryn

Proprietary local video editing engine, agent-first Rust CLI and native Rust Studio. NEXDARK LABS is a pending business brand. No public source license is granted. The mandatory native migration supersedes all earlier Tauri/Svelte plans.

Users brief an external agent in ordinary language; the CLI controls deterministic editing and the Studio displays the same project for review and manual editing. Editable educational explainers remain the next creative milestone. This build provides a constrained editing/export foundation and the native Studio migration; consult PROGRESS for tested scope and remaining limitations.

Build on Windows with Rust1.97.1/MSVC and locally installed FFmpeg/ffprobe:

```powershell
cargo build --workspace --locked
.\target\debug\lumeryn.exe --help
.\target\debug\lumeryn-studio.exe --project D:\path\project.lumeryn
.\scripts\check.ps1 -Media -Deny
.\scripts\native-smoke.ps1
```

Keep both binaries together. The shared project service is launched automatically in a hidden process and releases its project writer when idle. Studio uses egui/eframe/egui_dock and wgpu, with no Node.js or WebView dependency. The CLI works without launching Studio. Optional models and runtimes are not installed automatically. No installer or automatic updater is shipped yet.

See [CLI workflows](docs/cli/WORKFLOWS.md), [actual progress](PROGRESS.md), [task board](TASKS.md), [architecture](ARCHITECTURE.md), and [complete product requirements](docs/PRODUCT_SPEC.md). The native editor has left/right drawers, real preview/waveforms, basic clip operations, canvas choices and a reviewed offline asset library with audio audition. Export supports unrestricted landscape/portrait UHD,24/25/30/60FPS, installed H.264/H.265 encoders, quality/bitrate and automatic hardware discovery with software fallback. The graph remains one contiguous matching-CFR SDR video track with embedded audio/silence; native text/scenes/masks/effects/mixing are future slices. [Executed acceptance and limits](docs/testing/CLIPCHAMP_FOUNDATION.md) distinguish tested functionality from professional-editor parity.
