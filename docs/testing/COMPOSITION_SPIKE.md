# External composition feasibility and baseline

Executed: 2026-10-10. **This is not Lumeryn composition acceptance.** Production Rust, schema, UI and capabilities are unchanged. Root reviewed the actual generated overlay and gap frames. Fixtures and executable downloads remain ignored; no customer media was used.

## Reproduction

```powershell
./scripts/composition-spike.ps1
```

Requires trusted FFmpeg/ffprobe on PATH. Each run creates a unique `.lumeryn-scratch/composition-spike-<uuid>` directory and generates its own source/PNG/output; it does not overwrite owner files. It writes `report.json`, hashes and frames24/50. Paths enter separate process arguments; the filter is fixed numeric fixture data, not user executable text. The script is an external feasibility tool, not a new CLI editing command.

## Executed fixture and checks

FFmpeg8.1.1 Essentials Gyan GPL/version3 build, Windows x64. Generated1280x720/30FPS/60frame moving source, a translucent RGBA PNG, source crop, scale and30-degree rotation; moving video visible at frames0..29, image10..44 and opaque-black all-layer gap45..59. Explicit overlay EOF/interval bounds avoid repeating finished layers.

The final FFV1 MKV was probed with actual frame counting and fully decoded:1280x720,30/1FPS,60frames,2.000seconds. Black detection reported start1.5seconds (frame45), last sampled frame1.967seconds. Frame24 was visually inspected for the rotated translucent graphic over the video pattern; frame50 was fully black. These are visual/interval checks, not calibrated alpha/colour goldens. Crop's placement was exercised; the solid PNG does not verify an asymmetric crop's geometry. Audio, text, scene editing, GPU, encoder/hardware parity and UHD were not exercised.

Three sequential runs per workload,2 filter-complex threads, decoded inputs → filter → null output; elapsed wall time includes process startup and input decode. There is no encoded-output timing in the benchmark comparison.

| Run set | Plain single-video median | Video + transformed timed PNG/gap median |
|---|---:|---:|
| Initial local experiment |97.126ms|146.342ms|
| Tracked script rerun |98.507ms|154.097ms|

This small warm synthetic fixture does not establish native playback FPS, display latency, peak RAM/VRAM, sustained4K speed or a fastest backend. More work naturally costs more; no acceleration claim is made. Reference formula/format changes need their own conformance tests. [FFmpeg filter docs](https://ffmpeg.org/ffmpeg-filters.html#overlay) describe overlay/framesync parameters; they do not prove Lumeryn supports them.

Tracked-script evidence directory: `.lumeryn-scratch/composition-spike-5cc6ddee93ff4cb5bd3ebd571e7d5cdc`. Output SHA256 `BBA5C9AF348D23DF03023844E676C2AAC05CD5E486E38D56B49BD4A94F8BEA5F`; source `FF79E368533742B5364F85A04D69C8E96E274F0448D437DE72DEA8761405651A`; PNG `B3AC7C5634DAECE9E319256E31BE0F6D1DAB3228031DA3784C0DCF1E2F197F89`. Hashes belong to generated fixtures, not a golden update for production.

## Next gate

After the owner-authorized tool update, the same script also passed on9.0.2; full metadata/decode, timed gap and actual frames were rechecked. Version-specific numbers and evidence are in [FFmpeg9 compatibility](FFMPEG9_COMPATIBILITY.md). Do not combine version-specific hashes or treat their small timing difference as a controlled speedup.

Integrate a checked Rust VisualPlan and one shared reference lowering for preview/frame inspection/export before persisted multi-layer writes. Add asymmetric geometry, exact boundary/pixel tests, source metadata/alpha policy, migration/history, independent/embedded audio, resource limits and genuine UHD/portrait output. See [delivery plan](../roadmap/COMPOSITION_DELIVERY_PLAN.md).
