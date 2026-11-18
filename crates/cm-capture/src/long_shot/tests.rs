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
    // 0.3.6 P-3: the result says why, in one sentence, and the frames before the jump are kept.
    assert!(r.why.is_some(), "a refused part has a reason");
    assert!(r.image.frames >= 2, "the frames joined so far are kept");
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
    // The run asks `stop` more often than once a frame (after each join, while it looks again at
    // the same place), so how many frames fit in three calls is not the point: what was taken is
    // kept, row for row.
    assert!(r.image.frames >= 2, "{} frames", r.image.frames);
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
    assert_eq!(view.scrolls(), 4, "the step and three retries");
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
    assert_eq!(sc.0, 4, "the step and three retries");
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

/// Real pages, Nov 16, 2026 (Linux Firefox, macOS Chrome): a browser animates every wheel step
/// and the animation looks settled for a grab or two part-way. The distance learned from such a
/// frame was far too short (a third, a half), the next step overshot the overlap and the run
/// ended `no_overlap` after 3 frames. The frame is now looked at again after a longer wait
/// before it is believed, and what arrives late is joined on top.
#[test]
fn cap_6_smooth_scroll_on_every_step_with_pauses_does_not_poison_the_distance() {
    for ratio in [1.0, 1.7, 0.6] {
        for shares in [
            &[0.3, 0.3, 0.55, 0.55, 1.0][..],
            &[0.2, 0.2, 0.2, 0.7, 0.7, 1.0][..],
            &[0.4, 0.4, 1.0][..],
            &[0.5, 0.5, 0.5, 1.0][..],
            &[0.6, 0.6, 1.0][..],
            &[0.7, 0.7, 0.7, 1.0][..],
            &[0.35, 0.55, 0.55, 1.0][..],
        ] {
            let page = SyntheticPage::new(40, 12 * 300, 300);
            let view = page
                .clone()
                .view()
                .with_gain(ratio)
                .with_smooth_every_scroll(shares);
            let (mut src, mut sc) = (view.clone(), view);
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

/// A wheel that moves more for a big request than for a small one (macOS Chrome, Nov 16, 2026:
/// the first 480 px moved 480, a request of 1140 moved about 1355, past the overlap, and every
/// later step needed the scroll-back ladder, which gave up on the third page). A step that
/// needed the ladder teaches the driver to ask for a smaller share of the area.
struct BigStepGain {
    view: fake::PageView,
    threshold: u32,
    big: f64,
    backs: u32,
}

impl Scroller for BigStepGain {
    fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
        let g = if amount_px > self.threshold {
            self.big
        } else {
            1.0
        };
        self.view.scroll(d, (f64::from(amount_px) * g) as u32)
    }
    fn scroll_back(&mut self, d: Direction, amount_px: u32) -> Result<bool, CmError> {
        let _ = d;
        self.backs += 1;
        self.view.back(amount_px);
        Ok(true)
    }
}

#[test]
fn cap_6_a_wheel_that_moves_more_for_big_steps_does_not_need_the_ladder_every_time() {
    for big in [1.25, 1.4, 1.6] {
        let page = SyntheticPage::new(40, 30 * 300, 300);
        let view = page.clone().view();
        let mut src = view.clone();
        let mut sc = BigStepGain {
            view,
            threshold: 120,
            big,
            backs: 0,
        };
        let r = auto_with(&mut src, &mut sc);
        assert_eq!(r.end, EndReason::ReachedEnd, "big {big}");
        assert_eq!(r.image.frame, page.expected(), "big {big}");
        assert!(
            sc.backs <= 4,
            "big {big}: the ladder was needed {} times",
            sc.backs
        );
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

/// A real page, docs.rs in Firefox on Linux (Nov 16, 2026): the browser's toolbar is a fixed band,
/// but the first pair saw blank page rows under it as equal too (a header of 133 rows for a bar of
/// 105). When the page showed text there, every later pair was refused ("a fixed band changed"),
/// and the pair found again from itself had a footer of its own (blank rows at the bottom of the
/// page, equal in both frames), which was refused as a wider band. Both are looked at again with
/// the bands the capture had, and the run reaches the end of the page.
#[test]
fn cap_6_a_header_band_that_reached_into_blank_rows_and_a_blank_page_end_still_join() {
    let chrome = |pos: u32, w: u32, h: u32, data: &mut [u8]| {
        for y in 0..105 {
            for x in 0..w {
                let v = (x * 7 + y * 13) ^ (x >> 2);
                let i = ((y * w + x) * 4) as usize;
                data[i..i + 4].copy_from_slice(&[v as u8, (v >> 3) as u8, 90, 255]);
            }
        }
        // The page's own first rows under the bar are blank at the top of the page.
        let blank = |data: &mut [u8], from: u32, to: u32| {
            for y in from..to {
                for x in 0..w {
                    let i = ((y * w + x) * 4) as usize;
                    data[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        };
        if pos < 400 {
            blank(data, 105, 133);
        }
        // And its last rows are blank at the bottom.
        if pos >= 1200 {
            blank(data, h - 14, h);
        }
    };
    let page = SyntheticPage::new(600, 6 * 600, 600);
    let view = page.clone().view();
    let pos = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut src = Overlaid {
        view: view.clone(),
        pos: pos.clone(),
        over: chrome,
    };
    let mut sc = Tracking { view, pos };
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
    assert!(r.image.frames >= 5, "{} frames", r.image.frames);
    assert!(r.image.frame.height() > 2500, "{}", r.image.frame.height());
}

/// A real page, Wikipedia on a Retina Chrome window (Nov 16, 2026, Linux and macOS: `no_overlap`
/// after one frame): the text matches exactly at the true shift, but a column beside it (the
/// infobox, drawn at a fraction of a pixel; a hover card; a thumbnail preview) differs in every
/// frame, in a third of the width. No per-line tolerance covers a third of a line; the join is
/// found on the columns that hold the text, and only then. A page that jumped past the overlap
/// still ends `no_overlap` (the band matches nothing there).
#[test]
fn cap_6_a_third_of_the_width_that_changes_in_every_frame_does_not_stop_the_join() {
    let over = |_: u32, w: u32, h: u32, data: &mut [u8]| {
        // Noise that depends on where the page is (what the first rows show), so that two
        // grabs of the same place are equal and two places never are.
        let seed: u32 = data[..4 * 64].iter().map(|b| u32::from(*b)).sum();
        for y in 0..h {
            for x in (w * 7 / 10)..w {
                let v = (seed * 31 + y * 17 + x * 5) % 251;
                let i = ((y * w + x) * 4) as usize;
                data[i..i + 4].copy_from_slice(&[v as u8, (v / 2) as u8, 40, 255]);
            }
        }
    };
    for jump in [false, true] {
        let mut page = SyntheticPage::new(800, 8 * 600, 600);
        if jump {
            page = page.with_jump(2600, 1500);
        }
        let view = page.clone().view();
        let pos = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut src = Overlaid {
            view: view.clone(),
            pos: pos.clone(),
            over,
        };
        let mut sc = Tracking { view, pos };
        let r = run_auto(
            &mut src,
            &mut sc,
            &mut NoSleep::default(),
            cfg(Direction::Vertical),
            &mut || false,
            &mut |_| {},
        )
        .unwrap();
        if jump {
            assert_eq!(r.end, EndReason::NoOverlap, "a jump is never joined");
            continue;
        }
        assert_eq!(r.end, EndReason::ReachedEnd);
        let want = page.expected();
        assert_eq!(r.image.frame.height(), want.height());
        for y in 0..want.height() {
            for x in [0, 100, 300] {
                assert_eq!(r.image.frame.pixel(x, y), want.pixel(x, y), "at {x},{y}");
            }
        }
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

/// The old test, kept in spirit: full width, page columns exact, and the sides are the window's
/// own pixels (see `check_side_chrome`).
#[test]
fn cap_6_vertical_static_sidebar_is_kept_not_cut() {
    for (left, right) in [(100u32, 0u32), (0, 90), (100, 90)] {
        check_side_chrome(left, right);
    }
}

/// The chrome of a real window capture around a page with fixed side content (QA macOS pass 4,
/// LS-1): a fixed sidebar of `side` columns that differs in every row and stays while the page
/// scrolls, a grey margin of 4 px at the outer edge (the window's edge and shadow) and, in the
/// last 20 rows of every frame, a rounded corner (grey that grows towards the outer edge). Only
/// the sides that have a sidebar get the margin and the corners. Everything depends on the place
/// in the frame only, like the real thing.
const GREY: [u8; 4] = [200, 200, 200, 255];
const CORNER: u32 = 20;
const MARGIN: u32 = 4;

fn chrome_px(x: u32, y: u32, w: u32, h: u32, left: u32, right: u32) -> Option<[u8; 4]> {
    let (on_left, on_right) = (x < left, x >= w - right);
    if !on_left && !on_right {
        return None;
    }
    let edge = if on_left { x } else { w - 1 - x };
    let corner = h.saturating_sub(y).min(CORNER);
    if edge < MARGIN || (y + CORNER >= h && edge < CORNER - corner + 1) {
        return Some(GREY);
    }
    let v = x.wrapping_mul(7919) ^ y.wrapping_mul(104_729) ^ (y >> 3).wrapping_mul(31);
    Some([v as u8, (v >> 8) as u8, (v >> 16) as u8 | 1, 255])
}

fn draw_chrome(left: u32, right: u32) -> impl FnMut(u32, u32, u32, &mut [u8]) {
    move |_, w, h, data: &mut [u8]| {
        for y in 0..h {
            for x in (0..left).chain(w - right..w) {
                if let Some(p) = chrome_px(x, y, w, h, left, right) {
                    let i = ((y * w + x) * 4) as usize;
                    data[i..i + 4].copy_from_slice(&p);
                }
            }
        }
    }
}

/// Runs Auto scroll over a 240 px wide, 300 px high window of a page with the chrome above and
/// checks the image: the page columns are the page, every row once; the side columns of every
/// output row are the pixels of one row of one frame (found by their own signature), the rows go
/// on one after the other with at most one break per seam (a first-frame-bottom smear would
/// repeat one row), and the rounded corners are only where the last frame ends.
fn check_side_chrome(left: u32, right: u32) {
    let (w, h) = (240u32, 300u32);
    let page = SyntheticPage::new(w, 1500, h);
    let view = page.clone().view();
    let pos = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut src = Overlaid {
        view: view.clone(),
        pos: pos.clone(),
        over: draw_chrome(left, right),
    };
    let mut sc = Tracking { view, pos };
    let r = run_auto(
        &mut src,
        &mut sc,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || false,
        &mut |_| {},
    )
    .unwrap();
    let side = (left, right);
    assert_eq!(r.end, EndReason::ReachedEnd, "{side:?}");
    let want = page.expected();
    let got = &r.image.frame;
    assert_eq!(got.width(), w, "{side:?}: every column of the window");
    assert_eq!(got.height(), want.height(), "{side:?}");
    let sides: Vec<u32> = (0..left).chain(w - right..w).collect();
    // The rows a frame can supply, by their side signature.
    let mut rows = std::collections::HashMap::new();
    for y in 0..h {
        let sig: Vec<[u8; 4]> = sides
            .iter()
            .map(|&x| chrome_px(x, y, w, h, left, right).unwrap())
            .collect();
        assert!(
            rows.insert(sig, y).is_none(),
            "{side:?}: row {y} is not unique"
        );
    }
    let mut prev_y: Option<u32> = None;
    let mut breaks = 0;
    let total = got.height();
    for y in 0..total {
        for x in left..w - right {
            assert_eq!(
                got.pixel(x, y),
                want.pixel(x, y),
                "{side:?} page at {x},{y}"
            );
        }
        let sig: Vec<[u8; 4]> = sides.iter().map(|&x| got.pixel(x, y).unwrap()).collect();
        let Some(&src_y) = rows.get(&sig) else {
            panic!("{side:?}: row {y} of the image is no row of any frame at its sides");
        };
        // The corners belong to the end of the last frame and nowhere else.
        let in_corner_rows = src_y + CORNER >= h;
        assert_eq!(
            in_corner_rows,
            y + CORNER >= total,
            "{side:?}: row {y} comes from frame row {src_y}"
        );
        if prev_y.is_some_and(|p| src_y != p + 1) {
            breaks += 1;
        }
        prev_y = Some(src_y);
    }
    assert!(
        breaks < r.image.frames,
        "{side:?}: {breaks} breaks in the side rows for {} frames (a smeared row has many)",
        r.image.frames
    );
}

/// QA macOS pass 4, LS-1, and the maintainer's own captures (Nov 15, 2026): a window with a fixed
/// LEFT sidebar. Every later frame showed the window's desktop-grey corner and shadow where the
/// sidebar is: the first frame's last row was repeated down the side columns. Now the static
/// columns only mask the comparison and every row is composed from the frame that supplies it.
#[test]
fn ls_1_fixed_left_sidebar_keeps_every_frames_own_side_pixels() {
    check_side_chrome(100, 0);
}

#[test]
fn ls_1_fixed_right_sidebar_keeps_every_frames_own_side_pixels() {
    check_side_chrome(0, 90);
}

#[test]
fn ls_1_fixed_sidebars_on_both_sides_keep_every_frames_own_side_pixels() {
    check_side_chrome(100, 90);
}

/// Window moved or resized during an Auto run (QA macOS pass 4, LS-2): the toast said the page
/// jumped. The frames before are kept; the end says what happened.
struct Changing {
    view: fake::PageView,
    pos: std::rc::Rc<std::cell::Cell<u32>>,
    /// Page position from which the window is different.
    from: u32,
    kind: Change,
}

#[derive(Clone, Copy, PartialEq)]
enum Change {
    /// The frames are narrower (the window was resized).
    Resize,
    /// The frames are as big, but the window is somewhere else (the source says so, and what it
    /// shows is shifted).
    Move,
}

impl FrameSource for Changing {
    fn grab(&mut self) -> Result<Frame, cm_core::CmError> {
        let f = self.view.grab()?;
        if self.pos.get() < self.from {
            return Ok(f);
        }
        let (w, h) = (f.width() as usize, f.height() as usize);
        match self.kind {
            Change::Resize => {
                let mut data = Vec::new();
                for y in 0..h {
                    data.extend_from_slice(&f.rgba()[y * w * 4..(y * w + w - 10) * 4]);
                }
                Frame::new(w as u32 - 10, h as u32, data)
            }
            Change::Move => {
                let mut data = f.rgba().to_vec();
                for y in 0..h {
                    data[y * w * 4..(y + 1) * w * 4].rotate_right(7 * 4);
                }
                Frame::new(w as u32, h as u32, data)
            }
        }
    }

    fn target_changed(&mut self) -> bool {
        self.kind == Change::Move && self.pos.get() >= self.from
    }
}

fn window_changed_run(kind: Change) {
    let page = SyntheticPage::new(240, 1500, 300);
    let view = page.clone().view();
    let pos = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut src = Changing {
        view: view.clone(),
        pos: pos.clone(),
        from: 300,
        kind,
    };
    let mut sc = Tracking { view, pos };
    let r = run_auto(
        &mut src,
        &mut sc,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || false,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(r.end, EndReason::WindowChanged);
    assert_eq!(
        r.end.note(),
        Some("The window was moved or resized"),
        "not the page-jumped note"
    );
    // The frames before the change are kept, row for row.
    assert!(r.image.frames >= 2, "{} frames", r.image.frames);
    let want = page.expected();
    let got = &r.image.frame;
    assert_eq!(got.width(), 240);
    assert!(got.height() > 300 && got.height() < want.height());
    for y in 0..got.height() {
        for x in 0..240 {
            assert_eq!(got.pixel(x, y), want.pixel(x, y), "at {x},{y}");
        }
    }
}

#[test]
fn ls_2_a_resized_window_ends_as_window_changed_and_keeps_the_frames() {
    window_changed_run(Change::Resize);
}

#[test]
fn ls_2_a_moved_window_ends_as_window_changed_and_keeps_the_frames() {
    window_changed_run(Change::Move);
}

/// A page that jumps (no overlap, the window still where it was) is still `no_overlap`.
#[test]
fn ls_2_a_page_that_jumps_is_still_no_overlap() {
    let (r, _) = auto(SyntheticPage::new(240, 3000, 300).with_jump(900, 600), None);
    assert_eq!(r.end, EndReason::NoOverlap);
}

/// LS-4: the note names the limit that was met.
#[test]
fn ls_4_the_limit_note_names_the_limit_that_was_met() {
    // The caller's own max_px.
    let (r, _) = auto(SyntheticPage::new(240, 6000, 300), Some(1000));
    assert_eq!(r.end, EndReason::LimitReached);
    let limits = Limits::with_max_px(Some(1000));
    let note = r.end.note_for(limits, r.image.frame.height()).unwrap();
    assert_eq!(note, "Reached the size limit of 1000 px");
    assert_eq!(limit_kind(limits, 1000), LimitKind::MaxPx);
    // The 50 megapixel rule stops short of the length limit.
    let wide = Limits::default();
    assert_eq!(limit_kind(wide, 12_500), LimitKind::Pixels);
    assert_eq!(
        EndReason::LimitReached.note_for(wide, 12_500).unwrap(),
        "Reached the size limit of 50 megapixels"
    );
    // Only the product limit is "the longest image CaptureMann can edit"; an image a row or two
    // short of it stopped there (Edge, 1048x16383, was called a 50 megapixel stop).
    assert_eq!(limit_kind(wide, MAX_LONG_SIDE), LimitKind::Editor);
    assert_eq!(limit_kind(wide, MAX_LONG_SIDE - 1), LimitKind::Editor);
    assert_eq!(
        limit_kind(Limits::with_max_px(Some(1000)), 999),
        LimitKind::MaxPx
    );
    assert_eq!(
        EndReason::LimitReached
            .note_for(wide, MAX_LONG_SIDE)
            .unwrap(),
        "Reached the longest image CaptureMann can edit"
    );
    // Other endings keep their notes.
    assert_eq!(
        EndReason::NoOverlap.note_for(wide, 100).as_deref(),
        EndReason::NoOverlap.note()
    );
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

/// Windows Terminal, pass 3 (Nov 15, 2026): about 200 lines that differ only in their number
/// (the last digits), a block cursor that blinks at the end of the newest line, a prompt and
/// status strip at the bottom that does not scroll (and has its own blinking cell), and a wheel
/// that moves whole notches, one less now and then. The run ended `no_overlap` at line 111.
struct Term2 {
    pos: u32,
    max: u32,
    grabs: u32,
    scrolls: u32,
    /// Pixels the wheel moves per notch; every `jitter`-th step is one notch short.
    notch: u32,
    jitter: u32,
    footer: u32,
    /// (scroll number, extra notches): that scroll overshoots.
    kick: (u32, u32),
}

impl Term2 {
    const W: u32 = 900;
    const H: u32 = 300;
    const LINE: u32 = 20;

    fn pixel(&self, y: u32, x: u32) -> [u8; 4] {
        let blink = self.grabs.is_multiple_of(2);
        let mut v = 255u8;
        if self.footer > 0 && y >= Self::H - self.footer {
            // Status strip: fixed text, a cell that blinks.
            if (20..300).contains(&x) && (y + x / 7) % 5 < 2 {
                v = 60;
            }
            if (600..610).contains(&x) && blink {
                v = 0;
            }
            return [v, v, v, 255];
        }
        let g = self.pos + y;
        let (line, k) = (g / Self::LINE, g % Self::LINE);
        if (4..14).contains(&k) {
            // The same words on every line.
            if (60..500).contains(&x) && (x / 3 + k).is_multiple_of(3) {
                v = 90;
            }
            // The line number: three digits of 6 x 8 px, each a 5-bit hash of its value.
            for (d, div) in [(0u32, 100u32), (1, 10), (2, 1)] {
                let digit = (line / div) % 10;
                let x0 = 10 + d * 8;
                if (x0..x0 + 6).contains(&x) {
                    let bits = (digit + 1).wrapping_mul(2_654_435_761) >> 7;
                    if (bits >> (((x - x0) + (k - 4)) % 20)) & 1 == 1 {
                        v = 0;
                    }
                }
            }
        }
        // The block cursor at the end of line 100 (it scrolls with the text), blinking.
        if line == 100 && blink && (4..14).contains(&k) && (520..530).contains(&x) {
            v = 0;
        }
        [v, v, v, 255]
    }

    fn frame(&self) -> Frame {
        let mut d = Vec::new();
        for y in 0..Self::H {
            for x in 0..Self::W {
                d.extend_from_slice(&self.pixel(y, x));
            }
        }
        Frame::new(Self::W, Self::H, d).unwrap()
    }
}

struct Term2View(std::rc::Rc<std::cell::RefCell<Term2>>);

impl FrameSource for Term2View {
    fn grab(&mut self) -> Result<Frame, CmError> {
        let mut t = self.0.borrow_mut();
        t.grabs += 1;
        Ok(t.frame())
    }
}

impl Scroller for Term2View {
    fn scroll(&mut self, _: Direction, amount_px: u32) -> Result<(), CmError> {
        let mut t = self.0.borrow_mut();
        t.scrolls += 1;
        let mut notches = ((f64::from(amount_px) / f64::from(t.notch)).round() as u32).max(1);
        if t.kick.0 == t.scrolls {
            notches += t.kick.1;
        }
        if t.jitter > 0 && t.scrolls.is_multiple_of(t.jitter) {
            notches = notches.saturating_sub(1).max(1);
        }
        t.pos = (t.pos + notches * t.notch).min(t.max);
        Ok(())
    }

    fn scroll_back(&mut self, _: Direction, amount_px: u32) -> Result<bool, CmError> {
        let mut t = self.0.borrow_mut();
        let notches = (amount_px / t.notch).max(1);
        t.pos = t.pos.saturating_sub(notches * t.notch);
        Ok(true)
    }
}

fn run_term2(
    footer: u32,
    notch: u32,
    jitter: u32,
    kick: (u32, u32),
    max: u32,
) -> (LongShotResult, Term2) {
    let term = std::rc::Rc::new(std::cell::RefCell::new(Term2 {
        pos: 0,
        max,
        grabs: 0,
        scrolls: 0,
        notch,
        jitter,
        footer,
        kick,
    }));
    let (mut src, mut sc) = (Term2View(term.clone()), Term2View(term.clone()));
    let r = run_auto(
        &mut src,
        &mut sc,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || false,
        &mut |_| {},
    )
    .unwrap();
    let t = Term2 {
        pos: 0,
        max: 0,
        grabs: 1,
        scrolls: 0,
        notch,
        jitter,
        footer,
        kick,
    };
    (r, t)
}

#[test]
fn cap_6_terminal_near_identical_lines_with_a_status_strip_and_blinking_cursors() {
    for (footer, notch, jitter, kick) in [
        (0, 20, 0, (0, 0)),
        (40, 20, 0, (0, 0)),
        (40, 16, 5, (0, 0)),
        (40, 32, 3, (0, 0)),
        // One scroll overshoots by two notches and leaves 10 lines of overlap, too few and too
        // alike to tell where: it is not guessed (a false join or a false end), the run scrolls
        // back by half a step and looks again.
        (40, 20, 0, (7, 2)),
    ] {
        let (r, t) = run_term2(footer, notch, jitter, kick, 4000);
        assert_eq!(
            r.end,
            EndReason::ReachedEnd,
            "footer {footer} notch {notch} jitter {jitter}: {} px",
            r.image.frame.height()
        );
        let got = &r.image.frame;
        assert!(
            got.height() >= 4000,
            "footer {footer} notch {notch} jitter {jitter} kick {kick:?}: {}",
            got.height()
        );
        // The numbers (columns 10..34, clear of both cursors): every line once and in order,
        // down to the status strip.
        let reference = Term2 { footer: 0, ..t };
        for y in 0..got.height() - footer {
            for x in 10..34 {
                assert_eq!(
                    got.pixel(x, y),
                    Some(reference.pixel(y, x)),
                    "footer {footer} notch {notch} jitter {jitter} at {x},{y}"
                );
            }
        }
    }
}

/// Stop pressed while a frame is being joined, and the join fails at the same moment (the frame
/// shows the HUD closing): the run ends `stopped_by_user` and keeps the image (QA Windows pass 3:
/// `no_overlap` in 2 of 3 runs).
#[test]
fn cap_6_stop_while_the_join_fails_ends_as_stopped_by_user() {
    struct Pair {
        n: u32,
        stopped: std::rc::Rc<std::cell::Cell<bool>>,
        page: SyntheticPage,
    }
    // The second scroll's frame is another picture, and Stop is pressed while it is taken.
    impl FrameSource for Pair {
        fn grab(&mut self) -> Result<Frame, CmError> {
            self.n += 1;
            let f = self.page.clone().view().grab()?;
            if self.n >= 14 {
                self.stopped.set(true);
                let d: Vec<u8> = f.rgba().iter().map(|b| b ^ 0x55).collect();
                return Frame::new(f.width(), f.height(), d);
            }
            Ok(f)
        }
    }
    struct Nop;
    impl Scroller for Nop {
        fn scroll(&mut self, _: Direction, _: u32) -> Result<(), CmError> {
            Ok(())
        }
    }
    let stopped = std::rc::Rc::new(std::cell::Cell::new(false));
    let page = SyntheticPage::new(40, 1500, 300);
    let r = run_auto(
        &mut Pair {
            n: 0,
            stopped: stopped.clone(),
            page,
        },
        &mut Nop,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || stopped.get(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(r.end, EndReason::StoppedByUser);
    assert!(r.image.frame.height() > 0);
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

/// LS-seam (QA macOS pass 3, Nov 15, 2026): the window's rounded corners at the bottom of every
/// frame left grey notches at the outer edges at every seam. The lines of the next frame replace
/// the corner lines of the previous one; the first frame's top and the last frame's bottom stay
/// as captured.
#[test]
fn ls_seam_window_corners_leave_no_notch_at_the_seams() {
    let (rows, w) = (20u32, 3u32);
    let page = SyntheticPage::new(300, 5 * 300, 300).with_corner_notch(rows, w);
    let (r, _) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::ReachedEnd);
    let want = page.expected();
    assert_eq!(r.image.frame.height(), want.height());
    assert!(r.image.frames >= 6);
    let width = r.image.frame.width();
    for y in 0..want.height() {
        let edge = [0, 1, width - 2, width - 1];
        let first_top = y < rows;
        let last_bottom = y + rows >= want.height();
        for x in edge {
            if first_top || last_bottom {
                continue;
            }
            assert_eq!(
                r.image.frame.pixel(x, y),
                want.pixel(x, y),
                "notch at {x},{y}"
            );
        }
    }
    // The first frame's top and the last frame's bottom are untouched: still the corners.
    assert_eq!(r.image.frame.pixel(0, 0), Some([128, 128, 128, 255]));
    assert_eq!(
        r.image.frame.pixel(width - 1, want.height() - 1),
        Some([128, 128, 128, 255])
    );
}

/// LS-flaky (a), QA macOS pass 3: a page of identical paragraphs gave `no_overlap` because the
/// wheel moved a little more than asked and the expected step was not close enough to one of the
/// equally good shifts. The shift that is alone near the expected step is the join.
#[test]
fn ls_flaky_repeated_paragraphs_join_by_the_expected_step() {
    // The page ends on a step (108, then 240 each), as the last short scroll of identical
    // paragraphs has no clear shift to be found.
    let page = SyntheticPage::new(40, 408 + 4 * 240, 300).with_period(70);
    let view = page.clone().view().with_gain(1.2);
    let (mut src, mut sc) = (view.clone(), view.clone());
    let r = auto_with(&mut src, &mut sc);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.frame, page.expected());
}

/// ... and two shifts equally near the expected step are still refused: no guess.
#[test]
fn ls_flaky_repeated_paragraphs_still_refused_when_the_step_names_two() {
    let page = SyntheticPage::new(40, 2000, 300).with_period(70);
    let mut s = Stitcher::new(Direction::Vertical, Limits::default());
    s.push(&page.frame_at(0), None);
    // Asked 65, 100 happened: 30, 100 and 170 match and 30 and 100 are as far from 65 (35)
    // as each other, so none stands out: refused.
    assert_eq!(
        s.push(&page.frame_at(100), Some(65)),
        PushOutcome::NoOverlap
    );
}

/// LS-flaky (b): a scroller whose wheel only works at another point of the area (the first
/// point is covered by another window): the run asks for the next point and goes on.
#[test]
fn ls_flaky_first_step_is_retried_at_another_point() {
    struct Points {
        view: fake::PageView,
        at: usize,
        asked: Vec<usize>,
    }
    impl Scroller for Points {
        fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
            self.asked.push(self.at);
            if self.at == 0 {
                return Ok(());
            }
            self.view.scroll(d, amount_px)
        }
        fn next_point(&mut self) -> bool {
            self.at += 1;
            true
        }
    }
    let page = SyntheticPage::new(40, 5 * 300, 300);
    let view = page.clone().view();
    let mut src = view.clone();
    let mut sc = Points {
        view,
        at: 0,
        asked: Vec::new(),
    };
    let r = auto_with(&mut src, &mut sc);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.frame, page.expected());
    assert_eq!(&sc.asked[..3], &[0, 0, 1]);
}

/// LS-flaky (c): a fresh window ignores the first two wheel events; the third (the same point,
/// after a wait, then another point) scrolls. Nothing scrolled is only for a window that never
/// does.
#[test]
fn ls_flaky_fresh_window_that_ignores_two_steps() {
    struct Late(fake::PageView, u32);
    impl Scroller for Late {
        fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
            self.1 += 1;
            if self.1 <= 2 {
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

    // A window that never scrolls still ends with nothing scrolled.
    let mut src = page.view();
    struct Never;
    impl Scroller for Never {
        fn scroll(&mut self, _: Direction, _: u32) -> Result<(), CmError> {
            Ok(())
        }
    }
    let r = auto_with(&mut src, &mut Never);
    assert_eq!(r.end, EndReason::NothingScrolled);
}

// ---- QA macOS pass 6, LS-9: Auto scroll ended after 2 to 8 frames on every page ----

/// The macOS overlay scrollbar shows only while the page scrolls, so the first pair may have none
/// and a later pair a thumb over the outermost columns, in the fixed header and in the content.
/// The edge strip the first pair chose (none) was wrong, every join was refused ("a fixed band
/// changed", "no shift matches") and the run ended `no_overlap` after a few frames.
#[test]
fn ls_9_a_scrollbar_thumb_that_shows_after_the_first_pair_does_not_end_the_run() {
    for (band, thumb_w) in [(0u32, 12u32), (60, 12), (60, 26)] {
        let (w, h) = (300u32, 300u32);
        let page = SyntheticPage::new(w, 1500, h).with_bands(band, 0);
        let view = page.clone().view();
        let pos = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut src = Overlaid {
            view: view.clone(),
            pos: pos.clone(),
            over: move |p: u32, w: u32, h: u32, d: &mut [u8]| {
                // After the first pair: a thumb 100 rows tall that moves with the page, over the
                // rows of the header too, in a colour that differs in every frame.
                if p < 150 {
                    return;
                }
                let top = (p / 8).min(h - 100);
                for y in top..top + 100 {
                    for x in w - 4 - thumb_w..w - 4 {
                        let i = ((y * w + x) * 4) as usize;
                        d[i..i + 4].copy_from_slice(&[(p & 255) as u8, 9, 9, 255]);
                    }
                }
            },
        };
        let mut sc = Tracking { view, pos };
        let r = run_auto(
            &mut src,
            &mut sc,
            &mut NoSleep::default(),
            cfg(Direction::Vertical),
            &mut || false,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(r.end, EndReason::ReachedEnd, "band {band}, thumb {thumb_w}");
        assert_rows_match(
            &r.image.frame,
            &page.expected(),
            &format!("band {band}, thumb {thumb_w}"),
        );
    }
}

/// A wheel that moves 3 times what is asked from the second scroll on (it did not at the first),
/// back and forth alike: the step that was learned is too long, and the scroll back lands exactly
/// on the last frame taken. That is a known position, not the end: the step is tried smaller.
/// (Until Nov 15, 2026 such an identical frame was called `no_overlap` and the run ended.)
#[test]
fn ls_9_a_scroll_back_onto_the_last_frame_is_tried_again_with_a_smaller_step() {
    struct Wild {
        page: SyntheticPage,
        pos: u32,
        scrolls: u32,
    }
    #[derive(Clone)]
    struct WildView(std::rc::Rc<std::cell::RefCell<Wild>>);
    impl FrameSource for WildView {
        fn grab(&mut self) -> Result<Frame, CmError> {
            let w = self.0.borrow();
            Ok(w.page.frame_at(w.pos))
        }
    }
    impl Scroller for WildView {
        fn scroll(&mut self, _: Direction, amount_px: u32) -> Result<(), CmError> {
            let mut w = self.0.borrow_mut();
            w.scrolls += 1;
            let gain = if w.scrolls == 1 { 1 } else { 3 };
            w.pos = (w.pos + amount_px * gain).min(w.page.content_len - w.page.view);
            Ok(())
        }
        fn scroll_back(&mut self, _: Direction, amount_px: u32) -> Result<bool, CmError> {
            let mut w = self.0.borrow_mut();
            w.pos = w.pos.saturating_sub(amount_px * 3);
            Ok(true)
        }
    }
    let page = SyntheticPage::new(40, 1500, 300);
    let v = WildView(std::rc::Rc::new(std::cell::RefCell::new(Wild {
        page: page.clone(),
        pos: 0,
        scrolls: 0,
    })));
    let (mut src, mut sc) = (v.clone(), v);
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
    assert_eq!(r.image.frame, page.expected());
}

/// A frame that is still changing when the settle limit passes (a loaded machine, a browser's
/// smooth scroll) is waited for again before it is compared.
#[test]
fn ls_9_a_frame_that_settles_late_is_waited_for() {
    struct Slow(u32);
    impl FrameSource for Slow {
        fn grab(&mut self) -> Result<Frame, CmError> {
            self.0 += 1;
            let v = self.0.min(9) as u8;
            Frame::new(2, 2, vec![v; 16])
        }
    }
    let mut sleeper = NoSleep::default();
    let early = wait_settled(&mut Slow(0), &mut sleeper, Settle::default()).unwrap();
    assert!(!early.settled, "the plain wait gives up at its limit");
    let late = settle_frame(&mut Slow(0), &mut sleeper, Settle::default()).unwrap();
    assert!(late.settled);
    assert_eq!(late.frame.pixel(0, 0), Some([9, 9, 9, 9]));
}

// ---- Windows QA pass 6: a page with fixed bars on BOTH sides ended `no_overlap` after 1 frame ----

/// One fixed-bar page: bars of `left` and `right` columns (the bar columns hold the same pixels
/// in every frame; with `period` they repeat every that many rows, like a list of identical
/// entries), `shadow` columns next to each bar that blend the bar's shadow with whatever scrolls
/// under it, and a classic scrollbar of `sb` columns at the very edge of the window whose thumb
/// moves with the page (Edge on Windows draws one while the page scrolls).
fn bars_case(w: u32, h: u32, left: u32, right: u32, sb: u32, period: Option<u32>, shadow: u32) {
    let tag = format!("w {w} left {left} right {right} sb {sb} period {period:?} shadow {shadow}");
    let page = SyntheticPage::new(w, 1500, h);
    let view = page.clone().view();
    let pos = std::rc::Rc::new(std::cell::Cell::new(0));
    let bar_end = w - sb;
    let mut src = Overlaid {
        view: view.clone(),
        pos: pos.clone(),
        over: move |p: u32, w: u32, h: u32, d: &mut [u8]| {
            for y in 0..h {
                let yy = period.map_or(y, |n| y % n);
                for x in 0..w {
                    let i = ((y * w + x) * 4) as usize;
                    if x >= bar_end {
                        // The scrollbar: a static track and a thumb that moves.
                        let top = (p / 6) % (h - 80);
                        let on = y >= top && y < top + 80;
                        let v = if on {
                            90u8.wrapping_add((p & 31) as u8)
                        } else {
                            235
                        };
                        d[i..i + 4].copy_from_slice(&[v, v, v, 255]);
                    } else if x < left || x >= bar_end - right {
                        let k = if x < left {
                            x
                        } else {
                            x - (bar_end - right) + 1000
                        };
                        let v = k.wrapping_mul(7919) ^ yy.wrapping_mul(104_729);
                        d[i..i + 4].copy_from_slice(&[
                            v as u8,
                            (v >> 8) as u8,
                            (v >> 16) as u8 | 1,
                            255,
                        ]);
                    } else if (left..left + shadow).contains(&x)
                        || (bar_end - right - shadow..bar_end - right).contains(&x)
                    {
                        // A shadow: the page, darkened (the page's own pixel is part of it).
                        for c in 0..3 {
                            d[i + c] = d[i + c] / 2 + 20;
                        }
                    }
                }
            }
        },
    };
    let mut sc = Tracking { view, pos };
    let r = run_auto(
        &mut src,
        &mut sc,
        &mut NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || false,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(r.end, EndReason::ReachedEnd, "{tag}");
    let want = page.expected();
    let got = &r.image.frame;
    assert_eq!(got.height(), want.height(), "{tag}");
    // The page columns between the bars (and their shadows) are the page, every row once.
    let (x0, x1) = (left + shadow, bar_end - right - shadow);
    for y in 0..got.height() {
        for x in x0..x1 {
            assert_eq!(got.pixel(x, y), want.pixel(x, y), "{tag}: at {x},{y}");
        }
    }
}

/// The real Windows case: bars of 180 and 140 px in a 1082 px window, with the scrollbar at the
/// edge. The scrollbar sits at the outer end of the right bar, so the unbroken run of unchanging
/// columns from the right end was 0 and only the left bar was found; the right bar then made
/// every join fail and the first pair ended the run.
#[test]
fn win_6_fixed_bars_on_both_sides_with_a_scrollbar_join() {
    bars_case(300, 300, 60, 45, 12, None, 0);
}

#[test]
fn win_6_fixed_bar_on_one_side_with_a_scrollbar_joins() {
    bars_case(300, 300, 60, 0, 12, None, 0);
    bars_case(300, 300, 0, 60, 12, None, 0);
    bars_case(300, 300, 0, 60, 0, None, 0);
}

#[test]
fn win_6_bars_that_take_most_of_the_width_join() {
    for (l, r) in [(75u32, 75u32), (100, 100), (110, 90), (140, 0)] {
        bars_case(300, 300, l, r, 12, None, 0);
    }
}

#[test]
fn win_6_bars_with_repeating_rows_join() {
    bars_case(300, 300, 60, 45, 12, Some(20), 0);
    bars_case(300, 300, 60, 45, 0, Some(7), 0);
}

#[test]
fn win_6_bars_with_a_shadow_edge_join() {
    bars_case(300, 300, 60, 45, 12, None, 6);
    bars_case(300, 300, 60, 45, 0, None, 10);
}

// ---- Windows QA pass 6: a classic console window ended `nothing_scrolled` ----

/// A console window that takes no wheel at all (what conhost did for the sub-notch events and the
/// pointer-based wheel) but answers the scroller's other ways: `ways[0]` is the first
/// `last_resort`, `ways[1]` the second. Later `scroll` and `scroll_back` calls use the way that
/// worked.
struct Console {
    view: fake::PageView,
    /// Which `last_resort` call (1-based) makes the console move; 0 never.
    works_at: u32,
    asked: u32,
    /// Calls of `scroll` that were sent by the wheel and ignored.
    ignored: u32,
}

impl Scroller for Console {
    fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
        if self.works_at > 0 && self.asked >= self.works_at {
            return self.view.scroll(d, amount_px);
        }
        self.ignored += 1;
        Ok(())
    }

    fn scroll_back(&mut self, d: Direction, amount_px: u32) -> Result<bool, CmError> {
        self.view.scroll_back(d, amount_px)
    }

    fn last_resort(&mut self, d: Direction, amount_px: u32) -> Result<bool, CmError> {
        if self.asked >= 2 {
            return Ok(false);
        }
        self.asked += 1;
        if self.works_at > 0 && self.asked >= self.works_at {
            self.view.scroll(d, amount_px)?;
        }
        Ok(true)
    }
}

/// The wheel moved nothing at every point: the scroller's next way is tried, and the one that
/// works is used from then on.
#[test]
fn win_6_console_that_ignores_the_wheel_is_scrolled_by_the_next_way() {
    for works_at in [1u32, 2] {
        let page = SyntheticPage::new(40, 5 * 300, 300);
        let view = page.clone().view();
        let mut src = view.clone();
        let mut sc = Console {
            view,
            works_at,
            asked: 0,
            ignored: 0,
        };
        let r = auto_with(&mut src, &mut sc);
        assert_eq!(r.end, EndReason::ReachedEnd, "way {works_at}");
        assert_eq!(r.image.frame, page.expected(), "way {works_at}");
        assert_eq!(
            sc.asked, works_at,
            "way {works_at}: no more ways than needed"
        );
    }
}

/// A target with no other way (every scroller but the Windows console one) still ends
/// `nothing_scrolled`, and every way is tried first.
#[test]
fn win_6_a_window_that_never_scrolls_ends_nothing_scrolled_after_every_way() {
    let page = SyntheticPage::new(40, 5 * 300, 300);
    let view = page.clone().view();
    let mut src = view.clone();
    let mut sc = Console {
        view,
        works_at: 0,
        asked: 0,
        ignored: 0,
    };
    let r = auto_with(&mut src, &mut sc);
    assert_eq!(r.end, EndReason::NothingScrolled);
    assert_eq!(sc.asked, 2);
}

/// CAP-6.AC3 on a page of rows that are nearly alike (Linux QA, Nov 16, 2026: `long-page.html`
/// with `?jump=1`): rows 28 apart differ by four pixels of bar and a few of digits, so a pair
/// of frames that do not overlap at all still "matched" over the one row they could share with
/// the tolerant look, and 28 rows were silently left out. The run must stop at the jump.
#[test]
fn cap_6_ac3_a_jump_on_nearly_alike_rows_is_not_joined() {
    for skip in [1100u32, 1232, 1300, 1500, 2000] {
        let page = SyntheticPage::new(500, 80 * 44, 880)
            .with_ui_rows()
            .with_jump(1500, skip);
        let (r, _) = auto(page.clone(), None);
        assert_eq!(r.end, EndReason::NoOverlap, "skip {skip}");
        let kept = r.image.frame.height();
        assert_eq!(
            r.image.frame,
            page.expected_range(0, kept),
            "skip {skip}: what is kept is a prefix of the page"
        );
    }
}

/// The same page without a jump still joins every row once.
#[test]
fn cap_6_nearly_alike_rows_without_a_jump_join() {
    let page = SyntheticPage::new(500, 80 * 44, 880).with_ui_rows();
    let (r, _) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.frame, page.expected());
}

/// The pair the Linux run met: the page jumped by about two screens between two frames, and
/// rows 28 (or 7) apart look alike. Whatever the distance, the pair does not join.
#[test]
fn cap_6_ac3_nearly_alike_rows_two_screens_apart_do_not_join() {
    let page = SyntheticPage::new(500, 80 * 44, 880).with_ui_rows();
    for d in [1700u32, 1900, 2004, 2020, 2100] {
        let mut st = Stitcher::new(Direction::Vertical, Limits::with_max_px(None));
        st.push(&page.frame_at(0), None);
        assert!(matches!(
            st.push(&page.frame_at(704), Some(704)),
            PushOutcome::Added { .. }
        ));
        assert_eq!(
            st.push(&page.frame_at(704 + d), Some(704)),
            PushOutcome::NoOverlap,
            "d {d}"
        );
    }
}

/// A real marketing site (https://www.carchargeos.com, Linux, Nov 16, 2026: `no_overlap` after
/// the first frame): a sticky translucent header that is opaque in the first frame and frosted
/// over the scrolled page in every later one, a scrollbar, a floating scroll-to-top button, and
/// the browser's own toolbar above. The header's rows differ between any two frames; the rest of
/// the overlap is exact.
fn frosted_page() -> SyntheticPage {
    SyntheticPage::new(800, 6 * 1100, 1100)
        .with_bands(40, 0)
        .with_scrollbar(6)
        .with_frosted_header(70)
        .with_floating_button(40, 48)
}

#[test]
fn cap_6_a_translucent_sticky_header_with_a_floating_button_joins() {
    let page = frosted_page();
    let (r, _) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.header_len, 40);
    // Every content row once and in order (columns the button and the scrollbar never touch).
    assert_rows_match(&r.image.frame, &page.expected(), "frosted");
}

#[test]
fn cap_6_a_translucent_sticky_header_alone_joins() {
    let page = SyntheticPage::new(800, 5 * 700, 700).with_frosted_header(70);
    let (r, _) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_rows_match(&r.image.frame, &page.expected(), "frosted alone");
}

#[test]
fn cap_6_a_floating_button_alone_joins() {
    let page = SyntheticPage::new(800, 5 * 700, 700).with_floating_button(50, 48);
    let (r, _) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_rows_match(&r.image.frame, &page.expected(), "button alone");
}

/// The same page that jumps still stops, with an exact prefix kept.
#[test]
fn cap_6_a_translucent_header_page_that_jumps_still_ends_no_overlap() {
    let page = frosted_page().with_jump(1500, 900);
    let (r, _) = auto(page.clone(), None);
    assert_eq!(r.end, EndReason::NoOverlap);
    let kept = r.image.frame.height();
    assert!(kept > 700 && kept < 2600, "kept {kept}");
    assert_rows_match(&r.image.frame, &page.expected_range(0, kept - 40), "prefix");
}

/// Linux QA, Nov 16, 2026: a horizontal Long Shot of a page whose every row repeats numbered cells
/// (c001, c002, ...) every 194 px: shifts that are multiples of the cell width match except for
/// the label. The image was longer than the page (content repeated) and the run ended
/// `no_overlap`.
#[test]
fn ls_b_horizontal_cells_that_repeat_every_194_px_are_joined_at_the_true_shift() {
    for gain in [1.0, 0.6, 1.4] {
        let page = SyntheticPage::new(520, 8 * 1000, 1000)
            .horizontal()
            .with_cells(194);
        let view = page.clone().view().with_gain(gain);
        let (mut src, mut sc) = (view.clone(), view);
        let r = run_auto(
            &mut src,
            &mut sc,
            &mut NoSleep::default(),
            cfg(Direction::Horizontal),
            &mut || false,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(r.end, EndReason::ReachedEnd, "gain {gain}");
        assert_eq!(r.image.frame, page.expected(), "gain {gain}");
    }
}

/// QA macOS pass 7 (LS-13): on real pages the first scroll moved the page far from what was asked
/// (520 to 700 px of the step asked), and the first join was refused. The join is searched at
/// every shift, so a wheel that moves less or more than asked must join on a page with a
/// translucent header and a floating button too.
#[test]
fn ls_13_a_wheel_that_moves_far_less_or_more_than_asked_on_a_translucent_header_page() {
    for gain in [0.3, 0.45, 0.7, 1.9] {
        let page = frosted_page();
        let view = page.clone().view().with_gain(gain);
        let (mut src, mut sc) = (view.clone(), view);
        let r = run_auto(
            &mut src,
            &mut sc,
            &mut NoSleep::default(),
            cfg(Direction::Vertical),
            &mut || false,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(r.end, EndReason::ReachedEnd, "gain {gain}");
        assert_rows_match(&r.image.frame, &page.expected(), "gain");
    }
}

/// Linux Firefox, Wikipedia and YouTube (QA, Nov 16, 2026): the run ended `no_overlap` mid-page at
/// the same place every time, while a fresh stitcher joined the same two frames. The run kept the
/// mask of its first pair (none: the whole width joined then), and a contents list that the page
/// pinned later made static columns that no shift matches. A pair that does not join is now looked
/// at again with the side columns that stay put in that pair left out of the comparison.
#[test]
fn ls_fix_side_columns_that_became_fixed_after_the_first_pair_are_masked_later() {
    let page = SyntheticPage::new(240, 1500, 300);
    // A pinned list of 60 columns, ink in 6 of every 40 rows and white between, the same in every
    // frame.
    let with_list = |pos: u32| {
        let f = page.frame_at(pos);
        let (w, h) = (f.width(), f.height());
        let mut data = f.rgba().to_vec();
        for y in 0..h {
            for x in 0..60 {
                let v = (x * 31 + y * 17) as u8;
                let i = ((y * w + x) * 4) as usize;
                let px = if y % 40 < 6 {
                    [v, 40, v ^ 0x55, 255]
                } else {
                    [255, 255, 255, 255]
                };
                data[i..i + 4].copy_from_slice(&px);
            }
        }
        Frame::new(w, h, data).unwrap()
    };
    let mut s = Stitcher::new(Direction::Vertical, Limits::default());
    s.push(&with_list(0), None);
    // As after a first pair that joined on the whole width.
    s.assume_whole_width();
    let mut pos = 0;
    for _ in 0..4 {
        pos += 210;
        let o = s.push(&with_list(pos), Some(210));
        assert!(
            matches!(o, PushOutcome::Added { .. }),
            "pos {pos}: {o:?}, {:?}",
            s.last_refusal()
        );
    }
    let out = s.finish().unwrap();
    assert_eq!(out.frame.width(), 240, "no column is cut");
    assert_eq!(s.frames(), 5);
}

/// LS-12 (QA macOS pass 7): four of the five points of the area were covered by other windows and
/// the run gave up after the third try. Every point the scroller offers is tried.
#[test]
fn ls_12_the_last_of_five_points_is_tried_before_nothing_scrolled() {
    struct Points {
        view: fake::PageView,
        at: usize,
        points: usize,
    }
    impl Scroller for Points {
        fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
            if self.at < self.points - 1 {
                return Ok(());
            }
            self.view.scroll(d, amount_px)
        }
        fn next_point(&mut self) -> bool {
            if self.at + 1 >= self.points {
                return false;
            }
            self.at += 1;
            true
        }
    }
    let page = SyntheticPage::new(40, 5 * 300, 300);
    let view = page.clone().view();
    let mut src = view.clone();
    let mut sc = Points {
        view,
        at: 0,
        points: 5,
    };
    let r = auto_with(&mut src, &mut sc);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.frame, page.expected());
}

/// A scroller that can also scroll back, over a [`fake::PageView`].
struct Backable(fake::PageView);

impl Scroller for Backable {
    fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
        self.0.scroll(d, amount_px)
    }
    fn scroll_back(&mut self, _d: Direction, amount_px: u32) -> Result<bool, CmError> {
        self.0.back(amount_px);
        Ok(true)
    }
}

/// LS-15 (QA macOS pass 7): a run that starts on a page already at its end said `nothing_scrolled`
/// and kept one frame. It goes to the top first (once) and captures the whole page.
#[test]
fn ls_15_a_page_already_at_its_end_is_scrolled_to_the_top_first() {
    let page = SyntheticPage::new(40, 5 * 300, 300);
    let view = page.clone().view();
    view.clone().scroll(Direction::Vertical, 100_000).unwrap();
    let mut src = view.clone();
    let mut sc = Backable(view);
    let r = auto_with(&mut src, &mut sc);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.frame, page.expected());
}

/// ... and a page that scrolls neither way is still `nothing_scrolled`.
#[test]
fn ls_15_a_page_that_does_not_scroll_either_way_is_nothing_scrolled() {
    struct Stuck;
    impl Scroller for Stuck {
        fn scroll(&mut self, _d: Direction, _amount_px: u32) -> Result<(), CmError> {
            Ok(())
        }
        fn scroll_back(&mut self, _d: Direction, _amount_px: u32) -> Result<bool, CmError> {
            Ok(true)
        }
    }
    let page = SyntheticPage::new(40, 5 * 300, 300);
    let mut src = page.view();
    let r = auto_with(&mut src, &mut Stuck);
    assert_eq!(r.end, EndReason::NothingScrolled);
}

/// LS-14 (QA macOS pass 7): the target window was closed during the run: `internal` "Failed to
/// copy data" and no frames. The run ends `window_closed` and keeps the frames it has.
#[test]
fn ls_14_a_window_closed_during_the_run_ends_it_and_keeps_the_frames() {
    struct Dies {
        view: fake::PageView,
        grabs: u32,
        error: fn() -> CmError,
        says_gone: bool,
    }
    impl FrameSource for Dies {
        fn grab(&mut self) -> Result<Frame, CmError> {
            self.grabs += 1;
            if self.grabs > 14 {
                return Err((self.error)());
            }
            self.view.grab()
        }
        fn target_gone(&mut self) -> bool {
            self.says_gone
        }
    }
    let cases: [(fn() -> CmError, bool); 2] = [
        (|| CmError::not_found("Window 7 not found"), false),
        (|| CmError::internal("Failed to copy data"), true),
    ];
    for (error, says_gone) in cases {
        let page = SyntheticPage::new(40, 12 * 300, 300);
        let view = page.view();
        let mut src = Dies {
            view: view.clone(),
            grabs: 0,
            error,
            says_gone,
        };
        let r = auto_with(&mut src, &mut Backable(view));
        assert_eq!(r.end, EndReason::WindowClosed, "{says_gone}");
        assert!(r.image.frames >= 2, "frames kept: {}", r.image.frames);
    }
    // An error that is not a closed window still fails the run.
    let page = SyntheticPage::new(40, 12 * 300, 300);
    let view = page.view();
    let mut src = Dies {
        view: view.clone(),
        grabs: 0,
        error: || CmError::internal("boom"),
        says_gone: false,
    };
    let r = run_auto(
        &mut src,
        &mut Backable(view),
        &mut fake::NoSleep::default(),
        cfg(Direction::Vertical),
        &mut || false,
        &mut |_| {},
    );
    assert!(r.is_err());
}

/// A heading that reveals itself (QA macOS, carchargeos.com: "IT approves it. Marketing launches"
/// had its top half missing at a seam): for the first three grabs after every scroll a band of the
/// viewport is blank, which looks still (two equal frames) before the page draws it. The frames
/// are looked at again and the rows they supplied replaced.
#[test]
fn ls_seam_a_band_that_is_drawn_late_is_in_the_image() {
    struct Reveal {
        view: fake::PageView,
        since: std::rc::Rc<std::cell::Cell<u32>>,
    }
    impl FrameSource for Reveal {
        fn grab(&mut self) -> Result<Frame, CmError> {
            self.since.set(self.since.get() + 1);
            let f = self.view.grab()?;
            if self.since.get() > 3 {
                return Ok(f);
            }
            let (w, h) = (f.width(), f.height());
            let mut data = f.rgba().to_vec();
            for y in 120..160.min(h) {
                for x in 60..240.min(w) {
                    let i = ((y * w + x) * 4) as usize;
                    data[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
            Frame::new(w, h, data)
        }
    }
    struct Resets {
        view: fake::PageView,
        since: std::rc::Rc<std::cell::Cell<u32>>,
    }
    impl Scroller for Resets {
        fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
            self.since.set(0);
            self.view.scroll(d, amount_px)
        }
    }
    let page = SyntheticPage::new(300, 6 * 300, 300);
    let view = page.clone().view();
    let since = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut src = Reveal {
        view: view.clone(),
        since: since.clone(),
    };
    let mut sc = Resets { view, since };
    let r = auto_with(&mut src, &mut sc);
    assert_eq!(r.end, EndReason::ReachedEnd);
    assert_eq!(r.image.frame, page.expected());
}

// ---- Real sites, Windows Edge, Nov 16, 2026 (lsreal): one frame, `no_overlap`, on every page ----

/// The page position a [`Overlaid`] source is told by [`FirstBig`].
type PosCell = std::rc::Rc<std::cell::Cell<u32>>;

/// A wheel whose first step moves `first` times what was asked (Edge on Windows at 150 %: a
/// request of 234 px moved 600) and later ones `later` times, and that scrolls back by the same
/// gain as it scrolls forward, in whole notches of `notch` px at least.
struct FirstBig {
    view: fake::PageView,
    pos: PosCell,
    first: f64,
    later: f64,
    scrolls: u32,
    notch: u32,
    backs: u32,
}

impl FirstBig {
    fn moved(&self, amount_px: u32, gain: f64) -> u32 {
        (((f64::from(amount_px) * gain) / f64::from(self.notch)).round() as u32).max(1) * self.notch
    }
}

impl Scroller for FirstBig {
    fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
        self.scrolls += 1;
        let gain = if self.scrolls == 1 {
            self.first
        } else {
            self.later
        };
        let by = self.moved(amount_px, gain);
        self.view.scroll(d, by)?;
        self.pos.set(self.pos.get() + by);
        Ok(())
    }
    fn scroll_back(&mut self, _: Direction, amount_px: u32) -> Result<bool, CmError> {
        self.backs += 1;
        let by = self.moved(amount_px, self.later);
        self.view.back(by);
        self.pos.set(self.pos.get().saturating_sub(by));
        Ok(true)
    }
}

/// The first pair: the wheel moved the page 2.5 times as far as asked, so only a sliver of the
/// first frame is left, and the lower half of the browser's toolbar (a focus ring on a button)
/// changed with the scroll. The pair does not join: the overlap is too short to rest a join on
/// (the ring's rows differ in it). The driver scrolls back, as it does for a later pair, and the
/// join is found at the shift that leaves an overlap. (Windows, Edge, Wikipedia, YouTube,
/// docs.rs, paulgraham.com: `no_overlap` after the first frame, 3 of 3 runs, with a pair 600 px
/// apart in a 708 px content area.)
#[test]
fn lsreal_a_first_step_that_overshoots_is_scrolled_back_and_joined() {
    let ring = |pos: u32, w: u32, _h: u32, data: &mut [u8]| {
        if pos > 0 {
            for y in 20..40 {
                for x in (w / 2)..(w / 2 + 40) {
                    let i = ((y * w + x) * 4) as usize;
                    data[i..i + 4].copy_from_slice(&[30, 30, 120, 255]);
                }
            }
        }
    };
    for first in [2.0, 2.5, 3.0] {
        let page = SyntheticPage::new(300, 3000, 300).with_bands(40, 0);
        let view = page.clone().view();
        let pos = PosCell::default();
        let mut src = Overlaid {
            view: view.clone(),
            pos: pos.clone(),
            over: ring,
        };
        let mut sc = FirstBig {
            view,
            pos,
            first,
            later: 1.5,
            scrolls: 0,
            notch: 15,
            backs: 0,
        };
        let r = auto_with(&mut src, &mut sc);
        assert_eq!(r.end, EndReason::ReachedEnd, "first {first}");
        assert_eq!(r.image.frame, page.expected(), "first {first}");
        // The rescue of the first pair taught that the wheel moves more than asked, so the
        // steps after it do not overshoot again.
        assert!(sc.backs <= 3, "first {first}: {} scroll backs", sc.backs);
    }
}

/// BUG-6 (Windows, Edge at 150 %, YouTube): the wheel moves the page three times as far as asked,
/// and a sticky masthead with a tab bar covers a quarter of the window, so a step of 30 % of the
/// window already passes the last row of the first frame. The first pair is scrolled back and
/// joined, the next steps are sized for the wheel, and the page is taken to its end.
#[test]
fn bug_6_a_tall_sticky_header_and_a_wheel_of_three_times_the_request_joins_to_the_end() {
    for header in [160u32, 275] {
        let page = SyntheticPage::new(300, 6000, 1100).with_bands(header, 0);
        let view = page.clone().view();
        let pos = PosCell::default();
        let mut src = Overlaid {
            view: view.clone(),
            pos: pos.clone(),
            over: |_: u32, _: u32, _: u32, _: &mut [u8]| {},
        };
        let mut sc = FirstBig {
            view,
            pos,
            first: 3.0,
            later: 3.0,
            scrolls: 0,
            notch: 15,
            backs: 0,
        };
        let r = auto_with(&mut src, &mut sc);
        assert_eq!(r.end, EndReason::ReachedEnd, "header {header}");
        assert_eq!(r.image.frame, page.expected(), "header {header}");
        assert!(sc.backs <= 3, "header {header}: {} scroll backs", sc.backs);
    }
}

/// The same panels with the contents list's current item highlighted: the item changes with the
/// scroll position (Wikipedia on Windows, Edge, Nov 17, 2026: after the wheel took larger steps
/// the pair had "History" bold and "(Top)" blue, a few percent of the lines differed in the
/// panel's columns beyond the first one, the run of unchanging columns from the edge was cut
/// there and the panel's other columns spoiled every shift, `no_overlap` after 12 frames while
/// the page's columns matched at the true shift in 2 rows per mille).
#[test]
fn bug_6_a_panel_whose_current_item_changes_is_left_out_of_the_comparison() {
    let page = SyntheticPage::new(420, 4000, 300).with_scrollbar(14);
    let framed = |pos: u32| {
        let f = page.frame_at(pos);
        let (w, h) = (f.width(), f.height());
        let mut data = f.rgba().to_vec();
        for y in 0..h {
            for x in (0..90).chain(w - 100..w - 14) {
                let v = ((y / 20 * 37 + y * 29 + x * 11) % 200) as u8;
                let i = ((y * w + x) * 4) as usize;
                let mut px = if y % 20 < 10 {
                    [40 + v / 2, 40, 90 + v / 2, 255]
                } else {
                    [250, 250, 250, 255]
                };
                // The current item: 14 lines (4.7 % of 300) that change with the position.
                if (100..114).contains(&y) && (20..90).contains(&x) {
                    px = [
                        200 - (pos / 200 % 5) as u8 * 30,
                        40 + (pos / 200 % 5) as u8 * 20,
                        30,
                        255,
                    ];
                }
                data[i..i + 4].copy_from_slice(&px);
            }
        }
        Frame::new(w, h, data).unwrap()
    };
    let mut s = Stitcher::new(Direction::Vertical, Limits::default());
    s.push(&framed(1000), None);
    s.assume_trim(0, 14);
    for (pos, new_len) in [(1200u32, 200u32), (1410, 210), (1600, 190)] {
        let got = s.push(&framed(pos), Some(new_len));
        assert_eq!(
            got,
            PushOutcome::Added { new_len },
            "pos {pos}: {:?}",
            s.last_refusal()
        );
    }
}

/// A pair with little left in common: a translucent sticky header over the top of the next frame,
/// and, at the bottom of the first, content that had not been drawn yet (a card that reveals
/// itself as it scrolls into view), so that only a short stretch of the overlap matches. A real
/// page, Windows Edge, carchargeos.com, Nov 16, 2026: 85 matching rows of 354, below the 40 inked
/// lines a join needs. The shift the step names is believed on 20 lines when it is the only one
/// that fits near the step; a step that names another place is not.
#[test]
fn lsreal_a_short_match_is_believed_only_at_the_shift_the_step_names() {
    let page = SyntheticPage::new(300, 3000, 400)
        .with_bands(30, 0)
        .with_frosted_header(60);
    let first = {
        let f = page.frame_at(0);
        let (w, h) = (f.width() as usize, f.height() as usize);
        let mut data = f.rgba().to_vec();
        // Rows 320 to 400 not drawn yet: one flat colour.
        for y in 320..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                data[i..i + 4].copy_from_slice(&[250, 250, 252, 255]);
            }
        }
        Frame::new(w as u32, h as u32, data).unwrap()
    };
    let next = page.frame_at(200);
    let mut s = Stitcher::new(Direction::Vertical, Limits::default());
    s.push(&first, None);
    let got = s.push(&next, Some(200));
    assert_eq!(
        got,
        PushOutcome::Added { new_len: 200 },
        "{:?}",
        s.last_refusal()
    );
    // The same pair with a step that names another place is never believed.
    for wrong in [60, 380] {
        let mut s = Stitcher::new(Direction::Vertical, Limits::default());
        s.push(&first, None);
        assert_eq!(
            s.push(&next, Some(wrong)),
            PushOutcome::NoOverlap,
            "{wrong}"
        );
    }
}

/// A page of short paragraphs in a narrow column with wide blank gaps (paulgraham.com, Windows
/// Edge, Nov 16, 2026): two frames 300 px apart differ in a few stretches of rows and in the
/// columns of the text only, which is also what a hover card or a status bar looks like at one
/// scroll position. The pair was called "the same frame", the run ended `reached_end` after 12
/// frames of a page of more than 16,000 px, and nothing said that the end was not the end. A
/// pair whose rows mostly differ because the inked rows moved is a scroll, not a region.
#[test]
fn lsreal_a_sparse_text_page_that_scrolled_is_not_the_same_frame() {
    let (w, h) = (600usize, 300usize);
    let row = |y: usize| -> Vec<u8> {
        let mut line = vec![255u8; w * 4];
        if y % 200 < 60 {
            for x in 100..340 {
                let v = ((y * 31 + x * 17) ^ (x >> 1)) % 251;
                let i = x * 4;
                line[i..i + 4].copy_from_slice(&[v as u8, (v / 2) as u8, 20, 255]);
            }
        }
        line
    };
    // A floating button in the bottom right corner whose pixels follow the position (a shadow
    // over what scrolls under it): it keeps the exact look from matching.
    let frame_at = |pos: usize| {
        let mut data: Vec<u8> = (0..h).flat_map(|i| row(pos + i)).collect();
        for y in 262..h {
            for x in 540..590 {
                let i = (y * w + x) * 4;
                let v = ((pos * 7 + y * 3 + x) % 200) as u8;
                data[i..i + 4].copy_from_slice(&[v, v, 200, 255]);
            }
        }
        Frame::new(w as u32, h as u32, data).unwrap()
    };
    let mut s = Stitcher::new(Direction::Vertical, Limits::default());
    s.push(&frame_at(0), None);
    let got = s.push(&frame_at(200), Some(200));
    assert_eq!(
        got,
        PushOutcome::Added { new_len: 200 },
        "{:?}",
        s.last_refusal()
    );
    // The same position with a status bar drawn over the bottom rows is still the same frame.
    let mut bar = frame_at(0).rgba().to_vec();
    for y in 280..300 {
        for x in 0..200 {
            let i = (y * w + x) * 4;
            bar[i..i + 4].copy_from_slice(&[10, 10, 10, 255]);
        }
    }
    let mut s = Stitcher::new(Direction::Vertical, Limits::default());
    s.push(&frame_at(0), None);
    let bar = Frame::new(w as u32, h as u32, bar).unwrap();
    assert_eq!(s.push(&bar, Some(200)), PushOutcome::NoChange);
}

/// A wheel event that is lost in the middle of a page (a step that moved nothing) is not the end
/// of the content: the end is believed after one more step. (Windows, Edge, paulgraham.com,
/// Nov 16, 2026: `reached_end` after 12 frames of a page of more than 16,000 px.)
#[test]
fn lsreal_one_lost_wheel_step_is_not_the_end_of_the_page() {
    struct Lossy {
        view: fake::PageView,
        calls: u32,
        lose: u32,
    }
    impl Scroller for Lossy {
        fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
            self.calls += 1;
            if self.calls == self.lose {
                return Ok(());
            }
            self.view.scroll(d, amount_px)
        }
    }
    for lose in [3, 5, 8] {
        let page = SyntheticPage::new(40, 12 * 300, 300);
        let view = page.clone().view();
        let mut src = view.clone();
        let mut sc = Lossy {
            view,
            calls: 0,
            lose,
        };
        let r = auto_with(&mut src, &mut sc);
        assert_eq!(r.end, EndReason::ReachedEnd, "lose {lose}");
        assert_eq!(r.image.frame, page.expected(), "lose {lose}");
    }
}

/// As above, with the text lines of the card that was still drawing separated by blank lines that
/// match (a card's text lines are not one block): the blank lines between do not end the
/// stretch of lines that differ.
#[test]
fn lsreal_lines_that_differ_with_blank_lines_between_them_are_one_stretch() {
    let page = SyntheticPage::new(300, 3000, 400)
        .with_bands(30, 0)
        .with_frosted_header(60);
    let flat = [250u8, 250, 252, 255];
    let first = {
        let f = page.frame_at(0);
        let (w, h) = (f.width() as usize, f.height() as usize);
        let mut data = f.rgba().to_vec();
        for y in 320..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                data[i..i + 4].copy_from_slice(&flat);
            }
        }
        Frame::new(w as u32, h as u32, data).unwrap()
    };
    let next = {
        let f = page.frame_at(200);
        let (w, h) = (f.width() as usize, f.height() as usize);
        let mut data = f.rgba().to_vec();
        // Where the first frame has nothing yet, the card's text lines: three lines of text,
        // three blank ones (the same as the first frame's), and so on.
        for y in 120..h {
            if (y / 3) % 2 == 0 {
                for x in 0..w {
                    let i = (y * w + x) * 4;
                    data[i..i + 4].copy_from_slice(&flat);
                }
            }
        }
        Frame::new(w as u32, h as u32, data).unwrap()
    };
    let mut s = Stitcher::new(Direction::Vertical, Limits::default());
    s.push(&first, None);
    let got = s.push(&next, Some(200));
    assert_eq!(
        got,
        PushOutcome::Added { new_len: 200 },
        "{:?}",
        s.last_refusal()
    );
}

/// A page of cards (vertical borders on every line, short text blocks every 25 lines) whose next
/// frame has a translucent header over its top and whose first frame had not drawn the bottom
/// cards yet. The borders match at every shift, so on a few text lines the join cannot be told
/// from its neighbours by the usual look. When the wheel has moved the content by exactly 250 px at
/// each of the last steps, the shift 250 is believed on the lines of its own; a distance the
/// content did not move by is not (the borders match there as well). A real page, Windows Edge,
/// carchargeos.com, Nov 16, 2026: 7 lines of its own, the wheel's notches 300 px each time.
#[test]
fn lsreal_a_steady_wheel_names_the_shift_of_a_pair_with_little_in_common() {
    let (w, h) = (600usize, 400usize);
    let frame_at = |pos: usize, unrevealed_from: usize| {
        let mut data = Vec::with_capacity(w * h * 4);
        for i in 0..h {
            let y = pos + i;
            let mut line = vec![250u8; w * 4];
            // Card borders: the same on every line.
            for x in [50usize, 51, 548, 549] {
                line[x * 4..x * 4 + 4].copy_from_slice(&[120, 120, 130, 255]);
            }
            // Text lines of a card, every 25 lines, four lines tall.
            if y % 25 < 4 && i < unrevealed_from {
                for x in 100..400 {
                    let v = ((y * 31 + x * 17) ^ (x >> 1)) % 251;
                    line[x * 4..x * 4 + 4].copy_from_slice(&[v as u8, (v / 2) as u8, 20, 255]);
                }
            }
            // A translucent header over the top of every later frame: another look each time.
            if pos > 0 && i < 40 {
                for x in 0..w {
                    let v = ((pos * 13 + i * 7 + x * 3) % 90) as u8;
                    line[x * 4..x * 4 + 4].copy_from_slice(&[160 + v, 160 + v, 170, 255]);
                }
            }
            // A floating button at the bottom right whose shadow follows the position.
            if i >= h - 30 {
                for x in 520..580 {
                    let v = ((pos * 5 + i * 3 + x) % 100) as u8;
                    line[x * 4..x * 4 + 4].copy_from_slice(&[30, 60 + v, 200, 255]);
                }
            }
            data.extend_from_slice(&line);
        }
        Frame::new(w as u32, h as u32, data).unwrap()
    };
    let first = frame_at(0, 330);
    let next = frame_at(250, h);
    for (distance, joins) in [(Some(250), true), (Some(180), false), (None, false)] {
        let mut s = Stitcher::new(Direction::Vertical, Limits::default());
        s.push(&first, None);
        s.trust_hint(distance);
        let got = s.push(&next, Some(250));
        if joins {
            assert_eq!(
                got,
                PushOutcome::Added { new_len: 250 },
                "{:?}",
                s.last_refusal()
            );
        } else {
            assert_eq!(got, PushOutcome::NoOverlap, "{distance:?}");
        }
    }
}

/// Side panels that stay put (Wikipedia's contents list and appearance panel, Linux Firefox, Nov 16,
/// 2026: `no_overlap` at 7 to 10 frames, the panels' text at other rows in every shifted overlap).
/// The run trimmed the scrollbar's edge columns after its first pair, which joined without
/// the panels being in the way; a later pair has them, with list items and gaps between them (many
/// stretches of lines that differ), and the trimmed run refused it. The columns of the panels are
/// left out of the comparison only; the rows are composed from the trimmed frames as before, so
/// the image keeps its width, and the columns between the panels are the page row for row.
#[test]
fn lsreal_panels_that_stay_put_are_left_out_of_the_comparison_of_a_trimmed_run() {
    let page = SyntheticPage::new(420, 4000, 300).with_scrollbar(14);
    let framed = |pos: u32| {
        let f = page.frame_at(pos);
        let (w, h) = (f.width(), f.height());
        let mut data = f.rgba().to_vec();
        for y in 0..h {
            // List items ten lines tall with ten blank lines between them, the same in every
            // frame (fixed to the window).
            for x in (0..90).chain(w - 100..w - 14) {
                let v = ((y / 20 * 37 + y * 29 + x * 11) % 200) as u8;
                let i = ((y * w + x) * 4) as usize;
                let px = if y % 20 < 10 {
                    [40 + v / 2, 40, 90 + v / 2, 255]
                } else {
                    [250, 250, 250, 255]
                };
                data[i..i + 4].copy_from_slice(&px);
            }
        }
        Frame::new(w, h, data).unwrap()
    };
    let mut s = Stitcher::new(Direction::Vertical, Limits::default());
    s.push(&framed(1000), None);
    s.assume_trim(0, 14);
    let got = s.push(&framed(1200), Some(200));
    assert_eq!(
        got,
        PushOutcome::Added { new_len: 200 },
        "{:?}",
        s.last_refusal()
    );
    let got = s.push(&framed(1410), Some(200));
    assert_eq!(
        got,
        PushOutcome::Added { new_len: 210 },
        "{:?}",
        s.last_refusal()
    );
    let image = s.finish().unwrap().frame;
    let want = page.expected_range(1000, 1000 + 300 + 200 + 210);
    assert_eq!(image.height(), want.height());
    for y in 0..image.height() {
        for x in 100..(want.width() - 114) {
            assert_eq!(image.pixel(x, y), want.pixel(x, y), "({x}, {y})");
        }
    }
}
