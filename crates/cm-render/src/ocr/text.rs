//! Copy text from a screenshot (ANN-5): turns recognized [`TextLine`]s into plain text in
//! reading order, and recognizes just a region of an image.
//!
//! Both parts are general: nothing here knows a language, a phrase or a particular screenshot.
//! Reading order comes from the boxes only.

use super::{TextLine, TextRecognizer, TextWord};
use cm_core::{CmError, Rc};
use image::RgbaImage;

/// Margin, in pixels, added around a region before recognition so edge glyphs are not cut.
pub const REGION_MARGIN: u32 = 8;

/// How [`layout_text`] arranges lines.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutOptions {
    /// A blank line goes between two lines whose vertical gap is larger than this multiple of
    /// the median line height.
    pub paragraph_gap: f64,
    /// Keep columns apart (read one column top to bottom, then the next, with a blank line
    /// between). When false everything is one column in row order.
    pub columns: bool,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            paragraph_gap: 0.9,
            columns: true,
        }
    }
}

/// Plain text for `lines`: reading order from the boxes, a blank line at large vertical gaps
/// (relative to the median line height), columns that do not overlap in x kept apart, trailing
/// spaces trimmed. Hyphenated line ends are left as they are. Empty input gives an empty
/// string; there is no trailing newline.
pub fn layout_text(lines: &[TextLine], options: &LayoutOptions) -> String {
    let items: Vec<&TextLine> = lines
        .iter()
        .filter(|l| !l.text.trim().is_empty() && l.rect.is_finite())
        .collect();
    if items.is_empty() {
        return String::new();
    }
    let median_h = median(items.iter().map(|l| l.rect.h.max(1.0)).collect());
    let groups = if options.columns {
        split_columns(&items, median_h)
    } else {
        vec![items]
    };
    let mut blocks: Vec<String> = Vec::new();
    for col in groups {
        let text = column_text(&col, median_h, options.paragraph_gap);
        if !text.is_empty() {
            blocks.push(text);
        }
    }
    blocks.join("\n\n")
}

/// Number of whitespace-separated words in `text` (what the toast reports).
pub fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

/// Columns: the union of the lines' x-intervals falls apart where no line covers a stretch at
/// least `2 x median height` wide. A line that spans such a stretch (a wide heading) hides the
/// gutter, so a page with one is read as a single column.
fn split_columns<'a>(items: &[&'a TextLine], median_h: f64) -> Vec<Vec<&'a TextLine>> {
    let mut order: Vec<&TextLine> = items.to_vec();
    order.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
    let gutter = median_h * 2.0;
    let mut cols: Vec<(f64, Vec<&TextLine>)> = Vec::new();
    for l in order {
        let (x0, x1) = (l.rect.x, l.rect.x + l.rect.w);
        match cols.last_mut() {
            Some((right, members)) if x0 < *right + gutter => {
                *right = right.max(x1);
                members.push(l);
            }
            _ => cols.push((x1, vec![l])),
        }
    }
    cols.into_iter().map(|(_, m)| m).collect()
}

fn column_text(col: &[&TextLine], median_h: f64, paragraph_gap: f64) -> String {
    // Rows: lines on one baseline band (the recognizer split a line) join left to right.
    let mut sorted: Vec<&TextLine> = col.to_vec();
    sorted.sort_by(|a, b| {
        (a.rect.y + a.rect.h / 2.0)
            .total_cmp(&(b.rect.y + b.rect.h / 2.0))
            .then(a.rect.x.total_cmp(&b.rect.x))
    });
    let mut rows: Vec<Vec<&TextLine>> = Vec::new();
    for l in sorted {
        let joins = rows.last().is_some_and(|row| {
            let (top, bottom) = row_extent(row);
            let overlap = (bottom.min(l.rect.y + l.rect.h) - top.max(l.rect.y)).max(0.0);
            overlap >= 0.5 * (bottom - top).min(l.rect.h)
        });
        if joins {
            if let Some(row) = rows.last_mut() {
                row.push(l);
            }
        } else {
            rows.push(vec![l]);
        }
    }
    let mut out = String::new();
    let mut prev_bottom: Option<f64> = None;
    for mut row in rows {
        row.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
        let (top, bottom) = row_extent(&row);
        if let Some(pb) = prev_bottom {
            out.push('\n');
            if top - pb > paragraph_gap * median_h {
                out.push('\n');
            }
        }
        let text: Vec<&str> = row.iter().map(|l| l.text.trim()).collect();
        out.push_str(text.join(" ").trim_end());
        prev_bottom = Some(bottom);
    }
    out
}

fn row_extent(row: &[&TextLine]) -> (f64, f64) {
    let top = row.iter().map(|l| l.rect.y).fold(f64::INFINITY, f64::min);
    let bottom = row
        .iter()
        .map(|l| l.rect.y + l.rect.h)
        .fold(f64::NEG_INFINITY, f64::max);
    (top, bottom)
}

