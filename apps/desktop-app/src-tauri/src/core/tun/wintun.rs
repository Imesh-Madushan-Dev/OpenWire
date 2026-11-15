//! Wintun: WireGuard's virtual network adapter driver.
//!
//! `wintun.dll` ships beside the service's binary in Program Files (it is
//! WireGuard LLC's signed build; see `resources/wintun/README.md`). It is
//! loaded by absolute path with the search path limited to System32, so no
//! DLL of the same name planted anywhere else can stand in for it.
//!
//! The DLL's API is small, and bound by hand here rather than through a crate
//! that would pull a second copy of `windows-sys`.

use std::ffi::c_void;
use std::io;
use std::path::Path;
use std::sync::Arc;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{
    CloseHandle, FreeLibrary, GetLastError, ERROR_BUFFER_OVERFLOW, ERROR_HANDLE_EOF,
    ERROR_NO_MORE_ITEMS, HANDLE, HMODULE, WAIT_OBJECT_0,
};
use windows_sys::Win32::NetworkManagement::Ndis::NET_LUID_LH;
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, SetEvent, WaitForMultipleObjects, INFINITE,
};

use crate::core::winsvc::wide;

type AdapterHandle = *mut c_void;
type SessionHandle = *mut c_void;

type CreateAdapterFn = unsafe extern "system" fn(
    name: *const u16,
    tunnel_type: *const u16,
    guid: *const GUID,
) -> AdapterHandle;
type CloseAdapterFn = unsafe extern "system" fn(adapter: AdapterHandle);
type GetAdapterLuidFn = unsafe extern "system" fn(adapter: AdapterHandle, luid: *mut NET_LUID_LH);
type GetRunningDriverVersionFn = unsafe extern "system" fn() -> u32;
type DeleteDriverFn = unsafe extern "system" fn() -> i32;
type SetLoggerFn = unsafe extern "system" fn(logger: Option<LoggerFn>);
type LoggerFn = unsafe extern "system" fn(level: i32, timestamp: u64, message: *const u16);
type StartSessionFn =
    unsafe extern "system" fn(adapter: AdapterHandle, capacity: u32) -> SessionHandle;
type EndSessionFn = unsafe extern "system" fn(session: SessionHandle);
type GetReadWaitEventFn = unsafe extern "system" fn(session: SessionHandle) -> HANDLE;
type ReceivePacketFn = unsafe extern "system" fn(session: SessionHandle, size: *mut u32) -> *mut u8;
type ReleaseReceivePacketFn = unsafe extern "system" fn(session: SessionHandle, packet: *const u8);
type AllocateSendPacketFn = unsafe extern "system" fn(session: SessionHandle, size: u32) -> *mut u8;
type SendPacketFn = unsafe extern "system" fn(session: SessionHandle, packet: *const u8);

/// The loaded DLL and its entry points.
pub struct Wintun {
    module: HMODULE,
    create_adapter: CreateAdapterFn,
    close_adapter: CloseAdapterFn,
    get_adapter_luid: GetAdapterLuidFn,
    get_running_driver_version: GetRunningDriverVersionFn,
    delete_driver: DeleteDriverFn,
    start_session: StartSessionFn,
    end_session: EndSessionFn,
    get_read_wait_event: GetReadWaitEventFn,
    receive_packet: ReceivePacketFn,
    release_receive_packet: ReleaseReceivePacketFn,
    allocate_send_packet: AllocateSendPacketFn,
    send_packet: SendPacketFn,
}

// SAFETY: the module handle and function pointers are immutable after load,
// and Wintun documents every function used here as callable from any thread.
unsafe impl Send for Wintun {}
unsafe impl Sync for Wintun {}

/// Ring sizes Wintun accepts: powers of two in this range.
pub const MIN_RING: u32 = 0x2_0000;
pub const MAX_RING: u32 = 0x400_0000;

