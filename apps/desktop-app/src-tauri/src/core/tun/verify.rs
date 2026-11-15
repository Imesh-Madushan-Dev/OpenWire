//! Who is asking: the checks that stand between "can open the pipe" and "can
//! route this machine's traffic".
//!
//! The pipe's DACL lets any interactive user connect. On its own that would
//! let any program steer every app's traffic - other users' included - to a
//! port of its choosing. So a request is only honoured from the OpenWire
//! executable this service was installed from, and only to a proxy port that
//! very process is listening on.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
};
use windows_sys::Win32::Networking::WinSock::AF_INET;
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// The executable a process was started from.
pub fn process_path(pid: u32) -> Option<PathBuf> {
    // SAFETY: a failed open returns null, which is checked before use.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    // Long paths are possible; 32 K UTF-16 units is the API's ceiling.
    let mut buffer = vec![0u16; 32 * 1024];
    let mut len = buffer.len() as u32;
    // SAFETY: a live handle and a buffer of the stated length.
    let ok = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut len)
    };
    unsafe { CloseHandle(handle) };
    (ok != 0 && len > 0).then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..len as usize])))
}

/// Whether two paths name the same file, however they are spelled.
pub fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),
        _ => false,
    }
}

/// The process listening on `127.0.0.1:port` (or on every address, which
/// includes it).
pub fn loopback_listener(port: u16) -> Option<u32> {
    let mut size = 0u32;
    // Sizing call; the error it returns is expected.
    unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    if size == 0 {
        return None;
    }
    let mut buffer: Vec<u32> = vec![0; (size as usize).div_ceil(4)];
    // SAFETY: a buffer of at least `size` bytes, u32-aligned for the rows.
    let result = unsafe {
        GetExtendedTcpTable(
            buffer.as_mut_ptr().cast(),
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    if result != 0 {
        return None;
    }
    // SAFETY: on success the buffer holds the table header followed by
    // `dwNumEntries` rows.
    let table = unsafe { &*(buffer.as_ptr() as *const MIB_TCPTABLE_OWNER_PID) };
    let rows = unsafe {
        std::slice::from_raw_parts(
            std::ptr::addr_of!(table.table) as *const MIB_TCPROW_OWNER_PID,
            table.dwNumEntries as usize,
        )
    };
    rows.iter()
        .find(|row| {
            let local_port =
                u16::from_be_bytes([row.dwLocalPort as u8, (row.dwLocalPort >> 8) as u8]);
            let local = Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes());
            local_port == port && (local.is_loopback() || local.is_unspecified())
        })
        .map(|row| row.dwOwningPid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_found_by_its_path_and_its_listener() {
        let me = std::process::id();
        let path = process_path(me).expect("a process can read its own image path");
        assert!(same_file(&path, &std::env::current_exe().unwrap()));
        assert!(!same_file(
            &path,
            Path::new(r"C:\Windows\System32\notepad.exe")
        ));

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert_eq!(loopback_listener(port), Some(me));
        drop(listener);
        assert_eq!(loopback_listener(port), None);
    }
}