/// A region in whole pixels inside an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRegion {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl PixelRegion {
    /// The region as an [`Rc`].
    pub fn rc(self) -> Rc {
        Rc::new(
            f64::from(self.x),
            f64::from(self.y),
            f64::from(self.w),
            f64::from(self.h),
        )
    }

    /// The whole image.
    pub fn whole(size: (u32, u32)) -> PixelRegion {
        PixelRegion {
            x: 0,
            y: 0,
            w: size.0,
            h: size.1,
        }
    }

    /// `rect` rounded outwards to whole pixels and clamped to an image of `size`.
    ///
    /// # Errors
    /// `invalid_input` when the rectangle is not finite, empty, or does not touch the image.
    pub fn clamp(rect: Rc, size: (u32, u32)) -> Result<PixelRegion, CmError> {
        if !rect.is_finite() {
            return Err(CmError::invalid_field("region", "must be finite numbers"));
        }
        let r = rect.normalized();
        let x0 = r.x.floor().max(0.0);
        let y0 = r.y.floor().max(0.0);
        let x1 = (r.x + r.w).ceil().min(f64::from(size.0));
        let y1 = (r.y + r.h).ceil().min(f64::from(size.1));
        if r.w <= 0.0 || r.h <= 0.0 || x1 <= x0 || y1 <= y0 {
            return Err(CmError::invalid_field(
                "region",
                format!(
                    "must be a non-empty rectangle inside the {}x{} image",
                    size.0, size.1
                ),
            ));
        }
        Ok(PixelRegion {
            x: x0 as u32,
            y: y0 as u32,
            w: (x1 - x0) as u32,
            h: (y1 - y0) as u32,
        })
    }
}

/// Recognizes `region` of `image`: crops it with [`REGION_MARGIN`] px around (clamped to the
/// image), recognizes the crop, maps every box back to the full image's pixels and keeps the
/// lines whose centre is inside the region (a line the margin caught only by its edge is
/// dropped). With the whole image as region nothing is cropped.
///
/// # Errors
/// The recognizer's errors.
pub fn recognize_region(
    recognizer: &dyn TextRecognizer,
    image: &RgbaImage,
    region: PixelRegion,
) -> Result<Vec<TextLine>, CmError> {
    if region == PixelRegion::whole(image.dimensions()) {
        return recognizer.recognize(image);
    }
    let m = REGION_MARGIN;
    let x0 = region.x.saturating_sub(m);
    let y0 = region.y.saturating_sub(m);
    let x1 = (region.x + region.w + m).min(image.width());
    let y1 = (region.y + region.h + m).min(image.height());
    let crop = image::imageops::crop_imm(image, x0, y0, x1 - x0, y1 - y0).to_image();
    let lines = recognizer.recognize(&crop)?;
    let (dx, dy) = (f64::from(x0), f64::from(y0));
    let inside = region.rc();
    Ok(lines
        .into_iter()
        .map(|l| offset_line(l, dx, dy))
        .filter(|l| {
            let (cx, cy) = (l.rect.x + l.rect.w / 2.0, l.rect.y + l.rect.h / 2.0);
            cx >= inside.x
                && cx <= inside.x + inside.w
                && cy >= inside.y
                && cy <= inside.y + inside.h
        })
        .collect())
}

fn offset_rect(r: Rc, dx: f64, dy: f64) -> Rc {
    Rc::new(r.x + dx, r.y + dy, r.w, r.h)
}

