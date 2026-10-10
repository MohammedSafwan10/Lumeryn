# Studio typography: one coherent Inter interface

Research date: 2026-10-10. Inspected baseline `194f261`, bundled asset catalogue/notices/bytes and the locally resolved egui/epaint 0.36.2 sources. This is a font recommendation and implementation/acceptance contract. No Rust, dependencies, font bytes, project state or exported media were changed by this research. Metadata/hash inspection ran; native Inter Studio rendering and performance comparisons did not run here.

## Recommendation

Use the already audited **Inter Regular** as the first proportional UI font throughout Studio: header, rails/drawers, buttons, media names, Inspector labels/values, ruler/time readouts, status and ordinary project metadata. It provides a consistent visual voice without dependence on an installed Windows font, another download or a new rendering dependency. “Best” here is a design recommendation for this application's constraints, not a universal measured ranking.

Inter's official project describes its UI-oriented design and text/display optical sizes. Its slashed-zero and tabular-number designs are optional OpenType features; those upstream features do not establish that Studio enables them. Use default ordinary zeros and proportional readouts, as the approved B interface requires. [Inter official project](https://rsms.me/inter/), [pinned 4.1 release](https://github.com/rsms/inter/releases/tag/v4.1).

Keep this change in the UI presentation layer. An egui label font neither creates native creative text nor guarantees final-render title fidelity. Editable scene text still needs the shared Rust layout/font-identity/glyph-rendering contract in [ADR 0005](../adr/0005-typography-audio.md) and [ADR 0009](../adr/0009-semantic-scenes-explainers.md). Preview/export text must use project-resolved licensed fonts, not implicitly depend on Studio's installed UI family.

## Exact local assets and licence

The following existing files were inspected; retain their original bytes and notices. Paths below are relative to `D:/Dev/freela/Lumeryn/`.

| Role | Exact path | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| Recommended UI face | `assets/library/packs/inter.v4.1/Inter[opsz,wght].ttf` | 876,576 | `29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031` |
| Existing italic asset; not needed for initial UI | `assets/library/packs/inter.v4.1/Inter-Italic[opsz,wght].ttf` | 906,596 | `acd98e64795781b2058f07b18475e0ecee2a0fe2b42a49e2f9e37d0d6bf66ce6` |
| Copyright/OFL notice | `assets/library/notices/inter.v4.1/License.txt` | Retain full text | `262481e844521b326f5ecd053e59b98c8b2da78c8ee1bdbb6e8174305e54935a` |

Catalogue IDs are `inter.v4.1.inter-opsz-wght` and `inter.v4.1.inter-italic-opsz-wght`; provenance is in `assets/library/sources.json`, catalogue metadata in `assets/library/catalog.json` and pinned file identities in `assets/library/HASHES.json`. The archive source is the pinned Inter-4.1-GoogleFonts.zip linked from the release, not an arbitrary system-font copy. Existing asset QA reports Latin/Greek/Cyrillic/Vietnamese specimen inspection; that is prior asset evidence, not acceptance of the new native UI.

Read-only SFNT metadata inspection of the actual regular TTF established family `Inter`, subfamily `Regular`, embedded version `Version 4.001;git-66647c0bb`, 2,933 glyphs and OS/2 weight class 400. Pack release label 4.1 and the font's internal version string are distinct facts. Its `fvar` axes are `opsz` 14..32, default 14, and `wght` 100..900, default 400. Use this default instance initially. A variable file and parser support are not proof of all axis combinations, optical-size adaptation or a complete weight UI in Lumeryn.

