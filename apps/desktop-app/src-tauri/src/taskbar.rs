//! Live network speed on the Windows taskbar, just left of the notification
//! area - upload on top, download below, like TrafficMonitor.
//!
//! Windows 11 has no API for taskbar widgets (deskbands are gone), so this is
//! a small window parented into the taskbar itself. As the taskbar's child it
//! shows, hides and auto-hides with it, and never floats over a full-screen
//! app the way a topmost window would. It is drawn with GDI rather than a
//! webview: two lines of text once a second do not need a browser engine.
//!
//! The window lives on its own thread with its own message loop. Being a child
//! of Explorer's taskbar ties that thread's input to Explorer's, so nothing on
//! it may block: a click hands off to the main thread and returns at once.

use std::sync::atomic::{AtomicU64, Ordering};

static UP: AtomicU64 = AtomicU64::new(0);
static DOWN: AtomicU64 = AtomicU64::new(0);

/// The latest one-second sample, in bytes, device-wide. Fed by the stats
/// ticker whether or not the readout is showing; it is two atomic stores.
pub fn record(up: u64, down: u64) {
    UP.store(up, Ordering::Relaxed);
    DOWN.store(down, Ordering::Relaxed);
}

#[cfg(windows)]
pub use imp::set_enabled;

#[cfg(not(windows))]
pub fn set_enabled(_app: &tauri::AppHandle, _on: bool) {
    // ponytail: a menu-bar item on macOS, if the app ever ships there.
}

const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

/// Throughput the way the UI writes it (`formatSpeed` in src/lib/format.ts),
/// so the taskbar and the Network page never disagree.
fn format_speed(bytes_per_second: u64) -> String {
    if bytes_per_second == 0 {
        return "0 B/s".to_string();
    }
    let mut value = bytes_per_second as f64;
    let mut exponent = 0;
    while value >= 1024.0 && exponent < UNITS.len() - 1 {
        value /= 1024.0;
        exponent += 1;
    }
    // One decimal below 100 keeps the width stable without false precision.
    if value < 100.0 && exponent > 0 {
        format!("{value:.1} {}/s", UNITS[exponent])
    } else {
        format!("{} {}/s", value.round() as u64, UNITS[exponent])
    }
}

#[cfg(windows)]
mod imp {
    use std::cell::RefCell;
    use std::sync::atomic::Ordering;
    use std::sync::{Mutex, OnceLock};
    use std::thread::JoinHandle;

