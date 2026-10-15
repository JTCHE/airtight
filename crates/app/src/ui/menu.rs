//! Native popup menus, in the dark when the apps are.

use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCSTR, w};

pub enum Item {
    Command(usize, String, bool),
    /// A checked or radio item: id, text, checked.
    Check(usize, String, bool),
    Sub(String, Vec<Item>),
    Separator,
}

/// Menus follow the app theme only after uxtheme's unnamed
/// SetPreferredAppMode (ordinal 135, AllowDark = 1) and FlushMenuThemes
/// (ordinal 136). Without them the menus stay light.
pub fn follow_dark_mode() {
    unsafe {
        let Ok(uxtheme) = LoadLibraryW(w!("uxtheme.dll")) else { return };
        if let Some(set) = GetProcAddress(uxtheme, PCSTR(135 as _)) {
            let set: extern "system" fn(i32) -> i32 = std::mem::transmute(set);
            set(1);
        }
        if let Some(flush) = GetProcAddress(uxtheme, PCSTR(136 as _)) {
            let flush: extern "system" fn() = std::mem::transmute(flush);
            flush();
        }
    }
}

fn build(items: &[Item]) -> HMENU {
    let menu = unsafe { CreatePopupMenu() }.unwrap_or_default();
    for item in items {
        let (flags, id, text) = match item {
            Item::Command(id, text, enabled) => (if *enabled { MF_STRING } else { MF_STRING | MF_GRAYED }, *id, text.as_str()),
            Item::Check(id, text, on) => (if *on { MF_STRING | MF_CHECKED } else { MF_STRING }, *id, text.as_str()),
            Item::Sub(text, children) => (MF_POPUP, build(children).0 as usize, text.as_str()),
            Item::Separator => (MF_SEPARATOR, 0, ""),
        };
        let _ = unsafe { AppendMenuW(menu, flags, id, &HSTRING::from(text)) };
    }
    menu
}

/// Show the menu at `at` (screen pixels) and return the chosen id, 0 for none.
pub fn popup(hwnd: HWND, items: &[Item], at: POINT, above: bool) -> usize {
    let menu = build(items);
    let align = if above { TPM_BOTTOMALIGN } else { TPM_TOPALIGN };
    unsafe {
        // The menu closes on a click elsewhere only when its owner is in front.
        let _ = SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_RIGHTALIGN | align).0, at.x, at.y, hwnd, None);
        let _ = DestroyMenu(menu);
        chosen.0 as usize
    }
}
