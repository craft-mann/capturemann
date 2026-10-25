//! Text width without a window: the advance widths of the bundled Geist fonts, read straight from
//! the TrueType tables (`head`, `hhea`, `hmtx`, `cmap` formats 4 and 12), the same minimal reader
//! `tests/font_coverage.rs` uses for glyph coverage. No shaping: kerning and ligatures are ignored
//! (they only make text a little narrower), so a width here is an upper bound within a few percent.
//!
//! Used by `layout_width_tests` to check that a label fits its fixed-width control in English, in
//! Vietnamese and in the pseudo-locale.

use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Face {
    Regular,
    Medium,
    SemiBold,
    Bold,
    MonoRegular,
    MonoMedium,
}

impl Face {
    /// The font file GPUI picks for a family and a weight (mono has two weights; heavier ones
    /// use Medium).
    pub(crate) fn of(mono: bool, weight: u16) -> Face {
        match (mono, weight) {
            (true, w) if w >= 500 => Face::MonoMedium,
            (true, _) => Face::MonoRegular,
            (false, w) if w >= 700 => Face::Bold,
            (false, w) if w >= 600 => Face::SemiBold,
            (false, w) if w >= 500 => Face::Medium,
            (false, _) => Face::Regular,
        }
    }
}

struct Font {
    units_per_em: f32,
    advances: Vec<u16>,
    glyph_of: HashMap<u32, u16>,
}

fn u16_at(d: &[u8], o: usize) -> usize {
    u16::from_be_bytes([d[o], d[o + 1]]) as usize
}

fn u32_at(d: &[u8], o: usize) -> usize {
    u32::from_be_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as usize
}

fn table(d: &[u8], tag: &[u8; 4]) -> usize {
    (0..u16_at(d, 4))
        .map(|i| 12 + i * 16)
        .find(|&r| &d[r..r + 4] == tag)
        .map(|r| u32_at(d, r + 8))
        .unwrap_or_else(|| panic!("font table {}", String::from_utf8_lossy(tag)))
}

