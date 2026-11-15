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

/// A page that does not scroll gives the single frame, kept, and says that nothing scrolled (it cannot be told from a window that ignored the wheel).
#[test]
fn cap_6_page_shorter_than_the_area_is_one_frame() {
    let page = SyntheticPage::new(20, 200, 300);
    let (r, view) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::NothingScrolled);
    assert_eq!(r.image.frames, 1);
    assert_eq!(r.image.frame, page.frame_at(0));
    assert_eq!(view.scrolls(), 2);
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

/// Runs Hand scroll over `script`, then stops the user's way after `polls` polls.
fn hand(page: &SyntheticPage, script: Vec<u32>, polls: u32) -> LongShotResult {
    let mut src = HandScript::new(page.clone(), script);
    let mut n = 0;
    run_hand(
        &mut src,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || {
            n += 1;
            n > polls
        },
        &mut |_| {},
    )
    .unwrap()
}

/// Hand scroll that jumps too far does not end the capture: the frame is left out. When the user
/// stops there, the end is the "did not line up" note and the image so far is kept.
#[test]
fn cap_6_hand_scroll_too_fast_is_left_out_and_noted_on_stop() {
    let page = SyntheticPage::new(40, 3000, 300);
    let r = hand(&page, vec![0, 0, 0, 100, 100, 100, 1500, 1500, 1500], 20);
    assert_eq!(r.end, EndReason::NoOverlap);
    assert_eq!(r.end.note(), Some("Stopped: the next part did not line up"));
    assert_eq!(r.image.frame, page.expected_range(0, 400));
}

/// A frame that does not join (a fast flick) does not stop Hand scroll: the user scrolls back,
/// or less, and the next frame that joins is taken. No row is missing or doubled.
#[test]
fn cap_6_hand_scroll_continues_after_a_frame_that_did_not_line_up() {
    let page = SyntheticPage::new(40, 3000, 300).with_bands(30, 0);
    let script = vec![
        0, 0, 0, 100, 100, 100, // joins
        1500, 1500, 1500, // too far: left out
        1500, 1500, // the same frame again: not tried again
        200, 200, 200, // a position that joins the last good frame
        330, 330, 330,
    ];
    let r = hand(&page, script, 40);
    assert_eq!(r.end, EndReason::StoppedByUser);
    assert_eq!(r.end.note(), None);
    assert_eq!(r.image.frame, page.expected_range(0, 330 + 270));
}

/// Found in a Terminal window and a Chrome tab through the daemon (macOS, Nov 14, 2026): a
/// trackpad flick or a wheel with momentum carries the content further than one screen before it
/// settles, so the settled frame has no overlap with the last one taken. The frames seen on the
/// way (not settled) bridge it: the furthest one that still joined is taken first.
#[test]
fn cap_6_hand_scroll_flick_longer_than_a_screen_is_bridged() {
    let page = SyntheticPage::new(400, 6000, 1000).with_bands(30, 0);
    let script = vec![
        0, 0, 0, // settled
        200, 500, 800, // moving: each joins the first frame
        1300, 1300, 1300, // settled, 1,300 rows on: no overlap with the first frame
        1500, 1500, 1500,
    ];
    let r = hand(&page, script, 40);
    assert_eq!(r.end, EndReason::StoppedByUser);
    assert_eq!(r.image.frame, page.expected_range(0, 1500 + 970));
}

/// Found on Windows (Nov 14, 2026): small wheel steps one after the other (40 px every 250 ms)
/// keep the content moving, no two polls are ever equal, and no frame was taken at all. A frame
/// that is far enough on is taken as it is.
#[test]
fn cap_6_hand_scroll_that_never_settles_still_takes_frames() {
    let page = SyntheticPage::new(400, 8000, 1000).with_bands(30, 0);
    // 40 rows per poll, never the same twice, 60 polls: from 0 to 2,400.
    let script: Vec<u32> = (0..61).map(|i| i * 40).collect();
    let r = hand(&page, script, 80);
    assert_eq!(r.end, EndReason::StoppedByUser);
    // The last frame was taken too (the script ends there and holds still).
    // The first frame is the one the settle wait gave up on (120); the last one is 2,400.
    assert_eq!(r.image.frame, page.expected_range(120, 2400 + 970));
    assert!(r.image.frames >= 4, "frames: {}", r.image.frames);
}

