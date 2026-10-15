//! The tray icon, the panel and the toast, on one UI thread. The link and
//! discovery run elsewhere and post their results here.

mod access;
mod actions;
mod gfx;
mod glyph;
mod input;
mod logon;
mod menu;
mod panel;
mod target;
mod theme;
mod toast;
mod tray;
mod window;

use crate::link::{Link, Receiver, Shared, Snapshot, State};
use crate::nowplaying::Hub;
use crate::settings::Settings;
use gfx::{Gfx, Painter};
use panel::Panel;
use std::cell::Cell;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use target::Target;
use theme::Theme;
use toast::Toast;
use tray::Tray;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{InvalidateRect, ValidateRect};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

const WM_APP_TRAY: u32 = WM_APP + 1;
const WM_APP_LINK: u32 = WM_APP + 2;
const WM_APP_FOUND: u32 = WM_APP + 3;
const WM_APP_SHOW: u32 = WM_APP + 4;
const WM_APP_QUIT: u32 = WM_APP + 5;
const WM_APP_ACCESS: u32 = WM_APP + 6;

const TIMER_SPIN: usize = 1;
const TIMER_VOLUME: usize = 2;
const TIMER_ARM: usize = 3;
const TIMER_TOAST: usize = 4;

struct App {
    hwnd: HWND,
    toast_hwnd: HWND,
    gfx: Gfx,
    theme: Theme,
    mica: bool,
    panel: Panel,
    panel_target: Option<Target>,
    toast: Toast,
    toast_target: Option<Target>,
    toast_shown: bool,
    tray: Tray,
    settings: Arc<Mutex<Settings>>,
    link: Option<Link>,
    status_slot: Arc<Mutex<Snapshot>>,
    found_slot: Arc<Mutex<Option<Vec<Receiver>>>>,
    shown: bool,
    hidden_at: Instant,
    /// Hide on deactivation; off while the window settles and under menus.
    autohide: bool,
    scanned_at: Option<Instant>,
    toast_key: Option<Snapshot>,
    dragging: bool,
    /// What the screen reader reads, kept current on each paint.
    access: access::Shared,
}

thread_local! {
    static APP: Cell<*mut App> = const { Cell::new(std::ptr::null_mut()) };
}

fn app() -> Option<&'static mut App> {
    let p = APP.with(Cell::get);
    // SAFETY: set once in run() on this thread, freed after the loop ends.
    unsafe { p.as_mut() }
}

/// Ask a running instance to quit, for the installer.
pub fn quit_running() -> i32 {
    match unsafe { FindWindowW(window::CLASS, None) } {
        Ok(hwnd) => {
            let _ = unsafe { PostMessageW(Some(hwnd), WM_APP_QUIT, WPARAM(0), LPARAM(0)) };
            0
        }
        Err(_) => 1,
    }
}

