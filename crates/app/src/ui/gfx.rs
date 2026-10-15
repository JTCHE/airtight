//! Direct2D and DirectWrite: the shared factories, the text formats, and
//! the few shapes the panel and the toast draw. Layout is in DIPs.

use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::core::{Result, w};
use windows_numerics::Vector2;

#[derive(Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    pub fn inset(&self, d: f32) -> Rect {
        Rect::new(self.x + d, self.y + d, self.w - 2.0 * d, self.h - 2.0 * d)
    }

    fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F { left: self.x, top: self.y, right: self.x + self.w, bottom: self.y + self.h }
    }
}

/// Text styles: (size, weight).
#[derive(Clone, Copy)]
pub enum Font {
    Body,
    Caption,
    Title,
}

pub struct Gfx {
    pub factory: ID2D1Factory1,
    dwrite: IDWriteFactory,
    formats: [IDWriteTextFormat; 3],
    round: ID2D1StrokeStyle1,
}

fn format(dwrite: &IDWriteFactory, size: f32, weight: DWRITE_FONT_WEIGHT) -> Result<IDWriteTextFormat> {
    unsafe {
        let f = dwrite.CreateTextFormat(w!("Segoe UI Variable Text"), None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!(""))?;
        f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        Ok(f)
    }
}

impl Gfx {
    pub fn new() -> Result<Gfx> {
        unsafe {
            let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let formats = [
                format(&dwrite, 13.0, DWRITE_FONT_WEIGHT_NORMAL)?,
                format(&dwrite, 12.0, DWRITE_FONT_WEIGHT_NORMAL)?,
                format(&dwrite, 13.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?,
            ];
            let props = D2D1_STROKE_STYLE_PROPERTIES1 {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                ..Default::default()
            };
            let round = factory.CreateStrokeStyle(&props, None)?;
            Ok(Gfx { factory, dwrite, formats, round })
        }
    }

    /// A laid-out paragraph: cut with an ellipsis, or wrapped. `keep_after`
    /// keeps the text after that character when cutting, for a middle cut.
    pub fn layout(&self, text: &str, font: Font, width: f32, wrap: bool, keep_after: Option<char>) -> Option<IDWriteTextLayout> {
        let text: Vec<u16> = text.encode_utf16().collect();
        let format = &self.formats[font as usize];
        unsafe {
            let layout = self.dwrite.CreateTextLayout(&text, format, width.max(1.0), 1000.0).ok()?;
            if !wrap {
                layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP).ok()?;
                let sign = self.dwrite.CreateEllipsisTrimmingSign(format).ok()?;
                let trim = DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: keep_after.map_or(0, |c| c as u32),
                    delimiterCount: u32::from(keep_after.is_some()),
                };
                layout.SetTrimming(&trim, &sign).ok()?;
            }
            Some(layout)
        }
    }

    /// The height a wrapped paragraph takes.
    pub fn height(&self, text: &str, font: Font, width: f32) -> f32 {
        let mut metrics = DWRITE_TEXT_METRICS::default();
        let layout = self.layout(text, font, width, true, None);
        layout.map_or(16.0, |l| unsafe { l.GetMetrics(&mut metrics).map_or(16.0, |_| metrics.height) })
    }
}

/// Draws one frame on a device context.
pub struct Painter<'a> {
    pub gfx: &'a Gfx,
    pub dc: &'a ID2D1DeviceContext,
}

fn color(argb: u32) -> D2D1_COLOR_F {
    let ch = |s: u32| ((argb >> s) & 0xff) as f32 / 255.0;
    D2D1_COLOR_F { r: ch(16), g: ch(8), b: ch(0), a: ch(24) }
}

