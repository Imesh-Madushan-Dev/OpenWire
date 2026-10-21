//! Reads text off the system clipboard.
//!
//! Done here rather than with `navigator.clipboard` in the webview: WebView2
//! gates clipboard reads behind a permission prompt, and a share link holds
//! the server's credentials - reading it in the backend keeps it from passing
//! through the UI at all.

/// The clipboard's text, or `None` when it holds none (or another app has it
/// locked open).
#[cfg(windows)]
pub fn read_text() -> Option<String> {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    // CF_UNICODETEXT; the constant lives in the OLE feature, not worth pulling in.
    const CF_UNICODETEXT: u32 = 13;

    // SAFETY: plain query, no handles involved.
    if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) } == 0 {
        return None;
    }

    // Another app may hold the clipboard for a moment; a few short retries
    // cover the common case without stalling the caller.
    let mut opened = false;
    for _ in 0..5 {
        // SAFETY: a null owner associates the clipboard with this task.
        if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
            opened = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if !opened {
        return None;
    }

    // SAFETY: the clipboard is open; the handle stays owned by the system and
    // valid until CloseClipboard, and the lock is released before then.
    let text = unsafe {
        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle.is_null() {
            None
        } else {
            let data = GlobalLock(handle as _) as *const u16;
            if data.is_null() {
                None
            } else {
                // Bounded by the allocation, not just by the terminator, so a
                // malformed entry cannot send the read off the end.
                let capacity = GlobalSize(handle as _) / 2;
                let units = std::slice::from_raw_parts(data, capacity);
                let len = units.iter().position(|unit| *unit == 0).unwrap_or(capacity);
                let text = String::from_utf16_lossy(&units[..len]);
                GlobalUnlock(handle as _);
                Some(text)
            }
        }
    };
    unsafe { CloseClipboard() };
    text
}

#[cfg(not(windows))]
pub fn read_text() -> Option<String> {
    // ponytail: arboard would cover macOS and Linux if the app ships there.
    None
}