/// Found on Windows (Nov 14, 2026): a first Hand step of 18 px moves an overlay scrollbar's thumb
/// by 2 px, too little to be taken for a moving edge, and the pair was refused. A thumb is a few
/// pixels of a wide line: the tolerant look joins it.
#[test]
fn cap_6_a_tiny_first_step_with_a_scrollbar_thumb_joins() {
    let page = SyntheticPage::new(800, 20_000, 600).with_scrollbar(15);
    let r = hand(&page, vec![0, 0, 0, 18, 18, 18, 300, 300, 300], 30);
    assert_eq!(r.end, EndReason::StoppedByUser);
    assert_eq!(r.image.frames, 3);
    assert_eq!(
        r.image.frame.height(),
        page.expected_range(0, 300 + 600).height()
    );
}

/// Frames from the middle of a scroll are not taken when the settled frame joins: the pixels of a
/// page that is still animating are not what the person scrolled to.
#[test]
fn cap_6_hand_scroll_takes_no_moving_frame_when_the_settled_one_joins() {
    let page = SyntheticPage::new(400, 6000, 1000);
    // 100 and 250 are seen moving; only the settled 400 is taken (two frames in all).
    let script = vec![0, 0, 0, 100, 250, 400, 400, 400];
    let r = hand(&page, script, 30);
    assert_eq!(r.end, EndReason::StoppedByUser);
    assert_eq!(r.image.frame, page.expected_range(0, 400 + 1000));
    assert_eq!(r.image.frames, 2);
}

/// Back at the last good frame after a frame that was left out: a plain stop, no note.
#[test]
fn cap_6_hand_scroll_back_to_the_last_frame_clears_the_note() {
    let page = SyntheticPage::new(40, 3000, 300);
    let script = vec![0, 0, 0, 100, 100, 100, 1500, 1500, 1500, 100, 100, 100];
    let r = hand(&page, script, 20);
    assert_eq!(r.end, EndReason::StoppedByUser);
    assert_eq!(r.image.frame, page.expected_range(0, 400));
}

/// S18: the progress callback tells the HUD when the newest settled frame did not line up (so it
/// can say "Scroll back up a little") and again when a frame lines up or the person is back at the
/// last good frame. The count never changes on that notice.
#[test]
fn cap_6_hand_scroll_reports_when_a_frame_did_not_line_up_and_when_it_does_again() {
    let page = SyntheticPage::new(40, 3000, 300).with_bands(30, 0);
    let script = vec![
        0, 0, 0, 100, 100, 100, // joins
        1500, 1500, 1500, // too far: left out
        100, 100, 100, // back at the last good frame
        1500, 1500, 1500, // left out again
        200, 200, 200, // joins
    ];
    let mut src = HandScript::new(page, script);
    let mut seen: Vec<(u32, bool)> = Vec::new();
    let mut n = 0;
    run_hand(
        &mut src,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || {
            n += 1;
            n > 40
        },
        &mut |p| seen.push((p.frames, p.last_join_ok)),
    )
    .unwrap();
    // The same notice twice in a row says nothing new.
    seen.dedup();
    assert_eq!(
        seen,
        vec![
            (1, true),
            (2, true),
            (2, false),
            (2, true),
            (2, false),
            (3, true)
        ]
    );
}

/// Checks the image against the page on two columns the clock and the cursor never touch: the
/// column that carries the row number and one in the middle. Every row once and in order.
fn assert_rows_match(got: &Frame, want: &Frame, what: &str) {
    assert_eq!(got.height(), want.height(), "{what}: height");
    let w = got.width();
    for y in 0..want.height() {
        for x in [0, w / 2] {
            assert_eq!(got.pixel(x, y), want.pixel(x, y), "{what}: at {x},{y}");
        }
    }
}

/// Found on Chrome through the daemon (macOS, Nov 14, 2026): a sticky header with a clock and a
/// blinking caret changes in every frame. The first pair then had no fixed band, the bar was taken
/// for content that does not move, and no frame joined any more. A few differing pixels in a line
/// are not a different line.
#[test]
fn cap_6_ac1_a_clock_in_the_fixed_header_does_not_stop_the_join() {
    let page = SyntheticPage::new(800, 1500, 300)
        .with_bands(40, 0)
        .with_header_clock(10);
    let (r, _) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_rows_match(&r.image.frame, &page.expected(), "auto");
    let r = hand(
        &page,
        vec![0, 0, 0, 70, 70, 70, 150, 150, 150, 260, 260, 260],
        30,
    );
    assert_eq!(r.end, EndReason::StoppedByUser);
    assert_rows_match(&r.image.frame, &page.expected_range(0, 260 + 260), "hand");
}

