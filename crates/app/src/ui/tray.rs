//! The tray icon: three rising bars, full strength while streaming.

use super::WM_APP_TRAY;
use super::glyph::{bars, coverage};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, DestroyIcon, HICON, ICONINFO, SM_CXSMICON};

/// The glyph as premultiplied BGRA, `size` pixels a side.
fn glyph(size: u32, rgb: u32, alpha: f32) -> Vec<u8> {
    let bars = bars(size as f32);
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let c: f32 = bars.iter().map(|&b| coverage(x as f32, y as f32, b)).sum::<f32>().min(1.0) * alpha;
            let ch = |s: u32| (((rgb >> s) & 0xff) as f32 * c).round() as u8;
            out.extend_from_slice(&[ch(0), ch(8), ch(16), (c * 255.0).round() as u8]);
        }
    }
    out
}

fn icon(rgb: u32, alpha: f32) -> Option<HICON> {
    let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) } as u32;
    let pixels = glyph(size, rgb, alpha);
    unsafe {
        let color = CreateBitmap(size as i32, size as i32, 1, 32, Some(pixels.as_ptr().cast()));
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, None);
        let info = ICONINFO { fIcon: true.into(), hbmMask: mask, hbmColor: color, ..Default::default() };
        let icon = CreateIconIndirect(&info).ok();
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

pub struct Tray {
    hwnd: HWND,
    icon: Option<HICON>,
    shown: Option<(bool, bool, String)>,
}

impl Tray {
    pub fn new(hwnd: HWND) -> Tray {
        Tray { hwnd, icon: None, shown: None }
    }

    fn data(&self, tip: &str) -> NOTIFYICONDATAW {
        let mut d = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_APP_TRAY,
            hIcon: self.icon.unwrap_or_default(),
            ..Default::default()
        };
        for (i, c) in tip.encode_utf16().take(127).enumerate() {
            d.szTip[i] = c;
        }
        d
    }

    /// Show the state; `added` false re-adds the icon (Explorer restarted).
    pub fn update(&mut self, playing: bool, tip: &str, added: bool) {
        let light = super::theme::taskbar_light();
        let want = (playing, light, tip.to_string());
        if added && self.shown.as_ref() == Some(&want) {
            return;
        }
        if self.shown.as_ref().is_none_or(|s| (s.0, s.1) != (playing, light)) || self.icon.is_none() {
            let old = self.icon.take();
            self.icon = icon(if light { 0x1b1b1b } else { 0xffffff }, if playing { 1.0 } else { 0.55 });
            if let Some(old) = old {
                let _ = unsafe { DestroyIcon(old) };
            }
        }
        let d = self.data(tip);
        let modified = added && self.shown.is_some() && unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) }.as_bool();
        if !modified {
            let _ = unsafe { Shell_NotifyIconW(NIM_ADD, &d) };
        }
        self.shown = Some(want);
    }

    pub fn remove(&mut self) {
        let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &self.data("")) };
        self.shown = None;
    }
}