pub fn run() -> i32 {
    let _mutex = unsafe { CreateMutexW(None, true, w!("Local\\AirtightSingleInstance")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // A second launch means "show me the panel".
        if let Ok(hwnd) = unsafe { FindWindowW(window::CLASS, None) } {
            // This process got the click; pass the right to come to the front.
            let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
            let _ = unsafe { PostMessageW(Some(hwnd), WM_APP_SHOW, WPARAM(0), LPARAM(0)) };
        }
        return 0;
    }
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    airtight_audio::com();
    menu::follow_dark_mode();
    let Ok(app) = App::new() else { return 1 };
    let first_run = app.settings.lock().unwrap().receiver_host.is_none();
    APP.with(|a| a.set(Box::into_raw(Box::new(app))));
    let app = self::app().unwrap();
    app.start_link();
    app.sync_tray(true);
    if first_run {
        logon::set(true);
        app.show(); // show where to pick a receiver
    } else {
        app.refresh(); // the list is fresh by the time the panel opens
    }
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    0
}

impl App {
    fn new() -> windows::core::Result<App> {
        let (hwnd, toast_hwnd) = window::create(wndproc)?;
        let theme = Theme::current();
        let mica = window::style(hwnd, &theme, true);
        window::style(toast_hwnd, &theme, false);
        let settings = Settings::load();
        let receivers = settings.receivers.iter().filter_map(|(n, h)| Some(Receiver { name: n.clone(), host: h.parse().ok()? })).collect();
        let status = Snapshot { receiver: None, state: State::Idle, note: String::new() };
        let panel = Panel {
            receivers,
            status: status.clone(),
            volume: settings.volume,
            source: String::new(),
            scanning: false,
            hot: None,
            pressed: None,
            focus: None,
            keyboard: false,
            hint: true,
            angle: 0.0,
        };
        let mut app = App {
            hwnd,
            toast_hwnd,
            gfx: Gfx::new()?,
            theme,
            mica,
            panel,
            panel_target: None,
            toast: Toast::new(),
            toast_target: None,
            toast_shown: false,
            tray: Tray::new(hwnd),
            settings: Arc::new(Mutex::new(settings)),
            link: None,
            status_slot: Arc::new(Mutex::new(status)),
            found_slot: Arc::new(Mutex::new(None)),
            shown: false,
            hidden_at: Instant::now(),
            autohide: false,
            scanned_at: None,
            toast_key: None,
            dragging: false,
            access: access::Shared::default(),
        };
        app.update_source();
        app.populate(Vec::new(), false);
        Ok(app)
    }

    fn start_link(&mut self) {
        let (slot, hwnd) = (self.status_slot.clone(), self.hwnd.0 as isize);
        let on_change = Box::new(move |s: &Snapshot| {
            *slot.lock().unwrap() = s.clone();
            let _ = unsafe { PostMessageW(Some(HWND(hwnd as _)), WM_APP_LINK, WPARAM(0), LPARAM(0)) };
        });
        let settings = self.settings.clone();
        let on_receiver = Box::new(move |r: &Receiver| {
            let mut s = settings.lock().unwrap();
            s.receiver_name = Some(r.name.clone());
            s.receiver_host = Some(r.host.to_string());
            s.save();
        });
        let shared = Shared { settings: self.settings.clone(), media: Hub::start(), on_change, on_receiver };
        self.link = Some(Link::start(shared));
    }

    fn paint_panel(&mut self) {
        let layout = self.panel.layout(&self.gfx);
        access::update(&self.access, self.hwnd, &self.panel, &layout, window::scale(self.hwnd));
        if self.panel_target.is_none() {
            self.panel_target = Target::new(&self.gfx.factory, self.hwnd).ok();
        }
        let Some(target) = &mut self.panel_target else { return };
        let ok = target.fit(self.hwnd) && draw(target, &self.gfx, self.hwnd, |p| self.panel.paint(p, &self.theme, &layout, self.mica));
        if !ok {
            self.panel_target = None; // device lost: start over next paint
        }
    }

    fn paint_toast(&mut self) {
        if self.toast_target.is_none() {
            self.toast_target = Target::new(&self.gfx.factory, self.toast_hwnd).ok();
        }
        let Some(target) = &mut self.toast_target else { return };
        let ok = target.fit(self.toast_hwnd) && draw(target, &self.gfx, self.toast_hwnd, |p| self.toast.paint(p, &self.theme));
        if !ok {
            self.toast_target = None;
        }
    }

    fn redraw(&self) {
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
            let _ = InvalidateRect(Some(self.toast_hwnd), None, false);
        }
    }
}

fn draw(target: &Target, gfx: &Gfx, hwnd: HWND, paint: impl FnOnce(&Painter)) -> bool {
    let dpi = window::scale(hwnd) * 96.0;
    unsafe {
        target.dc.SetDpi(dpi, dpi);
        target.dc.BeginDraw();
        paint(&Painter { gfx, dc: &target.dc });
        target.dc.EndDraw(None, None).is_ok() && target.present()
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(a) = app() else { return unsafe { DefWindowProcW(hwnd, msg, wp, lp) } };
    let handled = if hwnd == a.toast_hwnd { a.toast_message(msg) } else { a.panel_message(msg, wp, lp) };
    handled.unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, msg, wp, lp) })
}

impl App {
    fn toast_message(&mut self, msg: u32) -> Option<LRESULT> {
        match msg {
            WM_PAINT => {
                self.paint_toast();
                let _ = unsafe { ValidateRect(Some(self.toast_hwnd), None) };
            }
            WM_MOUSEACTIVATE => return Some(LRESULT(MA_NOACTIVATE as isize)),
            WM_LBUTTONUP => {
                self.hide_toast();
                self.show();
            }
            _ => return None,
        }
        Some(LRESULT(0))
    }
}
