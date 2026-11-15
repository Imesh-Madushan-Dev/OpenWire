//! Named pipes between a service and the app: a DACL-protected server end,
//! and the process checks that make a squatter on either end pointless.

use std::ffi::c_void;

use tokio::net::windows::named_pipe::{NamedPipeClient, NamedPipeServer, ServerOptions};

use super::wide;

/// A pipe's security descriptor, built once from SDDL and reused for every
/// instance the server creates.
pub struct PipeSecurity {
    descriptor: windows_sys::Win32::Security::PSECURITY_DESCRIPTOR,
}

// SAFETY: the descriptor is immutable after creation and only read by
// CreateNamedPipeW, which may be called from any thread.
unsafe impl Send for PipeSecurity {}
unsafe impl Sync for PipeSecurity {}

/// SYSTEM and administrators in full, interactive users read-only: for a
/// service that only ever talks.
pub const SDDL_READ_ONLY: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GR;;;IU)";
/// SYSTEM and administrators in full, interactive users read and write: for a
/// service that takes requests. Who may connect is only the first check -
/// such a service still verifies the process on the other end.
pub const SDDL_READ_WRITE: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)";

/// Which ways bytes flow, seen from the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// The server only writes; it never parses a byte a client sends.
    Outbound,
    Duplex,
}

impl PipeSecurity {
    pub fn new(sddl: &str) -> std::io::Result<Self> {
        use windows_sys::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };

        let sddl = wide(sddl);
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: the API allocates the descriptor with LocalAlloc; it is
        // freed in Drop.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { descriptor })
    }

    /// One server instance of `name`. `first` refuses to start if someone
    /// else already owns the name, so a squatter cannot sit between the
    /// service and the app.
    pub fn create(
        &self,
        name: &str,
        first: bool,
        direction: Direction,
    ) -> std::io::Result<NamedPipeServer> {
        use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;

        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor,
            bInheritHandle: 0,
        };

        // SAFETY: the attributes point at a descriptor that outlives the call.
        unsafe {
            ServerOptions::new()
                .first_pipe_instance(first)
                .access_inbound(direction == Direction::Duplex)
                .access_outbound(true)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    name,
                    &mut attributes as *mut SECURITY_ATTRIBUTES as *mut c_void,
                )
        }
    }
}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        // SAFETY: allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW.
        unsafe { windows_sys::Win32::Foundation::LocalFree(self.descriptor as _) };
    }
}

/// The process on the client end of a server pipe.
pub fn client_pid(pipe: &NamedPipeServer) -> Option<u32> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId;

    let mut pid = 0u32;
    // SAFETY: a live pipe handle and a valid out-pointer.
    let ok = unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle() as _, &mut pid) };
    (ok != 0).then_some(pid)
}

/// The process serving a client pipe. Anyone can create a pipe by a given
/// name before the service does; comparing this with the pid the SCM reports
/// is what makes that pointless.
pub fn server_pid(pipe: &NamedPipeClient) -> Option<u32> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;

    let mut pid = 0u32;
    // SAFETY: a live pipe handle and a valid out-pointer.
    let ok = unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle() as _, &mut pid) };
    (ok != 0).then_some(pid)
}