fn offset_line(l: TextLine, dx: f64, dy: f64) -> TextLine {
    TextLine {
        rect: offset_rect(l.rect, dx, dy),
        words: l
            .words
            .into_iter()
            .map(|w| TextWord {
                text: w.text,
                rect: offset_rect(w.rect, dx, dy),
            })
            .collect(),
        ..l
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::ScriptedRecognizer;

    fn l(text: &str, x: f64, y: f64) -> TextLine {
        TextLine::laid_out(text, x, y, 8.0, 16.0)
    }

    fn text(lines: &[TextLine]) -> String {
        layout_text(lines, &LayoutOptions::default())
    }

    #[test]
    fn empty_and_blank_give_empty_text() {
        assert_eq!(text(&[]), "");
        assert_eq!(text(&[l("   ", 0.0, 0.0)]), "");
    }

    #[test]
    fn single_column_in_reading_order_whatever_the_input_order() {
        let lines = [
            l("third", 10.0, 60.0),
            l("first", 10.0, 10.0),
            l("second", 10.0, 35.0),
        ];
        assert_eq!(text(&lines), "first\nsecond\nthird");
    }

    #[test]
    fn a_large_vertical_gap_makes_a_blank_line() {
        let lines = [
            l("one", 10.0, 10.0),
            l("two", 10.0, 30.0),
            l("three", 10.0, 90.0),
        ];
        assert_eq!(text(&lines), "one\ntwo\n\nthree");
    }

    #[test]
    fn two_columns_are_read_one_after_the_other() {
        let lines = [
            l("left 1", 10.0, 10.0),
            l("right 1", 400.0, 10.0),
            l("left 2", 10.0, 30.0),
            l("right 2", 400.0, 30.0),
        ];
        assert_eq!(text(&lines), "left 1\nleft 2\n\nright 1\nright 2");
        let flat = layout_text(
            &lines,
            &LayoutOptions {
                columns: false,
                ..LayoutOptions::default()
            },
        );
        assert_eq!(flat, "left 1 right 1\nleft 2 right 2");
    }

    #[test]
    fn pieces_of_one_row_join_left_to_right() {
        let lines = [l("world", 80.0, 12.0), l("hello", 10.0, 10.0)];
        // Close in x: one column, one row.
        assert_eq!(text(&lines), "hello world");
    }

    #[test]
    fn trailing_spaces_are_trimmed_and_hyphens_kept() {
        let lines = [l("exam-  ", 10.0, 10.0), l("ple   ", 10.0, 30.0)];
        assert_eq!(text(&lines), "exam-\nple");
    }

    #[test]
    fn a_wide_heading_keeps_one_column() {
        let lines = [
            l(
                "a heading that spans the whole width of the page ok",
                10.0,
                10.0,
            ),
            l("left", 10.0, 50.0),
            l("right", 300.0, 50.0),
        ];
        assert_eq!(
            text(&lines),
            "a heading that spans the whole width of the page ok\n\nleft right"
        );
    }

    #[test]
    fn word_count_counts_words() {
        assert_eq!(word_count("a b\n\n c  d"), 4);
        assert_eq!(word_count(""), 0);
    }

    #[test]
    fn clamp_rounds_out_and_rejects_empty_or_outside() {
        let p = PixelRegion::clamp(Rc::new(-5.0, 10.4, 50.0, 20.2), (100, 100)).unwrap();
        assert_eq!(
            p,
            PixelRegion {
                x: 0,
                y: 10,
                w: 45,
                h: 21
            }
        );
        let rev = PixelRegion::clamp(Rc::new(60.0, 60.0, -20.0, -20.0), (100, 100)).unwrap();
        assert_eq!(
            rev,
            PixelRegion {
                x: 40,
                y: 40,
                w: 20,
                h: 20
            }
        );
        assert!(PixelRegion::clamp(Rc::new(0.0, 0.0, 0.0, 10.0), (100, 100)).is_err());
        assert!(PixelRegion::clamp(Rc::new(200.0, 0.0, 10.0, 10.0), (100, 100)).is_err());
        assert!(PixelRegion::clamp(Rc::new(f64::NAN, 0.0, 10.0, 10.0), (100, 100)).is_err());
    }

    /// Reports the crop's size in a line at crop-local (10, 10) plus one in the margin.
    struct CropProbe;
    impl TextRecognizer for CropProbe {
        fn name(&self) -> &'static str {
            "probe"
        }
        fn is_available(&self) -> bool {
            true
        }
        fn recognize(&self, image: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
            Ok(vec![
                TextLine::laid_out(
                    &format!("{}x{}", image.width(), image.height()),
                    10.0,
                    10.0,
                    4.0,
                    6.0,
                ),
                TextLine::laid_out("margin", 0.0, 0.0, 1.0, 2.0),
            ])
        }
    }

    #[test]
    fn region_is_cropped_with_a_margin_and_boxes_map_back() {
        let img = RgbaImage::new(200, 100);
        let region = PixelRegion {
            x: 50,
            y: 40,
            w: 60,
            h: 30,
        };
        let lines = recognize_region(&CropProbe, &img, region).unwrap();
        // crop = (60+16) x (30+16), origin (42, 32); the margin line is outside the region.
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "76x46");
        assert_eq!(lines[0].rect.x, 52.0);
        assert_eq!(lines[0].rect.y, 42.0);
        assert_eq!(lines[0].words[0].rect.x, 52.0);
    }

    #[test]
    fn tiny_region_at_the_corner_still_crops_inside_the_image() {
        let img = RgbaImage::new(20, 20);
        let region = PixelRegion {
            x: 0,
            y: 0,
            w: 2,
            h: 2,
        };
        let lines = recognize_region(&CropProbe, &img, region).unwrap();
        // The crop is 10x10; the probe's lines fall outside the 2x2 region.
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn whole_image_is_not_cropped() {
        let img = RgbaImage::new(30, 20);
        let r = ScriptedRecognizer::new(vec![l("x", 1.0, 1.0)]);
        let lines = recognize_region(&r, &img, PixelRegion::whole((30, 20))).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(r.calls(), 1);
    }
}