Inter is OFL-1.1. Bundle/embed it with Lumeryn while retaining its copyright and licence; do not sell the font alone or relicense the font as proprietary. Any later modified/subset/converted font requires the OFL naming and distribution review. The licence does not impose an open-source licence on Lumeryn or documents produced with the font. Include the accessible notice in any Studio packaging that embeds the bytes; source-tree presence alone is insufficient for an eventual distributed executable. [Inter 4.1 licence](https://raw.githubusercontent.com/rsms/inter/v4.1/LICENSE.txt), [local asset register](../licensing/ASSET_LIBRARY.md).

## Egui 0.36.2 integration and fallback limits

Start from `FontDefinitions::default()`, register the regular face once using `FontData::from_static` and shared `Arc`, then insert its name at the beginning of `FontFamily::Proportional`. Preserve the existing fallback names/order. Egui documents ordered glyph lookup: first font is primary, following entries are fallback. Set the definitions during Studio initialization, not on every update. [FontDefinitions 0.36.2](https://docs.rs/egui/0.36.2/egui/struct.FontDefinitions.html), [FontData 0.36.2](https://docs.rs/epaint/0.36.2/epaint/text/struct.FontData.html).

For `apps/lumeryn-studio/src/design.rs`, the compile-time asset reference is `../../../assets/library/packs/inter.v4.1/Inter[opsz,wght].ttf`. Embed the existing verified bytes so installed Studio does not require its launch directory or an optional copied asset-pack folder to supply the UI font. Use face index zero and default font tweaks. Ordinary controls already use proportional FontIds, so primary-family registration gives them one font without changing project semantics. Any intentionally monospace code/diagnostic view can retain Hack; ordinary clocks/numeric controls should remain Inter.

The current eframe manifest enables `default_fonts`. Local epaint defaults are:

- Proportional: `Ubuntu-Light`, `NotoEmoji-Regular`, `emoji-icon-font`.
- Monospace: `Hack`, `Ubuntu-Light`, `NotoEmoji-Regular`, `emoji-icon-font`.

Recommended proportional ordering is therefore Inter followed by the unchanged default proportional chain. A character absent from Inter can fall back; that may change its metrics/style. Default emoji fonts are not comprehensive script fallback. Do not replace this chain with a single font or advertise full CJK, Arabic, Indic or emoji coverage. A Noto Sans Latin/Greek/Cyrillic file is also not the complete Noto script collection.

The pinned renderer has already moved beyond older ab_glyph tutorials: local `epaint-0.36.2/src/text/font.rs` uses skrifa, vello_cpu and harfrust; `FontData::variation_axes()` reports supported axes and `FontTweak::coords` exposes coordinates. None needs a new dependency for the proposed default instance. The font-face run segmentation source explicitly leaves script-aware RTL/bidi handling as further work. This supports a conservative UI-font choice, not a multilingual typesetting guarantee. Avoid changing renderer/hinting/axis settings as part of this font-only slice. [FontTweak API](https://docs.rs/epaint/0.36.2/epaint/text/struct.FontTweak.html), [pinned egui release](https://github.com/emilk/egui/releases/tag/0.36.2).

Inter's default zeros must be checked in the actual native rendering. Do not claim optional `zero`/`tnum` switches from font existence; the inspected text format has no application feature-toggle contract for them. Stable clock layout can reserve a fixed widget width while keeping proportional glyphs.

## Alternatives considered

| Font | Current official evidence and trade-off | Decision |
| --- | --- | --- |
| Inter 4.1 existing pack | Audited local OFL bytes, UI-oriented design, concrete default Regular instance; no acquisition/runtime prerequisite. | Preferred now. Validate native readability before calling the switch accepted. |
| Segoe UI / Segoe UI Variable | Familiar Windows-native look, but Windows font availability is not font-file redistribution permission. Microsoft's FAQ separates application use from document embedding and prohibits ordinary redistribution of Windows fonts; Segoe UI Variable has additional non-Windows licensing limits. [Microsoft redistribution FAQ](https://learn.microsoft.com/en-us/typography/fonts/font-faq), [Segoe family](https://learn.microsoft.com/en-us/typography/font-list/segoe-ui). | No copying/bundling. Optional installed-system use is a separate platform feature and would make cross-platform appearance/metrics differ. No paid font acquisition is proposed. |
| Noto Sans | Maintained Latin/Greek/Cyrillic project uses OFL-1.1; useful separately audited script-specific fallback families can follow later. More fonts mean additional bytes, atlas work and coverage/licence/testing scope. [Noto upstream](https://github.com/notofonts/latin-greek-cyrillic), [licence](https://raw.githubusercontent.com/notofonts/latin-greek-cyrillic/main/OFL.txt). | Credible future fallback/alternative, not a reason to download a broad font collection now. No universal-coverage claim. |
| Source Sans 3 | Adobe explicitly designs it for UI environments, under OFL-1.1 with reserved name `Source`. Requires new pinned acquisition/inspection and native comparison; no existing app-specific evidence that it improves this UI. [Adobe README](https://raw.githubusercontent.com/adobe-fonts/source-sans/release/README.md), [licence](https://raw.githubusercontent.com/adobe-fonts/source-sans/release/LICENSE.md). | Good alternative if Inter fails measured/native legibility checks; no download required for the current recommendation. |

## Size discipline, native visual acceptance and performance budget

Keep the existing coherent scale initially: Heading 22 pt, Body/Button 15 pt and Small 12 pt. Current manually drawn timeline/detail labels span about 10..14 pt. Inspect every 10/11 pt use at actual Windows DPI; increase the specific label size or provide an unclipped tooltip if it is unreadable. Do not shrink controls to force every string into a compact drawer. Use visual hierarchy through spacing, color and this restrained size scale before adding weight variants.

Acceptance is actual native-window inspection at 1440×900, 1280×800 and 980×640, then 125/150/200% DPI when available. Check header/left/right rail/drawers, media tiles, timeline ruler/clip names, Inspector numeric inputs, notifications and tooltips. Verify text contrast, baseline alignment, no cropped descenders, no clipped buttons, wrapping/ellipsis/full-name access, and menus that remain reachable. Inspect `00:00`, `10:08`, `0.00`, `100%`, `1920 × 1080`, `30000/1001`, `O0`, `I1l`, accented names and multilingual filenames. Plain unslashed zeros, compact labels and long filenames require actual captures; source metadata does not settle their appearance.

Keep a diagnostic specimen for missing-script/combining-mark/emoji cases and report unsupported coverage accurately. Test visible caret/selection/editing behavior where text input is involved. Changing a font may alter layout widths, so compare the same real states and visually review the captures; screenshots should show the actual Inter app, not a generated mockup. Root's export-panel/project-drawer layout work has its own acceptance and must not be declared fixed merely because the font is consistent.

Performance targets are prospective. The regular font adds 876,576 font bytes (about 0.84 MiB) to an embedded binary before compression; decoded outlines/caches/atlas cost more and must be measured. Do not load italic or many named weight instances without a real use. Keep one initialization and bounded reused UI sizes; avoid repeatedly replacing font definitions or varying axes/sizes per frame, which can churn layout/glyph caches.

Compare ten-run cold startup/first-text times, atlas dimensions/bytes after visiting panels, process working set and UI CPU p50/p95 on the same host/states/build. Record actual egui telemetry and before/after captures. Suggested engineering target: warmed ordinary UI CPU p95 below 8 ms at the baseline resolution, no sustained atlas growth after repeating unchanged panels, and no material startup/idle regression. These are unrun targets and a CPU budget, not display/input latency or a 60 FPS guarantee. Existing historical 1.55/4.52 ms p50/p95 was the old-font history workflow and cannot establish Inter performance.

No model/media/backend change or schema migration is required. Root should run normal fmt/strict Clippy/tests for the actual Rust change, build Studio, execute relevant native smoke/captures, inspect the output and record measured limits in TASKS/PROGRESS. Licensing notices accompany future packaging; this research authorizes no public distribution.
