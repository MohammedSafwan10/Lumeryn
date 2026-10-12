//! CLI launch contracts. Real-window acceptance is explicit and Windows-only.
#[cfg(windows)]
use serde_json::Value;
#[cfg(windows)]
use std::path::Path;
use std::{ffi::OsStr, process::Command};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn cli(args: &[&OsStr]) -> TestResult<std::process::Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .args(args)
        .env_remove("RUST_LOG")
        .env_remove("LUMERYN_STUDIO_PATH")
        .output()?)
}

#[test]
fn studio_open_requires_project_and_bounded_wait() -> TestResult {
    let help = cli(&["studio".as_ref(), "open".as_ref(), "--help".as_ref()])?;
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout)?;
    assert!(text.contains("--project"));
    assert!(text.contains("--executable"));
    assert!(text.contains("--wait-seconds"));
    let missing = cli(&["studio".as_ref(), "open".as_ref()])?;
    assert!(!missing.status.success());
    for wait in ["0", "61"] {
        let invalid = cli(&[
            "studio".as_ref(),
            "open".as_ref(),
            "--project".as_ref(),
            "missing.lumeryn".as_ref(),
            "--wait-seconds".as_ref(),
            wait.as_ref(),
        ])?;
        assert!(!invalid.status.success());
        assert!(String::from_utf8(invalid.stderr)?.contains("--wait-seconds"));
    }
    Ok(())
}

