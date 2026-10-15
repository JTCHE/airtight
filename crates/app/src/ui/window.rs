//! The window class, the two windows, their DWM frame and their place
//! above the taskbar.

use super::theme::Theme;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

pub const CLASS: PCWSTR = w!("AirtightPanel");
const TOAST_CLASS: PCWSTR = w!("AirtightToast");

pub type Proc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

fn register(class: PCWSTR, proc: Proc) -> Result<()> {
    let instance = unsafe { GetModuleHandleW(None)? };
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: instance.into(),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        lpszClassName: class,
        ..Default::default()
    };
    unsafe { RegisterClassExW(&wc) };
    Ok(())
}

pub fn create(proc: Proc) -> Result<(HWND, HWND)> {
    register(CLASS, proc)?;
    register(TOAST_CLASS, proc)?;
    let instance = unsafe { GetModuleHandleW(None)? };
    unsafe {
        // All content comes through DirectComposition.
        let panel = CreateWindowExW(
            // Topmost like the system flyouts; it hides when it loses focus.
            WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST,
            CLASS,
            w!("Airtight"),
            WS_POPUP,
            0,
            0,
            320,
            200,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        let toast = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            TOAST_CLASS,
            w!("Airtight"),
            WS_POPUP,
            0,
            0,
            300,
            60,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        Ok((panel, toast))
    }
}

fn set(hwnd: HWND, attribute: DWMWINDOWATTRIBUTE, value: u32) -> bool {
    unsafe { DwmSetWindowAttribute(hwnd, attribute, (&raw const value).cast(), 4).is_ok() }
}

/// Dark title colors, rounded corners and the border; Mica behind the
/// panel. Returns false when Mica was refused.
pub fn style(hwnd: HWND, theme: &Theme, mica: bool) -> bool {
    set(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, u32::from(theme.dark));
    set(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND.0 as u32);
    let b = theme.border;
    set(hwnd, DWMWA_BORDER_COLOR, (b >> 16) & 0xff | (b & 0xff00) | (b & 0xff) << 16);
    if !mica {
        return false;
    }
    let margins = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
    let _ = unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins) };
    set(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW.0 as u32)
}

pub fn scale(hwnd: HWND) -> f32 {
    unsafe { GetDpiForWindow(hwnd) as f32 / 96.0 }
}

/// Put the window's bottom-right corner above the taskbar, on the screen
/// under the cursor (or the primary one), at `width` x `height` DIPs.
pub fn place(hwnd: HWND, width: f32, height: f32, under_cursor: bool) {
    let mut cursor = POINT::default();
    let _ = unsafe { GetCursorPos(&mut cursor) };
    let flag = if under_cursor { MONITOR_DEFAULTTONEAREST } else { MONITOR_DEFAULTTOPRIMARY };
    let at = if under_cursor { cursor } else { POINT::default() };
    let monitor = unsafe { MonitorFromPoint(at, flag) };
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return;
    }
    let s = scale(hwnd);
    let work: RECT = info.rcWork;
    // An auto-hidden taskbar reserves no work area, and then pops up over
    // the panel: leave room for it.
    let gap = if work.bottom < info.rcMonitor.bottom { 12.0 } else { 56.0 };
    let (w, h) = ((width * s).round() as i32, (height * s).round() as i32);
    let (x, y) = (work.right - w - (12.0 * s) as i32, work.bottom - h - (gap * s) as i32);
    let _ = unsafe { SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE) };
}
