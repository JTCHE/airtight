//! WinUI 3 tokens, light or dark from the Windows settings. Colors are
//! 0xAARRGGBB; translucent fills blend with Mica like the system controls.
//!
//! Contrast on the composited dark layer (#282828) and light layer
//! (#f9f9f9): text 14.7:1 / 16.4:1, muted 8.5:1 / 6.4:1, warn 11.2:1 /
//! 5.0:1, track 5.2:1 / 3.5:1.

use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::{PCWSTR, w};

#[derive(Clone, Copy, PartialEq)]
pub struct Theme {
    pub dark: bool,
    /// The solid window color, behind the toast and without Mica.
    pub bg: u32,
    /// The filled surface for the main part of the panel.
    pub layer: u32,
    pub stroke: u32,
    pub border: u32,
    pub hover: u32,
    pub press: u32,
    pub disc: u32,
    pub text: u32,
    pub muted: u32,
    pub track: u32,
    pub thumb: u32,
    pub accent: u32,
    pub on_accent: u32,
    pub warn: u32,
    pub focus: u32,
}

const DARK: Theme = Theme {
    dark: true,
    bg: 0xff202020,
    layer: 0x4c3a3a3a,
    stroke: 0x15ffffff,
    border: 0xff3d3d3d,
    hover: 0x0fffffff,
    press: 0x0affffff,
    disc: 0x1affffff,
    text: 0xffffffff,
    muted: 0xffc5c5c5,
    track: 0xff9a9a9a,
    thumb: 0xff454545,
    accent: 0xff60cdff,
    on_accent: 0xff000000,
    warn: 0xfffce100,
    focus: 0xffffffff,
};

const LIGHT: Theme = Theme {
    dark: false,
    bg: 0xfff3f3f3,
    layer: 0x80ffffff,
    stroke: 0x0f000000,
    border: 0xffd5d5d5,
    hover: 0x0a000000,
    press: 0x06000000,
    disc: 0x0f000000,
    text: 0xff1b1b1b,
    muted: 0xff5c5c5c,
    track: 0xff868686,
    thumb: 0xffffffff,
    accent: 0xff005fb8,
    on_accent: 0xffffffff,
    warn: 0xff9d5d00,
    focus: 0xff000000,
};

fn personalize(value: PCWSTR) -> Option<u32> {
    let (mut v, mut size) = (0u32, 4u32);
    let key = w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, key, value, RRF_RT_REG_DWORD, None, Some((&raw mut v).cast()), Some(&mut size)) };
    r.is_ok().then_some(v)
}

impl Theme {
    pub fn current() -> Theme {
        if personalize(w!("AppsUseLightTheme")) == Some(1) { LIGHT } else { DARK }
    }
}

/// The taskbar has its own setting; the tray glyph follows it.
pub fn taskbar_light() -> bool {
    personalize(w!("SystemUsesLightTheme")) == Some(1)
}
