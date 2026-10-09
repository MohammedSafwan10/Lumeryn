# Native Studio baseline and next optimizations

Measured 2026-10-09, debug Rust1.97.1 Windows x64. CPU i7-12700F,20logical cores, OS physical RAM34,188,763,136bytes. Studio adapter: NVIDIA RTX3050/Vulkan. NVIDIA's own counters reported driver610.88 and6144MiB adapter memory; this external probe is not yet Lumeryn's hardware scheduler telemetry.

| Measurement | Actual result and scope |
|---|---|
|Final moving720p smoke startup→first decoded GPU frame|828.11ms, from executable main; one run, includes native/GPU setup and source validation.|
|Native UI CPU update time|p50=1.4978ms,p95=5.7745ms,104samples; max81.8554ms includes startup. No display/GPU FPS inference.|
|GPU upload CPU submission|Last observed2.1681ms for720p RGBA; shared eframe device,12uploads. This is not GPU completion latency.|
|Background export interaction|29UI updates while export ran;60frames720p full decode verified.|
|Whole Studio process samples|Peak sampled working set280,420,352bytes; CPU2,000ms over4,324.69ms process observation. Excludes service/FFmpeg workers; sampling overhead affects results.|
|Whole-adapter GPU samples|0–11% utilization;991–1269MiB used. Other applications/desktop share these counters; per-Studio VRAM is unknown.|
|Paused native idle|One5,015ms sample showed0ms CPU increment at OS counter resolution; RAM202,043,392→202,067,968bytes. This is not a zero-resource guarantee; excludes service/decoder. Global GPU sample0%.|
|10,000-clip CPU timeline paint|100samples after10warmups: p50=.4429ms,p95=.5479ms;15visible clips painted. Metadata scans all10k; no indexed O(visible) or GPU throughput claim.|
|720p worker load/seek|Three first-frame70–80ms, seek126–136ms; expected500ms segment throughEOS completed506–517ms,0drops. See NATIVE_PREVIEW for raw runs.|
|720p baseline application export|One2second/60frame render382.5533ms with probe/hash/full-decode verification. MPEG4/AAC CPU preset, not a quality/throughput guarantee.|

Final artifacts: `.lumeryn-scratch/studio-c723fa36e51445dea5b364f24e541ea4/` and `.lumeryn-scratch/demo-dfede98ef6ea48229d8bf0f3a38a1b18/`. Idle: `.lumeryn-scratch/studio-idle-fd8eef34f2ef49d7929af5716c86ee77/idle.json`. Reproduce with scripts/native-smoke.ps1; counter samples are adapter-global, not process-private attribution. The live Studio was left open for user review. Initial earlier tiny color smokes measured first-frame780.94/1111.61ms; different source/load means these are not optimization improvement percentages.

## Measured fixes and current bottlenecks

Captured CLI launch previously waited≈10.1seconds because inherited capture handles survived in the daemon. The reviewed Win32 launch boundary disables inheritance; captured inspect then measured45.7ms and the unchanged malformed-import timing regression passes. This is a concrete reliability/performance improvement, not a speculative engine rewrite.

Paused playback formerly had a4ms actor tick and2ms full-audio-buffer polling. Presentation ticks are now conditional, producer capacity uses notifications and native audio pauses. The final idle measurement above verifies the resulting behavior; no comparable pre-change CPU sample exists, so no percent improvement is claimed. GPU textures are reused and uploaded only for a new generation/frame key. Repeated source clips share initial digest validation.

Source-beginning decoding on every seek preserves exact trims but scales poorly with long GOPs/duration. Full content hashing protects source identity but can dominate long-file open/edit latency. Clip visibility currently scans metadata; media labels use lookups for visible clips. Startup includes font atlas/native/GPU initialization. All are profiling candidates; preserve correctness before optimization.

## Next experiments, separate from current implementation

Benchmark representative1080p/4K sources and10minute A/V drift, repeated cold/warm starts, headroom under exports, stage histograms and full process-tree RAM. Pre-register target quality/timing tolerances. Compare identical codec/quality CPU versus hardware across frame counts, formats and devices; current hardware initialization results are in HARDWARE.md. Keep CPU fallback and precise runtime failure reports.

For texture interoperability, prototype hardware-decoder D3D11/D3D12 or Vulkan/CUDA resource sharing only where FFmpeg and wgpu APIs support correct ownership/fences/formats. Measure downloads/uploads versus shared surfaces; do not equate resident FFmpeg transcode with wgpu zero-copy. CPAL sample clocks and GPU completion/display latency need separate evidence.

Evaluate timestamp/frame-indexed seek and source-hash keyed intraframe proxies, retaining full-resolution original source evaluation for export. Cache only affected scene/clip outputs with dependency/version/color keys; test revision invalidation, disk quota and interruption. Preview downscaling is implemented, but disk proxies and incremental render caching are not. Consider interval-index timeline lookup and cached thumbnail/waveform extraction after real large-project profiles.

Optimized native C/C++ libraries, CPU SIMD dispatch and platform GPU backends are acceptable when reproducible measurements justify them. No handwritten Assembly or unsafe editing/render optimization is added. MLT remains an optional researched reference/backend with build-specific LGPL/GPL review; it cannot replace the authoritative Rust graph. See ADR0012.
