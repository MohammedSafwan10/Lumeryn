# Owner-authorized Windows FFmpeg tool update

Executed: 2026-10-10. The owner explicitly requested updating their PC's FFmpeg to the latest release during composition planning. This is an external local tool installation, not a Lumeryn dependency/backend change or a bundled redistribution. No source project/media was modified, prior tool uninstalled, running Studio/service stopped, public upload made or charge incurred.

## Selection and provenance

The official FFmpeg download page identifies **9.0.2** as the latest stable major release, dated 2026-09-18, and links Gyan's Windows builds. The maintained 8.1 branch has 8.1.3, but that is not the latest major release. [FFmpeg downloads](https://ffmpeg.org/download.html).

Gyan's release section provides 9.0.2 Essentials dated 2026-09-19. Selected the release build, avoiding the separate 2026-10-08 development snapshot. Essentials retains the existing installed variant. Gyan describes these Windows binaries as GPLv3, with Windows 10+ as their supported baseline. [Provider builds](https://www.gyan.dev/ffmpeg/builds/).

The official security page records fixes on 9.0 and 8.1.2 and explains that vulnerability findings must be reproduced and verified. The update does not establish that every vulnerability is absent or that arbitrary untrusted media is safe. [FFmpeg security](https://ffmpeg.org/security.html). Exact deployed build flags/licence/codec obligations still require review before any future redistribution. [FFmpeg legal guidance](https://ffmpeg.org/legal.html).

WinGet had the installed `Gyan.FFmpeg.Essentials`8.1.1 and offered only 9.0.1 during inspection. Downloaded the provider's released 9.0.2 archive directly rather than representing the older package-manager candidate as latest.

Verified archive:

| Evidence | Value |
| --- | --- |
| HTTPS package | `https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-9.0.2-essentials_build.7z` |
| Provider checksum | [9.0.2 Essentials SHA256](https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-9.0.2-essentials_build.7z.sha256) |
| Actual archive bytes |35,430,500 |
| Offered and computed SHA256 | `4705843ccaaf54257c16ad90f3e952ece33c17df964ecf7bfdbb0f49c7171077` |

Checksum matched before extraction or execution; archive paths were checked for the expected top-level directory and absence of parent traversal. This is provider HTTPS/checksum integrity verification, not an upstream PGP signature on the third-party executable. FFmpeg's signed source tarball is a different artifact.

Two slow ZIP download attempts were interrupted through their own execution sessions after the smaller verified 7z succeeded. Only those newly started download sessions were stopped. Their partial archives remain external and are not on PATH, executed or committed. The versioned installed tree came from the verified 7z through Windows' existing `tar`/libarchive; no archive utility was installed.

## Installed pair and preserved prior installation

New versioned bin directory:

`C:\Users\Thumbeja\AppData\Local\LumerynTools\ffmpeg\9.0.2\ffmpeg-9.0.2-essentials_build\bin`

| Tool | Actual version | Executable SHA256 |
| --- | --- | --- |
| `ffmpeg.exe` | `9.0.2-essentials_build-www.gyan.dev` | `3256173F3F8BFFD7DF12227C68ADF68025EDB1832273A9530688A7BB1ED8EDEC` |
| `ffprobe.exe` | `9.0.2-essentials_build-www.gyan.dev` | `F0D36ECBBDD3BCFAC3EFA078C96C7271C2E68B3810595552AC3B7F17E9A65C52` |

Both were extracted and version-checked before activation. A single user PATH prepend selects their shared bin directory; there is no interval in the activation where one new executable is selected independently of the other. Machine PATH and existing WinGet links were not changed. No permanent `LUMERYN_FFMPEG` or `LUMERYN_FFPROBE` override was added; existing values were absent in Process/User/Machine scopes.

Previous 8.1.1 remains intact at:

`C:\Users\Thumbeja\AppData\Local\Microsoft\WinGet\Packages\Gyan.FFmpeg.Essentials_Microsoft.Winget.Source_8wekyb3d8bbwe\ffmpeg-8.1.1-essentials_build\bin`

The WinGet links still target that directory. Its versioned files and package registration are preserved; a future WinGet upgrade will not automatically manage the separately installed LumerynTools directory.

External installation/rollback evidence directory:

`C:\Users\Thumbeja\AppData\Local\LumerynTools\downloads\ffmpeg-9.0.2-9b1e59dec2084bfe87dc13c7ef369cf1`

It contains the verified archive, previous user PATH/old target backup `user-path-before.json`, generated fixture, explicit-path and reconstructed-path Lumeryn doctor/probe reports. Binaries/fixtures/archives are outside the repository and not included in source commits.

## Executed checks and remaining acceptance

1. Both versioned executables report 9.0.2 successfully.
2. Generated a new 128×72 FFV1/stereoPCM fixture,30FPS,30frames,one second,48kHz audio. The new FFmpeg fully decoded both streams without errors. New ffprobe independently confirmed dimensions, exact rate/frame count, duration, channel count and sample rate.
3. Installed Lumeryn `doctor` with explicit new tool paths passed. `media probe` verified the real fixture with matching CFR/30frames, zero-origin video/audio, square pixels and source BLAKE3. This did not mutate an owner's project.
4. Persisted user PATH was read back and matched exactly one new bin prefix plus its previous value. Both previous 8.1.1 executables remain present.
5. A child Windows PowerShell launched with reconstructed Machine+UserPATH resolved both new 9.0.2 paths. Default-path installed Lumeryn `doctor` and `media probe` passed in that environment.
6. Separately, a new shell inheriting the **already-open Codex process** still resolved 8.1.1. Persisting user PATH does not rewrite an existing process environment. Existing Studio/service processes were not stopped or asserted to have switched versions.

The lead additionally executed six existing compiled suites with explicit 9.0.2 paths: workflow (2), media_regressions (4), UHD (4), audio_composition (3), import_stages (4) and fps_workflow (3), totalling **20 passed tests**. Logs are retained locally under `.lumeryn-scratch/ffmpeg9-regression/*.log`. No Rust rebuild or owner project edit was needed. These are executed 9.0.2 compatibility checks, not a relabelling of earlier 8.1.1 evidence.

These checks establish local installation, tool-pair activation under the tested refreshed environment and compatibility for those bounded workflows, including the existing UHD tests. They do **not** establish every editor/preview/device path, every GPU encoder/filter, physical A/V or newly planned composition functionality. Broader acceptance and the lead's further composition experiments are recorded separately.

## Using the new pair in an existing session

Existing shells/apps can retain inheritedPATH until their environment is refreshed or the application is relaunched from a refreshed parent. A sign-in session refresh may be necessary; no logout/restart was performed here. Lumeryn explicitly supports these flags, which avoid ambiguity even if a previously launched project service retains its earlierPATH:

```powershell
lumeryn --ffmpeg 'C:\Users\Thumbeja\AppData\Local\LumerynTools\ffmpeg\9.0.2\ffmpeg-9.0.2-essentials_build\bin\ffmpeg.exe' --ffprobe 'C:\Users\Thumbeja\AppData\Local\LumerynTools\ffmpeg\9.0.2\ffmpeg-9.0.2-essentials_build\bin\ffprobe.exe' doctor
```

For temporary current-shell selection, set `LUMERYN_FFMPEG`/`LUMERYN_FFPROBE` to those full paths before starting CLI/Studio or running media acceptance. This changes no persisted override. Already-open Studio retains the config captured when it started; do not close it without the owner's choice merely to refresh tools.

## Reversible rollback without deletion

Remove **only** the newly prepended bin component from user PATH; leave every later component and both installations intact. Do not replace user PATH wholesale from the backup if another tool has changed it since this installation. The following checks the exact first component and refuses an unexpected layout:

```powershell
$newFfmpegBin = 'C:\Users\Thumbeja\AppData\Local\LumerynTools\ffmpeg\9.0.2\ffmpeg-9.0.2-essentials_build\bin'
$currentUserPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
$expectedPrefix = $newFfmpegBin + ';'
if (!$currentUserPath.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'User PATH layout changed; inspect it before removing the specific tool component.'
}
[Environment]::SetEnvironmentVariable('PATH', $currentUserPath.Substring($expectedPrefix.Length), 'User')
```

Refresh/relaunch relevant shells before comparing resolution. With the added directory removed, the preserved WinGet links provide 8.1.1 under the previous search order. This rollback was documented, not executed; installation remains active. No recursive delete, symlink replacement, uninstall, force-kill or project migration is part of it.
