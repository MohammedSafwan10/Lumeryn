# Offline asset acquisition verification

Executed 2026-10-09 against the actual downloaded 352-file starter catalog. This verifies acquisition/media integrity; CLI/Studio gallery, placement, rendering and packaging acceptance belong to their implementation gates.

## Executed evidence

- `python scripts/library-assets.py verify`: 352 assets; 240 OGG, 110 PNG, 2 TTF; 6,075,184 asset bytes. All relative paths, unique IDs/paths, SHA256, exact byte counts, per-entry license notices and full-file hash manifest passed. Current catalog SHA256: `09caa5f0195ea1569c25e7a93326bdb6e42d80d822c7f0a7558406cea946ce2d`.
- `python scripts/library-assets.py audit --output .lumeryn-scratch/asset-acquisition/decoded-audit.json`: full FFmpeg decode passed for every 240 audio and 110 PNG files; FFprobe supplied codec, duration, sample rate and channel metadata. Both unmodified Inter variable fonts separately rendered regular/italic text samples using Pillow/FreeType.
- `python scripts/library-assets.py self-test`: seven safety tests passed. Cases cover traversal/absolute/ADS paths; ZIP case duplicates, escaping members and symlinks; existing-destination preservation; asset/notice tampering; insecure/unreviewed/credential-bearing URLs.
- `python scripts/library-assets.py acquire --output .lumeryn-scratch/asset-acquisition/reinstalled-final-library`: actual fresh HTTPS download/install passed using all pinned archive/direct-file hashes. The installed copy's catalog/hash inventory matched the reviewed original. The first execution exposed an eager Python default-expression bug; it was fixed before the successful rerun.
- Actual `target/debug/lumeryn.exe assets list`, `assets search --query music-loop --limit 128`, `assets inspect --id oga.hernandack.short-loops.v1.winter-dust` and `assets check` returned structured JSON with exit 0. Check verified 352 files/6,075,184 bytes; search returned the four loops; inspect resolved the selected contained, SHA256-verified source path. This first list execution returned the bounded first 128 entries, so full-catalog pagination is a separate follow-up verification. Binary SHA256 was `b8bf01ce7f3a4207ccb7c411d7a7518590a826b935acbb16cd92395bfd6f64c8`; JSON evidence is `.lumeryn-scratch/asset-acquisition/cli-library-evidence.json`.

Retained machine evidence: `assets/library/QA.json`, `HASHES.json`, source pins and per-entry metadata. Scratch contains the detailed decoder report, downloaded original ZIPs and the successful fresh installation. No generated media fixture, executable or archive is included in the installed catalog.

## Visual and audio review

The actual 110-image contact sheet at `.lumeryn-scratch/asset-acquisition/background-contact-sheet.png` was visually inspected. Included artwork is transparent cartoon outdoor elements: clouds, trees, hills, houses, towers and silhouettes. Flat pale variants need contrast against a suitable canvas. This is an explainer illustration pack, not photographic stock or a full collection of composed backgrounds.

The regular/italic font sample at `.lumeryn-scratch/asset-acquisition/inter-contact-sheet.png` was rendered and visually inspected for Latin, Greek, Cyrillic and Vietnamese strings. Native font loading, shaping and additional language coverage remain implementation checks.

The actual four music files were decoded and a 32s montage was generated and emitted for listening review at `.lumeryn-scratch/asset-acquisition/music-audition.mp3` (8s per track: A Brand New Wisdom, Just Saying Tho, Swinging Sweet, Winter Dust). Subjective listening approval, narration fit and audible loop-seam acceptance remain pending; emitting an audition is not a listening-quality claim. The original files are unchanged.

Objective analysis records mean/max volume and sample boundary discontinuity in `QA.json`. Three loops reached 0 dBFS in FFmpeg's integer-domain peak report; this is not a recommendation to place them at unity gain beneath narration. Winter Dust measured lower level. A boundary-jump comparison alone cannot establish a perceptually seamless loop; no automatic seamless/professional label was added. Future creative acceptance should audition full loops/repeated joins, choose appropriate gain/fades and listen alongside narration.

## Reproduction and limits

Run the script's default offline verification before exposing assets to agents or a gallery. Run `self-test` after acquisition-script changes. `audit` requires FFmpeg/ffprobe and a new report path; `acquire` requires network access and a new destination. Both refuse overwrite. The SHA256 pins are derived from actual reviewed downloads; changed upstream bytes fail explicitly and require a new provenance/license review.

The curated installed library is below 64 MiB. Original downloads and QA previews live in ignored scratch. Some application-level source extensions are ignored globally: root must explicitly track the authorized curated PNGs or add a narrowly scoped ignore exception during the verified milestone commit.

No subjective professional music-quality claim, native SVG/image placement claim, stock search relevance benchmark, installed-app bundle-copy claim or physical speaker/device test is made by this acquisition milestone. Those checks remain distinct from hash/decoder verification. No optional AI models or remote libraries were installed.

Read-only Rust/Studio review confirmed streamed asset hashing, canonical path confinement, contained notice paths, a bounded worker queue, virtualized text rows, project-epoch checks and explicit unsupported font/SVG import. Phase 1's gallery labels state that audio/image imports add immutable sources to My media and timeline placement remains the next slice. Two actionable review findings were reported to root: bounded list pagination was missing; worker reply notifications needed a repaint after send to avoid a wake-before-enqueue race on an idle empty Studio. Root owns fixes and their runtime acceptance.

Integration follow-up: pagination and reply repaint fixes passed the final87-test workspace gate. Final native image/audio library captures were inspected, with actual source thumbnails and bounded audition controls. Standalone decoder/device tests and actual CC0 OGG/PNG service import passed. The installed CLI catalogue check verified all352 files. [Integrated evidence](CLIPCHAMP_FOUNDATION.md) preserves remaining physical/listening/placement/packaging limits.
