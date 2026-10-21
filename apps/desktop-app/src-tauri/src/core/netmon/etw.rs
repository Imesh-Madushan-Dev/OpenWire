//! The real-time ETW session the service reads from.
//!
//! Two manifest providers, both filtered down to the handful of events used:
//! `Microsoft-Windows-Kernel-Network` for bytes, `Microsoft-Windows-DNS-Client`
//! for names. Starting a session needs admin or a service account, which is
//! the whole reason the monitor is a service.

use std::io;
use std::sync::Mutex;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, ERROR_SUCCESS};
use windows_sys::Win32::System::Diagnostics::Etw::{
    CloseTrace, ControlTraceW, EnableTraceEx2, OpenTraceW, ProcessTrace, StartTraceW,
    CONTROLTRACE_HANDLE, ENABLE_TRACE_PARAMETERS, ENABLE_TRACE_PARAMETERS_VERSION_2,
    EVENT_CONTROL_CODE_ENABLE_PROVIDER, EVENT_FILTER_DESCRIPTOR, EVENT_FILTER_TYPE_EVENT_ID,
    EVENT_RECORD, EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_LOGFILEW, EVENT_TRACE_PROPERTIES,
    EVENT_TRACE_REAL_TIME_MODE, PROCESSTRACE_HANDLE, PROCESS_TRACE_MODE_EVENT_RECORD,
    PROCESS_TRACE_MODE_REAL_TIME, TRACE_LEVEL_VERBOSE, WNODE_FLAG_TRACED_GUID,
};

use super::dns::DnsNames;
use super::flows::{AppName, FlowTable};
use super::parse::{self, DNS_QUERY_COMPLETED, KERNEL_NETWORK_EVENTS, KERNEL_NETWORK_KEYWORDS};
use super::sys::wide;

const SESSION_NAME: &str = "Maizy-NetMon";

const KERNEL_NETWORK: GUID = GUID::from_u128(0x7dd42a49_5329_4832_8dfd_43d979153a88);
const DNS_CLIENT: GUID = GUID::from_u128(0x1c95126e_7eea_49a9_a3fe_a378b03ddb4d);

/// What the event callback writes into and the service's ticker drains.
#[derive(Default)]
pub struct Collector {
    pub flows: Mutex<FlowTable>,
    pub dns: Mutex<DnsNames>,
}

/// A started session. Stopping it makes a blocked [`consume`] return.
pub struct Session {
    handle: CONTROLTRACE_HANDLE,
}

impl Session {
    pub fn start() -> io::Result<Self> {
        let mut props = Properties::new();
        let name = wide(SESSION_NAME);
        let mut handle = CONTROLTRACE_HANDLE::default();

        // SAFETY: `props` is sized for the struct plus the session name, as
        // StartTraceW requires, and outlives the call.
        let mut status = unsafe { StartTraceW(&mut handle, name.as_ptr(), props.as_mut_ptr()) };
        if status == ERROR_ALREADY_EXISTS {
            // Left over from a crash: real-time sessions outlive their owner.
            stop_by_name();
            props = Properties::new();
            status = unsafe { StartTraceW(&mut handle, name.as_ptr(), props.as_mut_ptr()) };
        }
        check(status)?;

        let session = Self { handle };
        session.enable(
            &KERNEL_NETWORK,
            KERNEL_NETWORK_KEYWORDS,
            &KERNEL_NETWORK_EVENTS,
        )?;
        // Names are a nicety: traffic still counts, by address, without them.
        if let Err(err) = session.enable(&DNS_CLIENT, u64::MAX, &[DNS_QUERY_COMPLETED]) {
            log::warn!("dns names unavailable: {err}");
        }
        Ok(session)
    }

    fn enable(&self, provider: &GUID, keywords: u64, events: &[u16]) -> io::Result<()> {
        // EVENT_FILTER_EVENT_ID is variable-length: a 4-byte head, then ids.
        let mut filter = Vec::with_capacity(4 + events.len() * 2);
        filter.push(1u8); // FilterIn: deliver only these ids
        filter.push(0u8);
        filter.extend((events.len() as u16).to_le_bytes());
        for id in events {
            filter.extend(id.to_le_bytes());
        }

        let mut descriptor = EVENT_FILTER_DESCRIPTOR {
            Ptr: filter.as_ptr() as u64,
            Size: filter.len() as u32,
            Type: EVENT_FILTER_TYPE_EVENT_ID,
        };
        let parameters = ENABLE_TRACE_PARAMETERS {
            Version: ENABLE_TRACE_PARAMETERS_VERSION_2,
            EnableProperty: 0,
            ControlFlags: 0,
            SourceId: GUID::from_u128(0),
            EnableFilterDesc: &mut descriptor,
            FilterDescCount: 1,
        };

        let enable = |params: *const ENABLE_TRACE_PARAMETERS| unsafe {
            EnableTraceEx2(
                self.handle,
                provider,
                EVENT_CONTROL_CODE_ENABLE_PROVIDER,
                TRACE_LEVEL_VERBOSE as u8,
                keywords,
                0,
                0,
                params,
            )
        };

        // The filter only spares the callback work; if this build of Windows
        // will not take it, every event is delivered and the callback skips
        // what it does not want.
        match enable(&parameters) {
            ERROR_SUCCESS => Ok(()),
            _ => check(enable(std::ptr::null())),
        }
    }

