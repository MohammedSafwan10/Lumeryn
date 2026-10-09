# Engineering risk register

| Risk | Mitigation / evidence needed | Owner / status |
|---|---|---|
| Preview embedding and frame/audio latency | Compare native child surface, standalone surface, encoded stream using measured 1080p/4K proxies; no raw JSON-frame transport | Platform / open |
| Installed FFmpeg licensing and vulnerabilities | Record version/config, keep external, no bundling; audit codec patent obligations and upgrades | Media / open release review |
| Schema and concurrent human/agent edits | Transactional revision checks; later sole-writer service and authenticated revision events | Core / initial implementation |
| Rational/VFR/sample timing | Integer project frames, exact ratios; reject unsupported export sources; later explicit resampling/time-remap semantics | Timeline / constrained first slice |
| GPU memory / unknown device telemetry | Report unknown; profile real peak RAM/VRAM; conservative reservations before concurrency | Resources / planned |
| Color accuracy | Baseline SDR only; OpenColorIO and known-reference golden tests before grading/HDR claims | Renderer / open |
| Model/asset redistribution | Separate code, binary and weight licenses; exact checkpoint audit; no automatic installation | Licensing / planned |
| Malicious media and symlink/source changes | Protocol limits, canonical paths, bounded workers; OS isolation and content hashing later | Security / residual risk |
| Scalability of JSON graph | Small snapshot baseline; indexed normalized schema migration after long-timeline measurements | Storage / prototype |
