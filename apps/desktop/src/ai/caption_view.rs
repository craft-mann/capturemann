//! The display-free decisions of the Captions mode (R3c): how wide the inspector gets, when the
//! list follows the playhead, where the blocks of the captions lane go, and when "play this line"
//! stops. The drawing is in `ui/recording_editor/ai_cards.rs` and `timeline.rs`.

use std::time::Duration;

use cm_ai::caption::CaptionLine;
use cm_ai::doubtful::{line_text, step, Doubtful};
use cm_core::MediaTime;

use super::captions::{caret_char_index, issues, plain};
use crate::theme::scale;

// ---- the width of the inspector ---------------------------------------------------------

/// The inspector (tab rail, gap and panel card) on every tab but Captions, px.
pub const INSPECTOR_W: f32 = 400.0;
/// The least the inspector shrinks to before the preview does, px.
pub const INSPECTOR_MIN_W: f32 = 340.0;
/// The inspector in Captions mode, px (board R3c).
pub const CAPTIONS_W: f32 = 600.0;
/// The preview column keeps at least this much in Captions mode, px.
pub const PREVIEW_MIN_W: f32 = 360.0;
/// Width the window gives to everything else on the editor row: the 64 px rail, 24 px page
/// padding on both sides and the 16 px gap between preview and inspector.
pub const EDITOR_CHROME_W: f32 = scale::RAIL_W + 48.0 + 16.0;

/// The inspector width for a window `window_w` px wide. Captions mode wants [`CAPTIONS_W`] but
/// leaves the preview at least [`PREVIEW_MIN_W`]; when the window is too narrow for both the
/// inspector goes back down to its normal width (never below [`INSPECTOR_MIN_W`]) and the preview
/// takes what is left. Every other tab uses [`INSPECTOR_W`].
pub fn inspector_width(captions_mode: bool, window_w: f32) -> f32 {
    if !captions_mode {
        return INSPECTOR_W;
    }
    let room = window_w - EDITOR_CHROME_W - PREVIEW_MIN_W;
    room.clamp(INSPECTOR_MIN_W, CAPTIONS_W)
}

// ---- following the playhead -------------------------------------------------------------

/// After a manual scroll the list leaves the user alone this long.
pub const FOLLOW_PAUSE: Duration = Duration::from_secs(2);

/// Whether the list may scroll itself to the line under the playhead: not while a text field has
/// the keyboard (the user is typing), and not within [`FOLLOW_PAUSE`] of a manual scroll.
pub fn follow_allowed(since_manual_scroll: Option<Duration>, field_focused: bool) -> bool {
    !field_focused && since_manual_scroll.is_none_or(|d| d >= FOLLOW_PAUSE)
}

/// The line to scroll to now: the active one, when it is not the one the list last followed.
/// `None` when there is nothing new to show (so a user who scrolled away is not pulled back every
/// frame, only when the playhead enters another line).
pub fn follow_target(active: Option<usize>, last_followed: Option<usize>) -> Option<usize> {
    active.filter(|&a| Some(a) != last_followed)
}

// ---- the captions lane ------------------------------------------------------------------

/// Least width of a lane block, px, so a short line can still be clicked.
pub const LANE_MIN_BLOCK_W: f32 = 6.0;
/// Space between neighbouring blocks, px.
pub const LANE_GAP: f32 = 1.0;

/// One block of the captions lane.
#[derive(Debug, Clone, PartialEq)]
pub struct LaneBlock {
    pub index: usize,
    pub x: f32,
    pub w: f32,
    /// Under the playhead.
    pub active: bool,
    pub selected: bool,
    /// Breaks a rule (amber outline).
    pub problem: bool,
}

/// The blocks of the lane for a lane `width` px wide whose time axis is `x_of`. Lines outside the
/// visible part are left out; one that is cut at an edge is clipped to it. Each block is shrunk by
/// [`LANE_GAP`] so neighbours do not touch, and is at least [`LANE_MIN_BLOCK_W`] wide.
pub fn lane_blocks(
    lines: &[CaptionLine],
    x_of: impl Fn(MediaTime) -> f32,
    width: f32,
    playhead: MediaTime,
    selected: Option<usize>,
) -> Vec<LaneBlock> {
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let (x0, x1) = (x_of(l.start), x_of(l.end));
        if x1 < 0.0 || x0 > width {
            continue;
        }
        let (a, b) = (x0.max(0.0), x1.min(width));
        let w = (b - a - LANE_GAP).max(LANE_MIN_BLOCK_W);
        let prev_end = i.checked_sub(1).and_then(|p| lines.get(p)).map(|p| p.end);
        out.push(LaneBlock {
            index: i,
            x: a,
            w,
            active: l.start <= playhead && playhead < l.end,
            selected: selected == Some(i),
            problem: !issues(l, prev_end).is_empty(),
        });
    }
    out
}

/// The line whose block a click at lane x `x` hits (the last one wins on a tie), if any.
pub fn lane_hit(blocks: &[LaneBlock], x: f32) -> Option<usize> {
    blocks
        .iter()
        .rev()
        .find(|b| x >= b.x && x <= b.x + b.w)
        .map(|b| b.index)
}

