//! Draws the exe icon from the tray glyph and links it as a .res file, so
//! Explorer and the Start menu show it. No resource compiler needed.

#[path = "src/ui/glyph.rs"]
mod glyph;

use std::path::PathBuf;

const SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];
/// The light-theme accent, readable on light and dark Start menus.
const TILE: [u8; 3] = [0xb8, 0x5f, 0x00];
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;

/// White bars on a rounded accent square, as straight BGRA, top-down.
fn tile(size: u32) -> Vec<u8> {
    let s = size as f32;
    let square = (s * 0.03, s * 0.03, s * 0.94, s * 0.94, s * 0.2);
    let inset = s * 0.18;
    let bars = glyph::bars(s - 2.0 * inset).map(|(x, y, w, h, r)| (x + inset, y + inset, w, h, r));
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f32, y as f32);
            let back = glyph::coverage(px, py, square);
            let bar = bars.iter().map(|&b| glyph::coverage(px, py, b)).sum::<f32>().min(1.0) * back;
            let mix = |c: u8| (f32::from(c) * (1.0 - bar) + 255.0 * bar).round() as u8;
            out.extend_from_slice(&[mix(TILE[0]), mix(TILE[1]), mix(TILE[2]), (back * 255.0).round() as u8]);
        }
    }
    out
}

/// One icon image: a BITMAPINFOHEADER, the rows bottom-up, an empty AND mask.
fn image(size: u32) -> Vec<u8> {
    let mask_row = size.div_ceil(32) * 4;
    let mut out = Vec::new();
    for v in [40, size, size * 2] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    let pixels = tile(size);
    for row in pixels.chunks(size as usize * 4).rev() {
        out.extend_from_slice(row);
    }
    out.resize(out.len() + (mask_row * size) as usize, 0);
    out
}

/// One entry of a .res file: a 32-byte header with ordinal type and name.
fn entry(out: &mut Vec<u8>, kind: u16, id: u16, data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    for v in [0xffff, kind, 0xffff, id] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&[0; 4]); // data version
    out.extend_from_slice(&0x1010u16.to_le_bytes()); // moveable, discardable
    out.extend_from_slice(&0x0409u16.to_le_bytes());
    out.extend_from_slice(&[0; 8]); // version, characteristics
    out.extend_from_slice(data);
    out.resize(out.len().next_multiple_of(4), 0);
}

fn res() -> Vec<u8> {
    let mut out = Vec::new();
    entry(&mut out, 0, 0, &[]); // the empty entry that opens every .res file
    let mut group = [0u16, 1, SIZES.len() as u16].map(u16::to_le_bytes).concat();
    for (i, &size) in SIZES.iter().enumerate() {
        let data = image(size);
        let id = i as u16 + 1;
        entry(&mut out, RT_ICON, id, &data);
        let side = if size >= 256 { 0 } else { size as u8 };
        group.extend_from_slice(&[side, side, 0, 0]);
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(&32u16.to_le_bytes());
        group.extend_from_slice(&(data.len() as u32).to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
    }
    entry(&mut out, RT_GROUP_ICON, 1, &group);
    out
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/ui/glyph.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let path = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("airtight.res");
    std::fs::write(&path, res()).unwrap();
    println!("cargo:rustc-link-arg-bins={}", path.display());
}
