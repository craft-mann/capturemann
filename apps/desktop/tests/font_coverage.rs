//! i18n spike S1: the bundled UI fonts must be able to draw every Vietnamese letter, so a
//! Vietnamese label never mixes Geist with system-fallback glyphs inside one word.
//!
//! Headless: reads the font files and parses the `cmap` table with a minimal reader (formats 4
//! and 12), no window, no GPU. The fonts listed here are the same files `ui/paint.rs` embeds.

use std::collections::BTreeSet;

const FONT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts");

/// Every font file registered in `ui/paint.rs` (`FONTS`).
const BUNDLED: [&str; 6] = [
    "Geist-Regular.ttf",
    "Geist-Medium.ttf",
    "Geist-SemiBold.ttf",
    "Geist-Bold.ttf",
    "GeistMono-Regular.ttf",
    "GeistMono-Medium.ttf",
];

fn u16_at(d: &[u8], o: usize) -> usize {
    u16::from_be_bytes([d[o], d[o + 1]]) as usize
}

fn u32_at(d: &[u8], o: usize) -> usize {
    u32::from_be_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as usize
}

/// The set of code points a TrueType font maps (Unicode cmap, format 4 and 12).
fn cmap(d: &[u8]) -> BTreeSet<u32> {
    let tables = u16_at(d, 4);
    let cmap_off = (0..tables)
        .map(|i| 12 + i * 16)
        .find(|&r| &d[r..r + 4] == b"cmap")
        .map(|r| u32_at(d, r + 8))
        .expect("cmap table");
    let n = u16_at(d, cmap_off + 2);
    let mut out = BTreeSet::new();
    for i in 0..n {
        let rec = cmap_off + 4 + i * 8;
        let (platform, encoding) = (u16_at(d, rec), u16_at(d, rec + 2));
        // Unicode (0) or Windows Unicode BMP (3,1) / full repertoire (3,10).
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
                            out.insert(c as u32);
                        }
                    }
                }
            }
            12 => {
                for g in 0..u32_at(d, t + 12) {
                    let o = t + 16 + g * 12;
                    let (start, end) = (u32_at(d, o), u32_at(d, o + 4));
                    out.extend(start as u32..=end as u32);
                }
            }
            _ => {}
        }
    }
    out
}

/// The 134 precomposed code points a Vietnamese UI can need: the single-mark letters of the
/// alphabet (including Đ, Ă, Ơ, Ư, as Latin-1 and Latin Extended-A/B) and the stacked-tone
/// forms U+1EA0..U+1EF9.
fn vietnamese() -> Vec<u32> {
    let base = "ÀÁÂÃÈÉÊÌÍÒÓÔÕÙÚÝĂĐĨŨƠƯàáâãèéêìíòóôõùúýăđĩũơư";
    let mut v: Vec<u32> = base.chars().map(|c| c as u32).collect();
    v.extend(0x1EA0..=0x1EF9);
    v
}

#[test]
fn vietnamese_has_the_expected_size() {
    assert_eq!(vietnamese().len(), 134);
}

#[test]
fn bundled_fonts_draw_every_vietnamese_letter() {
    let wanted = vietnamese();
    let mut report = Vec::new();
    for name in BUNDLED {
        let data = std::fs::read(format!("{FONT_DIR}/{name}")).expect(name);
        let have = cmap(&data);
        assert!(have.contains(&('A' as u32)), "{name}: cmap parse sanity");
        let missing: String = wanted
            .iter()
            .filter(|c| !have.contains(c))
            .filter_map(|&c| char::from_u32(c))
            .collect();
        if !missing.is_empty() {
            report.push(format!(
                "{name} lacks {}: {missing}",
                missing.chars().count()
            ));
        }
    }
    assert!(
        report.is_empty(),
        "missing Vietnamese glyphs:\n{}",
        report.join("\n")
    );
}

#[test]
fn every_embedded_font_is_listed_here() {
    let src = include_str!("../src/ui/paint.rs");
    for name in BUNDLED {
        assert!(src.contains(name), "{name} is not embedded by ui/paint.rs");
    }
    let embedded = src.matches("include_bytes!(\"../../assets/fonts/").count();
    assert_eq!(
        embedded,
        BUNDLED.len(),
        "paint.rs embeds a font this test does not check"
    );
}
