# Curated offline asset library

Acquired and reviewed 2026-10-09. Lumeryn's original code, branding and original template definitions remain proprietary. This library contains third-party files with their original CC0/OFL rights; it does not change the project's license. No original Lumeryn assets have been dedicated to the public domain.

## Included files and permission

| Pack | Installed files | License and provenance | Intended use |
|---|---:|---|---|
| Kenney UI Audio 1.0 | 51 OGG |[Official pack](https://kenney.nl/assets/ui-audio), original `License.txt`, CC0 |Clicks, switches and short interface cues. The actual archive contains 51 Audio files, despite the page's 50-file listing. Preview audio and URL shortcuts were excluded. |
| Kenney Interface Sounds 1.0 | 100 OGG |[Official pack](https://kenney.nl/assets/interface-sounds), original `License.txt`, CC0 |Confirmation, open/close, reveal and interaction cues. |
| Kenney Music Jingles 1.0 | 85 OGG |[Official pack](https://kenney.nl/assets/music-jingles), original `License.txt`, CC0 |Short stingers in several styles. These are not a full-length professional music catalog. |
| Kenney Background Elements 1.0 | 110 PNG |[Official pack](https://kenney.nl/assets/background-elements), original `License.txt`, CC0 |Trees, clouds, buildings and transparent flat/silhouette illustrations for explainer scenes. No sample sheets, logos or master vector source files included. |
| hernandack Short Loops Background Music Pack 1.00 | 4 OGG |[Individual author's listing](https://opengameart.org/content/short-loops-background-music-pack), CC0 |A Brand New Wisdom, Just Saying Tho, Swinging Sweet, Winter Dust. Loops 16.6-24.9s; audition and narration fit review remain required. |
| Inter 4.1 | 2 variable TTF |[Official release](https://github.com/rsms/inter/releases/tag/v4.1), [upstream archive](https://github.com/rsms/inter/files/15438431/Inter-4.1-GoogleFonts.zip), [pinned copyright/OFL](https://raw.githubusercontent.com/rsms/inter/v4.1/LICENSE.txt) |Regular/italic title and body font choices. Latin, Greek, Cyrillic and Vietnamese sample rendering was inspected; no Arabic/Indic coverage claim. |

All 352 catalog files total 6,075,184 bytes. The complete retained directory is 6,533,582 bytes (about 6.23 MiB) including catalogs, notices and audit metadata; exact current bytes and SHA256 are reported by `python scripts/library-assets.py verify`. Source archives are retained only in ignored acquisition scratch, not shipped.

Raw CC0 redistribution and commercial use follow the waiver/fallback license in the [CC0 legal code](https://creativecommons.org/publicdomain/zero/1.0/legalcode.en). Kenney separately confirms commercial use and optional attribution in its [official support page](https://kenney.nl/support); the Kenney logo is excluded. Per-pack notices retain original copyright/permission text unchanged. The complete CC0 text is retained unchanged from the SPDX license-list-data 3.28.0 canonical text, with the authoritative legal-code URL. Credit to Kenney and hernandack is optional but catalog attribution preserves it.

Inter retains its SIL Open Font License 1.1 and upstream copyright. Redistribution of the corresponding font must include that notice; do not sell the fonts alone or relicense the font itself. These fonts are unmodified; changes require a separate naming/notice review. OFL does not require Lumeryn code or exported documents to become open source. See the [official OFL text](https://openfontlicense.org/open-font-license-official-text/).

## Excluded sources

No paid download, donation, account, credential, upload or model acquisition occurred. No Clipchamp media/templates/branding were copied. Microsoft permits appropriate commercial videos exported from Clipchamp but prohibits redistributing individual stock or using it in other editors, as stated in its [stock audio instructions](https://support.microsoft.com/en-us/clipchamp/how-to-add-stock-music-and-audio-to-videos). Current [Pixabay terms](https://pixabay.com/service/terms/) and [Pexels terms](https://www.pexels.com/license/) do not provide the raw stock-library redistribution scope used here, so their catalogs are excluded. Noncommercial, GPL/AGPL, source-available and unclear asset licenses are excluded from this starter.

Future original Lumeryn vectors/templates need a proprietary asset-use permission, distinct from CC0 downloads. TTS/transcription voices, model weights and supporting runtimes require individual model/dataset/dependency reviews and remain outside this library. No blanket inference from a model repository's code license is sufficient.

## Provenance, install and source safety

`assets/library/catalog.json` is version 1 with stable IDs, slash-relative paths, pack/category/tags, provider/source URL, license ID and notice path, SHA256, exact byte counts and measured audio/image metadata. `sources.json` pins each reviewed archive/direct download; `HASHES.json` covers every shipped file including notices and metadata. The provider pages establish provenance; locally measured checksums establish reproducibility/integrity and do not authenticate a creator's ownership by themselves.

The acquisition script accepts only reviewed HTTPS hosts, bounded GET responses, expected SHA256 and safe archive members. It rejects escapes/ADS paths, symlinks, case-insensitive duplicates, oversized archives/members and suspicious compression ratios. It installs through a private staging directory into a fresh destination and refuses an existing destination. Installed size is capped at 64 MiB. It never removes an installed library, changes customer source files or uploads media. The script's temporary directories contain only files it created.

Library media remains external immutable source material. Importing a library item into a project must create a stable reference/copy policy through the shared Rust project service; aliases and project-reference deletion must not rename/delete the library originals. Removing a installed pack is a separate future operation: it must check project references and require a deliberate user action. No automatic uninstall/delete command exists in this script.
