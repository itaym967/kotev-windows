//! Floating recording pill that never takes focus (port of RecordingOverlayController.swift).

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

pub const LABEL: &str = "overlay";
const TOP_MARGIN: i32 = 18;
const ERROR_VISIBLE: Duration = Duration::from_secs(5);

static GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// Red dot: actively listening.
    Listening,
    /// Orange dot: starting / finishing.
    Busy,
    Error,
}

#[derive(Serialize, Clone)]
struct Payload {
    kind: Kind,
    label: String,
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

/// Apply non-activating, tool-window, click-through styles once at startup.
pub fn configure(app: &AppHandle) {
    let Some(win) = window(app) else { return };
    let _ = win.set_ignore_cursor_events(true);
    #[cfg(windows)]
    if let Ok(hwnd) = win.hwnd() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        };
        unsafe {
            let hwnd = hwnd.0 as _;
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST) as isize,
            );
        }
    }
}

pub fn show(app: &AppHandle, kind: Kind, label: String) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let Some(win) = window(app) else { return };
    let _ = app.emit_to(LABEL, "overlay", Payload { kind, label });
    position_and_show(&win);
    if matches!(kind, Kind::Error) {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(ERROR_VISIBLE);
            if GENERATION.load(Ordering::SeqCst) == generation {
                hide(&app);
            }
        });
    }
}

pub fn hide(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    let Some(win) = window(app) else { return };
    // Shown with a raw ShowWindow, so Tauri still thinks it's hidden and would ignore hide().
    #[cfg(windows)]
    if let Ok(hwnd) = win.hwnd() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
        unsafe { ShowWindow(hwnd.0 as _, SW_HIDE) };
        return;
    }
    let _ = win.hide();
}

pub fn level(app: &AppHandle, value: f32) {
    let _ = app.emit_to(LABEL, "overlay-level", value);
}

#[cfg(windows)]
fn position_and_show(win: &WebviewWindow) {
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SW_SHOWNOACTIVATE,
    };
    unsafe {
        // Center on the monitor that holds the window the user is dictating into.
        let monitor = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) != 0 {
            let width = win.outer_size().map(|s| s.width as i32).unwrap_or(420);
            let work = info.rcWork;
            let x = work.left + (work.right - work.left - width) / 2;
            let _ = win.set_position(PhysicalPosition::new(x, work.top + TOP_MARGIN));
        }
        if let Ok(hwnd) = win.hwnd() {
            let hwnd = hwnd.0 as _;
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE);
        }
    }
}

#[cfg(not(windows))]
fn position_and_show(win: &WebviewWindow) {
    let _ = win.show();
}
