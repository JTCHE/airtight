//! The panel's messages: pointer, keyboard, menus, timers and the posts
//! from the other threads.

use super::menu::{Item, popup};
use super::panel::{Panel, Part};
use super::*;
use crate::link::Event;
use std::sync::OnceLock;
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Accessibility::{UiaReturnRawElementProvider, UiaRootObjectId};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::core::{HSTRING, PCWSTR};

const BUFFERS: [u32; 5] = [120, 150, 250, 400, 600];
const MENU_DEFAULT_OUTPUT: usize = 99;
const MENU_ENDPOINT: usize = 100;
const MENU_BUFFER: usize = 200;
const MENU_AUTO_CONNECT: usize = 300;
const MENU_AUTO_START: usize = 301;
const MENU_LOGON: usize = 302;
const MENU_SEARCH: usize = 400;
const MENU_LOG: usize = 401;
const MENU_QUIT: usize = 402;
const MENU_OPEN: usize = 403;

/// Explorer sends this when it restarts: the tray icon is gone.
fn taskbar_created() -> u32 {
    static ID: OnceLock<u32> = OnceLock::new();
    *ID.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) })
}

fn low(v: isize) -> f32 {
    f32::from(v as u16 as i16)
}

impl App {
    pub(super) fn panel_message(&mut self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        let s = window::scale(self.hwnd);
        let (x, y) = (low(lp.0) / s, low(lp.0 >> 16) / s);
        match msg {
            WM_PAINT => {
                self.paint_panel();
                let _ = unsafe { ValidateRect(Some(self.hwnd), None) };
            }
            WM_APP_TRAY => self.tray_click(lp.0 as u32 & 0xffff),
            WM_APP_LINK => self.on_link(),
            WM_APP_FOUND => self.on_found(),
            WM_APP_SHOW => self.show(),
            WM_APP_QUIT => self.quit(),
            WM_APP_ACCESS => match access::action(wp, lp) {
                (Part::Slider, Some(volume)) => self.set_volume(volume),
                (part, None) => self.activate(part),
                _ => {}
            },
            WM_GETOBJECT if lp.0 as i32 == UiaRootObjectId => {
                let root = access::root(self.hwnd, &self.access);
                return Some(unsafe { UiaReturnRawElementProvider(self.hwnd, wp, lp, &root) });
            }
            WM_TIMER => self.timer(wp.0),
            WM_ACTIVATE if wp.0 & 0xffff == WA_INACTIVE as usize && self.autohide => self.hide(),
            WM_MOUSEMOVE => self.pointer_move(x, y),
            WM_MOUSELEAVE => {
                (self.panel.hot, self.panel.hint) = (None, true);
                self.redraw();
            }
            WM_LBUTTONDOWN => self.pointer_down(x, y),
            WM_LBUTTONUP => self.pointer_up(x, y),
            WM_KEYDOWN => self.key(wp.0 as u16),
            WM_SETTINGCHANGE if lp.0 != 0 && unsafe { PCWSTR(lp.0 as _).to_string() }.is_ok_and(|s| s == "ImmersiveColorSet") => {
                self.theme_changed()
            }
            WM_DPICHANGED => self.relayout(),
            WM_CLOSE => self.hide(),
            WM_DESTROY => unsafe { PostQuitMessage(0) },
            m if m == taskbar_created() => self.sync_tray(false),
            _ => return None,
        }
        Some(LRESULT(0))
    }

