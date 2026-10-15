//! What the panel does: show and hide, follow the link, keep the list,
//! the toast and the tray in step.

use super::panel::{WIDTH, state_text};
use super::toast::{self, Action};
use super::*;
use crate::link::Event;
use std::time::Duration;

/// Opening the panel rescans when the list is older than this.
const RESCAN: Duration = Duration::from_secs(60);

impl App {
    pub(super) fn show(&mut self) {
        let height = self.panel.layout(&self.gfx).height;
        window::place(self.hwnd, WIDTH, height, true);
        (self.panel.keyboard, self.panel.focus, self.panel.hot) = (false, None, None);
        self.autohide = false;
        self.hide_toast();
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = SetForegroundWindow(self.hwnd);
            // Let the window settle before a click elsewhere may close it.
            SetTimer(Some(self.hwnd), TIMER_ARM, 300, None);
        }
        self.shown = true;
        self.sync_spin();
        if self.panel.receivers.is_empty() || self.scanned_at.is_none_or(|t| t.elapsed() > RESCAN) {
            self.refresh();
        }
    }

    pub(super) fn hide(&mut self) {
        if self.shown {
            let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
            self.shown = false;
            self.hidden_at = Instant::now();
            self.dragging = false;
            self.sync_spin();
        }
    }

    pub(super) fn toggle(&mut self) {
        if self.shown {
            self.hide();
        } else if self.hidden_at.elapsed() > Duration::from_millis(300) {
            // A tray click first deactivates the panel: do not reopen at once.
            self.show();
        }
    }

    /// Fit the window to the content while it shows.
    pub(super) fn relayout(&mut self) {
        if self.shown {
            let height = self.panel.layout(&self.gfx).height;
            window::place(self.hwnd, WIDTH, height, true);
        }
        self.redraw();
    }

    pub(super) fn on_link(&mut self) {
        let status = self.status_slot.lock().unwrap().clone();
        self.panel.status = status.clone();
        let missing = status.receiver.as_ref().filter(|r| !self.panel.receivers.iter().any(|x| x.host == r.host));
        if missing.is_some() {
            self.populate(Vec::new(), false);
        }
        self.sync_tray(true);
        if self.toast_key.as_ref() != Some(&status) {
            self.toast_key = Some(status.clone());
            if self.shown {
                self.hide_toast(); // the panel already says it
            } else if status.state != State::Idle || !status.note.is_empty() || self.toast_shown {
                self.show_toast(&status);
            }
        }
        self.sync_spin();
        self.relayout();
    }

    /// Update the list in place. `scanned`: a discovery result.
    pub(super) fn populate(&mut self, found: Vec<Receiver>, scanned: bool) {
        if scanned {
            self.panel.scanning = false;
            self.scanned_at = Some(Instant::now());
            if !found.is_empty() {
                let mut s = self.settings.lock().unwrap();
                s.receivers = found.iter().map(|r| (r.name.clone(), r.host.to_string())).collect();
                s.save();
            }
        }
        // A scan that saw nothing is more likely mDNS flaking than an empty
        // network, so it keeps the rows it had.
        let mut list = if scanned && !found.is_empty() { Vec::new() } else { self.panel.receivers.clone() };
        let active = self.panel.status.receiver.clone();
        for r in found.into_iter().chain(active) {
            match list.iter_mut().find(|x: &&mut Receiver| x.host == r.host) {
                Some(x) => x.name = r.name,
                None => list.push(r),
            }
        }
        list.sort_by_key(|r| r.name.to_lowercase());
        self.panel.receivers = list;
        if self.panel.focus.is_some_and(|f| matches!(f, panel::Part::Row(i) if i >= self.panel.receivers.len())) {
            self.panel.focus = None;
        }
        self.relayout();
    }

    pub(super) fn refresh(&mut self) {
        if self.panel.scanning {
            return;
        }
        self.panel.scanning = true;
        self.redraw();
        let (slot, hwnd) = (self.found_slot.clone(), self.hwnd.0 as isize);
        std::thread::spawn(move || {
            *slot.lock().unwrap() = Some(crate::discover());
            let _ = unsafe { PostMessageW(Some(HWND(hwnd as _)), WM_APP_FOUND, WPARAM(0), LPARAM(0)) };
        });
    }

    pub(super) fn on_found(&mut self) {
        let found = self.found_slot.lock().unwrap().take();
        if let Some(found) = found {
            self.populate(found, true);
        }
    }

    pub(super) fn set_volume(&mut self, volume: u8) {
        if volume != self.panel.volume {
            self.panel.volume = volume;
            self.settings.lock().unwrap().volume = volume;
            // One command per pause in the drag, not per step.
            unsafe { SetTimer(Some(self.hwnd), TIMER_VOLUME, 60, None) };
            self.redraw();
        }
    }

    pub(super) fn send_volume(&mut self) {
        let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_VOLUME) };
        self.settings.lock().unwrap().save();
        self.link_send(Event::Volume(self.panel.volume));
    }

    pub(super) fn link_send(&self, event: Event) {
        if let Some(link) = &self.link {
            link.send(event);
        }
    }

    pub(super) fn update_source(&mut self) {
        let s = self.settings.lock().unwrap();
        let default = || airtight_audio::endpoint::default().map(|e| e.name);
        let name = s.endpoint.clone().or_else(default).unwrap_or_else(|| "No output".into());
        self.panel.source = format!("{name} · {} ms buffer", s.latency_ms);
    }

    pub(super) fn sync_tray(&mut self, added: bool) {
        let s = &self.panel.status;
        let tip = match (&s.receiver, s.state) {
            (Some(r), state) if state != State::Idle => format!("Airtight: {} · {}", state_text(state), r.name),
            _ => "Airtight: Not connected".into(),
        };
        self.tray.update(s.state == State::Playing, &tip, added);
    }

    /// Run the spinner timer only while a spinner shows.
    pub(super) fn sync_spin(&mut self) {
        let spinning = (self.shown && self.panel.working()) || (self.toast_shown && self.toast.mode == toast::Mode::Spin);
        unsafe {
            if spinning {
                SetTimer(Some(self.hwnd), TIMER_SPIN, 16, None);
            } else {
                let _ = KillTimer(Some(self.hwnd), TIMER_SPIN);
            }
        }
    }

    pub(super) fn spin(&mut self) {
        self.panel.angle = (self.panel.angle + 6.0) % 360.0;
        self.toast.angle = self.panel.angle;
        self.redraw();
    }

    fn show_toast(&mut self, status: &Snapshot) {
        let action = self.toast.update(status);
        let linger = match action {
            Action::Hide => return self.hide_toast(),
            Action::Stay => None,
            Action::Linger(ms) => Some(ms),
        };
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_TOAST);
            if let Some(ms) = linger {
                SetTimer(Some(self.hwnd), TIMER_TOAST, ms, None);
            }
        }
        if !self.toast_shown {
            window::place(self.toast_hwnd, toast::WIDTH, toast::HEIGHT, false);
            let _ = unsafe { ShowWindow(self.toast_hwnd, SW_SHOWNOACTIVATE) };
            self.toast_shown = true;
        }
        self.sync_spin();
        self.redraw();
    }

    pub(super) fn hide_toast(&mut self) {
        let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_TOAST) };
        if self.toast_shown {
            let _ = unsafe { ShowWindow(self.toast_hwnd, SW_HIDE) };
            self.toast_shown = false;
            self.sync_spin();
        }
    }

    pub(super) fn theme_changed(&mut self) {
        self.theme = Theme::current();
        self.mica = window::style(self.hwnd, &self.theme, true);
        window::style(self.toast_hwnd, &self.theme, false);
        menu::follow_dark_mode();
        self.sync_tray(true);
        self.redraw();
    }

    pub(super) fn quit(&mut self) {
        self.hide();
        self.hide_toast();
        self.tray.remove();
        // Wait for TEARDOWN, so the receiver takes the next session at once.
        if let Some(link) = self.link.take() {
            link.quit();
        }
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}