/// A frame that differs from the last one only by the clock or a caret is not a frame that did
/// not line up: nothing changed.
#[test]
fn cap_6_a_blink_alone_is_no_change() {
    let page = SyntheticPage::new(800, 1500, 300)
        .with_bands(40, 0)
        .with_header_clock(10);
    // Positions 0 and 7 give different clocks, the same content.
    let mut s = Stitcher::new(Direction::Vertical, Limits::default());
    s.push(&page.frame_at(0), None);
    let mut other = page.clone();
    other.header_clock = 12;
    assert_eq!(s.push(&other.frame_at(0), None), PushOutcome::NoChange);
}

/// A terminal's block cursor: 34 rows of a 1,500-row view, blinking, on the line that scrolls.
/// It is more than the 2 % of lines the overlap may differ by, and it used to refuse the join.
#[test]
fn cap_6_ac1_a_blinking_cursor_in_the_content_does_not_stop_the_join() {
    let page = SyntheticPage::new(400, 5000, 1500).with_blinking_cursor(900, 34, 18);
    let positions = vec![0, 0, 0, 305, 305, 305, 700, 700, 700, 1105, 1105, 1105];
    let mut seen_blink = 0;
    for p in [0u32, 305, 700, 1105] {
        seen_blink += (p / 5) % 2;
    }
    assert!(
        seen_blink > 0 && seen_blink < 4,
        "the cursor must blink in the script"
    );
    let r = hand(&page, positions, 30);
    assert_eq!(r.end, EndReason::StoppedByUser);
    assert_rows_match(&r.image.frame, &page.expected_range(0, 1105 + 1500), "hand");
}

/// A sticky header that hides once the page scrolls down (the Chrome toolbar-style "headroom"
/// bar): the fixed band shrinks, the capture goes on, and the image still has every row once and
/// in order (the bar stays once at the top).
#[test]
fn cap_6_a_header_that_hides_while_scrolling_does_not_stop_the_capture() {
    let page = SyntheticPage::new(40, 2000, 300)
        .with_bands(40, 0)
        .with_hiding_header(150);
    let r = hand(
        &page,
        vec![
            0, 0, 0, 60, 60, 60, 160, 160, 160, 300, 300, 300, 460, 460, 460,
        ],
        40,
    );
    assert_eq!(r.end, EndReason::StoppedByUser);
    // The last viewport shows content rows up to 460 + 260.
    let plain = SyntheticPage::new(40, 2000, 300).with_bands(40, 0);
    assert_eq!(r.image.frame, plain.expected_range(0, 460 + 260));
    let (r, _) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.frame, plain.expected());
}

