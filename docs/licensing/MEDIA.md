# Media dependency and distribution review

Reviewed: 2026-10-09. Lumeryn original code remains proprietary. This register is an engineering assessment and distribution gate, not a license grant or a conclusion about every jurisdiction.

## Installed external tool

| Property | Observed value |
| --- | --- |
| Executables | Separately installed ffmpeg.exe and ffprobe.exe through WinGet Links |
| Version | 8.1.1-essentials_build-www.gyan.dev |
| Relevant build flags | --enable-gpl --enable-version3 --enable-static --enable-libx264 --enable-libx265 --enable-librubberband; no --enable-nonfree reported |
| Reported license | GNU GPL version 3 or later (`ffmpeg -L`) |
| ffmpeg SHA-256 | 228D7A8556258DE907FDB55F36850078EBC7680B84EC30D84EA02E99BEC1D1EB |
| ffprobe SHA-256 | 0FDE260F5ABD35C9CAFD96F594CC76365A780C1B73A90E35B6A3409EA1DB1BF0 |
| Integration | External command execution for local development; no Rust FFI linkage, downloaded copy, or bundled binary |
| Redistribution approval | Not approved; requires separate owner/legal review |

FFmpeg's base license is LGPL 2.1-or-later; GPL-enabled configurations change the FFmpeg license. The official checklist recommends omitting GPL/nonfree options and dynamic linking for an LGPL library integration, with matching source and notices. Native codec selection does not change the license of the entire installed executable. [Official legal guidance](https://ffmpeg.org/legal.html), [version-specific LICENSE.md](https://github.com/FFmpeg/FFmpeg/blob/n8.1.1/LICENSE.md)

## Codec policy and alternatives

First prototypes explicitly select native `mpeg4`, `aac`, `ffv1`, and `pcm_s16le`. These do not require selecting x264/x265, but they are FFmpeg components under the applicable FFmpeg license, not MIT/Apache codecs. MPEG-4/AAC software-license suitability does not establish patent clearance.

Do not bundle this GPL build, statically link libav into Lumeryn, or ship GPL/nonfree plugins under an assumption that subprocess execution settles legal questions. Using an existing external executable is the current engineering boundary; a commercial distribution or deeper integration requires analysis of that actual arrangement. Document third-party ownership separately from Lumeryn ownership.

| Candidate | Intended role and review |
| --- | --- |
| Audited LGPL-only FFmpeg build | Preferred future FFmpeg packaging candidate; record exact source/configuration, dependency licenses, binary hashes, notices, modifications, replacement/relink rights as applicable. |
| FFV1 plus PCM | Regression/intermediate alternative without MPEG-4/AAC output; still audit the FFmpeg implementation and distribution. |
| OS/hardware encoders | Optional future H.264/HEVC/AV1 delivery; verify vendor SDK, driver redistribution and codec patent terms. |
| OpenH264 or AV1 libraries | Evaluate separately for quality, platform support, build dependencies, and source/binary patent terms before adoption. No package installed or approved here. |

## Before any distribution

The owner must review the selected binary and all compiled-in libraries, its source delivery/offer obligations, attribution and license texts, EULA interaction, and codec patent obligations in intended markets. Keep source/configuration correspondence and security update provenance. A tested native encoder is not evidence that commercial shipping has been legally cleared. The first slice ships no third-party multimedia binary and changes no Lumeryn ownership terms.
