//! A small heads-up in the screen corner while a connection comes up. It
//! says Airtight heard the audio and is connecting, then goes once the
//! stream plays. It never takes focus; a click opens the panel.

use super::gfx::{Font, Painter, Rect};
use super::panel::{state_text, toast_icon};
use super::theme::Theme;
use crate::link::{Snapshot, State};

pub const WIDTH: f32 = 300.0;
pub const HEIGHT: f32 = 60.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Spin,
    Done,
    Warn,
}

pub struct Toast {
    pub mode: Mode,
    pub title: String,
    pub detail: String,
    pub angle: f32,
}

/// What a status change does to the toast.
pub enum Action {
    /// Show it and keep it up.
    Stay,
    /// Show it, then hide it after this many milliseconds.
    Linger(u32),
    Hide,
}

impl Toast {
    pub fn new() -> Toast {
        Toast { mode: Mode::Spin, title: String::new(), detail: String::new(), angle: 0.0 }
    }

    pub fn update(&mut self, status: &Snapshot) -> Action {
        self.title = status.receiver.as_ref().map_or("AirPlay".into(), |r| r.name.clone());
        let (mode, detail, action) = match status.state {
            State::Connecting | State::Syncing => (Mode::Spin, state_text(status.state).to_string(), Action::Stay),
            State::Playing => (Mode::Done, "Connected".to_string(), Action::Linger(1600)),
            State::Retrying if !status.note.is_empty() => (Mode::Spin, status.note.clone(), Action::Stay),
            State::Retrying => (Mode::Spin, state_text(status.state).to_string(), Action::Stay),
            State::Idle if !status.note.is_empty() => (Mode::Warn, status.note.clone(), Action::Linger(6000)),
            State::Idle => return Action::Hide,
        };
        (self.mode, self.detail) = (mode, detail);
        action
    }

    pub fn paint(&self, p: &Painter, t: &Theme) {
        p.clear(t.bg);
        toast_icon(p, t, self.mode, self.angle);
        let w = WIDTH - 60.0 - 16.0;
        p.text(&self.title, Font::Title, Rect::new(60.0, 12.0, w, 18.0), t.text);
        p.text(&self.detail, Font::Caption, Rect::new(60.0, 31.0, w, 16.0), t.muted);
    }
}
