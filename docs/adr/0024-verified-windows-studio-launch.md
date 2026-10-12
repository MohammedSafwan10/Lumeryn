# ADR0024: Native Studio launch and observable window status

Date: 2026-10-12. Status: accepted; implementation and final launch acceptance recorded in STUDIO_LAUNCH.

## Context and decision

The owner and external agents saw a console after requesting Studio and were told it had opened. A process PID, successful headless `project open`, or an automated framebuffer capture cannot establish that the requested editor is visible on the owner's desktop. The current Studio already has a zero-project Welcome/Open/New flow; replacing it would add no benefit. The project service already launches with `CREATE_NO_WINDOW` independently of Studio.

Studio uses the Windows GUI subsystem in debug and release, so the common development binary does not allocate a console. Interactive native initialization errors use the existing rfd dialog; automated smoke/capture keep redirected diagnostics, failing exit codes and artifact checks without blocking dialogs. A separate common media-worker factory also sets Windows CREATE_NO_WINDOW for FFmpeg/ffprobe and bounded tool workers. GUI-subsystem Studio alone cannot prevent a console-subsystem decoder from creating a terminal; all production source decode/streaming/thumbnail/waveform/probe/render/encoder checks use the factory. This is standard process configuration, not a GUI dependency in the headless core.

A root launcher selects an already-built, PE-checked Studio and matching sibling CLI, opens Welcome unless the user explicitly supplies a project, and drains local diagnostic logs. An opt-in shortcut links directly to the GUI binary. No implicit compilation, download, project selection, migration, content insertion or paid service.

`lumeryn studio open --project <path> --executable <trusted GUI path>` validates the selected snapshot headlessly then launches with argument arrays. It observes only its own child process's exact-title native window, requiring visible/unminimized/uncloaked state intersecting the virtual desktop for a stable interval. Early exit and bounded timeout fail with PID/local diagnostics. The native spawn disables inherited handles so a long-lived GUI cannot keep an agent's CLI stdout/stderr pipes open. A create-new local startup log is owned by Studio rather than inherited standard handles. No global PID guessing, shell project arguments, arbitrary PATH search, focus-stealing hacks or destructive process termination. `LUMERYN_STUDIO_PATH` or a sibling GUI executable may select the build; the installed standalone CLI otherwise needs an explicit Studio path.

Result semantics are `native_window_observed`, not a guarantee that the owner saw unobscured pixels, that their displayed session is this session, or that the selected project finished decoding. Foreground/session/rectangle observations and unknown frame/project-loaded/user-screen fields are explicit. Native commands/editing continue through the same service; opening does not mutate/migrate the project. Physical user feedback remains the final visibility check.

## Alternatives and limits

- A release-only subsystem leaves debug launches opening consoles. Both builds are GUI; the CLI remains console-based.
- Forcing every GUI through a hidden process launch can hide the requested editor. Hidden processes are for the service/logging supervisor; Studio is launched normally.
- Assuming all agent subprocesses are isolated from the interactive desktop is incorrect. Windows window-station/logon-session rules determine access. Conversely, framebuffer captures alone do not prove foreground visibility.
- A new launch IPC registry, installer, embedded agent or second project service is unnecessary. Existing OS queries provide a narrow observable contract. Install/update/signing remain separate roadmap work.
- Startup dialog usability, unobscured physical display and cross-session/RDP behavior need explicit manual checks. On timeout only the launched child remains for diagnosis, and agents must not report that the editor opened.

The CLI reuses the pinned existing windows-sys0.61.2 with narrowly enabled window/DWM/session APIs; reviewed required FFI is isolated from the headless Rust core. No new third-party library/version/license exception, public source license or backend replacement. Per-user logs remain local; retention/rotation is staged.

## Official references

[Rust GUI subsystem](https://doc.rust-lang.org/reference/runtime.html#the-windows_subsystem-attribute), [Microsoft window-station process rules](https://learn.microsoft.com/en-us/windows/win32/winstation/process-connection-to-a-window-station), [EnumWindows](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumwindows), [IsWindowVisible](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-iswindowvisible), [DWM attributes](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute).

[Executed evidence and limits](../testing/STUDIO_LAUNCH.md).
