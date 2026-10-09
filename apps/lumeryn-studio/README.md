# Lumeryn native Studio

The proprietary native egui/eframe/egui_dock application connects to Lumeryn's shared local Rust project service. Build the workspace so the `lumeryn` CLI/service executable is beside Studio:

```powershell
cargo build --workspace --locked -j 4
./target/debug/lumeryn-studio.exe --project D:/path/to/project.lumeryn
```

Open/import media by path or drag/drop. Select timeline clips to inspect and apply validated trims. Scrub the ruler or preview slider; Space toggles play/pause and arrows step frames. Change the preview quality, move dock tabs or restore beginner/professional presets. Export uses a new MP4 destination through the background project service; its cancel button requests actual job cancellation. CLI commits appear through live revision events.

Current engine scope is one CFR video track with embedded source audio and baseline contiguous MP4 export. Format settings and revision history are read-only; undo/redo, effects, multi-track composition, graphics/keyframes, thumbnails/waveforms, proxies and GPU device-loss recovery are staged. Errors and actual preview/audio status appear in Diagnostics; unsupported features are not represented as empty working panels.

Smoke verification modifies a disposable single-clip project (at least eight frames), restores the clip duration through the actual CLI and adds revision history. Use a new artifact directory:

```powershell
./target/debug/lumeryn-studio.exe --smoke --project D:/temporary/fixture.lumeryn --artifact-dir D:/temporary/smoke-new --ffmpeg C:/tools/ffmpeg.exe --ffprobe C:/tools/ffprobe.exe
```

The native screenshot, exported video and structured report are saved there. Smoke storage is isolated from normal user layouts. See [implementation and verification](../../docs/research/NATIVE_STUDIO.md) for exact checks and honest performance limits.
