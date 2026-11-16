//! How big the step picture is in the guide editor (design board S19, QA G1): the picture keeps
//! its proportions and fits inside the right column, it is never cropped and never drawn larger
//! than its own size. A tall picture (a scrolling page) is fitted by height and centred, with a
//! chip that says a click shows it at full size.

/// The right column's rows above and below the picture, px: title row, note, action row and
/// meta line (design S19), and the four 12 px gaps between the five children.
pub const TITLE_ROW: f32 = 40.0;
pub const NOTE: f32 = 40.0;
pub const ACTION_ROW: f32 = 32.0;
pub const META: f32 = 16.0;
pub const GAPS: f32 = 4.0 * 12.0;
/// The picture is at least this high; below that the column scrolls instead.
pub const MIN_HEIGHT: f32 = 240.0;
/// A fitted width under this share of the column is a narrow picture.
pub const TALL_SHARE: f32 = 0.6;
/// A picture is "tall" (a scrolling page, a Long Shot) only when it is at least this many times
/// higher than wide; a plain window frame (about 1:1 to 1:1.3) is not (QA CG-12).
pub const TALL_ASPECT: f32 = 1.8;
/// What the page puts above the editor's columns (window title, toolbar, banner) at its usual
/// size, so a 900 px window has a 700 px column (the board's 524 px picture).
pub const PAGE_CHROME: f32 = 200.0;
/// The left column (steps list) and the paddings of the right one, px.
pub const LEFT_COLUMN: f32 = 380.0;
pub const RIGHT_PADDING: f32 = 24.0;

/// The tallest the picture may be in a column `column_h` high.
pub fn max_height(column_h: f32) -> f32 {
    (column_h - TITLE_ROW - NOTE - ACTION_ROW - META - GAPS).max(MIN_HEIGHT)
}

/// The right column's height and the width left for the picture in a window `window_w` by
/// `window_h` logical px.
pub fn column_size(window_w: f32, window_h: f32) -> (f32, f32) {
    (
        (window_h - PAGE_CHROME).max(0.0),
        (window_w - LEFT_COLUMN - 1.0 - 2.0 * RIGHT_PADDING).max(0.0),
    )
}

/// The picture's place in the column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    pub w: f32,
    pub h: f32,
    /// The picture is really tall (height at least `TALL_ASPECT` times its width) and was fitted
    /// narrower than the column.
    pub tall: bool,
}

/// Fits a picture of `natural_w` by `natural_h` logical px (its pixels over the display's scale)
/// into `col_w` and `max_h`: contain, never upscaled.
pub fn fit(natural_w: f32, natural_h: f32, col_w: f32, max_h: f32) -> Fit {
    let (nw, nh) = (natural_w.max(1.0), natural_h.max(1.0));
    let scale = (col_w / nw).min(max_h / nh).clamp(0.0, 1.0);
    let (w, h) = (nw * scale, nh * scale);
    Fit {
        w,
        h,
        tall: nh >= TALL_ASPECT * nw && w < TALL_SHARE * col_w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// At a 900 px window the picture is about 524 px high (the board's number).
    #[test]
    fn s19_the_maximum_height_at_a_900_px_window_is_524() {
        let (col_h, _) = column_size(1440.0, 900.0);
        assert_eq!(col_h, 700.0);
        assert_eq!(max_height(col_h), 524.0);
    }

    /// Below 240 px the picture stops shrinking and the column scrolls.
    #[test]
    fn s19_the_minimum_height_is_240() {
        assert_eq!(max_height(300.0), MIN_HEIGHT);
        assert_eq!(max_height(0.0), MIN_HEIGHT);
    }

    /// A normal window picture fills the column's width and is never upscaled.
    #[test]
    fn s19_a_normal_picture_fills_the_column_and_is_not_upscaled() {
        let f = fit(2000.0, 1000.0, 600.0, 524.0);
        assert_eq!((f.w, f.h, f.tall), (600.0, 300.0, false));
        // A small picture stays at 100 %.
        let small = fit(300.0, 200.0, 600.0, 524.0);
        assert_eq!((small.w, small.h), (300.0, 200.0));
    }

    /// A picture that is taller than the room is fitted by height, whole.
    #[test]
    fn s19_a_tall_picture_is_fitted_by_height_and_flagged() {
        let f = fit(758.0, 6360.0, 600.0, 524.0);
        assert!((f.h - 524.0).abs() < 1e-3);
        assert!((f.w / f.h - 758.0 / 6360.0).abs() < 1e-6, "aspect kept");
        assert!(f.w < 600.0 && f.tall);
        // The aspect is the line (CG-12): a window frame of 770 x 860 is fitted narrower than
        // 60 % of a wide column but is not a long picture.
        let window = fit(770.0, 860.0, 1011.0, 524.0);
        assert!(window.w < TALL_SHARE * 1011.0);
        assert!(
            !window.tall,
            "a normal tall window frame is not a long image"
        );
        // Exactly at the threshold counts.
        assert!(fit(500.0, 500.0 * TALL_ASPECT, 1000.0, 524.0).tall);
        assert!(!fit(500.0, 500.0 * TALL_ASPECT - 5.0, 1000.0, 524.0).tall);
    }

    /// Contain never crops: the fitted size always fits the box and keeps the proportions.
    #[test]
    fn s19_the_fit_is_inside_the_box_for_any_picture() {
        for (w, h) in [
            (10.0, 10.0),
            (4000.0, 300.0),
            (300.0, 4000.0),
            (1.0, 5000.0),
        ] {
            for (cw, mh) in [(500.0, 240.0), (900.0, 524.0), (320.0, 700.0)] {
                let f = fit(w, h, cw, mh);
                assert!(f.w <= cw + 1e-3 && f.h <= mh + 1e-3, "{w}x{h} in {cw}x{mh}");
                assert!(f.w <= w + 1e-3 && f.h <= h + 1e-3, "never upscaled");
                assert!((f.w / f.h - w / h).abs() < 1e-3, "aspect");
            }
        }
    }
}