/// Found on Chrome (macOS, Nov 14, 2026): the overlay scrollbar appears once the page scrolls and
/// its short thumb, near the top of the page, lies on the sticky header's rows alone. The first
/// pair then looked as if the header ended where the thumb starts, and nothing joined. The
/// moving columns are found in the bands too, and cut.
#[test]
fn cap_6_ac1_an_overlay_scrollbar_over_the_header_does_not_stop_the_join() {
    for sb in [10, 15, 30] {
        let page = SyntheticPage::new(300, 3 * 300, 300)
            .with_bands(60, 0)
            .with_overlay_scrollbar(sb);
        let (r, _) = auto(page.clone(), None);
        assert_eq!(r.end, EndReason::ReachedEnd, "auto, scrollbar {sb}");
        assert_eq!(
            r.image.frame.height(),
            page.expected().height(),
            "auto {sb}"
        );
        let r = hand(&page, vec![0, 0, 0, 90, 90, 90, 180, 180, 180], 20);
        assert_eq!(r.end, EndReason::StoppedByUser, "hand, scrollbar {sb}");
        assert_eq!(
            r.image.frame.height(),
            page.expected_range(0, 180 + 240).height(),
            "hand {sb}"
        );
    }
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
    s.push(&blank(100), None);
    assert_eq!(s.push(&blank(200), None), PushOutcome::NoOverlap);
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

/// The preview of a capture whose lines were trimmed (a scrollbar cut off): the daemon asks for it
/// after every joined frame, and it used to index past the trimmed lines (found on Chrome, Nov 14,
/// 2026: the worker panicked and the capture failed).
#[test]
fn cap_6_preview_of_a_trimmed_capture_does_not_panic() {
    let page = SyntheticPage::new(300, 1500, 300).with_scrollbar(20);
    let view = page.clone().view();
    let (mut src, mut sc) = (view.clone(), view.clone());
    let mut sizes = Vec::new();
    let r = run_auto_ex(
        &mut src,
        &mut sc,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || false,
        &mut |_| {},
        &mut |st: &Stitcher| {
            sizes.push(st.thumbnail(150, 400).map(|f| (f.width(), f.height())));
        },
    )
    .unwrap();
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert!(r.image.frame.width() < 300);
    assert!(sizes.iter().all(Option::is_some));
}

/// Chrome on a Retina Mac (Nov 14, 2026): the toolbar of two captures differs by one colour step
/// in some pixels, so an exact comparison found no header and no join. The bands are found
/// with a small per-channel tolerance.
#[test]
fn header_with_one_step_noise_still_joins() {
    const W: usize = 120;
    const H: usize = 300;
    const HEADER: usize = 40;
    // The page: row r of the document has a unique pattern.
    let doc_row = |r: usize| -> Vec<u8> {
        (0..W)
            .flat_map(|x| {
                let v = ((r * 37 + x * 11) % 251) as u8;
                [v, v ^ 0x55, (r % 256) as u8, 255]
            })
            .collect()
    };
    let frame = |scroll: usize, noise: u8| -> Frame {
        let mut data = Vec::new();
        for y in 0..H {
            if y < HEADER {
                // The toolbar: fixed, plus a one-step change in a few channels.
                data.extend((0..W).flat_map(|x| {
                    let n = u8::from(noise == 1 && (x + y) % 7 == 0);
                    [30 + n, 30, 40, 255]
                }));
            } else {
                let mut row = doc_row(scroll + y - HEADER);
                if noise == 1 {
                    // The page itself is re-composited too: one step in scattered pixels.
                    for x in (y % 5..W).step_by(5) {
                        row[x * 4] = row[x * 4].wrapping_add(1);
                    }
                }
                data.extend(row);
            }
        }
        Frame::new(W as u32, H as u32, data).unwrap()
    };
    let mut st = Stitcher::new(Direction::Vertical, Limits::default());
    st.push(&frame(0, 0), None);
    let out = st.push(&frame(150, 1), Some(150));
    assert_eq!(out, PushOutcome::Added { new_len: 150 });
    assert_eq!(st.bands(), (HEADER as u32, 0));
    // The end of the page: the same content again, noise differing: no change, not "no overlap".
    assert_eq!(
        st.push(&frame(150, 0), Some(150)),
        PushOutcome::NoChange,
        "a frame that differs by one colour step is the same frame"
    );
}

/// QA, Nov 14, 2026: a run whose scrolls changed nothing was reported as the end of the content
/// (one frame, `reached_end`, no note). It must say that nothing scrolled.
#[test]
fn a_scroll_that_moves_nothing_is_not_the_end_of_the_content() {
    struct Still(crate::Frame);
    impl FrameSource for Still {
        fn grab(&mut self) -> Result<crate::Frame, cm_core::CmError> {
            Ok(self.0.clone())
        }
    }
    #[derive(Default)]
    struct Deaf(u32);
    impl Scroller for Deaf {
        fn scroll(&mut self, _: Direction, _: u32) -> Result<(), cm_core::CmError> {
            self.0 += 1;
            Ok(())
        }
    }
    let frame = crate::Frame::new(40, 100, vec![200; 40 * 100 * 4]).unwrap();
    let (mut src, mut sc) = (Still(frame), Deaf::default());
    let r = run_auto(
        &mut src,
        &mut sc,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || false,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(r.end, EndReason::NothingScrolled);
    assert_eq!(r.image.frames, 1);
    assert_eq!(sc.0, 2, "one retry");
    assert!(r.end.note().is_some());
}

/// Windows, Edge (Nov 14, 2026): smooth scrolling paused near the start of a scroll for two
/// grabs, which looked settled; the distance learned from it was far too short, the next step
/// was huge and the join was lost ("did not line up" after one frame). What arrives late is
/// joined on top and the capture goes on to the end of the page.
#[test]
fn cap_6_smooth_scroll_that_pauses_early_is_not_believed() {
    struct Wheel(fake::PageView, f64);
    impl Scroller for Wheel {
        fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
            self.0.scroll(d, (f64::from(amount_px) * self.1) as u32)
        }
    }
    for ratio in [1.0, 2.0, 2.6] {
        for shares in [&[0.05, 0.05, 0.05, 1.0][..], &[0.1, 0.1, 0.1, 0.6, 1.0][..]] {
            let page = SyntheticPage::new(40, 5 * 300, 300);
            let view = page.clone().view().with_smooth(shares);
            let (mut src, mut sc) = (view.clone(), Wheel(view, ratio));
            let r = run_auto(
                &mut src,
                &mut sc,
                &mut NoSleep::default(),
                cfg(page.direction),
                &mut || false,
                &mut |_| {},
            )
            .unwrap();
            assert_eq!(
                r.end,
                EndReason::ReachedEnd,
                "ratio {ratio} shares {shares:?}"
            );
            assert_eq!(
                r.image.frame,
                page.expected(),
                "ratio {ratio} shares {shares:?}"
            );
        }
    }
}

/// A frame source that draws something over what the page shows: a browser's toolbar, a noise
/// block. `over` gets the page position, the frame's size and its pixels as RGBA.
struct Overlaid<F: FnMut(u32, u32, u32, &mut [u8])> {
    view: fake::PageView,
    pos: std::rc::Rc<std::cell::Cell<u32>>,
    over: F,
}

impl<F: FnMut(u32, u32, u32, &mut [u8])> FrameSource for Overlaid<F> {
    fn grab(&mut self) -> Result<Frame, cm_core::CmError> {
        let f = self.view.grab()?;
        let (w, h) = (f.width(), f.height());
        let mut data = f.rgba().to_vec();
        (self.over)(self.pos.get(), w, h, &mut data);
        Frame::new(w, h, data)
    }
}

/// A scroller that tells the overlay where the page is.
struct Tracking {
    view: fake::PageView,
    pos: std::rc::Rc<std::cell::Cell<u32>>,
}

impl Scroller for Tracking {
    fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
        self.view.scroll(d, amount_px)?;
        self.pos.set(self.pos.get() + amount_px);
        Ok(())
    }
}

