//! Golden stitching tests on synthetic frame sequences (CAP-6 AC1 to AC5; no screen needed).
//! The row-hash check of AC1 is "the stitched image equals the page, row for row": every row
//! carries its own index, so a duplicated or missing row cannot pass.

use super::fake::{Flicker, HandScript, NoSleep, SyntheticPage};
use super::*;

fn cfg(dir: Direction) -> LongShotConfig {
    LongShotConfig {
        direction: dir,
        ..Default::default()
    }
}

fn auto(page: SyntheticPage, max_px: Option<u32>) -> (LongShotResult, fake::PageView) {
    let view = page.clone().view();
    let (mut src, mut sc) = (view.clone(), view.clone());
    let mut config = cfg(page.direction);
    config.limits = Limits::with_max_px(max_px);
    let r = run_auto(
        &mut src,
        &mut sc,
        &mut NoSleep::default(),
        config,
        &mut || false,
        &mut |_| {},
    )
    .unwrap();
    (r, view)
}

/// CAP-6.AC1: a page five screens long, every row once and in order.
#[test]
fn cap_6_ac1_five_screens_every_row_once_in_order() {
    let page = SyntheticPage::new(40, 5 * 300, 300);
    let (r, _) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.frame, page.expected());
    assert!(r.image.frames >= 6);
}

/// The page ends part-way through a step: the last short scroll still joins.
#[test]
fn cap_6_ac1_content_not_a_multiple_of_the_step() {
    let page = SyntheticPage::new(33, 1234, 301);
    let (r, _) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.frame, page.expected());
}

/// CAP-6.AC2: a fixed header and footer appear once, at the top and the bottom.
#[test]
fn cap_6_ac2_fixed_header_and_footer_kept_once() {
    let page = SyntheticPage::new(40, 5 * 260, 340).with_bands(50, 30);
    let (r, _) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.header_len, 50);
    assert_eq!(r.image.footer_len, 30);
    assert_eq!(r.image.frame, page.expected());
}

/// CAP-6.AC3: a jump (no overlap) stops at the last good join; the frames before it are kept.
#[test]
fn cap_6_ac3_a_jump_stops_and_never_guesses() {
    let page = SyntheticPage::new(40, 2000, 300).with_jump(500, 700);
    let (r, _) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::NoOverlap);
    assert_eq!(r.end.note(), Some("Stopped: the next part did not line up"));
    // Whatever was kept is an exact prefix of the page: no wrong join.
    let kept = r.image.frame.height();
    assert!((300..1000).contains(&kept), "kept {kept}");
    assert_eq!(r.image.frame, page.expected_range(0, kept));
}

/// CAP-6.AC4: a page longer than the limit stops at the limit and the image is kept.
#[test]
fn cap_6_ac4_stops_at_the_limit() {
    let page = SyntheticPage::new(20, 3000, 300);
    let (r, _) = auto(page.clone(), Some(1000));
    assert_eq!(r.end, EndReason::LimitReached);
    assert_eq!(
        r.end.note(),
        Some("Reached the longest image CaptureMann can edit")
    );
    assert_eq!(r.image.frame.height(), 1000);
    assert_eq!(r.image.frame, page.expected_range(0, 1000));
}

#[test]
fn cap_6_ac4_pixel_limit_applies_too() {
    let page = SyntheticPage::new(1000, 4000, 300);
    let view = page.clone().view();
    let mut config = cfg(Direction::Vertical);
    config.limits = Limits {
        max_len: MAX_LONG_SIDE,
        max_pixels: 1000 * 900,
    };
    let r = run_auto(
        &mut view.clone(),
        &mut view.clone(),
        &mut NoSleep::default(),
        config,
        &mut || false,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(r.end, EndReason::LimitReached);
    assert_eq!(r.image.frame.height(), 900);
}

#[test]
fn product_limits_are_the_import_limits() {
    assert_eq!(MAX_LONG_SIDE, 16_384);
    assert_eq!(MAX_PIXELS, 50_000_000);
    assert_eq!(Limits::with_max_px(Some(99_999)).max_len, 16_384);
    assert_eq!(Limits::with_max_px(Some(500)).max_len, 500);
}

/// Horizontal scrolling uses the same method on columns.
#[test]
fn cap_6_horizontal_is_the_same_method_on_columns() {
    let page = SyntheticPage::new(30, 1500, 320)
        .with_bands(20, 20)
        .horizontal();
    let (r, _) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.frame, page.expected());
    assert_eq!(r.image.frame.height(), 30);
}