impl Wintun {
    pub fn load(path: &Path) -> io::Result<Arc<Self>> {
        let file = wide(path);
        // SAFETY: an absolute, NUL-terminated path. The flags confine the
        // DLL's own dependencies to System32 and its folder.
        let module = unsafe {
            LoadLibraryExW(
                file.as_ptr(),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
        if module.is_null() {
            return Err(io::Error::last_os_error());
        }

        type Export = unsafe extern "system" fn() -> isize;
        macro_rules! bind {
            ($name:literal, $signature:ty) => {{
                // SAFETY: a NUL-terminated ASCII name on a loaded module.
                match unsafe { GetProcAddress(module, concat!($name, "\0").as_ptr()) } {
                    // SAFETY: the export has the signature wintun.h declares
                    // for it, which the target type mirrors.
                    Some(function) => unsafe {
                        std::mem::transmute::<Export, $signature>(function)
                    },
                    None => {
                        unsafe { FreeLibrary(module) };
                        return Err(io::Error::new(
                            io::ErrorKind::NotFound,
                            concat!("wintun.dll has no ", $name),
                        ));
                    }
                }
            }};
        }

        let wintun = Self {
            module,
            create_adapter: bind!("WintunCreateAdapter", CreateAdapterFn),
            close_adapter: bind!("WintunCloseAdapter", CloseAdapterFn),
            get_adapter_luid: bind!("WintunGetAdapterLUID", GetAdapterLuidFn),
            get_running_driver_version: bind!(
                "WintunGetRunningDriverVersion",
                GetRunningDriverVersionFn
            ),
            delete_driver: bind!("WintunDeleteDriver", DeleteDriverFn),
            start_session: bind!("WintunStartSession", StartSessionFn),
            end_session: bind!("WintunEndSession", EndSessionFn),
            get_read_wait_event: bind!("WintunGetReadWaitEvent", GetReadWaitEventFn),
            receive_packet: bind!("WintunReceivePacket", ReceivePacketFn),
            release_receive_packet: bind!("WintunReleaseReceivePacket", ReleaseReceivePacketFn),
            allocate_send_packet: bind!("WintunAllocateSendPacket", AllocateSendPacketFn),
            send_packet: bind!("WintunSendPacket", SendPacketFn),
        };

        let set_logger: SetLoggerFn = bind!("WintunSetLogger", SetLoggerFn);
        // SAFETY: the callback is a plain function valid for the process.
        unsafe { set_logger(Some(forward_log)) };

        Ok(Arc::new(wintun))
    }

    /// The loaded driver's version as `(major, minor)`, once one is running.
    pub fn driver_version(&self) -> Option<(u16, u16)> {
        // SAFETY: no arguments; zero means no driver is loaded yet.
        let version = unsafe { (self.get_running_driver_version)() };
        (version != 0).then_some(((version >> 16) as u16, version as u16))
    }

    /// Removes the driver from the system if no adapter is using it - other
    /// software using Wintun keeps it. For uninstall.
    pub fn delete_driver(&self) -> bool {
        // SAFETY: no arguments.
        unsafe { (self.delete_driver)() != 0 }
    }
}

impl Drop for Wintun {
    fn drop(&mut self) {
        // SAFETY: loaded in `load` and not freed elsewhere. Every adapter and
        // session holds an Arc to this, so none outlives the module.
        unsafe { FreeLibrary(self.module) };
    }
}

unsafe extern "system" fn forward_log(level: i32, _timestamp: u64, message: *const u16) {
    if message.is_null() {
        return;
    }
    // SAFETY: Wintun passes a NUL-terminated string valid for the call.
    let len = (0..)
        .take_while(|&i| unsafe { *message.add(i) } != 0)
        .count();
    let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(message, len) });
    match level {
        0 => log::info!("wintun: {text}"),
        1 => log::warn!("wintun: {text}"),
        _ => log::error!("wintun: {text}"),
    }
}

/// An adapter this process created. Dropping it removes the adapter from the
/// system, and with it every address and route on it.
pub struct Adapter {
    wintun: Arc<Wintun>,
    handle: AdapterHandle,
}

// SAFETY: see `Wintun`.
unsafe impl Send for Adapter {}
unsafe impl Sync for Adapter {}

impl Adapter {
    /// `guid` is fixed so Windows recognises the same network each time,
    /// rather than adding "Network 2", "Network 3"... to its profiles.
    pub fn create(
        wintun: &Arc<Wintun>,
        name: &str,
        tunnel_type: &str,
        guid: u128,
    ) -> io::Result<Self> {
        let name = wide(name);
        let tunnel_type = wide(tunnel_type);
        let guid = GUID::from_u128(guid);
        // SAFETY: NUL-terminated strings and a GUID, all outliving the call.
        let handle = unsafe { (wintun.create_adapter)(name.as_ptr(), tunnel_type.as_ptr(), &guid) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            wintun: Arc::clone(wintun),
            handle,
        })
    }

    pub fn luid(&self) -> NET_LUID_LH {
        let mut luid = NET_LUID_LH { Value: 0 };
        // SAFETY: a live adapter handle and a valid out-pointer.
        unsafe { (self.wintun.get_adapter_luid)(self.handle, &mut luid) };
        luid
    }
}

