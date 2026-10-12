# Native Windows launch

Reviewed 2026-10-12 against the current source and official documentation. This is launch usability, not an installer or desktop-session workaround.

## Findings

- `apps/lumeryn-studio/src/main.rs` lacked a GUI subsystem declaration. Studio now uses `#![cfg_attr(windows, windows_subsystem = "windows")]` in debug **and** release. A release-only declaration would leave the owner's debug executable opening a console. The separate `lumeryn.exe` remains a console CLI. [Rust Reference](https://doc.rust-lang.org/reference/runtime.html#the-windows_subsystem-attribute).
- Zero-argument startup already exists: `Options.project` is optional, `Studio::new` opens only an explicitly supplied project, and the preview contains Welcome/Open project/New project controls using native Windows `rfd` pickers. Replacing that flow or adding demo media is unnecessary.
- `Session::open` dispatches to the backend worker; `Client::connect` discovers/starts the authenticated project service. The Windows service launcher already uses `CREATE_NO_WINDOW`. That backend remains independent from the graphical window.
- Windows desktop access depends on the actual window station/logon session and inherited desktop, rather than whether an AI launched the process. A noninteractive service session differs from an interactive desktop app's child process. Native capture proves that a rendering window existed, not that it appeared in front of the owner's other windows. [Microsoft process connection rules](https://learn.microsoft.com/en-us/windows/win32/winstation/process-connection-to-a-window-station).
- The existing saved window geometry inspected on this machine was outer `(0,0)` and inner approximately `1902×1000`, without fullscreen/maximized flags. It did not establish an offscreen-placement defect. The saved workspace and owner projects are preserved.

## Opening the current build

Build the workspace together, so Studio has the matching sibling project-service executable:

```powershell
cargo build --workspace --locked
```

Double-click `launch-studio.cmd`, or run `./launch-studio.ps1`. The command entry briefly starts a hidden PowerShell supervisor; the Studio window itself is not hidden. With no explicit project the Welcome screen opens. No build/download/project creation/migration occurs automatically.

The launcher prefers a normal `target/release` executable, then `target/debug`. Both must actually be GUI PE builds; stale console executables fail with a rebuild instruction. For a tested isolated build, explicitly select it:

```powershell
./launch-studio.ps1 -StudioExecutable 'D:\Dev\freela\Lumeryn\.lumeryn-scratch\composition-target\debug\lumeryn-studio.exe'
```

`LUMERYN_STUDIO_PATH` provides the same explicit selection. `-Project` accepts one existing `.lumeryn` file; it does not migrate it. `-CheckOnly` validates the executable/service/project and reports the chosen paths and arguments without launching or writing a log.

```powershell
./launch-studio.ps1 -CheckOnly
./scripts/create-studio-shortcut.ps1
```

Shortcut creation is opt-in, points straight to the verified GUI binary, and refuses to overwrite an existing shortcut. `-StudioExecutable` selects an isolated build; `-Destination` permits a chosen `.lnk` path. A future rebuild must preserve that binary path; this is a development convenience, not installation/update management.

## Diagnostics and limits

Normal native initialization failures now show a Windows error dialog and return a failing exit code. Existing redirected stderr remains usable; `--help`/`--version` should be read through a pipe or redirect when invoked from a console because a GUI-subsystem executable does not attach to that console. Automated smoke/capture requests do not show blocking startup-error dialogs, including malformed requests containing their exact mode flag; their exit codes and artifact reports remain authoritative.

`--launch-log <new-file>` supplies a bounded native startup sink for a launcher that intentionally inherits no standard handles. The parent creates only the containing local directory; Studio atomically creates the new file and refuses to overwrite an existing one. A synced JSON line records `startup_requested`; initialization failures append `startup_failed` with at most4096 Unicode characters of diagnostic text. The sink does not install a global tracing subscriber, dump arguments, record project payloads or capture ongoing editing/export logs. Directory/file errors fail startup explicitly; the filename may remain absent if Windows never starts the process. CLI launch inspection and actual readiness remain separate from this requested-start record.

The launcher drains stdout/stderr concurrently to fresh `%LOCALAPPDATA%/Lumeryn/logs/<id>.stdout.log` and `.stderr.log` files and reports a nonzero exit with the log path. It does not attach a console or alter the window station. Logs are local and may contain failure paths; automatic log rotation and installer-managed shortcuts remain staged. Directly double-clicking the executable has startup-error dialogs but no persistent redirected log supervisor.

## Executed evidence

- Focused `cargo build --locked -p lumeryn-studio`, strict all-target Clippy and default Studio tests passed again after the startup sink: 39 tests passed, two external-media tests remained ignored in this focused run. Root owns the wider media/full-workspace gates. Owned Rust formatting and whitespace checks passed.
- Actual rebuilt `.lumeryn-scratch/composition-target/debug/lumeryn-studio.exe` PE header has subsystem `2` (Windows GUI). Redirected `--help` returned exit0 with usage; malformed `--capture-only` returned exit2 with the missing artifact argument on stderr, without a blocking error dialog.
- A zero-project native980×640 capture exited0, reported `passed=true`, `session_error=null`, `project_revision=null`, no pending imports and no generated project. Actual pixels were visually inspected: Welcome/Open project/New project controls are clear and there is no error banner. Evidence is local `.lumeryn-scratch/launch-91cf4a23de91416c950efe8fec684d98/studio-capture.json` and `screenshot-capture.png`; it does not prove the owner saw it on the foreground desktop.
- Both PowerShell scripts parsed successfully. Launcher preflight rejected missing executable, the old console-linked default debug build, missing project and wrong extension. Explicit environment selection, an existing literal filename containing spaces/ampersand/semicolon, and GUI/service pairing passed. The literal filename fixture tested argument handling only, not database validity.
- Shortcut creation/target/empty arguments and overwrite refusal passed in a disposable directory. Root then created `C:/Users/Thumbeja/Desktop/Lumeryn Studio.lnk` pointing directly at the refreshed normal GUI executable with no arguments. The owner confirmed the earlier editor was visible; physical double-click, foreground placement and interactive startup-error dialog usability remain manual checks. Saved workspace/project bytes were not rewritten by shortcut creation.
- Startup-log unit coverage passed create-new refusal/preserved bytes and bounded Unicode JSON lines. Actual automated invocations rejected an existing log with exit1 and unchanged contents; a disposable artifact preflight failure returned exit1 and wrote the requested/failure records without blocking on a modal. Evidence is local `.lumeryn-scratch/launch-log-ede0fed95ce74b81b6995788df346f50`. The CLI specialist separately verifies the non-inherited-handle launch while the real GUI remains alive; a requested-start log alone is not successful initialization evidence.

## Background decoder console correction

The owner subsequently confirmed the editor itself appeared, but reported two extra console windows on startup and layer creation. Root inspected actual process ancestry: the extra `conhost` processes belonged to FFmpeg children of the owner's Studio, rather than the Studio GUI binary. This was a separate, reproducible worker-launch defect. A GUI subsystem declaration on Studio alone cannot stop console-subsystem child tools from allocating their own consoles.

One core factory, `media::worker_command`, now constructs standard process commands with Windows `CREATE_NO_WINDOW` (`0x08000000`); `async_worker_command` is its Tokio adapter. It allocates no console for FFmpeg/ffprobe/helper console workers while preserving normal argument arrays, environment, pipes, EOF, cancellation, timeouts and kill-on-drop semantics. The policy does not hide Studio or import any window/UI library into the headless core. Microsoft documents that this flag leaves the console handle unset for console applications; it differs from window-show/hide controls. [Official creation flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags), [Tokio command API](https://docs.rs/tokio/latest/tokio/process/struct.Command.html).

Audit of all production Rust process constructors found and corrected four sites: core canonical-audio decoding, core generic media/probe/render execution, preview video/audio `WorkerPipe`, and Studio waveform decoding. Thumbnail workers and the native verification CLI helper already route through core `MediaTools::run`, so they inherit the correction. Remaining direct tool constructors in production source directories are test fixtures under `cfg(test)` (preview audio and Studio assets); the separate authenticated service and interactive Studio launchers have their own reviewed launch policy. No production source/media arguments or lifecycle changes were added.

Executed checks after this correction:

- Strict all-target Clippy and default tests passed for core, preview and Studio; owned formatting/whitespace passed. The newly built Studio is isolated under `.lumeryn-scratch/composition-target`, leaving the owner's running normal-build editor intact.
- Actual Windows test `media::tests::worker_from_no_console_parent_preserves_pipes_without_console` starts a detached test parent, asserts its `GetConsoleWindow()` is null, launches a console-subsystem child through the real factory, and asserts that child's console handle is also null. Redirected stdout/stderr markers reach EOF and the process succeeds under a ten-second timeout. The only Win32 window query is isolated test FFI, with a safety contract; production uses the standard process API and adds no dependency. Precompiled rerun passed in approximately30ms. Exact local output and structured evidence: `.lumeryn-scratch/worker-console-de28380d1afb49c58ca84cba1e44d87d/console-probe.txt` and `evidence.json`.
- Seventeen focused actual checks passed with configured FFmpeg9.0.2: that console test, audio-probe3, import-stages4, composition-preview1, generated-preview6 and native asset-worker2. They cover decoded audio/probe timeout rejection, source changes, timing, forward/backward composite seek parity, empty-project decode cancellation/recovery, EOF, native device play/pause/seek, actual waveforms and static PNG thumbnails.

Final full locked fmt/strict-Clippy/default/media/deny gate passed225distinct tests; the21-check actual-window composition/UHD workflow passed on the corrected binary. Native/debug hashes and measurements are in composition-evidence.json. After observing that the previous owner editor had exited, root preserved rollback binaries, copied the hash-matched CLI/GUI pair to normal target/debug, and reopened Atlas at inspected revision9. The receipt `.lumeryn-scratch/studio-owner-console-fixed.json` records stable visible/non-minimized/uncloaked desktop window observation (foreground false). No owner project/source/console/editor process was edited or closed by this correction.

Read-only process observation found the new decoder console-host processes with zero main-window handles and no WindowsTerminal/OpenConsole process; headless host existence is not itself a visible popup. The genuine factory console-handle test and actual parent-budgeted agent launch/EOF test passed. The owner subsequently answered “Yes, no extra terminals” for the refreshed editor. This physical confirmation complements the automated worker and launch checks; broader device/session acceptance remains unexecuted. Local current CLI installation/discovery passed; automatic retention, signing/installer and broader desktop/device acceptance remain staged.

After the responsive media-footer and actual PNG source-gesture corrections, the final full gate passed225distinct tests (171default+54additional); final native composition/UHD acceptance passed21checks in `.lumeryn-scratch/studio-02622dd60b2a435bbe5d4259dd96c270/`. Root observed no running Studio, preserved another rollback binary, copied the hash-matched corrected GUI into normal target/debug and launched through the installed CLI. `.lumeryn-scratch/studio-owner-final-layout.json` observes native Studio PID32816 at headlessly inspected Atlas revision20 without a project edit. Earlier owner revisions9/19/20 reflect their intervening edits, not root edits. The direct Desktop shortcut resolves the same updated binary. Media grid scrolling and fully visible actions were inspected at980×640/1280×720; subjective pointer/DPI acceptance remains separate.
