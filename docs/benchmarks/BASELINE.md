# First application acceptance measurement

Date: 2026-10-09. Debug Rust build, Windows x86_64, actual Intel Core i7-12700F (12 physical/20 logical cores), 34,188,763,136 bytes reported physical memory. GPU model/VRAM/utilization were not measured by Lumeryn and remain unknown. Do not infer the reference RTX3050 profile.

Run `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/demo.ps1`. The script generates original3-second320x180/30fps lossless FFV1 video (red/green/blue) with48kHz440Hz PCM sine audio, creates/imports/adds/trims through Lumeryn, exports source frames15..45, reopens revision3, verifies database+source assets, and generates a six-frame contact sheet from the actual MP4.

Observed one application render wall time: **197.2938 ms**, including CLI process/startup, source hash checks, native MPEG4/AAC encoding, output probe and complete decode. Output:30frames,320x180,30/1fps, full-decode verified. This is one tiny smoke run, not a throughput/latency percentile or 1080p/4K/scheduler benchmark. App CPU threads and memory limits do not constitute measured GPU support or production realtime guarantees.

Artifacts for this run: `.lumeryn-scratch/demo-f26b2e5d16ad4eb0b9fa4eafe8d9e535/{project.lumeryn,generated.mkv,trimmed.mp4,contact-sheet.png,summary.json}` (ignored, not committed). The lead visually inspected the sheet: first row red, second row green, matching the selected half-second halves. Automated acceptance separately checks all30decoded RGB frames, audio sample count within AAC padding tolerance, non-silent signal, exact fractional-rate output, and source-byte preservation.

Backend-only experiments in MEDIA.md are separate evidence and are not application performance claims. Next measurements: stage timing/distributions, detailed motion/quality fixtures, immutable-source/hash overhead on large footage, decoder surface/RAM peak, app-only GPU encode/decode initialization and per-codec quality comparisons, native preview cadence/seek and ten-minute AV drift.
