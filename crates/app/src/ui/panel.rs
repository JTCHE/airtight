//! The panel: one row per receiver, the volume, and a footer with the
//! source and the menu. Every part paints itself; the panel is made once
//! and only shown and hidden after that.

use super::gfx::{Font, Gfx, Painter, Rect, speaker, v2};
use super::theme::Theme;
use crate::link::{Receiver, Snapshot, State};

pub const WIDTH: f32 = 320.0;
const ROW: f32 = 44.0;
const ROW_GAP: f32 = 2.0;
const SLIDER_INSET: f32 = 8.0;

/// What the pointer or the keyboard is on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    Row(usize),
    Slider,
    More,
}

pub struct Panel {
    pub receivers: Vec<Receiver>,
    pub status: Snapshot,
    pub volume: u8,
    /// "endpoint · N ms buffer".
    pub source: String,
    pub scanning: bool,
    pub hot: Option<Part>,
    pub pressed: Option<Part>,
    pub focus: Option<Part>,
    /// The focus moved by keyboard: show the ring.
    pub keyboard: bool,
    /// The "Disconnect" hint waits for a fresh hover: right after a click
    /// the cursor is still there, and the hint would hide the progress.
    pub hint: bool,
    /// Spinner angle, degrees.
    pub angle: f32,
}

pub struct Layout {
    pub rows: Vec<Rect>,
    pub empty: Option<Rect>,
    pub note: Option<(Rect, String)>,
    pub label: Rect,
    pub slider: Rect,
    pub value: Rect,
    pub footer: f32,
    pub source: Rect,
    pub more: Rect,
    pub height: f32,
}

pub fn state_text(state: State) -> &'static str {
    match state {
        State::Connecting => "Connecting…",
        State::Syncing => "Syncing clock…",
        State::Playing => "Streaming",
        State::Retrying => "Reconnecting…",
        State::Idle => "Connected",
    }
}

impl Panel {
    /// The receiver this row stands for is the one being kept connected.
    pub fn active(&self, i: usize) -> bool {
        let r = &self.receivers[i];
        self.status.state != State::Idle && self.status.receiver.as_ref().is_some_and(|a| a.host == r.host)
    }

    pub fn working(&self) -> bool {
        (0..self.receivers.len()).any(|i| self.active(i)) && self.status.state != State::Playing
    }

    /// While a receiver is active its row carries the state; the note shows
    /// on its own line only after a give-up, when no row is active.
    fn note(&self) -> Option<String> {
        (self.status.state == State::Idle && !self.status.note.is_empty()).then(|| self.status.note.clone())
    }

    pub fn layout(&self, gfx: &Gfx) -> Layout {
        let mut y = 4.0;
        let rows: Vec<Rect> = (0..self.receivers.len())
            .map(|i| Rect::new(4.0, y + i as f32 * (ROW + ROW_GAP), WIDTH - 8.0, ROW))
            .collect();
        y += rows.len() as f32 * (ROW + ROW_GAP) - if rows.is_empty() { 0.0 } else { ROW_GAP };
        let empty = rows.is_empty().then(|| {
            let r = Rect::new(16.0, y + 10.0, WIDTH - 32.0, gfx.height(&self.empty_text(), Font::Caption, WIDTH - 32.0));
            y += r.h + 20.0;
            r
        });
        let note = self.note().map(|text| {
            let r = Rect::new(16.0, y + 6.0, WIDTH - 32.0, gfx.height(&text, Font::Caption, WIDTH - 32.0));
            y += r.h + 6.0;
            (r, text)
        });
        y += 8.0;
        let label = Rect::new(16.0, y, 44.0, 28.0);
        let slider = Rect::new(label.x + label.w + 8.0, y, WIDTH - 16.0 - 24.0 - 8.0 - (label.x + label.w + 8.0), 28.0);
        let value = Rect::new(WIDTH - 16.0 - 24.0, y, 24.0, 28.0);
        let footer = y + 28.0 + 8.0;
        let more = Rect::new(WIDTH - 8.0 - 28.0, footer + 5.0, 28.0, 28.0);
        let source = Rect::new(16.0, footer + 5.0, more.x - 8.0 - 16.0, 28.0);
        Layout { rows, empty, note, label, slider, value, footer, source, more, height: footer + 1.0 + 38.0 }
    }

    pub fn empty_text(&self) -> String {
        if self.scanning {
            "Searching for AirPlay receivers…".into()
        } else {
            "No AirPlay receivers found. Use More options to search again.".into()
        }
    }

    pub fn hit(&self, layout: &Layout, x: f32, y: f32) -> Option<Part> {
        if let Some(i) = layout.rows.iter().position(|r| r.contains(x, y)) {
            return Some(Part::Row(i));
        }
        if layout.slider.contains(x, y) {
            return Some(Part::Slider);
        }
        layout.more.contains(x, y).then_some(Part::More)
    }

    /// The volume at pointer x on the slider.
    pub fn volume_at(layout: &Layout, x: f32) -> u8 {
        let span = (layout.slider.w - 2.0 * SLIDER_INSET).max(1.0);
        ((x - layout.slider.x - SLIDER_INSET) / span * 100.0).round().clamp(0.0, 100.0) as u8
    }