    pub fn stop(&self) {
        stop_by_name();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Stops the named session, whoever started it.
pub fn stop_by_name() {
    let mut props = Properties::new();
    let name = wide(SESSION_NAME);
    // SAFETY: as for StartTraceW. A session that is not running reports an
    // error, which is the outcome we wanted anyway.
    unsafe {
        ControlTraceW(
            CONTROLTRACE_HANDLE::default(),
            name.as_ptr(),
            props.as_mut_ptr(),
            EVENT_TRACE_CONTROL_STOP,
        )
    };
}

/// Feeds the session's events into `collector` until the session stops.
/// Blocks the calling thread for that whole time.
pub fn consume(collector: &Collector) -> io::Result<()> {
    let mut name = wide(SESSION_NAME);

    // SAFETY: zero is a valid starting state for this plain-data struct; the
    // fields OpenTraceW reads are set below.
    let mut logfile: EVENT_TRACE_LOGFILEW = unsafe { std::mem::zeroed() };
    logfile.LoggerName = name.as_mut_ptr();
    logfile.Anonymous1.ProcessTraceMode =
        PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
    logfile.Anonymous2.EventRecordCallback = Some(on_event);
    logfile.Context = collector as *const Collector as *mut core::ffi::c_void;

    // SAFETY: `logfile` and `name` live until ProcessTrace returns, and the
    // callback context is a reference the caller keeps alive just as long.
    let trace: PROCESSTRACE_HANDLE = unsafe { OpenTraceW(&mut logfile) };
    if trace.Value == u64::MAX || trace.Value == u64::from(u32::MAX) {
        return Err(io::Error::last_os_error());
    }

    let status = unsafe { ProcessTrace(&trace, 1, std::ptr::null(), std::ptr::null()) };
    unsafe { CloseTrace(trace) };
    check(status)
}

/// Called by ETW for every delivered event, on the ProcessTrace thread.
///
/// Hot: during a fast download this runs tens of thousands of times a second.
/// It must not allocate per packet, must not block, and must never panic -
/// unwinding out of an `extern "system"` callback aborts the service.
unsafe extern "system" fn on_event(record: *mut EVENT_RECORD) {
    let Some(record) = record.as_ref() else {
        return;
    };
    let Some(collector) = (record.UserContext as *const Collector).as_ref() else {
        return;
    };
    if record.UserData.is_null() {
        return;
    }
    let data = std::slice::from_raw_parts(
        record.UserData as *const u8,
        usize::from(record.UserDataLength),
    );

    let header = &record.EventHeader;
    let id = header.EventDescriptor.Id;

    if same_guid(&header.ProviderId, &KERNEL_NETWORK) {
        let Some(packet) = parse::network_event(id, data) else {
            return;
        };
        if let Ok(mut flows) = collector.flows.lock() {
            flows.add(&packet, name_process);
        }
    } else if id == DNS_QUERY_COMPLETED && same_guid(&header.ProviderId, &DNS_CLIENT) {
        let Some((name, addresses)) = parse::dns_event(data) else {
            return;
        };
        if let Ok(mut dns) = collector.dns.lock() {
            dns.record(&name, &addresses);
        }
    }
}

fn name_process(pid: u32) -> AppName {
    use crate::core::usage::{process, srum};

    match pid {
        // Idle: receives the kernel could not pin on a process.
        0 => AppName::named(srum::SYSTEM_KEY),
        // The kernel itself - file sharing, mostly.
        4 => AppName::named("System"),
        _ => match process::process_image(pid) {
            Some((name, path)) => AppName {
                name: name.as_str().into(),
                path: Some(path.as_str().into()),
            },
            None => AppName::named(process::UNKNOWN_APP),
        },
    }
}

fn same_guid(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

fn check(status: u32) -> io::Result<()> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}

/// `EVENT_TRACE_PROPERTIES` followed by room for the session name, which the
/// API writes after the struct. Backed by `u64`s for alignment.
struct Properties {
    buffer: Vec<u64>,
}

impl Properties {
    fn new() -> Self {
        let header = std::mem::size_of::<EVENT_TRACE_PROPERTIES>();
        let total = header + (SESSION_NAME.len() + 1) * 2;
        let mut buffer = vec![0u64; total.div_ceil(8)];

        // SAFETY: the buffer is at least one struct long and suitably aligned.
        let props = unsafe { &mut *(buffer.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES) };
        props.Wnode.BufferSize = total as u32;
        props.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
        // QPC timestamps; the ticker keeps its own clock, so precision is moot.
        props.Wnode.ClientContext = 1;
        props.LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
        // 64 KB buffers flushed every second keep latency under the tick.
        props.BufferSize = 64;
        props.MinimumBuffers = 4;
        props.MaximumBuffers = 64;
        props.FlushTimer = 1;
        props.LoggerNameOffset = header as u32;
        props.LogFileNameOffset = 0;

        Self { buffer }
    }

    fn as_mut_ptr(&mut self) -> *mut EVENT_TRACE_PROPERTIES {
        self.buffer.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without admin rights StartTraceW must refuse on access grounds - or,
    /// on a machine with the monitor installed, because its service already
    /// holds the session. A malformed properties block is rejected as an
    /// invalid parameter instead, so this pins the struct layout even on
    /// unelevated runs. Elevated, it starts and stops a real session.
    #[test]
    fn starting_a_session_fails_on_rights_not_on_layout() {
        const ACCESS_DENIED: i32 = 5;
        const ALREADY_RUNNING: i32 = 183;
        match Session::start() {
            Ok(session) => session.stop(),
            Err(err) => assert!(
                matches!(err.raw_os_error(), Some(ACCESS_DENIED | ALREADY_RUNNING)),
                "{err}"
            ),
        }
    }
}
