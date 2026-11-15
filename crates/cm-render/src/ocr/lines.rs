//! Tidying the lines a recognizer returns before Blueprint (ANN-11) and Retype (ANN-9) use them
//! as boxes: split a line at a wide horizontal gap, grow a tight box over the anti-aliased edge
//! of its glyphs, and drop lines that are not text (an icon read as "O"). Everything here is
//! measured from the pixels (no phrases, sizes or per-application rules).

use cm_core::Rc;
use image::RgbaImage;

use super::{recognize_region, PixelRegion, TextLine, TextRecognizer};

/// A column or row holds structure when its channels vary by at least this many levels.
const INK_RANGE: u8 = 32;
/// A gap of empty columns at least this many times the ink height splits a line. A word space is
/// about a third of the ink height; the gap between two buttons or table columns is more than one.
const GAP_FACTOR: f64 = 1.0;
/// Never split at a gap narrower than this (px).
const GAP_MIN_PX: u32 = 8;
/// The most a tight box grows on a side to take the glyph edge (px).
const GROW_MAX_PX: u32 = 2;
/// Margin kept around the ink after fitting (px).
const FIT_PAD_PX: u32 = 1;
/// A line of at most this many letters and digits is checked for being an icon.
const ICON_MAX_CHARS: usize = 2;
/// Share of a box's pixels differing from its surroundings above which a short line is a filled
/// shape, not text.
const ICON_INK_FRACTION: f64 = 0.38;

fn px(img: &RgbaImage, x: u32, y: u32) -> [u8; 3] {
    let p = img.get_pixel(x, y).0;
    [p[0], p[1], p[2]]
}

fn range3(lo: &mut [u8; 3], hi: &mut [u8; 3], p: [u8; 3]) {
    for i in 0..3 {
        lo[i] = lo[i].min(p[i]);
        hi[i] = hi[i].max(p[i]);
    }
}

fn spread(lo: [u8; 3], hi: [u8; 3]) -> u8 {
    (0..3).map(|i| hi[i] - lo[i]).max().unwrap_or(0)
}

/// Whether column `x` has structure between rows `y0..y1`.
fn column_has_ink(img: &RgbaImage, x: u32, y0: u32, y1: u32) -> bool {
    let (mut lo, mut hi) = ([255; 3], [0; 3]);
    for y in y0..y1 {
        range3(&mut lo, &mut hi, px(img, x, y));
    }
    spread(lo, hi) >= INK_RANGE
}

fn row_has_ink(img: &RgbaImage, y: u32, x0: u32, x1: u32) -> bool {
    let (mut lo, mut hi) = ([255; 3], [0; 3]);
    for x in x0..x1 {
        range3(&mut lo, &mut hi, px(img, x, y));
    }
    spread(lo, hi) >= INK_RANGE
}

/// The box of `rect` in whole pixels `(x0, y0, x1, y1)`, clipped to the image.
fn whole(rect: Rc, img: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let r = rect.normalized();
    let x0 = r.x.floor().max(0.0);
    let y0 = r.y.floor().max(0.0);
    let x1 = (r.x + r.w).ceil().min(f64::from(img.width()));
    let y1 = (r.y + r.h).ceil().min(f64::from(img.height()));
    (x1 > x0 && y1 > y0).then_some((x0 as u32, y0 as u32, x1 as u32, y1 as u32))
}

/// Runs of columns with ink in `x0..x1` as `[start, end)` pairs.
fn ink_runs(img: &RgbaImage, x0: u32, x1: u32, y0: u32, y1: u32) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for x in x0..x1 {
        if column_has_ink(img, x, y0, y1) {
            match runs.last_mut() {
                Some(last) if last.1 == x => last.1 = x + 1,
                _ => runs.push((x, x + 1)),
            }
        }
    }
    runs
}

/// Rows (inside `y0..y1`) with ink over the columns `x0..x1`, as the first and last row.
fn ink_rows(img: &RgbaImage, x0: u32, x1: u32, y0: u32, y1: u32) -> Option<(u32, u32)> {
    let mut found: Option<(u32, u32)> = None;
    for y in y0..y1 {
        if row_has_ink(img, y, x0, x1) {
            found = Some(found.map_or((y, y), |(a, _)| (a, y)));
        }
    }
    found
}

/// The pieces `(x0, x1)` a line splits into at wide gaps between ink columns (one piece when it
/// does not split). `ink_h` is the height of the ink, the unit of the gap width.
fn split_columns(img: &RgbaImage, b: (u32, u32, u32, u32)) -> Vec<(u32, u32)> {
    let (x0, y0, x1, y1) = b;
    let runs = ink_runs(img, x0, x1, y0, y1);
    if runs.len() < 2 {
        return vec![(x0, x1)];
    }
    let all = (runs[0].0, runs[runs.len() - 1].1);
    let Some((ra, rb)) = ink_rows(img, all.0, all.1, y0, y1) else {
        return vec![(x0, x1)];
    };
    let ink_h = f64::from(rb - ra + 1);
    let min_gap = (ink_h * GAP_FACTOR).ceil().max(f64::from(GAP_MIN_PX)) as u32;
    let mut pieces = vec![runs[0]];
    for r in &runs[1..] {
        let last = pieces.last_mut().unwrap_or_else(|| unreachable!());
        if r.0 - last.1 >= min_gap {
            pieces.push(*r);
        } else {
            last.1 = r.1;
        }
    }
    pieces
}