/// Windows, Edge and Chrome, whole window, Nov 15, 2026: a page that scrolls sideways under the
/// browser's tab strip and toolbar ended after one frame (`no_overlap`). The toolbar's rows run
/// across the scroll axis and their icons do not move with the page, so no column of one frame
/// matched a column of the next. The static rows (and a static status bar) are cut instead; the
/// page between them is stitched, every column once and in order.
#[test]
fn cap_6_horizontal_whole_window_with_a_static_toolbar_joins() {
    for (top, bottom) in [(60u32, 0u32), (60, 14), (0, 14)] {
        let page = SyntheticPage::new(240, 3000, 600).horizontal();
        let view = page.clone().view();
        let pos = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut src = Overlaid {
            view: view.clone(),
            pos: pos.clone(),
            over: |_, w, h, data: &mut [u8]| {
                // Static chrome: every pixel depends on its place only (icons, text, tabs).
                for y in (0..top).chain(h - bottom..h) {
                    for x in 0..w {
                        let v = (x * 7 + y * 13) ^ (x >> 2);
                        let i = ((y * w + x) * 4) as usize;
                        data[i..i + 4].copy_from_slice(&[v as u8, (v >> 3) as u8, 90, 255]);
                    }
                }
            },
        };
        let mut sc = Tracking { view, pos };
        let r = run_auto(
            &mut src,
            &mut sc,
            &mut NoSleep::default(),
            cfg(Direction::Horizontal),
            &mut || false,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(r.end, EndReason::ReachedEnd, "top {top} bottom {bottom}");
        let want = page.expected();
        let got = &r.image.frame;
        assert_eq!(got.width(), want.width(), "top {top} bottom {bottom}");
        assert_eq!(got.height(), 240 - top - bottom, "cut to the page");
        for x in 0..want.width() {
            for y in [0, got.height() / 2, got.height() - 1] {
                assert_eq!(
                    got.pixel(x, y),
                    want.pixel(x, y + top),
                    "top {top} bottom {bottom} at {x},{y}"
                );
            }
        }
    }
}

/// A terminal's numbered lines (Windows Terminal, Nov 15, 2026: 200 lines, stopped at line 86):
/// text that differs from line to line in a few pixels, and a cell that changes in every frame
/// (the cursor, a spinner) in the middle of the lines, so the exact look finds nothing and the
/// tolerant look calls every shift equal. The shift whose overlap differs least is the join.
struct Term {
    pos: u32,
    max: u32,
    noisy: bool,
}

impl Term {
    const W: u32 = 900;
    const H: u32 = 300;
    const LINE: u32 = 20;

    fn pixel(&self, g: u32, x: u32) -> [u8; 4] {
        let (line, k) = (g / Self::LINE, g % Self::LINE);
        let mut v = 255u8;
        if (4..14).contains(&k) && (10..26).contains(&x) {
            let h = line.wrapping_mul(2_654_435_761) ^ k.wrapping_mul(40_503);
            if (h >> (x - 10)) & 1 == 1 {
                v = 0;
            }
        }
        if self.noisy && (400..410).contains(&x) && k < 12 {
            v = if (self.pos / 3 + x).is_multiple_of(2) {
                20
            } else {
                200
            };
        }
        [v, v, v, 255]
    }

    fn frame(&self) -> Frame {
        let mut d = Vec::new();
        for y in 0..Self::H {
            for x in 0..Self::W {
                d.extend_from_slice(&self.pixel(self.pos + y, x));
            }
        }
        Frame::new(Self::W, Self::H, d).unwrap()
    }
}

struct TermView(std::rc::Rc<std::cell::RefCell<Term>>);

impl FrameSource for TermView {
    fn grab(&mut self) -> Result<Frame, CmError> {
        Ok(self.0.borrow().frame())
    }
}

impl Scroller for TermView {
    fn scroll(&mut self, _: Direction, amount_px: u32) -> Result<(), CmError> {
        let mut t = self.0.borrow_mut();
        t.pos = (t.pos + amount_px).min(t.max);
        Ok(())
    }
}

#[test]
fn cap_6_terminal_lines_that_differ_in_a_few_pixels_do_not_stop_the_join() {
    let term = std::rc::Rc::new(std::cell::RefCell::new(Term {
        pos: 0,
        max: 2000,
        noisy: true,
    }));
    let (mut src, mut sc) = (TermView(term.clone()), TermView(term.clone()));
    let r = run_auto(
        &mut src,
        &mut sc,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || false,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(r.end, EndReason::ReachedEnd);
    let reference = Term {
        pos: 0,
        max: 0,
        noisy: false,
    };
    let got = &r.image.frame;
    assert_eq!(got.height(), 2000 + Term::H);
    // The text columns (the noise is in 400..410): every row once and in order.
    for y in 0..got.height() {
        for x in 8..30 {
            assert_eq!(got.pixel(x, y), Some(reference.pixel(y, x)), "at {x},{y}");
        }
    }
}

/// Slow continuous Hand scroll (QA, Windows, Nov 15, 2026: 30 small wheel steps 250 ms apart gave
/// one frame). Decision of Nov 15, 2026: a frame is taken when the content has moved a third of
/// the area from the last one taken, or has held still; a small shift is never a reason to drop a
/// frame. Steps of 15 px that never hold still: a frame every third of a screen.
#[test]
fn cap_6_hand_scroll_with_continuous_small_steps_takes_a_frame_every_third_of_the_area() {
    let page = SyntheticPage::new(400, 8000, 1000).with_bands(30, 0);
    let script: Vec<u32> = (0..121).map(|i| i * 15).collect();
    let r = hand(&page, script, 160);
    assert_eq!(r.end, EndReason::StoppedByUser);
    // The first frame is the one the settle wait gave up on (3 steps on); the end is 1,800.
    assert_eq!(r.image.frame, page.expected_range(45, 1800 + 970));
    // About 970 / 3 rows between frames: more than 5 frames over 1,750 rows.
    assert!(r.image.frames >= 5, "frames: {}", r.image.frames);
}

/// The same scroll with the content holding still for a moment between the steps: every pause is
/// a frame, however small the shift.
#[test]
fn cap_6_hand_scroll_takes_a_frame_whenever_the_content_holds_still() {
    let page = SyntheticPage::new(400, 8000, 1000).with_bands(30, 0);
    let mut script = Vec::new();
    for i in 0..41u32 {
        script.extend_from_slice(&[i * 20, i * 20, i * 20]);
    }
    let r = hand(&page, script, 200);
    assert_eq!(r.end, EndReason::StoppedByUser);
    assert_eq!(r.image.frame, page.expected_range(0, 800 + 970));
    assert!(r.image.frames >= 30, "frames: {}", r.image.frames);
}

/// A source whose frames in `bad` (counted from 0) are another picture: a window that has just
/// come to the front, a dialog fading out.
struct Glitch<S: FrameSource> {
    inner: S,
    n: u32,
    bad: std::ops::Range<u32>,
}

impl<S: FrameSource> FrameSource for Glitch<S> {
    fn grab(&mut self) -> Result<Frame, CmError> {
        let f = self.inner.grab()?;
        let n = self.n;
        self.n += 1;
        if !self.bad.contains(&n) {
            return Ok(f);
        }
        let data: Vec<u8> = f
            .rgba()
            .chunks(4)
            .flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2], 255])
            .collect();
        Frame::new(f.width(), f.height(), data)
    }
}