    pub fn paint(&self, p: &Painter, t: &Theme, l: &Layout, mica: bool) {
        p.clear(if mica { 0 } else { t.bg });
        p.fill(Rect::new(0.0, 0.0, WIDTH, l.footer), 0.0, t.layer);
        p.fill(Rect::new(0.0, l.footer, WIDTH, 1.0), 0.0, t.stroke);
        for (i, r) in l.rows.iter().enumerate() {
            self.paint_row(p, t, i, *r);
        }
        if let Some(r) = l.empty {
            p.text_with(&self.empty_text(), Font::Caption, r, t.muted, true, None);
        }
        if let Some((r, text)) = &l.note {
            p.text_with(text, Font::Caption, *r, t.warn, true, None);
        }
        p.text("Volume", Font::Caption, l.label, t.muted);
        self.paint_slider(p, t, l.slider);
        p.text(&self.volume.to_string(), Font::Caption, l.value, t.text);
        p.text_with(&self.source, Font::Caption, l.source, t.muted, false, Some('·'));
        self.button_back(p, t, Part::More, l.more);
        let c = (l.more.x + 14.0, l.more.y + 14.0);
        for dx in [-5.0, 0.0, 5.0] {
            p.circle(c.0 + dx, c.1, 1.4, t.text);
        }
    }

    fn button_back(&self, p: &Painter, t: &Theme, part: Part, r: Rect) {
        if self.pressed == Some(part) && self.hot == Some(part) {
            p.fill(r, 4.0, t.press);
        } else if self.hot == Some(part) {
            p.fill(r, 4.0, t.hover);
        }
        if self.keyboard && self.focus == Some(part) {
            p.outline(r, 4.0, 2.0, t.focus);
        }
    }

    fn paint_row(&self, p: &Painter, t: &Theme, i: usize, r: Rect) {
        self.button_back(p, t, Part::Row(i), r);
        let (active, state) = (self.active(i), self.status.state);
        let streaming = active && state == State::Playing;
        let (cx, cy) = (r.x + 24.0, r.y + r.h / 2.0);
        p.circle(cx, cy, 14.0, if streaming { t.accent } else { t.disc });
        speaker(p, Rect::new(cx - 7.0, cy - 7.0, 14.0, 14.0), if streaming { t.on_accent } else { t.text });
        if active && !streaming {
            let color = if state == State::Retrying { t.warn } else { t.accent };
            p.arc(cx, cy, 13.0, self.angle, 100.0, 2.0, color);
        }
        let (x, w) = (r.x + 48.0, r.w - 48.0 - 12.0);
        let name = &self.receivers[i].name;
        if !active {
            p.text(name, Font::Body, Rect::new(x, r.y, w, r.h), t.text);
            return;
        }
        p.text(name, Font::Body, Rect::new(x, cy - 18.0, w, 18.0), t.text);
        let (status, color) = if streaming && self.hot == Some(Part::Row(i)) && self.hint {
            ("Disconnect".to_string(), t.text) // what a click does
        } else if state == State::Retrying && !self.status.note.is_empty() {
            (self.status.note.clone(), t.warn)
        } else {
            (state_text(state).to_string(), t.muted)
        };
        p.text(&status, Font::Caption, Rect::new(x, cy + 1.0, w, 16.0), color);
    }

    fn paint_slider(&self, p: &Painter, t: &Theme, r: Rect) {
        let cy = r.y + r.h / 2.0;
        let span = r.w - 2.0 * SLIDER_INSET;
        let x = r.x + SLIDER_INSET + span * f32::from(self.volume) / 100.0;
        p.fill(Rect::new(r.x + SLIDER_INSET, cy - 2.0, span, 4.0), 2.0, t.track);
        p.fill(Rect::new(r.x + SLIDER_INSET, cy - 2.0, x - r.x - SLIDER_INSET, 4.0), 2.0, t.accent);
        if self.keyboard && self.focus == Some(Part::Slider) {
            p.ring(x, cy, 11.0, 2.0, t.focus);
        }
        p.circle(x, cy, 8.0, t.border);
        p.circle(x, cy, 7.0, t.thumb);
        let big = self.hot == Some(Part::Slider) || self.pressed == Some(Part::Slider);
        p.circle(x, cy, if big { 5.0 } else { 4.0 }, t.accent);
    }
}

/// The toast icon: a turning ring, a check, or an exclamation mark.
pub fn toast_icon(p: &Painter, t: &Theme, mode: super::toast::Mode, angle: f32) {
    use super::toast::Mode;
    let (cx, cy) = (30.0, 30.0);
    match mode {
        Mode::Spin => {
            p.ring(cx, cy, 12.0, 2.5, t.disc);
            p.arc(cx, cy, 12.0, angle, 100.0, 2.5, t.accent);
        }
        Mode::Done => {
            p.circle(cx, cy, 16.0, t.accent);
            p.path(&[v2(cx - 6.0, cy + 0.5), v2(cx - 2.0, cy + 4.5), v2(cx + 6.0, cy - 4.5)], 2.4, false, t.on_accent);
        }
        Mode::Warn => {
            p.circle(cx, cy, 16.0, t.warn);
            p.path(&[v2(cx, cy - 7.0), v2(cx, cy + 2.0)], 2.4, false, t.on_accent);
            p.circle(cx, cy + 7.0, 1.4, t.on_accent);
        }
    }
}