/// Code point to glyph id from the Unicode `cmap` subtables (format 4 and 12).
fn cmap(d: &[u8]) -> HashMap<u32, u16> {
    let cmap_off = table(d, b"cmap");
    let mut out = HashMap::new();
    for i in 0..u16_at(d, cmap_off + 2) {
        let rec = cmap_off + 4 + i * 8;
        let (platform, encoding) = (u16_at(d, rec), u16_at(d, rec + 2));
        if !(platform == 0 || (platform == 3 && (encoding == 1 || encoding == 10))) {
            continue;
        }
        let t = cmap_off + u32_at(d, rec + 4);
        match u16_at(d, t) {
            4 => {
                let segs = u16_at(d, t + 6) / 2;
                let (ends, starts) = (t + 14, t + 16 + segs * 2);
                let (deltas, ranges) = (starts + segs * 2, starts + segs * 4);
                for s in 0..segs {
                    let (end, start) = (u16_at(d, ends + s * 2), u16_at(d, starts + s * 2));
                    let (delta, range) = (u16_at(d, deltas + s * 2), u16_at(d, ranges + s * 2));
                    for c in start..=end {
                        if c == 0xFFFF {
                            continue;
                        }
                        let gid = if range == 0 {
                            (c + delta) & 0xFFFF
                        } else {
                            let p = ranges + s * 2 + range + (c - start) * 2;
                            match u16_at(d, p) {
                                0 => 0,
                                g => (g + delta) & 0xFFFF,
                            }
                        };
                        if gid != 0 {
                            out.insert(c as u32, gid as u16);
                        }
                    }
                }
            }
            12 => {
                for g in 0..u32_at(d, t + 12) {
                    let o = t + 16 + g * 12;
                    let (start, end, first) = (u32_at(d, o), u32_at(d, o + 4), u32_at(d, o + 8));
                    for c in start..=end {
                        out.insert(c as u32, (first + c - start) as u16);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn parse(d: &[u8]) -> Font {
    let units_per_em = u16_at(d, table(d, b"head") + 18) as f32;
    let hhea = table(d, b"hhea");
    let n = u16_at(d, hhea + 34);
    let hmtx = table(d, b"hmtx");
    let advances = (0..n).map(|g| u16_at(d, hmtx + g * 4) as u16).collect();
    Font {
        units_per_em,
        advances,
        glyph_of: cmap(d),
    }
}

fn font(face: Face) -> &'static Font {
    static FONTS: OnceLock<Vec<Font>> = OnceLock::new();
    let all = FONTS.get_or_init(|| {
        [
            &include_bytes!("../assets/fonts/Geist-Regular.ttf")[..],
            &include_bytes!("../assets/fonts/Geist-Medium.ttf")[..],
            &include_bytes!("../assets/fonts/Geist-SemiBold.ttf")[..],
            &include_bytes!("../assets/fonts/Geist-Bold.ttf")[..],
            &include_bytes!("../assets/fonts/GeistMono-Regular.ttf")[..],
            &include_bytes!("../assets/fonts/GeistMono-Medium.ttf")[..],
        ]
        .iter()
        .map(|d| parse(d))
        .collect()
    });
    &all[match face {
        Face::Regular => 0,
        Face::Medium => 1,
        Face::SemiBold => 2,
        Face::Bold => 3,
        Face::MonoRegular => 4,
        Face::MonoMedium => 5,
    }]
}

/// The advance width of `text` in logical px at `size` px (no kerning, no line breaks).
pub(crate) fn width(text: &str, face: Face, size: f32) -> f32 {
    let f = font(face);
    let units: f32 = text
        .chars()
        .map(|c| {
            let gid = f.glyph_of.get(&(c as u32)).copied().unwrap_or(0) as usize;
            f.advances[gid.min(f.advances.len() - 1)] as f32
        })
        .sum();
    units * size / f.units_per_em
}

/// How many lines `text` needs in a box `avail` px wide when it wraps at spaces, or `None` when a
/// single word is wider than the box (it would overflow whatever the line count).
pub(crate) fn lines(text: &str, face: Face, size: f32, avail: f32) -> Option<usize> {
    let space = width(" ", face, size);
    let mut used = 0.0f32;
    let mut n = 1;
    for word in text.split_whitespace() {
        let w = width(word, face, size);
        if w > avail + 0.01 {
            return None;
        }
        if used == 0.0 {
            used = w;
        } else if used + space + w <= avail + 0.01 {
            used += space + w;
        } else {
            n += 1;
            used = w;
        }
    }
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_follow_the_font() {
        // Geist's digits and mono letters have fixed advances; a space is narrower than an `m`.
        let (a, b) = (
            width("0", Face::Regular, 100.),
            width("1", Face::Regular, 100.),
        );
        assert!(a > 40. && a < 80., "{a}");
        assert!(b > 0.);
        assert!(width("mm", Face::Regular, 14.) > width("m", Face::Regular, 14.));
        let m = width("i", Face::MonoRegular, 12.);
        assert!(
            (m - width("W", Face::MonoRegular, 12.)).abs() < 0.01,
            "mono is fixed pitch"
        );
        // Heavier is wider, bigger is wider, longer is wider.
        assert!(width("Stop", Face::Bold, 14.) > width("Stop", Face::Regular, 14.));
        assert!(width("Stop", Face::Regular, 15.) > width("Stop", Face::Regular, 14.));
        // Vietnamese letters have real glyphs (no .notdef fallback).
        assert!(width("ệ", Face::Regular, 14.) > 5.);
        assert_eq!(width("", Face::Regular, 14.), 0.);
    }

    #[test]
    fn wrapping_counts_lines() {
        let one = width("alpha beta", Face::Regular, 14.);
        assert_eq!(lines("alpha beta", Face::Regular, 14., one + 1.), Some(1));
        assert_eq!(lines("alpha beta", Face::Regular, 14., one - 5.), Some(2));
        assert_eq!(lines("alpha beta", Face::Regular, 14., 5.), None);
    }
}