pub fn v2(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

impl Painter<'_> {
    fn brush(&self, argb: u32) -> Option<ID2D1SolidColorBrush> {
        unsafe { self.dc.CreateSolidColorBrush(&color(argb), None).ok() }
    }

    pub fn clear(&self, argb: u32) {
        unsafe { self.dc.Clear(Some(&color(argb))) };
    }

    pub fn fill(&self, r: Rect, radius: f32, argb: u32) {
        let Some(b) = self.brush(argb) else { return };
        let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius };
        unsafe { self.dc.FillRoundedRectangle(&rr, &b) };
    }

    pub fn outline(&self, r: Rect, radius: f32, width: f32, argb: u32) {
        let Some(b) = self.brush(argb) else { return };
        let rr = D2D1_ROUNDED_RECT { rect: r.inset(width / 2.0).d2d(), radiusX: radius, radiusY: radius };
        unsafe { self.dc.DrawRoundedRectangle(&rr, &b, width, None) };
    }

    pub fn circle(&self, cx: f32, cy: f32, r: f32, argb: u32) {
        let Some(b) = self.brush(argb) else { return };
        let e = D2D1_ELLIPSE { point: v2(cx, cy), radiusX: r, radiusY: r };
        unsafe { self.dc.FillEllipse(&e, &b) };
    }

    pub fn ring(&self, cx: f32, cy: f32, r: f32, width: f32, argb: u32) {
        let Some(b) = self.brush(argb) else { return };
        let e = D2D1_ELLIPSE { point: v2(cx, cy), radiusX: r, radiusY: r };
        unsafe { self.dc.DrawEllipse(&e, &b, width, None) };
    }

    /// An open path through `points`, or a filled one.
    pub fn path(&self, points: &[Vector2], width: f32, filled: bool, argb: u32) {
        let (Some(b), Ok(geometry)) = (self.brush(argb), unsafe { self.gfx.factory.CreatePathGeometry() }) else { return };
        unsafe {
            let Ok(sink) = geometry.Open() else { return };
            let begin = if filled { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW };
            sink.BeginFigure(points[0], begin);
            sink.AddLines(&points[1..]);
            sink.EndFigure(if filled { D2D1_FIGURE_END_CLOSED } else { D2D1_FIGURE_END_OPEN });
            let _ = sink.Close();
            if filled {
                self.dc.FillGeometry(&geometry, &b, None);
            } else {
                self.dc.DrawGeometry(&geometry, &b, width, &self.gfx.round);
            }
        }
    }

    /// An arc on the circle (cx, cy, r) from `start` degrees, clockwise,
    /// `sweep` degrees long, with round ends.
    pub fn arc(&self, cx: f32, cy: f32, r: f32, start: f32, sweep: f32, width: f32, argb: u32) {
        let point = |deg: f32| v2(cx + r * deg.to_radians().cos(), cy + r * deg.to_radians().sin());
        let (Some(b), Ok(geometry)) = (self.brush(argb), unsafe { self.gfx.factory.CreatePathGeometry() }) else { return };
        unsafe {
            let Ok(sink) = geometry.Open() else { return };
            sink.BeginFigure(point(start), D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: point(start + sweep),
                size: D2D_SIZE_F { width: r, height: r },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: if sweep > 180.0 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            let _ = sink.Close();
            self.dc.DrawGeometry(&geometry, &b, width, &self.gfx.round);
        }
    }

    pub fn text(&self, text: &str, font: Font, r: Rect, argb: u32) {
        self.text_with(text, font, r, argb, false, None);
    }

    pub fn text_with(&self, text: &str, font: Font, r: Rect, argb: u32, wrap: bool, keep_after: Option<char>) {
        let (Some(b), Some(layout)) = (self.brush(argb), self.gfx.layout(text, font, r.w, wrap, keep_after)) else { return };
        unsafe {
            let _ = layout.SetMaxHeight(r.h);
            self.dc.DrawTextLayout(v2(r.x, r.y), &layout, &b, D2D1_DRAW_TEXT_OPTIONS_CLIP);
        }
    }
}

/// A speaker cone with one wave, centred in `r`.
pub fn speaker(p: &Painter, r: Rect, argb: u32) {
    let s = r.w.min(r.h);
    let (x0, y0) = (r.x + (r.w - s) / 2.0, r.y + (r.h - s) / 2.0);
    let at = |x: f32, y: f32| v2(x0 + s * x, y0 + s * y);
    let cone = [at(0.18, 0.40), at(0.34, 0.40), at(0.52, 0.24), at(0.52, 0.76), at(0.34, 0.60), at(0.18, 0.60)];
    p.path(&cone, 0.0, true, argb);
    p.arc(x0 + s * 0.60, y0 + s * 0.50, s * 0.20, -55.0, 110.0, s * 0.08, argb);
}
