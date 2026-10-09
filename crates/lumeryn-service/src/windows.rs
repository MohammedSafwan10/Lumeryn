//! Windows-only transport boundary; no Win32 dependencies enter the engine.
use lumeryn_core::{Error, Result};
use std::{ffi::c_void, path::Path, ptr};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, LocalFree},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SDDL_REVISION_1,
        },
        GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    System::Threading::{
        CREATE_NO_WINDOW, CreateProcessW, GetCurrentProcess, OpenProcessToken, PROCESS_INFORMATION,
        STARTUPINFOW,
    },
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: this object uniquely owns a successful OpenProcessToken handle.
        if unsafe { CloseHandle(self.0) } == 0 {
            tracing::debug!("token handle cleanup failed");
        }
    }
}
struct LocalAllocation(*mut c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: API-allocated buffer is freed once, after all uses have ended.
        if !unsafe { LocalFree(self.0) }.is_null() {
            tracing::debug!("security buffer cleanup failed");
        }
    }
}

pub(crate) fn current_user_sid() -> Result<String> {
    let mut raw = ptr::null_mut();
    // SAFETY: valid pseudo-process handle and writable output; query access only.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
        return Err(Error::Io(std::io::Error::last_os_error()));
    }
    let token = Handle(raw);
    let mut bytes = 0;
    // SAFETY: size-query contract accepts null buffer and writes required length.
    let query = unsafe { GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut bytes) };
    if query != 0 || std::io::Error::last_os_error().raw_os_error() != Some(122) {
        return Err(Error::Io(std::io::Error::last_os_error()));
    }
    if bytes == 0 || bytes > 64 * 1024 {
        return Err(Error::Io(std::io::Error::last_os_error()));
    }
    // Pointer-sized storage satisfies TOKEN_USER alignment, unlike a Vec<u8>.
    let mut buffer = vec![0usize; (bytes as usize).div_ceil(std::mem::size_of::<usize>())];
    // SAFETY: aligned allocated storage spans at least bytes, token is still live.
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            bytes,
            &mut bytes,
        )
    } == 0
    {
        return Err(Error::Io(std::io::Error::last_os_error()));
    }
    // SAFETY: successful TokenUser query initialized an aligned TOKEN_USER and its SID.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut text = ptr::null_mut();
    // SAFETY: SID remains live in buffer and text is a valid writable output.
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) } == 0 {
        return Err(Error::Io(std::io::Error::last_os_error()));
    }
    let _allocation = LocalAllocation(text.cast());
    let mut length = 0;
    // SAFETY: ConvertSidToStringSidW returns a NUL-terminated string; SID text is short.
    while length < 256 && unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    if length == 256 {
        return Err(Error::Invalid("unexpected SID length".into()));
    }
    // SAFETY: measured initialized UTF-16 slice, allocation remains owned above.
    String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })
        .map_err(|_| Error::Invalid("invalid SID string".into()))
}

pub(crate) fn pipe_name(path: &Path) -> Result<String> {
    let user = current_user_sid()?;
    let path = path
        .to_str()
        .ok_or_else(|| Error::Invalid("IPC project path must be valid Unicode".into()))?
        .to_lowercase();
    let mut hash = blake3::Hasher::new();
    hash.update(user.as_bytes());
    hash.update(&[0]);
    hash.update(path.as_bytes());
    Ok(format!(r"\\.\pipe\lumeryn-v1-{}", hash.finalize().to_hex()))
}

pub(crate) fn create_pipe(name: &str, first: bool) -> Result<NamedPipeServer> {
    let sid = current_user_sid()?;
    let descriptor = format!("O:{sid}D:P(A;;GA;;;{sid})");
    let wide = descriptor
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut raw = ptr::null_mut();
    // SAFETY: NUL-terminated SDDL and valid outputs; conversion owns returned allocation.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide.as_ptr(),
            SDDL_REVISION_1,
            &mut raw,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(Error::Io(std::io::Error::last_os_error()));
    }
    let _allocation = LocalAllocation(raw);
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: raw,
        bInheritHandle: 0,
    };
    let mut options = ServerOptions::new();
    options
        .first_pipe_instance(first)
        .reject_remote_clients(true)
        .in_buffer_size(64 * 1024)
        .out_buffer_size(64 * 1024);
    // SAFETY: attributes and self-relative descriptor remain live during synchronous creation;
    // Windows copies the descriptor into the kernel pipe object. Handle is not inherited.
    Ok(unsafe {
        options.create_with_security_attributes_raw(
            name,
            (&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
        )
    }?)
}