fn auto_with(src: &mut dyn FrameSource, sc: &mut dyn Scroller) -> LongShotResult {
    run_auto(
        src,
        sc,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || false,
        &mut |_| {},
    )
    .unwrap()
}

/// QA, Windows, Nov 15, 2026: the very first Auto capture ended after one frame (`no_overlap`),
/// not seen again. A first frame taken while the window was still changing refuses every join:
/// the first frame is taken again after a short wait.
#[test]
fn cap_6_auto_first_frame_taken_while_the_screen_still_changes() {
    let page = SyntheticPage::new(40, 5 * 300, 300);
    let view = page.clone().view();
    let mut src = Glitch {
        inner: view.clone(),
        n: 0,
        bad: 0..2,
    };
    let mut sc = view;
    let r = auto_with(&mut src, &mut sc);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.frame, page.expected());
}

/// The first pair that does not join is looked at again before the run gives up: the picture
/// after the first scroll was another one (a dialog still fading) and the next one joins.
#[test]
fn cap_6_auto_first_pair_is_retried_before_no_overlap() {
    let page = SyntheticPage::new(40, 5 * 300, 300);
    let view = page.clone().view();
    let mut src = Glitch {
        inner: view.clone(),
        n: 0,
        bad: 4..6,
    };
    let mut sc = view;
    let r = auto_with(&mut src, &mut sc);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.frame, page.expected());
}

