# Lumeryn native Studio

The proprietary native egui/eframe/egui_dock application connects to Lumeryn's shared local Rust project service. Build the workspace so the `lumeryn` CLI/service executable is beside Studio:

```powershell
cargo build --workspace --locked -j 4
./target/debug/lumeryn-studio.exe --project D:/path/to/project.lumeryn
```

The approved **B / Studio** interface keeps Media, Preview, Clip inspector and a full-width timeline together. Native Windows dialogs create/open projects, import media and choose a new export destination without blocking the UI. Drag/drop and manual paths in Project tools remain available. Select a timeline clip to inspect and apply validated trims, or drag its right edge. Scrub the ruler or preview slider; icon transport controls and Space toggle play/pause, and arrows step frames. Preview clocks use proportional digits; precise trim fields retain frame conversion. At compact widths, the header's Clip action opens a scrollable inspector. History, actual settings, activity and diagnostics live in an advanced docking window; Reset view restores the main composition.

Media cards and timeline tiles show one real representative source thumbnail per imported asset. The linked Source audio lane shows a subdued envelope from decoded source PCM; display normalization does not change audio. This disposable in-memory presentation cache covers the first 128 imported assets, with bounded workers/queues, 192×108 thumbnails and at most 1,024 waveform peaks. Repeated thumbnail tiles do not represent different moments. Loading, missing and extraction-error states remain explicit; persistent smart caching is staged.

Monitor mute/volume applies real gain to native preview audio only; it does not edit the project, export audio or playback clock. Preview quality can be changed independently. Export uses a new MP4 destination through the background project service; its cancel button requests actual job cancellation. CLI commits appear through live revision events.

Current engine scope is one CFR video track with embedded source audio and baseline contiguous MP4 export. Format settings and revision history are read-only; undo/redo, effects, multi-track composition, graphics/keyframes, persistent caches, proxies and GPU device-loss recovery are staged. The B presentation changes preserve the core graph/schema and authoritative service. Diagnostics expose actual preview/audio status; unsupported features are not represented as working controls. Final62-test/dependency gates and actual native workflow/state evidence are recorded in [redesign acceptance](../../docs/testing/STUDIO_REDESIGN.md); physical dialog/pointer/high-DPI checks remain pending.

Smoke verification modifies a disposable single-clip project (at least eight frames), restores the clip duration through the actual CLI and adds revision history. Use a new artifact directory:

```powershell
./target/debug/lumeryn-studio.exe --smoke --project D:/temporary/fixture.lumeryn --artifact-dir D:/temporary/smoke-new --ffmpeg C:/tools/ffmpeg.exe --ffprobe C:/tools/ffprobe.exe
```

The native screenshot, exported video and structured report are saved there. Smoke storage is isolated from normal user layouts. See [implementation and verification](../../docs/research/NATIVE_STUDIO.md) for exact checks and honest performance limits.