/// The text a block shows: the caption on one line (the lane truncates it to the block).
pub fn lane_label(l: &CaptionLine) -> String {
    plain(&l.text)
}

// ---- play this line ---------------------------------------------------------------------

/// A line being played on its own: from its start to `end`, then pause there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinePlay {
    pub line: usize,
    pub end: MediaTime,
}

impl LinePlay {
    /// Playback should stop now: the playhead reached the end of the line (or went past it, or
    /// playback was stopped some other way).
    pub fn finished(&self, playhead: MediaTime, playing: bool) -> bool {
        !playing || playhead >= self.end
    }
}

// ---- doubtful words (AI-6a) -----------------------------------------------------------------

/// Where "next doubtful word" or "previous doubtful word" lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoubtfulJump {
    /// The caption line to select and edit.
    pub line: usize,
    /// The caret: a byte offset into [`line_text`] of that line (what the field is loaded with).
    pub byte: usize,
    /// Where the word starts in the recording: the playhead goes there.
    pub start: MediaTime,
}

/// The jump from the current position. `line` is the line being edited, or the selected line, or
/// the one under the playhead; `caret` is the field's raw text and caret byte when that line is
/// being edited (the caret is mapped onto the text the row shows, which has its own spacing), and
/// `None` when it is not. Wraps around at the ends; `None` when nothing is doubtful.
pub fn doubtful_jump(
    spans: &[Doubtful],
    lines: &[CaptionLine],
    line: usize,
    caret: Option<(&str, usize)>,
    forward: bool,
) -> Option<DoubtfulJump> {
    let shown_caret = caret.and_then(|(raw, byte)| {
        let target = line_text(lines.get(line)?);
        let chars = caret_char_index(raw, byte, &target);
        Some(
            target
                .char_indices()
                .nth(chars)
                .map_or(target.len(), |(b, _)| b),
        )
    });
    let d = &spans[step(spans, line, shown_caret, forward)?];
    Some(DoubtfulJump {
        line: d.line,
        byte: d.range.start,
        start: d.start,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: f64) -> MediaTime {
        MediaTime::from_secs_f64(s).unwrap()
    }

    fn line(a: f64, b: f64, text: &str) -> CaptionLine {
        CaptionLine {
            start: t(a),
            end: t(b),
            text: text.into(),
        }
    }

    #[test]
    fn doubtful_jumps_step_through_lines_and_map_the_caret() {
        use cm_ai::doubtful::doubtful_spans;
        use cm_ai::transcribe::Word;
        let w = |text: &str, a: f64, b: f64, c: f32| Word {
            text: text.into(),
            start: t(a),
            end: t(b),
            confidence: c,
        };
        let words = vec![
            w("Open", 0.0, 0.4, 0.9),
            w("Logo", 0.4, 0.8, 0.3),
            w("Drive", 0.8, 1.2, 0.9),
            w("đê", 2.0, 2.4, 0.2),
            w("save", 2.4, 2.8, 0.9),
        ];
        // The second line is stored decomposed.
        let lines = [
            line(0.0, 1.5, "Open Logo Drive"),
            line(1.8, 3.0, "\u{0111}e\u{0302} save"),
        ];
        let spans = doubtful_spans(&lines, &words);
        assert_eq!(spans.len(), 2);
        // Not editing line 0: its own doubtful word is next.
        let j = doubtful_jump(&spans, &lines, 0, None, true).unwrap();
        assert_eq!((j.line, j.byte, j.start), (0, 5, t(0.4)));
        // Editing line 0 with the caret after "Logo" (raw text has a double space before Drive).
        let raw = "Open Logo  Drive";
        let j = doubtful_jump(&spans, &lines, 0, Some((raw, 9)), true).unwrap();
        assert_eq!((j.line, j.byte), (1, 0));
        // Back from line 1's start goes to Logo; again wraps to the last word.
        let j = doubtful_jump(&spans, &lines, 1, Some(("đê save", 0)), false).unwrap();
        assert_eq!((j.line, j.byte), (0, 5));
        let j = doubtful_jump(&spans, &lines, 0, Some((raw, 0)), false).unwrap();
        assert_eq!(j.line, 1, "wraps to the last doubtful word");
        // Nothing doubtful: no jump.
        assert_eq!(doubtful_jump(&[], &lines, 0, None, true), None);
    }

    #[test]
    fn other_tabs_keep_the_normal_inspector() {
        for w in [720.0, 1180.0, 1440.0, 2560.0] {
            assert_eq!(inspector_width(false, w), INSPECTOR_W);
        }
    }

    #[test]
    fn captions_mode_is_600_when_the_window_allows_it_and_the_preview_keeps_360() {
        // 1440: 1440 - 128 - 360 = 952 -> 600.
        assert_eq!(inspector_width(true, 1440.0), CAPTIONS_W);
        // The exact fit: 128 + 360 + 600 = 1088.
        assert_eq!(inspector_width(true, 1088.0), CAPTIONS_W);
        // The 1180 px width of the plan has room to spare.
        assert_eq!(inspector_width(true, 1180.0), CAPTIONS_W);
        // Narrower: the inspector gives up width first, never the preview's 360.
        let w = inspector_width(true, 1000.0);
        assert!(w < CAPTIONS_W && w > INSPECTOR_MIN_W, "{w}");
        assert_eq!(1000.0 - EDITOR_CHROME_W - w, PREVIEW_MIN_W);
    }

    #[test]
    fn a_narrow_window_goes_back_to_the_normal_width_floor() {
        assert_eq!(inspector_width(true, 720.0), INSPECTOR_MIN_W);
        assert_eq!(inspector_width(true, 0.0), INSPECTOR_MIN_W);
        // Never wider than the normal inspector plus what the preview can spare.
        for w in (600..2000).step_by(37) {
            let i = inspector_width(true, w as f32);
            assert!((INSPECTOR_MIN_W..=CAPTIONS_W).contains(&i));
        }
    }

    #[test]
    fn the_list_never_follows_while_typing_or_just_after_a_manual_scroll() {
        assert!(follow_allowed(None, false));
        assert!(!follow_allowed(None, true), "a field has focus");
        assert!(!follow_allowed(Some(Duration::from_millis(500)), false));
        assert!(!follow_allowed(Some(Duration::from_millis(1999)), false));
        assert!(follow_allowed(Some(Duration::from_secs(2)), false));
        assert!(!follow_allowed(Some(Duration::from_secs(5)), true));
    }

    #[test]
    fn following_scrolls_only_when_the_active_line_changes() {
        assert_eq!(follow_target(Some(3), None), Some(3));
        assert_eq!(follow_target(Some(3), Some(2)), Some(3));
        assert_eq!(follow_target(Some(3), Some(3)), None);
        assert_eq!(follow_target(None, Some(3)), None);
    }

    #[test]
    fn lane_blocks_follow_the_time_axis_and_flag_state() {
        let lines = [
            line(0.0, 2.0, "one"),
            line(2.0, 4.0, "two"),
            line(4.5, 4.9, "short"),
        ];
        // 100 px per second, 1000 px wide.
        let b = lane_blocks(
            &lines,
            |m| m.as_secs_f64() as f32 * 100.0,
            1000.0,
            t(2.5),
            Some(0),
        );
        assert_eq!(b.len(), 3);
        assert_eq!((b[0].x, b[0].w), (0.0, 199.0));
        assert_eq!((b[1].x, b[1].w), (200.0, 199.0));
        assert!(b[1].active && !b[0].active);
        assert!(b[0].selected && !b[1].selected);
        // 0.4 s is under the 1 s minimum: flagged amber.
        assert!(b[2].problem && !b[0].problem);
    }

    #[test]
    fn lane_blocks_are_clipped_skipped_and_never_too_small_to_click() {
        let lines = [
            line(0.0, 1.0, "gone"),
            line(5.0, 8.0, "cut at the left"),
            line(9.0, 9.01, "tiny"),
            line(20.0, 22.0, "beyond"),
        ];
        // Scrolled to 4 s: x = (t - 4) * 100, 600 px wide (sees 4 s to 10 s).
        let b = lane_blocks(
            &lines,
            |m| (m.as_secs_f64() as f32 - 4.0) * 100.0,
            600.0,
            t(0.0),
            None,
        );
        let idx: Vec<usize> = b.iter().map(|b| b.index).collect();
        assert_eq!(idx, vec![1, 2]);
        assert_eq!(b[1].w, LANE_MIN_BLOCK_W);
        // Scrolled further: a line cut by the left edge starts at 0.
        let c = lane_blocks(
            &lines,
            |m| (m.as_secs_f64() as f32 - 6.0) * 100.0,
            600.0,
            t(0.0),
            None,
        );
        assert_eq!(c[0].index, 1);
        assert_eq!(c[0].x, 0.0);
        assert!((c[0].w - 199.0).abs() < 1e-3, "{}", c[0].w);
    }

    #[test]
    fn a_click_on_the_lane_picks_the_block_under_it() {
        let lines = [line(0.0, 2.0, "a"), line(2.0, 4.0, "b")];
        let b = lane_blocks(
            &lines,
            |m| m.as_secs_f64() as f32 * 100.0,
            1000.0,
            t(9.0),
            None,
        );
        assert_eq!(lane_hit(&b, 50.0), Some(0));
        assert_eq!(lane_hit(&b, 250.0), Some(1));
        assert_eq!(lane_hit(&b, 600.0), None);
    }

    #[test]
    fn the_lane_label_is_one_line() {
        assert_eq!(lane_label(&line(0.0, 2.0, "two\nrows")), "two rows");
    }

    #[test]
    fn playing_a_line_stops_at_its_end_or_when_playback_stops() {
        let p = LinePlay {
            line: 2,
            end: t(5.0),
        };
        assert!(!p.finished(t(4.9), true));
        assert!(p.finished(t(5.0), true));
        assert!(p.finished(t(6.0), true));
        assert!(
            p.finished(t(3.0), false),
            "paused by the user: forget the line"
        );
    }
}