impl Drop for Adapter {
    fn drop(&mut self) {
        // SAFETY: created by WintunCreateAdapter, closed once.
        unsafe { (self.wintun.close_adapter)(self.handle) };
    }
}

/// What a read from the session produced.
pub enum Received {
    Packet(Vec<u8>),
    /// [`Session::shutdown`] was called.
    Shutdown,
    /// The adapter went away underneath the session.
    Closed,
}

/// A packet session on an adapter.
pub struct Session {
    adapter: Arc<Adapter>,
    handle: SessionHandle,
    read_event: HANDLE,
    /// Set to wake a reader blocked in [`Session::receive`].
    quit_event: HANDLE,
}

// SAFETY: Wintun's send and receive functions are documented thread-safe;
// the events are kernel objects usable from any thread.
unsafe impl Send for Session {}
unsafe impl Sync for Session {}

impl Session {
    pub fn start(adapter: Arc<Adapter>, ring: u32) -> io::Result<Self> {
        let ring = ring
            .clamp(MIN_RING, MAX_RING)
            .next_power_of_two()
            .min(MAX_RING);
        // SAFETY: a live adapter handle.
        let handle = unsafe { (adapter.wintun.start_session)(adapter.handle, ring) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a live session; the event belongs to it and is not closed
        // by us.
        let read_event = unsafe { (adapter.wintun.get_read_wait_event)(handle) };
        // SAFETY: an anonymous manual-reset event, unsignalled.
        let quit_event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if quit_event.is_null() {
            let err = io::Error::last_os_error();
            unsafe { (adapter.wintun.end_session)(handle) };
            return Err(err);
        }
        Ok(Self {
            adapter,
            handle,
            read_event,
            quit_event,
        })
    }

    /// Blocks until a packet arrives, the session is shut down, or the
    /// adapter goes away. For a dedicated thread.
    pub fn receive(&self) -> Received {
        let wintun = &self.adapter.wintun;
        loop {
            let mut size = 0u32;
            // SAFETY: a live session and a valid out-pointer.
            let packet = unsafe { (wintun.receive_packet)(self.handle, &mut size) };
            if !packet.is_null() {
                // SAFETY: Wintun hands out `size` readable bytes, valid until
                // released - which happens right after the copy.
                let bytes = unsafe { std::slice::from_raw_parts(packet, size as usize) }.to_vec();
                unsafe { (wintun.release_receive_packet)(self.handle, packet) };
                return Received::Packet(bytes);
            }
            match unsafe { GetLastError() } {
                ERROR_NO_MORE_ITEMS => {
                    let events = [self.read_event, self.quit_event];
                    // SAFETY: two live event handles.
                    let woke = unsafe { WaitForMultipleObjects(2, events.as_ptr(), 0, INFINITE) };
                    match woke {
                        WAIT_OBJECT_0 => {}
                        w if w == WAIT_OBJECT_0 + 1 => return Received::Shutdown,
                        // A failed wait would fail again at once; looping on
                        // it would spin a core.
                        _ => {
                            log::error!("wintun wait failed: {}", io::Error::last_os_error());
                            return Received::Closed;
                        }
                    }
                }
                ERROR_HANDLE_EOF => return Received::Closed,
                other => {
                    log::error!("wintun receive failed ({other})");
                    return Received::Closed;
                }
            }
        }
    }

    /// Queues one packet for Windows. A full ring drops it.
    pub fn send(&self, packet: &[u8]) {
        let Ok(size) = u32::try_from(packet.len()) else {
            return;
        };
        let wintun = &self.adapter.wintun;
        // SAFETY: a live session.
        let buffer = unsafe { (wintun.allocate_send_packet)(self.handle, size) };
        if buffer.is_null() {
            let code = unsafe { GetLastError() };
            if code != ERROR_BUFFER_OVERFLOW && code != ERROR_HANDLE_EOF {
                log::warn!("wintun send failed ({code})");
            }
            return;
        }
        // SAFETY: Wintun allocated `size` writable bytes for this packet.
        unsafe {
            std::ptr::copy_nonoverlapping(packet.as_ptr(), buffer, packet.len());
            (wintun.send_packet)(self.handle, buffer);
        }
    }

    /// Wakes the reader so its thread can be joined before the session ends.
    pub fn shutdown(&self) {
        // SAFETY: a live event handle.
        unsafe { SetEvent(self.quit_event) };
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: the reader thread holds an Arc to this, so by the time it
        // drops nothing is inside `receive` any more.
        unsafe {
            (self.adapter.wintun.end_session)(self.handle);
            CloseHandle(self.quit_event);
        }
    }
}
