//! The Airtight glyph: three rising bars. The tray draws it at run time;
//! build.rs includes this file to draw the exe icon.

/// x, y, width, height and corner radius, in pixels.
pub type Shape = (f32, f32, f32, f32, f32);

/// Coverage of a rounded rectangle at one pixel, 4 x 4 samples.
pub fn coverage(px: f32, py: f32, (x, y, w, h, r): Shape) -> f32 {
    let mut hit = 0;
    for sy in 0..4 {
        for sx in 0..4 {
            let (qx, qy) = (px + (sx as f32 + 0.5) / 4.0, py + (sy as f32 + 0.5) / 4.0);
            let dx = (x + r - qx).max(qx - (x + w - r)).max(0.0);
            let dy = (y + r - qy).max(qy - (y + h - r)).max(0.0);
            let inside = qx >= x && qx <= x + w && qy >= y && qy <= y + h;
            if inside && dx * dx + dy * dy <= r * r {
                hit += 1;
            }
        }
    }
    hit as f32 / 16.0
}

/// The three bars in a square `size` pixels a side.
pub fn bars(size: f32) -> [Shape; 3] {
    let u = size / 16.0;
    let bar = |i: f32, h: f32| (u * (3.0 + i * 4.0), size - u * (1.5 + h), u * 2.2, u * h, u);
    [bar(0.0, 5.0), bar(1.0, 9.0), bar(2.0, 13.0)]
}
