//! Windows native launch verification. Window APIs stay outside the headless engine.
use lumeryn_core::{Error, Result};
use serde_json::Value;
use std::{path::Path, time::Duration};

/// Launch only an explicitly selected project using a trusted GUI executable.
/// Success means Windows observed our child's visible, unminimized, uncloaked,
/// on-desktop window; it cannot prove that a person saw unobscured pixels.
pub async fn open(project: &Path, executable: Option<&Path>, timeout: Duration) -> Result<Value> {
    if !(Duration::from_secs(1)..=Duration::from_secs(60)).contains(&timeout) {
        return Err(Error::Invalid("Studio wait must be 1..60 seconds".into()));
    }
    let project = project
        .canonicalize()
        .map_err(|error| Error::NotFound(format!("project {}: {error}", project.display())))?;
    if !project.is_file() {
        return Err(Error::Invalid(
            "Studio project must be an existing file".into(),
        ));
    }
    #[cfg(windows)]
    {
        windows::open(&project, executable, timeout).await
    }
    #[cfg(not(windows))]
    {
        let _ = (project, executable);
        Err(Error::Unsupported(
            "verified native Studio launching is currently Windows-only".into(),
        ))
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use serde_json::json;
    use std::{
        fs::File,
        io::{Read, Seek, SeekFrom},
        os::windows::ffi::OsStrExt,
        path::PathBuf,
        ptr,
        time::Instant,
    };
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, HANDLE, HWND, LPARAM, RECT, SetLastError, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
        System::{
            RemoteDesktop::{ProcessIdToSessionId, WTSGetActiveConsoleSessionId},
            Threading::{
                CREATE_NO_WINDOW, CreateProcessW, GetExitCodeProcess, PROCESS_INFORMATION,
                STARTUPINFOW, WaitForSingleObject,
            },
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetForegroundWindow, GetSystemMetrics, GetWindowRect, GetWindowTextW,
            GetWindowThreadProcessId, IsIconic, IsWindowVisible, SM_CXVIRTUALSCREEN,
            SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        },
    };

    const TITLE: &str = "Lumeryn Studio";

    struct ProcessHandle(HANDLE);
    impl Drop for ProcessHandle {
        fn drop(&mut self) {
            // SAFETY: Each non-null process/thread handle returned by successful
            // CreateProcessW is uniquely owned and closed exactly once here.
            if unsafe { CloseHandle(self.0) } == 0 {
                tracing::warn!(error = %std::io::Error::last_os_error(), "close Studio launch handle failed");
            }
        }
    }

    struct Child {
        process: ProcessHandle,
        pid: u32,
    }
    impl Child {
        fn try_wait(&self) -> Result<Option<u32>> {
            // SAFETY: This is the live process handle owned by this Child. A
            // zero-duration wait checks termination without blocking the caller.
            match unsafe { WaitForSingleObject(self.process.0, 0) } {
                WAIT_TIMEOUT => Ok(None),
                WAIT_OBJECT_0 => {
                    let mut status = 0;
                    // SAFETY: A terminated owned process and valid DWORD output.
                    if unsafe { GetExitCodeProcess(self.process.0, &mut status) } == 0 {
                        return Err(std::io::Error::last_os_error().into());
                    }
                    Ok(Some(status))
                }
                _ => Err(std::io::Error::last_os_error().into()),
            }
        }
    }

    fn quote_file(path: &Path) -> Result<String> {
        let text = path
            .to_str()
            .ok_or_else(|| Error::Invalid("Studio paths must be valid Unicode".into()))?;
        if text.contains(['\0', '"']) || text.ends_with(['\\', '/']) {
            return Err(Error::Invalid(
                "Studio launch requires a literal file path".into(),
            ));
        }
        // Existing file paths cannot contain quotes/NUL or end in a separator.
        // C-runtime quotes therefore preserve all whitespace/shell punctuation.
        Ok(format!("\"{text}\""))
    }

    fn spawn(executable: &Path, project: &Path, diagnostics: &Path) -> Result<Child> {
        let application = executable
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut arguments = format!(
            "{} --project {} --launch-log {}",
            quote_file(executable)?,
            quote_file(project)?,
            quote_file(diagnostics)?
        )
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
        let startup = STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            ..Default::default()
        };
        let mut information = PROCESS_INFORMATION::default();
        // SAFETY: Explicit trusted executable and writable NUL-terminated
        // command line contain only validated file arguments, never shell code.
        // All pointers stay valid during synchronous CreateProcessW. FALSE
        // inheritance prevents ancestor CLI capture pipes (not just standard
        // handles) from surviving in the GUI and hanging agent output readers.
        // Startup diagnostics are written by Studio itself via --launch-log.
        let created = unsafe {
            CreateProcessW(
                application.as_ptr(),
                arguments.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                0,
                CREATE_NO_WINDOW,
                ptr::null(),
                ptr::null(),
                &startup,
                &mut information,
            )
        };
        if created == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let process = ProcessHandle(information.hProcess);
        let _thread = ProcessHandle(information.hThread);
        Ok(Child {
            process,
            pid: information.dwProcessId,
        })
    }

    fn executable(explicit: Option<&Path>) -> Result<PathBuf> {
        let selected = match explicit {
            Some(path) => path.to_path_buf(),
            None => match std::env::var_os("LUMERYN_STUDIO_PATH") {
                Some(path) => PathBuf::from(path),
                None => std::env::current_exe()?
                    .parent()
                    .ok_or_else(|| Error::Invalid("CLI executable has no parent directory".into()))?
                    .join("lumeryn-studio.exe"),
            },
        };
        let selected = selected.canonicalize().map_err(|error| Error::NotFound(format!(
            "Studio executable {}: {error}. Supply --executable <trusted lumeryn-studio.exe> or LUMERYN_STUDIO_PATH; no PATH or working-directory search is performed",
            selected.display()
        )))?;
        if !selected.is_file()
            || !selected
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        {
            return Err(Error::Invalid(
                "Studio executable must be an existing .exe file, never a shell script".into(),
            ));
        }
        require_gui_subsystem(&selected)?;
        Ok(selected)
    }

    /// Check only the bounded PE header, not a signature/authenticity guarantee.
    fn require_gui_subsystem(path: &Path) -> Result<()> {
        let mut file = File::open(path)?;
        let invalid = || {
            Error::Invalid(format!(
                "{} is not a Windows GUI-subsystem executable. Rebuild the native Studio; stale console builds can create the extra terminal",
                path.display()
            ))
        };
        let mut dos = [0_u8; 64];
        file.read_exact(&mut dos).map_err(|_| invalid())?;
        if &dos[..2] != b"MZ" {
            return Err(invalid());
        }
        let offset = u64::from(u32::from_le_bytes([dos[60], dos[61], dos[62], dos[63]]));
        if !(64..=1_048_576).contains(&offset) {
            return Err(invalid());
        }
        file.seek(SeekFrom::Start(offset))?;
        let mut header = [0_u8; 94];
        file.read_exact(&mut header).map_err(|_| invalid())?;
        let optional_size = u16::from_le_bytes([header[20], header[21]]);
        let magic = u16::from_le_bytes([header[24], header[25]]);
        let subsystem = u16::from_le_bytes([header[92], header[93]]);
        if &header[..4] != b"PE\0\0"
            || optional_size < 70
            || !matches!(magic, 0x10b | 0x20b)
            || subsystem != 2
        {
            return Err(invalid());
        }
        Ok(())
    }

    struct Window {
        handle: usize,
        visible: bool,
        minimized: bool,
        on_desktop: bool,
        cloaked: Option<bool>,
        foreground: bool,
        rect: RECT,
    }
    impl Window {
        fn observable(&self) -> bool {
            self.visible && !self.minimized && self.on_desktop && self.cloaked == Some(false)
        }
        fn json(&self) -> Value {
            json!({
                "handle": self.handle, "title": TITLE, "visible": self.visible,
                "minimized": self.minimized, "intersects_virtual_desktop": self.on_desktop,
                "cloaked": self.cloaked, "foreground": self.foreground,
                "rect": {"left": self.rect.left, "top": self.rect.top,
                    "right": self.rect.right, "bottom": self.rect.bottom}
            })
        }
    }
    struct Query {
        pid: u32,
        windows: Vec<Window>,
    }

    unsafe extern "system" fn visit(handle: HWND, context: LPARAM) -> i32 {
        // SAFETY: EnumWindows is synchronous; `context` points to the live,
        // uniquely borrowed Query in observe(). No pointer escapes this call.
        let query = unsafe { &mut *(context as *mut Query) };
        let mut pid = 0;
        // SAFETY: OS provides a window handle; out pointer is valid stack storage.
        if unsafe { GetWindowThreadProcessId(handle, &mut pid) } == 0 || pid != query.pid {
            return 1;
        }
        let mut title = [0_u16; 256];
        // SAFETY: The writable title buffer has exactly the supplied capacity.
        let length = unsafe { GetWindowTextW(handle, title.as_mut_ptr(), 256) };
        if length <= 0 || String::from_utf16_lossy(&title[..length as usize]) != TITLE {
            return 1;
        }
        let mut rect = RECT::default();
        // SAFETY: OS handle is used only for this query; RECT is valid out storage.
        if unsafe { GetWindowRect(handle, &mut rect) } == 0 {
            return 1;
        }
        let mut cloaked = 0_u32;
        // SAFETY: DWMWA_CLOAKED writes one DWORD into the correctly sized/aligned
        // local storage. Query failure is unknown, never treated as uncloaked.
        let cloak_status = unsafe {
            DwmGetWindowAttribute(
                handle,
                DWMWA_CLOAKED as u32,
                (&mut cloaked as *mut u32).cast(),
                size_of::<u32>() as u32,
            )
        };
        // SAFETY: These Win32 read-only queries take no pointers; stale handles
        // yield false. The desktop dimensions are queried afresh for each window.
        let (visible, minimized, foreground, left, top, width, height) = unsafe {
            (
                IsWindowVisible(handle) != 0,
                IsIconic(handle) != 0,
                GetForegroundWindow() == handle,
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };
        let on_desktop = width > 0
            && height > 0
            && rect.right > rect.left
            && rect.bottom > rect.top
            && rect.right > left
            && rect.bottom > top
            && rect.left < left.saturating_add(width)
            && rect.top < top.saturating_add(height);
        query.windows.push(Window {
            handle: handle as usize,
            visible,
            minimized,
            on_desktop,
            cloaked: (cloak_status == 0).then_some(cloaked != 0),
            foreground,
            rect,
        });
        1
    }

    fn observe(pid: u32) -> Result<Vec<Window>> {
        let mut query = Query {
            pid,
            windows: Vec::new(),
        };
        // SAFETY: `visit` always continues enumeration and uses the Query pointer
        // only during this synchronous call, while Query remains uniquely owned.
        unsafe { SetLastError(0) };
        if unsafe { EnumWindows(Some(visit), (&mut query as *mut Query) as LPARAM) } == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(0) {
                return Err(error.into());
            }
        }
        Ok(query.windows)
    }

    fn session(pid: u32) -> Value {
        let mut id = 0;
        // SAFETY: PID is a value and session ID has valid writable stack storage.
        if unsafe { ProcessIdToSessionId(pid, &mut id) } == 0 {
            json!({"id": null, "error": std::io::Error::last_os_error().to_string()})
        } else {
            json!({"id": id, "error": null})
        }
    }

    fn active_console_session() -> Option<u32> {
        // SAFETY: This read-only Windows query takes no pointers. UINT_MAX
        // explicitly represents no attached console session, not session zero.
        let id = unsafe { WTSGetActiveConsoleSessionId() };
        (id != u32::MAX).then_some(id)
    }

    pub(super) async fn open(
        project: &Path,
        explicit: Option<&Path>,
        timeout: Duration,
    ) -> Result<Value> {
        let executable = executable(explicit)?;
        let directory = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| {
                Error::Invalid("LOCALAPPDATA is required for per-user launch diagnostics".into())
            })?
            .join("Lumeryn")
            .join("diagnostics");
        std::fs::create_dir_all(&directory)?;
        let log_path = directory.join(format!("studio-launch-{}.log", uuid::Uuid::new_v4()));
        // Studio owns create_new so startup diagnostics never overwrite an
        // existing file; if OS process creation fails no log is produced.
        let child = spawn(&executable, project, &log_path).map_err(|error| {
            Error::Io(std::io::Error::other(format!(
                "Studio launch failed: {error}; diagnostics {}",
                log_path.display()
            )))
        })?;
        let pid = child.pid;
        let start = Instant::now();
        let mut stable_window: Option<(usize, Instant)> = None;
        loop {
            if let Some(status) = child.try_wait()? {
                return Err(Error::Io(std::io::Error::other(format!(
                    "Studio child PID {pid} exited ({status}) before a usable native window was observed; diagnostics {}",
                    log_path.display()
                ))));
            }
            let windows = observe(pid)?;
            if let Some(window) = windows.iter().find(|window| window.observable()) {
                match stable_window {
                    Some((handle, since))
                        if handle == window.handle
                            && since.elapsed() >= Duration::from_millis(250) =>
                    {
                        // Recheck child liveness after observing the stable window.
                        if child.try_wait()?.is_none() {
                            return Ok(json!({
                                "status": "native_window_observed", "pid": pid,
                                "executable": executable, "project": project,
                                "window_found": true, "window": window.json(),
                                "caller_session": session(std::process::id()), "child_session": session(pid),
                                "active_console_session_id": active_console_session(),
                                "elapsed_ms": start.elapsed().as_millis(), "diagnostics": log_path,
                                "project_loaded_in_gui": null, "frame_rendered": null,
                                "unobscured_on_user_screen": null,
                                "observation_limit": "Win32 window state is not physical user-screen or rendered-project confirmation"
                            }));
                        }
                    }
                    Some((handle, _)) if handle == window.handle => {}
                    _ => stable_window = Some((window.handle, Instant::now())),
                }
            } else {
                stable_window = None;
            }
            if start.elapsed() >= timeout {
                return Err(Error::Io(std::io::Error::other(format!(
                    "Studio PID {pid} is running but no stable visible, unminimized, uncloaked on-desktop Lumeryn Studio window was observed within {} seconds. Observed windows: {}. Process was left running for diagnosis; do not infer that Studio opened. Diagnostics {}. Verify the active desktop/session or launch the native executable directly",
                    timeout.as_secs(),
                    json!(windows.iter().map(Window::json).collect::<Vec<_>>()),
                    log_path.display()
                ))));
            }
            tokio::time::sleep(Duration::from_millis(75)).await;
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        fn fixture(subsystem: u16) -> Vec<u8> {
            let mut bytes = vec![0; 158];
            bytes[..2].copy_from_slice(b"MZ");
            bytes[60..64].copy_from_slice(&64_u32.to_le_bytes());
            bytes[64..68].copy_from_slice(b"PE\0\0");
            bytes[84..86].copy_from_slice(&70_u16.to_le_bytes());
            bytes[88..90].copy_from_slice(&0x20b_u16.to_le_bytes());
            bytes[156..158].copy_from_slice(&subsystem.to_le_bytes());
            bytes
        }
        #[test]
        fn pe_admission_rejects_console_truncated_and_oversized_headers() -> Result<()> {
            let directory = tempfile::tempdir()?;
            let path = directory.path().join("studio.exe");
            std::fs::write(&path, fixture(2))?;
            require_gui_subsystem(&path)?;
            std::fs::write(&path, fixture(3))?;
            assert!(require_gui_subsystem(&path).is_err());
            std::fs::write(&path, b"MZ")?;
            assert!(require_gui_subsystem(&path).is_err());
            let mut oversized = fixture(2);
            oversized[60..64].copy_from_slice(&u32::MAX.to_le_bytes());
            std::fs::write(&path, oversized)?;
            assert!(require_gui_subsystem(&path).is_err());
            Ok(())
        }
        #[test]
        fn invisible_minimized_cloaked_and_offscreen_windows_cannot_succeed() {
            let mut window = Window {
                handle: 1,
                visible: true,
                minimized: false,
                on_desktop: true,
                cloaked: Some(false),
                foreground: false,
                rect: RECT::default(),
            };
            assert!(window.observable());
            window.minimized = true;
            assert!(!window.observable());
            window.minimized = false;
            window.cloaked = None;
            assert!(!window.observable());
            window.cloaked = Some(true);
            assert!(!window.observable());
            window.cloaked = Some(false);
            window.on_desktop = false;
            assert!(!window.observable());
            window.on_desktop = true;
            window.visible = false;
            assert!(!window.observable());
        }
    }
}