    use tauri::AppHandle;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW,
        CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC,
        GetTextExtentPoint32W, GetTextMetricsW, InvalidateRect, ReleaseDC, SelectObject,
        SetBkMode, SetTextColor, ANTIALIASED_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET,
        DEFAULT_PITCH, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE,
        FW_NORMAL, HFONT, OUT_DEFAULT_PRECIS, PAINTSTRUCT, SRCCOPY, TEXTMETRICW, TRANSPARENT,
    };
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowExW,
        FindWindowW, GetClientRect, GetMessageW, GetParent, GetWindowRect, IsWindow,
        KillTimer, LoadCursorW, PeekMessageW, PostThreadMessageW, RegisterClassExW,
        SetLayeredWindowAttributes, SetTimer, SetWindowPos, ShowWindow, TranslateMessage,
        HWND_TOP, IDC_HAND, LWA_COLORKEY, MSG, PM_NOREMOVE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
        SW_HIDE, WM_LBUTTONUP, WM_PAINT, WM_QUIT, WM_TIMER, WM_USER, WNDCLASSEXW, WS_CHILD,
        WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_VISIBLE,
    };

    use super::{format_speed, DOWN, UP};

    const CLASS: &str = "OpenWireTaskbarSpeed";
    /// Explorer is polled at this pace: the readout updates, and follows the
    /// tray as it grows, shrinks, moves, or Explorer restarts.
    const TICK_MS: u32 = 1000;
    /// Room left between the readout and the tray's overflow chevron, in DIPs.
    const GAP: i32 = 6;
    const POINT_SIZE: i32 = 9;
    /// The direction chevrons come from Windows' own icon font - the taskbar
    /// draws its overflow chevron the same way - a size under the text.
    const ICON_FACE: &str = "Segoe MDL2 Assets";
    const ICON_POINT_SIZE: i32 = 7;
    const CHEVRON_UP: &str = "\u{E70E}";
    const CHEVRON_DOWN: &str = "\u{E70D}";

    struct Running {
        thread_id: u32,
        join: JoinHandle<()>,
    }

    static RUNNING: Mutex<Option<Running>> = Mutex::new(None);
    static APP: OnceLock<AppHandle> = OnceLock::new();

    /// Shows or removes the readout. Idempotent; safe from any thread.
    pub fn set_enabled(app: &AppHandle, on: bool) {
        let _ = APP.set(app.clone());
        let mut running = RUNNING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        if on && running.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("taskbar-speed".into())
                .spawn(move || {
                    // SAFETY: plain Win32 calls on this thread's own queue.
                    unsafe {
                        // Create the queue before announcing the thread, or a
                        // quit posted straight away would have nowhere to land.
                        let mut msg: MSG = std::mem::zeroed();
                        PeekMessageW(&mut msg, std::ptr::null_mut(), WM_USER, WM_USER, PM_NOREMOVE);
                        let _ = tx.send(GetCurrentThreadId());
                        run();
                    }
                });
            match spawned {
                Ok(join) => {
                    if let Ok(thread_id) = rx.recv() {
                        *running = Some(Running { thread_id, join });
                    }
                }
                Err(err) => log::warn!("taskbar readout thread: {err}"),
            }
        } else if !on {
            if let Some(Running { thread_id, join }) = running.take() {
                // SAFETY: the id is a live thread with a queue (see above).
                unsafe { PostThreadMessageW(thread_id, WM_QUIT, 0, 0) };
                let _ = join.join();
            }
        }
    }

    /// Everything the window needs, owned by its thread.
    struct Widget {
        hwnd: HWND,
        taskbar: HWND,
        dpi: u32,
        font: HFONT,
        icons: HFONT,
        width: i32,
        height: i32,
        light: bool,
        up: String,
        down: String,
    }

    thread_local! {
        static WIDGET: RefCell<Widget> = const {
            RefCell::new(Widget {
                hwnd: std::ptr::null_mut(),
                taskbar: std::ptr::null_mut(),
                dpi: 0,
                font: std::ptr::null_mut(),
                icons: std::ptr::null_mut(),
                width: 0,
                height: 0,
                light: false,
                up: String::new(),
                down: String::new(),
            })
        };
    }

    unsafe fn run() {
        let class = wide(CLASS);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(window_proc),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_HAND),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        // Fails harmlessly when a previous run already registered it.
        RegisterClassExW(&wc);

        tick();
        // A thread timer, not a window one: it must outlive the window, which
        // Explorer destroys whenever it restarts.
        let timer = SetTimer(std::ptr::null_mut(), 0, TICK_MS, None);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.message == WM_TIMER && msg.hwnd.is_null() {
                tick();
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        KillTimer(std::ptr::null_mut(), timer);
        WIDGET.with(|w| {
            let mut w = w.borrow_mut();
            if !w.hwnd.is_null() && IsWindow(w.hwnd) != 0 {
                DestroyWindow(w.hwnd);
            }
            w.release_fonts();
            w.hwnd = std::ptr::null_mut();
        });
    }

    /// Re-finds the taskbar, re-embeds if Explorer restarted, lays out beside
    /// the tray, and repaints with the latest sample.
    unsafe fn tick() {
        let taskbar = FindWindowW(wide("Shell_TrayWnd").as_ptr(), std::ptr::null());

        // Borrow, decide, release - then call into Win32, which may re-enter
        // the window procedure.
        let (hwnd, place) = WIDGET.with(|w| {
            let mut w = w.borrow_mut();

            if !w.hwnd.is_null() && (IsWindow(w.hwnd) == 0 || GetParent(w.hwnd) != taskbar) {
                if IsWindow(w.hwnd) != 0 {
                    DestroyWindow(w.hwnd);
                }
                w.hwnd = std::ptr::null_mut();
            }
            w.taskbar = taskbar;
            if taskbar.is_null() {
                return (std::ptr::null_mut(), None);
            }

            let dpi = match GetDpiForWindow(taskbar) {
                0 => 96,
                dpi => dpi,
            };
            if dpi != w.dpi || w.font.is_null() {
                w.resize(dpi);
            }
            w.light = taskbar_is_light();
            w.up = format_speed(UP.load(Ordering::Relaxed));
            w.down = format_speed(DOWN.load(Ordering::Relaxed));

            (w.hwnd, w.placement())
        });

        if taskbar.is_null() {
            return;
        }
        let Some((x, y, width, height)) = place else {
            if !hwnd.is_null() {
                ShowWindow(hwnd, SW_HIDE);
            }
            return;
        };

        let hwnd = if hwnd.is_null() {
            let created = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_NOACTIVATE,
                wide(CLASS).as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
                x,
                y,
                width,
                height,
                taskbar,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            if created.is_null() {
                return;
            }
            WIDGET.with(|w| w.borrow_mut().hwnd = created);
            created
        } else {
            hwnd
        };

        let light = WIDGET.with(|w| w.borrow().light);
        // Pixels exactly the key colour are see-through, so the taskbar's own
        // material shows around the text. Text edges blend towards the key, so
        // it sits close to the taskbar's colour in each mode.
        SetLayeredWindowAttributes(hwnd, key_colour(light), 255, LWA_COLORKEY);
        // On top of the taskbar's XAML content, every tick: Explorer re-stacks
        // its own children when the taskbar changes.
        SetWindowPos(hwnd, HWND_TOP, x, y, width, height, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        InvalidateRect(hwnd, std::ptr::null(), 0);
    }

    impl Widget {
        /// New fonts and box for a DPI.
        unsafe fn resize(&mut self, dpi: u32) {
            self.release_fonts();
            self.dpi = dpi;
            self.font = font("Segoe UI", POINT_SIZE, dpi);
            self.icons = font(ICON_FACE, ICON_POINT_SIZE, dpi);

            // Sized for the widest value the format can produce, so the box
            // never jitters as the numbers change.
            let dc = GetDC(std::ptr::null_mut());
            let old = SelectObject(dc, self.font);
            let mut metrics: TEXTMETRICW = std::mem::zeroed();
            GetTextMetricsW(dc, &mut metrics);
            let widest = ["8888 KB/s", "88.8 MB/s"]
                .iter()
                .map(|text| text_width(dc, text))
                .max()
                .unwrap_or(0);
            let space = text_width(dc, " ");
            SelectObject(dc, self.icons);
            let chevron = text_width(dc, CHEVRON_DOWN);
            SelectObject(dc, old);
            ReleaseDC(std::ptr::null_mut(), dc);

            self.width = chevron + space + widest;
            self.height = metrics.tmHeight * 2;
        }

        unsafe fn release_fonts(&mut self) {
            for font in [&mut self.font, &mut self.icons] {
                if !font.is_null() {
                    DeleteObject(*font);
                    *font = std::ptr::null_mut();
                }
            }
        }

        /// Where the readout goes, in the taskbar's client coordinates: just
        /// left of the tray, centred vertically. `None` when it cannot sit
        /// there - a vertical taskbar, or no tray to anchor to.
        unsafe fn placement(&self) -> Option<(i32, i32, i32, i32)> {
            let mut bar: RECT = std::mem::zeroed();
            GetWindowRect(self.taskbar, &mut bar);
            if bar.bottom - bar.top > bar.right - bar.left {
                return None;
            }
            let tray = FindWindowExW(
                self.taskbar,
                std::ptr::null_mut(),
                wide("TrayNotifyWnd").as_ptr(),
                std::ptr::null(),
            );
            if tray.is_null() {
                return None;
            }
            let mut notify: RECT = std::mem::zeroed();
            GetWindowRect(tray, &mut notify);

            let gap = GAP * self.dpi as i32 / 96;
            let x = notify.left - bar.left - self.width - gap;
            let y = (bar.bottom - bar.top - self.height) / 2;
            (x > 0).then_some((x, y, self.width, self.height))
        }
    }

    unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_PAINT => {
                paint(hwnd);
                0
            }
            WM_LBUTTONUP => {
                // Never wait on the main thread from here (see the module docs).
                if let Some(app) = APP.get() {
                    let handle = app.clone();
                    let _ = app.run_on_main_thread(move || crate::tray::focus_window(&handle));
                }
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    unsafe fn paint(hwnd: HWND) {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(hwnd, &mut ps);
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut client);
        let (width, height) = (client.right, client.bottom);

        // Drawn off-screen and copied in one go, so a repaint never flickers.
        let memory = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, width, height);
        let old_bitmap = SelectObject(memory, bitmap);

        WIDGET.with(|w| {
            let w = w.borrow();
            let background = CreateSolidBrush(key_colour(w.light));
            FillRect(memory, &client, background);
            DeleteObject(background);

            let old_font = SelectObject(memory, w.font);
            SetBkMode(memory, TRANSPARENT as i32);
            let ink = if w.light { rgb(26, 26, 26) } else { rgb(255, 255, 255) };
            // The chart colours: magenta outgoing, gold incoming - deepened on
            // a light taskbar, where the bright ones wash out.
            let (up_colour, down_colour) = if w.light {
                (rgb(184, 31, 99), rgb(163, 127, 0))
            } else {
                (rgb(255, 77, 148), rgb(255, 214, 10))
            };

            let half = height / 2;
            let rows = [
                (CHEVRON_UP, up_colour, &w.up, RECT { left: 0, top: 0, right: width, bottom: half }),
                (CHEVRON_DOWN, down_colour, &w.down, RECT { left: 0, top: half, right: width, bottom: height }),
            ];
            for (chevron, colour, value, mut row) in rows {
                let flags = DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX;
                SelectObject(memory, w.icons);
                SetTextColor(memory, colour);
                draw(memory, chevron, &mut row, flags | DT_LEFT);
                SelectObject(memory, w.font);
                SetTextColor(memory, ink);
                draw(memory, value, &mut row, flags | DT_RIGHT);
            }
            SelectObject(memory, old_font);
        });

        BitBlt(dc, 0, 0, width, height, memory, 0, 0, SRCCOPY);
        SelectObject(memory, old_bitmap);
        DeleteObject(bitmap);
        DeleteDC(memory);
        EndPaint(hwnd, &ps);
    }

    unsafe fn font(face: &str, points: i32, dpi: u32) -> HFONT {
        CreateFontW(
            -(points * dpi as i32 / 72),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            ANTIALIASED_QUALITY as u32,
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            wide(face).as_ptr(),
        )
    }

    unsafe fn draw(dc: windows_sys::Win32::Graphics::Gdi::HDC, text: &str, rect: &mut RECT, flags: u32) {
        let text: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(dc, text.as_ptr(), text.len() as i32, rect, flags);
    }

    unsafe fn text_width(dc: windows_sys::Win32::Graphics::Gdi::HDC, text: &str) -> i32 {
        let text: Vec<u16> = text.encode_utf16().collect();
        let mut size = std::mem::zeroed();
        GetTextExtentPoint32W(dc, text.as_ptr(), text.len() as i32, &mut size);
        size.cx
    }

    /// Whether the taskbar is light - its own setting, separate from apps'.
    fn taskbar_is_light() -> bool {
        use winreg::enums::HKEY_CURRENT_USER;
        winreg::RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
            .and_then(|key| key.get_value::<u32, _>("SystemUsesLightTheme"))
            .is_ok_and(|light| light == 1)
    }

    fn key_colour(light: bool) -> u32 {
        if light {
            rgb(238, 238, 238)
        } else {
            rgb(28, 28, 28)
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    const fn rgb(r: u8, g: u8, b: u8) -> u32 {
        r as u32 | (g as u32) << 8 | (b as u32) << 16
    }
}

#[cfg(test)]
mod tests {
    use super::format_speed;

    #[test]
    fn speeds_read_like_the_ui() {
        assert_eq!(format_speed(0), "0 B/s");
        assert_eq!(format_speed(512), "512 B/s");
        assert_eq!(format_speed(1536), "1.5 KB/s");
        assert_eq!(format_speed(150 * 1024), "150 KB/s");
        assert_eq!(format_speed(1023 * 1024), "1023 KB/s");
        assert_eq!(format_speed(2_516_582), "2.4 MB/s");
    }
}
