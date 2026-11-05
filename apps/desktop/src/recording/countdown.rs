//! The countdown window (DESIGN 5.3, REC-1.AC5, REC-4): a 160 px disc centered on the recording
//! area with the digit in Geist Mono 96/600 and a 4 px `rec` ring that drains each second.
//! Space skips it and Esc cancels it with nothing recorded.

use std::time::Duration;

use cm_i18n::tr;

use crate::recording::geometry::Rect;

/// Disc diameter (DESIGN 5.3).
pub const DISC: f64 = 160.0;
/// Ring stroke.
pub const RING: f64 = 4.0;
/// Digit size.
pub const DIGIT_SIZE: f32 = 96.0;
/// Transparent room around the disc in its window.
pub const PAD: f64 = 8.0;
/// Length of one countdown step (REC-4: 1 s).
pub const STEP: Duration = Duration::from_secs(1);

/// Height of the row under the disc that says what the keys do.
pub const HINT_ROW: f64 = 30.0;

/// The countdown's window: the disc (`DISC + 2 PAD` square) centered on the recorded area, with
/// the key hint row under it, kept on `display`.
pub fn window_rect(area: &Rect, display: &Rect) -> Rect {
    let side = DISC + 2.0 * PAD;
    let h = side + HINT_ROW;
    let (cx, cy) = area.center();
    let x = (cx - side / 2.0).clamp(display.x, (display.right() - side).max(display.x));
    let y = (cy - side / 2.0).clamp(display.y, (display.bottom() - h).max(display.y));
    Rect::new(x, y, side, h)
}

/// Whether the countdown must ask the engine where the recording is before it opens: a start
/// countdown (not a resume) whose indicator facts have not arrived. The engine publishes them
/// with `recording.starting`, which comes AFTER the countdown, but `get_recording_status`
/// answers with them from the first second. Without them the disc opened on a stale or the
/// primary display (Windows round 3, F3).
pub fn needs_lookup(resume: bool, have_indicator: bool, looked_up: bool, looking: bool) -> bool {
    !resume && !have_indicator && !looked_up && !looking
}

/// How much of the ring is left `elapsed` into the current step: 1 at the start of a second,
/// draining to 0 as the next digit arrives. With `animate` off (reduced motion) the ring is
/// shown full.
pub fn ring_fraction(elapsed: Duration, step: Duration, animate: bool) -> f64 {
    if !animate || step.is_zero() {
        return 1.0;
    }
    (1.0 - elapsed.as_secs_f64() / step.as_secs_f64()).clamp(0.0, 1.0)
}

/// Screen-reader text for the countdown.
pub fn label(remaining: u32, resume: bool) -> String {
    if resume {
        tr!("Resuming in {remaining}", remaining = remaining)
    } else {
        tr!("Recording starts in {remaining}", remaining = remaining)
    }
}

/// The line under the disc. The window opens without taking the keyboard, so the keys work after
/// a click (R4-11); the resume countdown is one second long and says nothing.
pub fn hint(resume: bool) -> String {
    if resume {
        String::new()
    } else {
        tr!("Click, then Space skips \u{00B7} Esc cancels")
    }
}

/// What a key does while the countdown is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Skip,
    Cancel,
    None,
}