/// Splits `line` at wide horizontal gaps. Each piece is read again from its own crop, so its text
/// and confidence are its own. `None` when the line does not split (or the pieces cannot be read:
/// then the recognizer's own line stays).
fn split_line(
    recognizer: &dyn TextRecognizer,
    img: &RgbaImage,
    line: &TextLine,
) -> Option<Vec<TextLine>> {
    let b = whole(line.rect, img)?;
    let pieces = split_columns(img, b);
    if pieces.len() < 2 {
        return None;
    }
    let mut out = Vec::new();
    for (px0, px1) in pieces {
        let (px0, px1) = (px0.saturating_sub(1).max(b.0), (px1 + 1).min(b.2));
        let (ry0, ry1) = ink_rows(img, px0, px1, b.1, b.3).unwrap_or((b.1, b.3 - 1));
        let (ry0, ry1) = (ry0.saturating_sub(2).max(b.1), (ry1 + 3).min(b.3));
        let region = PixelRegion {
            x: px0,
            y: ry0,
            w: px1 - px0,
            h: ry1 - ry0,
        };
        let found = recognize_region(recognizer, img, region).ok()?;
        let text: Vec<&str> = found.iter().map(|l| l.text.trim()).collect();
        let text = text.join(" ");
        if text.trim().is_empty() {
            // An icon or a control edge in the gap: not text, left out.
            continue;
        }
        let conf = found.iter().map(|l| l.confidence).fold(1.0_f32, f32::min);
        let words = found.iter().flat_map(|l| l.words.clone()).collect();
        out.push(TextLine {
            text,
            rect: region.rc(),
            confidence: conf,
            words,
        });
    }
    (!out.is_empty()).then_some(out)
}

/// Fits `rect` to the ink inside it: empty margins (a recognizer box wider than its word, as a
/// box that takes in the button around a label) are cut, and a side the ink touches grows by up
/// to [`GROW_MAX_PX`] while the pixel row or column just outside it still has glyph structure
/// (a tight box leaves the anti-aliased edge of the glyphs). Every side then gets [`FIT_PAD_PX`]
/// of margin. A box with no ink in it is returned as it was.
fn fit_to_ink(img: &RgbaImage, rect: Rc) -> Rc {
    let Some((ox0, oy0, ox1, oy1)) = whole(rect, img) else {
        return rect;
    };
    let runs = ink_runs(img, ox0, ox1, oy0, oy1);
    let (Some(first), Some(last)) = (runs.first(), runs.last()) else {
        return rect;
    };
    let (mut x0, mut x1) = (first.0, last.1);
    let Some((ry0, ry1)) = ink_rows(img, x0, x1, oy0, oy1) else {
        return rect;
    };
    let (mut y0, mut y1) = (ry0, ry1 + 1);
    for _ in 0..GROW_MAX_PX {
        if y0 == oy0 && y0 > 0 && oy0 - y0 < GROW_MAX_PX && row_has_ink(img, y0 - 1, x0, x1) {
            y0 -= 1;
        }
        if y1 == oy1 && y1 < img.height() && row_has_ink(img, y1, x0, x1) {
            y1 += 1;
        }
        if x0 == ox0 && x0 > 0 && column_has_ink(img, x0 - 1, y0, y1) {
            x0 -= 1;
        }
        if x1 == ox1 && x1 < img.width() && column_has_ink(img, x1, y0, y1) {
            x1 += 1;
        }
    }
    let (x0, y0) = (x0.saturating_sub(FIT_PAD_PX), y0.saturating_sub(FIT_PAD_PX));
    let (x1, y1) = (
        (x1 + FIT_PAD_PX).min(img.width()),
        (y1 + FIT_PAD_PX).min(img.height()),
    );
    Rc::new(
        f64::from(x0),
        f64::from(y0),
        f64::from(x1 - x0),
        f64::from(y1 - y0),
    )
}

/// The share of the box's pixels that differ from the median colour of the pixels just outside
/// it by at least [`INK_RANGE`] levels.
fn ink_fraction(img: &RgbaImage, rect: Rc) -> Option<f64> {
    let (x0, y0, x1, y1) = whole(rect, img)?;
    let mut ring: [Vec<u8>; 3] = Default::default();
    let mut take = |x: u32, y: u32| {
        let p = px(img, x, y);
        for i in 0..3 {
            ring[i].push(p[i]);
        }
    };
    let (rx0, ry0) = (x0.saturating_sub(2), y0.saturating_sub(2));
    let (rx1, ry1) = ((x1 + 2).min(img.width()), (y1 + 2).min(img.height()));
    for y in ry0..ry1 {
        for x in rx0..rx1 {
            if x < x0 || x >= x1 || y < y0 || y >= y1 {
                take(x, y);
            }
        }
    }
    if ring[0].is_empty() {
        return None;
    }
    let med: Vec<u8> = ring
        .iter_mut()
        .map(|c| {
            c.sort_unstable();
            c[c.len() / 2]
        })
        .collect();
    let mut n = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = px(img, x, y);
            if (0..3).any(|i| p[i].abs_diff(med[i]) >= INK_RANGE) {
                n += 1;
            }
        }
    }
    Some(f64::from(n) / f64::from((x1 - x0) * (y1 - y0)))
}