pub(crate) fn launch_service(executable: &Path, project: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let application = executable
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let quote = |path: &Path| -> Result<String> {
        let path = path
            .to_str()
            .ok_or_else(|| Error::Invalid("service paths must be valid Unicode".into()))?;
        if path.contains(['\0', '"']) {
            return Err(Error::Invalid("invalid service path".into()));
        }
        // These arguments are executable/project file paths, not directories ending in a slash.
        Ok(format!("\"{path}\""))
    };
    let command = format!(
        "{} service run --project {}",
        quote(executable)?,
        quote(project)?
    );
    let mut wide = command
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: explicit executable and writable NUL-terminated command line; all pointers
    // outlive synchronous CreateProcessW. FALSE inheritance prevents every inherited capture
    // pipe, not just the three standard handles, from keeping CLI output readers alive.
    // CREATE_NO_WINDOW creates no console; no shell, elevated token or executable text is used.
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            wide.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            CREATE_NO_WINDOW,
            ptr::null(),
            ptr::null(),
            &startup,
            &mut process,
        )
    };
    if created == 0 {
        return Err(Error::Io(std::io::Error::last_os_error()));
    }
    let _process = Handle(process.hProcess);
    let _thread = Handle(process.hThread);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE,
        Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT},
        DACL_SECURITY_INFORMATION, GetAce, GetSecurityDescriptorControl, SE_DACL_PROTECTED,
    };
    #[tokio::test]
    async fn exclusive_endpoint_and_explicit_user_descriptor() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = crate::absolute_project(&directory.path().join("secure.lumeryn"))?;
        let name = pipe_name(&path)?;
        let first = create_pipe(&name, true)?;
        assert!(
            matches!(create_pipe(&name,true),Err(Error::Io(error)) if error.kind()==std::io::ErrorKind::PermissionDenied)
        );
        assert!(current_user_sid()?.starts_with("S-1-"));
        let mut descriptor = ptr::null_mut();
        let mut acl = ptr::null_mut();
        // SAFETY: live pipe handle; writable outputs receive descriptor-owned pointers.
        let status = unsafe {
            GetSecurityInfo(
                first.as_raw_handle(),
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut acl,
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        if status != 0 {
            return Err(Error::Io(std::io::Error::from_raw_os_error(status as i32)));
        }
        let _allocation = LocalAllocation(descriptor);
        assert!(!acl.is_null());
        // SAFETY: nonnull ACL returned as part of the live allocated security descriptor.
        assert_eq!(unsafe { (*acl).AceCount }, 1);
        let mut control = 0;
        let mut revision = 0;
        // SAFETY: descriptor remains live; outputs are valid stack addresses.
        if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }
        assert_ne!(control & SE_DACL_PROTECTED, 0);
        let mut ace = ptr::null_mut();
        // SAFETY: verified ACL has one ACE; output points inside descriptor-owned buffer.
        if unsafe { GetAce(acl, 0, &mut ace) } == 0 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }
        // SAFETY: our only SDDL ACE is ACCESS_ALLOWED_ACE; SID begins at SidStart.
        let sid = unsafe { ptr::addr_of!((*ace.cast::<ACCESS_ALLOWED_ACE>()).SidStart) };
        let mut text = ptr::null_mut();
        // SAFETY: ACE SID remains live and correctly initialized by Windows.
        if unsafe { ConvertSidToStringSidW(sid.cast_mut().cast(), &mut text) } == 0 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }
        let _sid_allocation = LocalAllocation(text.cast());
        let mut length = 0;
        // SAFETY: Windows returns a NUL-terminated SID string, valid while allocation is live.
        while length < 256 && unsafe { *text.add(length) } != 0 {
            length += 1;
        }
        assert!(length < 256);
        // SAFETY: measured initialized UTF-16 slice within the returned SID allocation.
        let actual = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })
            .map_err(|_| Error::Invalid("invalid SID text".into()))?;
        assert_eq!(actual, current_user_sid()?);
        Ok(())
    }
}