#[cfg(windows)]
fn create(project: &Path) -> TestResult {
    let created = cli(&[
        "project".as_ref(),
        "create".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--name".as_ref(),
        "Launch contract".as_ref(),
    ])?;
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn studio_open_rejects_missing_or_script_executables_before_spawn() -> TestResult {
    let directory = tempfile::tempdir()?;
    let project = directory.path().join("project.lumeryn");
    create(&project)?;
    for (name, contents) in [
        ("missing.exe", None),
        ("fake.ps1", Some("throw 'must never execute'")),
        ("fake.exe", Some("not a PE executable")),
    ] {
        let executable = directory.path().join(name);
        if let Some(contents) = contents {
            std::fs::write(&executable, contents)?;
        }
        let rejected = cli(&[
            "studio".as_ref(),
            "open".as_ref(),
            "--project".as_ref(),
            project.as_os_str(),
            "--executable".as_ref(),
            executable.as_os_str(),
        ])?;
        assert!(!rejected.status.success());
        let envelope: Value = serde_json::from_slice(&rejected.stderr)?;
        assert_eq!(envelope["ok"], false);
        assert!(
            !envelope["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .contains("must never execute")
        );
        let code = envelope["error"]["code"].as_str().ok_or("error code")?;
        assert!(matches!(code, "not_found" | "invalid_input"));
    }
    // A real console-subsystem binary must also be rejected before execution.
    let console = cli(&[
        "studio".as_ref(),
        "open".as_ref(),
        "--project".as_ref(),
        project.as_os_str(),
        "--executable".as_ref(),
        env!("CARGO_BIN_EXE_lumeryn").as_ref(),
    ])?;
    assert!(!console.status.success());
    let envelope: Value = serde_json::from_slice(&console.stderr)?;
    assert_eq!(envelope["error"]["code"], "invalid_input");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .ok_or("message")?
            .contains("GUI-subsystem")
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn studio_launch_does_not_report_success_for_early_exit_or_no_window_timeout() -> TestResult {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("launch_fixture.rs");
    let executable = directory.path().join("launch_fixture.exe");
    std::fs::write(
        &source,
        r#"#![windows_subsystem = "windows"]
fn main() {
    if std::env::args_os().any(|arg| arg.to_string_lossy().contains("timeout-project")) {
        std::thread::sleep(std::time::Duration::from_secs(3));
    } else {
        std::process::exit(7);
    }
}
"#,
    )?;
    let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compiled = Command::new(compiler)
        .arg("--edition=2024")
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()?;
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    for (name, expected) in [
        ("exit-project.lumeryn", "exited"),
        ("timeout-project.lumeryn", "no stable visible"),
    ] {
        let project = directory.path().join(name);
        create(&project)?;
        let failed = cli(&[
            "studio".as_ref(),
            "open".as_ref(),
            "--project".as_ref(),
            project.as_os_str(),
            "--executable".as_ref(),
            executable.as_os_str(),
            "--wait-seconds".as_ref(),
            "1".as_ref(),
        ])?;
        assert!(
            !failed.status.success(),
            "a PID without a native window is never success"
        );
        assert!(failed.stdout.is_empty());
        let envelope: Value = serde_json::from_slice(&failed.stderr)?;
        let message = envelope["error"]["message"].as_str().ok_or("message")?;
        assert!(message.contains(expected), "{message}");
        assert!(message.contains("diagnostics"));
    }
    // The no-window fixture exits itself; no unrelated process is terminated.
    std::thread::sleep(std::time::Duration::from_secs(3));
    Ok(())
}

#[cfg(windows)]
#[test]
#[ignore = "requires built Windows GUI Studio and interactive desktop; set LUMERYN_TEST_STUDIO_PATH"]
fn actual_studio_child_has_owned_visible_native_window_and_closes_gracefully() -> TestResult {
    use std::{
        io::{BufRead, BufReader, Read},
        process::Stdio,
        sync::mpsc,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
    };
    let executable = std::env::var_os("LUMERYN_TEST_STUDIO_PATH")
        .ok_or("set LUMERYN_TEST_STUDIO_PATH to the freshly built GUI-subsystem Studio")?;
    let directory = tempfile::tempdir()?;
    // Spaces/punctuation remain one literal argument, never shell text.
    let project = directory.path().join("launch & literal project.lumeryn");
    create(&project)?;
    let started = Instant::now();
    let mut launching_cli = Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .args([
            "studio".as_ref(),
            "open".as_ref(),
            "--project".as_ref(),
            project.as_os_str(),
            "--executable".as_ref(),
            executable.as_os_str(),
            "--wait-seconds".as_ref(),
            "30".as_ref(),
        ])
        .env_remove("RUST_LOG")
        .env_remove("LUMERYN_STUDIO_PATH")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = launching_cli.stdout.take().ok_or("CLI stdout pipe")?;
    let stderr = launching_cli.stderr.take().ok_or("CLI stderr pipe")?;
    let (line_tx, line_rx) = mpsc::channel();
    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let first = reader.read_line(&mut line).map(|_| line);
        // A closed receiver means the bounded test cancelled observation.
        if line_tx.send(first).is_err() {
            return;
        }
        let mut remaining = Vec::new();
        let result = reader.read_to_end(&mut remaining).map(|_| remaining);
        if stdout_tx.send(result).is_err() {
            tracing::debug!("bounded launch test cancelled stdout observation");
        }
    });
    std::thread::spawn(move || {
        let mut reader = stderr;
        let mut bytes = Vec::new();
        let result = reader.read_to_end(&mut bytes).map(|_| bytes);
        if stderr_tx.send(result).is_err() {
            tracing::debug!("bounded launch test cancelled stderr observation");
        }
    });
    // Independent parent-side deadline: a capture-pipe inheritance regression
    // must fail in finite time even after the launching CLI itself has exited.
    let first_line = line_rx.recv_timeout(Duration::from_secs(45))??;
    let envelope: Value = serde_json::from_str(&first_line)?;
    let result = &envelope["result"];
    let pid = u32::try_from(result["pid"].as_u64().ok_or("child PID")?)?;
    let handle = usize::try_from(result["window"]["handle"].as_u64().ok_or("owned window")?)?
        as windows_sys::Win32::Foundation::HWND;
    let mut actual_pid = 0;
    // SAFETY: The returned window handle is queried with valid stack output
    // storage, then verified to still belong to this test's launched child.
    if unsafe { GetWindowThreadProcessId(handle, &mut actual_pid) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    assert_eq!(actual_pid, pid, "never close an unrelated/reused window");
    let eof_deadline = Instant::now() + Duration::from_secs(5);
    let stdout_eof = stdout_rx.recv_timeout(eof_deadline.saturating_duration_since(Instant::now()));
    let stderr_eof = stderr_rx.recv_timeout(eof_deadline.saturating_duration_since(Instant::now()));
    let eof_observed_ms = started.elapsed().as_millis();
    let mut cli_status = launching_cli.try_wait()?;
    while cli_status.is_none() && Instant::now() < eof_deadline {
        std::thread::sleep(Duration::from_millis(20));
        cli_status = launching_cli.try_wait()?;
    }
    let mut owner_at_eof = 0;
    // SAFETY: Read-only ownership query uses valid local output storage. This
    // demonstrates the GUI is still alive when both capture streams reach EOF.
    let window_alive_at_eof =
        unsafe { GetWindowThreadProcessId(handle, &mut owner_at_eof) } != 0 && owner_at_eof == pid;
    // Clean up our known GUI before asserting EOF failure, so the precise
    // inherited-pipe regression cannot leave this disposable window orphaned.
    let mut owner_before_close = 0;
    // SAFETY: Only the still-owned child window is eligible for WM_CLOSE.
    if unsafe { GetWindowThreadProcessId(handle, &mut owner_before_close) } == 0
        || owner_before_close != pid
    {
        return Err("child window disappeared or was reused before cleanup".into());
    }
    // SAFETY: Only the just-verified child-owned window receives WM_CLOSE; this
    // asks the app to shut down normally, without terminating any other process.
    if unsafe { PostMessageW(handle, WM_CLOSE, 0, 0) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut owner = 0;
        // SAFETY: Read-only query uses valid output storage. Destruction/reuse
        // ends our observation; the test never messages a reused handle.
        let thread = unsafe { GetWindowThreadProcessId(handle, &mut owner) };
        if thread == 0 || owner != pid {
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err("child-owned native window did not close after WM_CLOSE".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let stdout_tail =
        stdout_eof.map_err(|_| "stdout EOF deadline exceeded while GUI was alive")??;
    let stderr_bytes =
        stderr_eof.map_err(|_| "stderr EOF deadline exceeded while GUI was alive")??;
    assert!(
        stdout_tail.is_empty(),
        "unexpected output after the JSON envelope"
    );
    assert!(
        stderr_bytes.is_empty(),
        "{}",
        String::from_utf8_lossy(&stderr_bytes)
    );
    assert!(
        cli_status
            .ok_or("launching CLI exit deadline exceeded")?
            .success()
    );
    assert!(
        window_alive_at_eof,
        "GUI must survive CLI exit and capture EOF"
    );
    assert_eq!(result["status"], "native_window_observed");
    assert_eq!(result["window_found"], true);
    assert_eq!(result["window"]["title"], "Lumeryn Studio");
    assert_eq!(result["window"]["visible"], true);
    assert_eq!(result["window"]["minimized"], false);
    assert_eq!(result["window"]["cloaked"], false);
    assert_eq!(result["window"]["intersects_virtual_desktop"], true);
    assert!(result["frame_rendered"].is_null());
    assert!(result["unobscured_on_user_screen"].is_null());
    if let Some(report) = std::env::var_os("LUMERYN_TEST_STUDIO_REPORT") {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(report)?;
        file.write_all(&serde_json::to_vec_pretty(&serde_json::json!({
            "observed": envelope, "close_requested": true, "owned_window_closed": true,
            "physical_user_confirmation": null,
            "stdout_eof_while_gui_alive": true, "stderr_eof_while_gui_alive": true,
            "cli_exited_while_gui_alive": true, "parent_observed_eof_ms": eof_observed_ms
        }))?)?;
    }
    Ok(())
}