/// REC-1.AC5: Space skips, Esc cancels.
pub fn key_action(key: &str) -> KeyAction {
    match key {
        "space" | " " => KeyAction::Skip,
        "escape" => KeyAction::Cancel,
        _ => KeyAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disc_is_centered_on_the_area_and_kept_on_screen() {
        let display = Rect::new(0.0, 0.0, 1440.0, 900.0);
        let w = window_rect(&Rect::new(160.0, 110.0, 960.0, 540.0), &display);
        // The disc (the top square of the window) is centered on the area.
        assert_eq!((w.x + w.w / 2.0, w.y + w.w / 2.0), (640.0, 380.0));
        assert_eq!(w.w, DISC + 2.0 * PAD);
        assert_eq!(w.h, w.w + HINT_ROW);
        // An area at the corner pushes the window back on the display.
        let w = window_rect(&Rect::new(-100.0, -100.0, 100.0, 100.0), &display);
        assert_eq!((w.x, w.y), (0.0, 0.0));
    }

    /// DESIGN 5.3: the ring drains over each second; reduced motion shows numbers only.
    #[test]
    fn ring_drains_each_second() {
        assert_eq!(ring_fraction(Duration::ZERO, STEP, true), 1.0);
        assert!((ring_fraction(Duration::from_millis(250), STEP, true) - 0.75).abs() < 1e-9);
        assert_eq!(ring_fraction(Duration::from_millis(1500), STEP, true), 0.0);
        assert_eq!(ring_fraction(Duration::from_millis(900), STEP, false), 1.0);
        assert_eq!(
            ring_fraction(Duration::from_millis(5), Duration::ZERO, true),
            1.0
        );
    }

    /// REC-1.AC5: Space skips, Esc cancels.
    #[test]
    fn rec1_ac5_keys() {
        assert_eq!(key_action("space"), KeyAction::Skip);
        assert_eq!(key_action("escape"), KeyAction::Cancel);
        assert_eq!(key_action("a"), KeyAction::None);
    }

    /// R4-11: the hint says the keys need a click first; the 1 s resume countdown has none.
    #[test]
    fn r4_11_hint_mentions_the_click() {
        assert!(hint(false).starts_with("Click"));
        assert!(hint(true).is_empty());
    }

    #[test]
    fn labels() {
        assert_eq!(label(3, false), "Recording starts in 3");
        assert_eq!(label(1, true), "Resuming in 1");
    }

    /// I18N P2: the digit survives in every language.
    #[test]
    fn i18n_label_keeps_the_digit() {
        use cm_i18n::{with_language, Lang};
        for lang in [Lang::En, Lang::Vi, Lang::EnXa] {
            with_language(lang, || {
                assert!(label(3, false).contains('3'));
                assert!(label(1, true).contains('1'));
            });
        }
    }

    #[test]
    fn the_lookup_happens_once_for_a_start_countdown_without_facts() {
        assert!(needs_lookup(false, false, false, false));
        assert!(
            !needs_lookup(true, false, false, false),
            "a resume knows its area"
        );
        assert!(!needs_lookup(false, true, false, false));
        assert!(!needs_lookup(false, false, true, false), "asked already");
        assert!(!needs_lookup(false, false, false, true), "being asked");
    }

    /// Round 3 (lab laptop): the disc is centred on the recorded display, in physical px, whichever
    /// monitor holds the pointer or was recorded last.
    mod lab {
        use super::*;
        use crate::recording::geometry::area_points;
        use crate::recording::monitors::{index_for_area_px, window_bounds, Monitor};

        fn lab() -> [Monitor; 2] {
            let m = |id, f: [f64; 4], w: [f64; 4]| Monitor {
                id,
                frame_px: Rect::new(f[0], f[1], f[2], f[3]),
                work_px: Rect::new(w[0], w[1], w[2], w[3]),
                scale: 1.5,
            };
            [
                m(1, [0., 0., 1920., 1080.], [0., 0., 1920., 1032.]),
                m(2, [1920., 0., 3840., 2160.], [1920., 0., 3840., 2160.]),
            ]
        }

        /// The disc's centre in physical px for a recording of `bounds`, and its monitor.
        fn disc_centre(bounds: [i64; 4]) -> (usize, (f64, f64)) {
            let ms = lab();
            let i = index_for_area_px(&ms, bounds).expect("a monitor holds the area");
            let display = ms[i].frame_points();
            let area = area_points(bounds, 1.5);
            let w = window_rect(&area, &display);
            let px = ms[i].to_physical(&window_bounds(&w, &display, true));
            (
                i,
                (px.x + px.w / 2.0, px.y + (DISC + 2.0 * PAD) / 2.0 * 1.5),
            )
        }

        #[test]
        fn recording_the_external_puts_the_disc_on_the_external() {
            let (i, (x, y)) = disc_centre([1920, 0, 3840, 2160]);
            assert_eq!(i, 1);
            assert_eq!((x, y), (3840.0, 1080.0));
        }

        #[test]
        fn recording_the_panel_puts_the_disc_on_the_panel() {
            let (i, (x, y)) = disc_centre([0, 0, 1920, 1080]);
            assert_eq!(i, 0);
            assert_eq!((x, y), (960.0, 540.0));
        }

        #[test]
        fn a_region_on_the_external_keeps_its_centre() {
            // 800x600 region at (2400, 300) on the external: centre (2800, 600).
            let (i, (x, y)) = disc_centre([2400, 300, 800, 600]);
            assert_eq!(i, 1);
            assert_eq!((x, y), (2800.0, 600.0));
        }
    }
}