/// Esc or Stop keeps what has been captured.
#[test]
fn cap_6_stop_keeps_the_frames_so_far() {
    let page = SyntheticPage::new(20, 3000, 300);
    let view = page.clone().view();
    let mut calls = 0;
    let r = run_auto(
        &mut view.clone(),
        &mut view.clone(),
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || {
            calls += 1;
            calls > 3
        },
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(r.end, EndReason::StoppedByUser);
    assert_eq!(r.image.frames, 4);
    let kept = r.image.frame.height();
    assert_eq!(r.image.frame, page.expected_range(0, kept));
}

/// A page that does not scroll gives the single frame, and ends at once.
#[test]
fn cap_6_page_shorter_than_the_area_is_one_frame() {
    let page = SyntheticPage::new(20, 200, 300);
    let (r, view) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.frames, 1);
    assert_eq!(r.image.frame, page.frame_at(0));
    assert_eq!(view.scrolls(), 1);
}

/// CAP-6.AC5 (engine part): hand scrolling, at an uneven pace with mid-scroll frames that are
/// not taken, gives the same image. The Wayland page itself is lab work (spike S6).
#[test]
fn cap_6_ac5_hand_scroll_settles_then_takes_a_frame() {
    let page = SyntheticPage::new(40, 1500, 300).with_bands(30, 0);
    // Each stop repeats (settled); the in-between positions appear once (moving).
    let script = vec![
        0, 0, 0, 60, 130, 220, 220, 220, 390, 390, 390, 400, 640, 640, 640, 880, 880, 880, 1000,
        1130, 1130, 1130,
    ];
    let mut src = HandScript::new(page.clone(), script);
    let mut polls = 0;
    let r = run_hand(
        &mut src,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || {
            polls += 1;
            polls > 40
        },
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(r.end, EndReason::StoppedByUser);
    // The viewport showed content up to row 1130 + 270.
    assert_eq!(r.image.frame, page.expected_range(0, 1400));
}

/// Hand scroll that jumps too far stops with the same note.
#[test]
fn cap_6_hand_scroll_too_fast_stops() {
    let page = SyntheticPage::new(40, 3000, 300);
    let mut src = HandScript::new(page.clone(), vec![0, 0, 0, 100, 100, 100, 1500, 1500, 1500]);
    let r = run_hand(
        &mut src,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || false,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(r.end, EndReason::NoOverlap);
    assert_eq!(r.image.frame, page.expected_range(0, 400));
}

/// A blank stretch gives several equal joins: without a hint (hand scroll) it stops rather than
/// guess; with the auto-scroll step it still joins correctly.
#[test]
fn cap_6_ambiguous_overlap_is_not_guessed() {
    use super::stitch::{PushOutcome, Stitcher};
    let blank = |extra: u8| {
        // A frame of identical rows apart from the last few.
        let (w, h) = (4u32, 100u32);
        let mut d = vec![255u8; (w * h * 4) as usize];
        let n = d.len();
        d[n - 4] = extra;
        crate::Frame::new(w, h, d).unwrap()
    };
    let mut s = Stitcher::new(Direction::Vertical, Limits::default());
    s.push(&blank(1), None);
    assert_eq!(s.push(&blank(2), None), PushOutcome::NoOverlap);
    assert_eq!(s.frames(), 1);
}

#[test]
fn a_frame_of_another_size_does_not_join() {
    let page = SyntheticPage::new(20, 600, 100);
    let other = SyntheticPage::new(20, 600, 120);
    let mut s = Stitcher::new(Direction::Vertical, Limits::default());
    s.push(&page.frame_at(0), None);
    assert_eq!(s.push(&other.frame_at(60), None), PushOutcome::NoOverlap);
}

/// Settle detection: equal after one interval, and the 150 ms limit for an animation.
#[test]
fn settle_waits_for_two_equal_frames_and_gives_up_at_the_limit() {
    let page = SyntheticPage::new(8, 100, 20);
    let mut view = page.view();
    let mut sleep = NoSleep::default();
    let s = wait_settled(&mut view, &mut sleep, Settle::default()).unwrap();
    assert!(s.settled);
    assert_eq!(sleep.slept, Duration::from_millis(50));

    let mut sleep = NoSleep::default();
    let s = wait_settled(&mut Flicker(0), &mut sleep, Settle::default()).unwrap();
    assert!(!s.settled);
    assert_eq!(sleep.slept, Duration::from_millis(150));
}

/// Small noise (a caret blinking in a few overlapping lines) does not break the join.
#[test]
fn near_exact_overlap_tolerates_a_blinking_caret() {
    let page = SyntheticPage::new(30, 2000, 400);
    let a = page.frame_at(0);
    let mut b_rgba = page.frame_at(160).into_rgba();
    // Change 2 of the ~80 overlapping rows (rows 10 and 20 of frame b).
    for row in [10usize, 20] {
        b_rgba[row * 30 * 4 + 8] ^= 0xff;
    }
    let b = crate::Frame::new(30, 400, b_rgba).unwrap();
    let mut s = Stitcher::new(Direction::Vertical, Limits::default());
    s.push(&a, None);
    assert!(matches!(
        s.push(&b, Some(160)),
        PushOutcome::Added { new_len: 160 }
    ));
}

/// A real wheel moves more or less than asked (it differs per app): the driver learns the ratio
/// from the joins and still gets every row once, with no overlap lost.
#[test]
fn cap_6_ac1_a_wheel_that_moves_two_and_a_half_times_the_request() {
    struct Wheel(fake::PageView, f64);
    impl Scroller for Wheel {
        fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
            self.0.scroll(d, (f64::from(amount_px) * self.1) as u32)
        }
    }
    for ratio in [0.5, 1.0, 2.5] {
        let page = SyntheticPage::new(40, 5 * 300, 300);
        let view = page.clone().view();
        let (mut src, mut sc) = (view.clone(), Wheel(view, ratio));
        let r = run_auto(
            &mut src,
            &mut sc,
            &mut NoSleep::default(),
            cfg(Direction::Vertical),
            &mut || false,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(r.end, EndReason::ReachedEnd, "ratio {ratio}");
        assert_eq!(r.image.frame, page.expected(), "ratio {ratio}");
    }
}

/// Found on a real window (Windows, Nov 14, 2026): the thumb of the scrollbar moves with the
/// content, so no two frames have equal rows. The engine joins without the scrollbar columns and
/// the image is the page without them, every row once and in order.
#[test]
fn cap_6_ac1_a_moving_scrollbar_does_not_stop_the_join() {
    for sb in [15, 17, 26, 34] {
        let page = SyntheticPage::new(300, 5 * 300, 300).with_scrollbar(sb);
        let (r, _) = auto(page.clone(), None);
        assert_eq!(r.end, EndReason::ReachedEnd, "scrollbar {sb}");
        let w = r.image.frame.width();
        assert!(w >= 300 - sb - 4, "trim not too wide: {w}");
        let want = page.expected();
        assert_eq!(r.image.frame.height(), want.height(), "scrollbar {sb}");
        for y in (0..want.height()).step_by(7) {
            for x in [0, 1, w / 2, w - 5] {
                assert_eq!(
                    r.image.frame.pixel(x, y),
                    want.pixel(x, y),
                    "scrollbar {sb} at {x},{y}"
                );
            }
        }
    }
}

/// A real window draws its rounded corners and edge line next to the fixed bands, so the lines
/// there differ in every frame (Edge on Windows at 150 %, Nov 14, 2026). They are not compared
/// and the next frame's lines replace them: every row in between is still once and in order.
#[test]
fn cap_6_ac1_corner_rows_that_differ_in_every_frame() {
    for steps in [300, 150] {
        let page = SyntheticPage::new(300, 5 * 300, steps)
            .with_bands(40, 8)
            .with_corner_rows(5)
            .with_scrollbar(17);
        let (r, _) = auto(page.clone(), None);
        assert_eq!(r.end, EndReason::ReachedEnd, "view {steps}");
        let want = page.expected();
        assert_eq!(r.image.frame.height(), want.height(), "view {steps}");
        let w = r.image.frame.width();
        // Compare every row except the 5 under the header and the 5 above the footer, which
        // belong to the first and the last frame and are the corner artifacts themselves.
        for y in (40 + 5..want.height() - 8 - 5).step_by(3) {
            for x in [0, 1, w / 2, w - 1] {
                assert_eq!(
                    r.image.frame.pixel(x, y),
                    want.pixel(x, y),
                    "view {steps} at {x},{y}"
                );
            }
        }
    }
}

/// A jump is still refused with the trim attempts: no wrong join is made to fit.
#[test]
fn cap_6_ac3_a_jump_with_a_scrollbar_still_stops() {
    let page = SyntheticPage::new(300, 2000, 300)
        .with_jump(500, 700)
        .with_scrollbar(17);
    let (r, _) = auto(page, None);
    assert_eq!(r.end, EndReason::NoOverlap);
}

/// The side preview (S2): scaled to the panel width, never enlarged, newest rows kept when taller.
#[test]
fn cap_6_preview_thumbnail_is_scaled_and_follows_the_end() {
    let page = SyntheticPage::new(40, 1200, 300);
    let view = page.clone().view();
    let (mut src, mut sc) = (view.clone(), view.clone());
    let mut seen = Vec::new();
    let r = run_auto_ex(
        &mut src,
        &mut sc,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || false,
        &mut |_| {},
        &mut |st: &Stitcher| {
            seen.push(st.thumbnail(20, 50).unwrap());
        },
    )
    .unwrap();
    assert_eq!(seen.len() as u32, r.image.frames);
    // 40 px wide into 20: half size; 50 rows at most (the newest ones).
    let last = seen.last().unwrap();
    assert_eq!(last.width(), 20);
    assert_eq!(last.height(), 50);
    // Never enlarged: a wide panel leaves the width alone.
    let st = Stitcher::new(Direction::Vertical, Limits::default());
    assert!(st.thumbnail(20, 50).is_none());
    let mut st = Stitcher::new(Direction::Vertical, Limits::default());
    st.push(&page.frame_at(0), None);
    assert_eq!(st.thumbnail(400, 400).unwrap().width(), 40);
}