    fn timer(&mut self, id: usize) {
        match id {
            TIMER_SPIN => self.spin(),
            TIMER_VOLUME => self.send_volume(),
            TIMER_ARM => {
                let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_ARM) };
                self.autohide = true;
            }
            TIMER_TOAST => self.hide_toast(),
            _ => {}
        }
    }

    fn tray_click(&mut self, message: u32) {
        match message {
            WM_LBUTTONUP => self.toggle(),
            WM_RBUTTONUP => {
                let mut at = POINT::default();
                let _ = unsafe { GetCursorPos(&mut at) };
                let items = [Item::Command(MENU_OPEN, "Open Airtight".into(), true), Item::Command(MENU_QUIT, "Quit Airtight".into(), true)];
                match popup(self.hwnd, &items, at, true) {
                    MENU_OPEN => self.show(),
                    MENU_QUIT => self.quit(),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn pointer_move(&mut self, x: f32, y: f32) {
        let layout = self.panel.layout(&self.gfx);
        if self.dragging {
            self.set_volume(Panel::volume_at(&layout, x));
            return;
        }
        let hot = self.panel.hit(&layout, x, y);
        if hot != self.panel.hot {
            if matches!(self.panel.hot, Some(Part::Row(_))) {
                self.panel.hint = true; // a fresh hover shows the hint again
            }
            self.panel.hot = hot;
            let mut track = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: self.hwnd, dwHoverTime: 0 };
            let _ = unsafe { TrackMouseEvent(&mut track) };
            self.redraw();
        }
    }

    fn pointer_down(&mut self, x: f32, y: f32) {
        let layout = self.panel.layout(&self.gfx);
        let hit = self.panel.hit(&layout, x, y);
        (self.panel.pressed, self.panel.keyboard) = (hit, false);
        if hit.is_some() {
            self.panel.focus = hit;
        }
        if hit == Some(Part::Slider) {
            self.dragging = true;
            unsafe { SetCapture(self.hwnd) };
            self.set_volume(Panel::volume_at(&layout, x));
        }
        self.redraw();
    }

    fn pointer_up(&mut self, x: f32, y: f32) {
        if std::mem::take(&mut self.dragging) {
            let _ = unsafe { ReleaseCapture() };
        }
        let hit = self.panel.hit(&self.panel.layout(&self.gfx), x, y);
        let pressed = self.panel.pressed.take();
        if pressed == hit {
            if let Some(part) = hit.filter(|p| *p != Part::Slider) {
                self.activate(part);
            }
        }
        self.redraw();
    }

    fn activate(&mut self, part: Part) {
        match part {
            Part::Row(i) => {
                self.panel.hint = false;
                let receiver = self.panel.receivers[i].clone();
                self.link_send(Event::Toggle(receiver));
            }
            Part::More => self.more_menu(),
            Part::Slider => {}
        }
    }

    fn key(&mut self, key: u16) {
        let rows = self.panel.receivers.len();
        let order: Vec<Part> = (0..rows).map(Part::Row).chain([Part::Slider, Part::More]).collect();
        let at = self.panel.focus.and_then(|f| order.iter().position(|p| *p == f));
        let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
        let volume = i32::from(self.panel.volume);
        let set = |v: i32| v.clamp(0, 100) as u8;
        match VIRTUAL_KEY(key) {
            VK_ESCAPE => return self.hide(),
            VK_TAB => {
                let n = order.len();
                let next = match (at, shift) {
                    (None, false) => 0,
                    (None, true) => n - 1,
                    (Some(i), false) => (i + 1) % n,
                    (Some(i), true) => (i + n - 1) % n,
                };
                self.panel.focus = Some(order[next]);
            }
            VK_UP | VK_DOWN if rows > 0 && self.panel.focus != Some(Part::Slider) => {
                let i = match self.panel.focus {
                    Some(Part::Row(i)) => i as i32,
                    _ => -1,
                };
                let step = if VIRTUAL_KEY(key) == VK_DOWN { 1 } else { -1 };
                self.panel.focus = Some(Part::Row((i + step).clamp(0, rows as i32 - 1) as usize));
            }
            VK_LEFT | VK_DOWN if self.panel.focus == Some(Part::Slider) => self.set_volume(set(volume - 1)),
            VK_RIGHT | VK_UP if self.panel.focus == Some(Part::Slider) => self.set_volume(set(volume + 1)),
            VK_NEXT if self.panel.focus == Some(Part::Slider) => self.set_volume(set(volume - 10)),
            VK_PRIOR if self.panel.focus == Some(Part::Slider) => self.set_volume(set(volume + 10)),
            VK_HOME if self.panel.focus == Some(Part::Slider) => self.set_volume(0),
            VK_END if self.panel.focus == Some(Part::Slider) => self.set_volume(100),
            VK_RETURN | VK_SPACE => {
                if let Some(part) = self.panel.focus {
                    self.activate(part);
                }
            }
            _ => return,
        }
        if let Some(part) = self.panel.focus.filter(|f| Some(*f) != order.get(at.unwrap_or(usize::MAX)).copied()) {
            access::focus_moved(self.hwnd, &self.access, part);
        }
        self.panel.keyboard = true;
        self.redraw();
    }

    fn more_menu(&mut self) {
        let s = self.settings.lock().unwrap().clone();
        let endpoints = airtight_audio::endpoint::list();
        let mut sources = vec![Item::Check(MENU_DEFAULT_OUTPUT, "Default output".into(), s.endpoint.is_none())];
        sources.extend(endpoints.iter().enumerate().map(|(i, e)| Item::Check(MENU_ENDPOINT + i, e.name.clone(), s.endpoint.as_deref() == Some(&e.name))));
        let buffers = BUFFERS.iter().enumerate().map(|(i, ms)| Item::Check(MENU_BUFFER + i, format!("{ms} ms"), s.latency_ms == *ms));
        let items = [
            Item::Sub("Audio source".into(), sources),
            Item::Sub("Buffer".into(), buffers.collect()),
            Item::Separator,
            Item::Check(MENU_AUTO_CONNECT, "Connect on launch".into(), s.auto_connect),
            Item::Check(MENU_AUTO_START, "Connect when audio plays".into(), s.auto_start),
            Item::Check(MENU_LOGON, "Start with Windows".into(), logon::enabled()),
            Item::Separator,
            Item::Command(MENU_SEARCH, "Search for receivers".into(), !self.panel.scanning),
            Item::Command(MENU_LOG, "Open session log".into(), airtight_log::path().exists()),
            Item::Separator,
            Item::Command(MENU_QUIT, "Quit Airtight".into(), true),
        ];
        let layout = self.panel.layout(&self.gfx);
        let scale = window::scale(self.hwnd);
        let mut at = POINT { x: ((layout.more.x + layout.more.w) * scale) as i32, y: ((layout.more.y + layout.more.h) * scale) as i32 };
        let _ = unsafe { ClientToScreen(self.hwnd, &mut at) };
        self.autohide = false; // the menu takes the focus: do not close under it
        let chosen = popup(self.hwnd, &items, at, false);
        self.autohide = true;
        self.panel.pressed = None;
        self.menu_command(chosen, &endpoints);
    }

    fn menu_command(&mut self, chosen: usize, endpoints: &[airtight_audio::Endpoint]) {
        let restart = |app: &mut App, change: &dyn Fn(&mut Settings)| {
            let mut s = app.settings.lock().unwrap();
            change(&mut s);
            s.save();
            drop(s);
            app.update_source();
            app.relayout();
            app.link_send(Event::Settings(true));
        };
        match chosen {
            MENU_DEFAULT_OUTPUT => restart(self, &|s| s.endpoint = None),
            c if (MENU_ENDPOINT..MENU_ENDPOINT + endpoints.len()).contains(&c) => {
                let name = endpoints[c - MENU_ENDPOINT].name.clone();
                restart(self, &move |s| s.endpoint = Some(name.clone()));
            }
            c if (MENU_BUFFER..MENU_BUFFER + BUFFERS.len()).contains(&c) => restart(self, &|s| s.latency_ms = BUFFERS[c - MENU_BUFFER]),
            MENU_AUTO_CONNECT | MENU_AUTO_START => {
                let mut s = self.settings.lock().unwrap();
                if chosen == MENU_AUTO_CONNECT {
                    s.auto_connect = !s.auto_connect;
                } else {
                    s.auto_start = !s.auto_start;
                }
                s.save();
                drop(s);
                self.link_send(Event::Settings(false));
            }
            MENU_LOGON => logon::set(!logon::enabled()),
            MENU_SEARCH => self.refresh(),
            MENU_LOG => {
                let path = HSTRING::from(airtight_log::path().as_os_str());
                unsafe { ShellExecuteW(None, w!("open"), &path, None, None, SW_SHOWNORMAL) };
            }
            MENU_QUIT => self.quit(),
            _ => {}
        }
    }
}