/// A scroller that ignores the first step (the window was not ready for the wheel yet): the
/// run scrolls once more and goes on instead of ending after one frame.
#[test]
fn cap_6_auto_scroller_that_ignores_the_first_step() {
    struct Late(fake::PageView, u32);
    impl Scroller for Late {
        fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
            self.1 += 1;
            if self.1 == 1 {
                return Ok(());
            }
            self.0.scroll(d, amount_px)
        }
    }
    let page = SyntheticPage::new(40, 5 * 300, 300);
    let view = page.clone().view();
    let mut src = view.clone();
    let mut sc = Late(view, 0);
    let r = auto_with(&mut src, &mut sc);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.frame, page.expected());
    assert!(sc.1 > 1);
}

/// Hand scroll whose first frame was not what stayed on screen: no later frame joins it, but two
/// later ones join each other, and the capture goes on from there instead of ending with one
/// frame (QA, Windows, Nov 15, 2026).
#[test]
fn cap_6_hand_scroll_rebases_on_a_first_frame_that_joins_nothing() {
    let page = SyntheticPage::new(400, 6000, 1000).with_bands(30, 0);
    let script = vec![
        0, 0, 0, // the first frame (another picture, see the glitch)
        200, 200, 200, // refused
        500, 500, 500, // refused too, but it joins the one before
        800, 800, 800,
    ];
    let mut src = Glitch {
        inner: HandScript::new(page.clone(), script),
        n: 0,
        bad: 0..3,
    };
    let mut n = 0;
    let r = run_hand(
        &mut src,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || {
            n += 1;
            n > 30
        },
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(r.end, EndReason::StoppedByUser);
    assert_eq!(r.image.frame, page.expected_range(200, 800 + 970));
}