/// Whether the line looks like text rather than a flat shape the recognizer read as a letter
/// (a round icon as "O", a badge as "0"): a very short line whose box is mostly filled. Longer
/// lines are always kept (text over a photo is busy but real).
pub fn looks_like_text(img: &RgbaImage, line: &TextLine) -> bool {
    let Some(frac) = ink_fraction(img, line.rect) else {
        return true;
    };
    let chars = line.text.chars().filter(|c| c.is_alphanumeric()).count();
    !(chars <= ICON_MAX_CHARS && frac >= ICON_INK_FRACTION)
}

/// Tidies `lines` of `img`: splits lines at wide gaps, drops lines that are not text and grows
/// tight boxes over their glyph edges. The order of the lines is kept.
pub fn refine_lines(
    recognizer: &dyn TextRecognizer,
    img: &RgbaImage,
    lines: Vec<TextLine>,
) -> Vec<TextLine> {
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        let parts = split_line(recognizer, img, &line).unwrap_or_else(|| vec![line]);
        for mut p in parts {
            if !looks_like_text(img, &p) {
                continue;
            }
            p.rect = fit_to_ink(img, p.rect);
            out.push(p);
        }
    }
    out
}

/// Share of a line box's height added above it by [`pad_line_boxes`].
const PAD_TOP: f64 = 0.20;
/// Share of a line box's height added below it.
const PAD_BOTTOM: f64 = 0.15;
/// Horizontal padding: this share of the height, but at least [`PAD_X_MIN_PX`].
const PAD_X_FRAC: f64 = 0.04;
const PAD_X_MIN_PX: f64 = 2.0;
/// Two padded boxes of neighbouring lines may overlap by this many px at most.
const PAD_OVERLAP_PX: f64 = 1.0;

/// Pads the boxes of recognized text lines to the whole text line (ANN-11, ANN-9). Some
/// recognizers (Windows) report only the height of the letters' bodies (17 to 19 px for 28 px
/// text), so a bar or an erase over the box leaves the tops of tall letters and the tails of
/// g, y, p visible. Each box grows by 20 % of its height on top, 15 % at the bottom and by 2 px
/// or 4 % of the height (the larger) on each side, clamped to the image; where another box
/// sits above or below it (overlapping horizontally) the growth in the gap is shrunk so the two
/// overlap by at most 1 px. Whole pixels in, whole pixels out. This is the single place the
/// padding happens: the boxes it returns are what `suggest_blueprint`, the editor, Bulk export
/// and agents see, and what a `blueprint` op stores.
pub fn pad_line_boxes(boxes: &[Rc], image_w: u32, image_h: u32) -> Vec<Rc> {
    let n = boxes.len();
    let mut top: Vec<f64> = boxes.iter().map(|b| b.h * PAD_TOP).collect();
    let mut bottom: Vec<f64> = boxes.iter().map(|b| b.h * PAD_BOTTOM).collect();
    // Growth cut by a neighbour is rounded down so whole pixels keep the 1 px overlap limit.
    let mut cut_top = vec![false; n];
    let mut cut_bottom = vec![false; n];
    for i in 0..n {
        for j in 0..n {
            let (a, b) = (boxes[i], boxes[j]);
            // `a` above `b`: the vertical gap is what is left between them.
            let h_overlap = a.x < b.x + b.w && b.x < a.x + a.w;
            if i == j || !h_overlap || b.y + b.h <= a.y || b.y < a.y {
                continue;
            }
            let gap = b.y - (a.y + a.h);
            let want = bottom[i] + top[j];
            let allowed = (gap + PAD_OVERLAP_PX).max(0.0);
            if want > allowed {
                let f = allowed / want;
                bottom[i] *= f;
                top[j] *= f;
                cut_bottom[i] = true;
                cut_top[j] = true;
            }
        }
    }
    let (iw, ih) = (f64::from(image_w), f64::from(image_h));
    boxes
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let px = (b.h * PAD_X_FRAC).max(PAD_X_MIN_PX);
            let x0 = (b.x - px).floor().max(0.0);
            let x1 = (b.x + b.w + px).ceil().min(iw);
            let round = |v: f64, cut: bool| if cut { v.floor() } else { v.ceil() };
            let y0 = (b.y - round(top[i], cut_top[i])).max(0.0);
            let y1 = (b.y + b.h + round(bottom[i], cut_bottom[i])).min(ih);
            Rc::new(x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0))
        })
        .collect()
}
