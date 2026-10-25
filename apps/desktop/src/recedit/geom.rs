//! Timeline geometry: session time to pixels, the segments of an edit list, the hatching of cuts
//! and hit tests (EDT-1, DESIGN §5.5). The axis is **session time** (the time the edits use), so a
//! cut stays visible as a hatched block and can be restored; each kept segment carries its speed as
//! a badge.

use cm_core::{EditList, MediaTime, Span};

/// Height of the ruler strip, px.
pub const RULER_H: f32 = 24.0;
/// Height of one track bar, px (redesign R3: taller rows than the 34 px of the first kit).
pub const ROW_H: f32 = 36.0;
/// Width of the label column left of the track area, px (redesign R3: icon and name, 100 px;
/// the first kit had 80).
pub const LABEL_W: f32 = 100.0;
/// Gap between track bars, px.
pub const ROW_GAP: f32 = 6.0;

/// Height of the ruler plus `rows` track bars.
pub fn body_height(rows: usize) -> f32 {
    RULER_H + rows as f32 * ROW_H + rows.saturating_sub(1) as f32 * ROW_GAP + 6.0
}

/// Top of track row `i` (0 is the screen bar), px from the top of the ruler.
pub fn row_top(i: usize) -> f32 {
    RULER_H + 4.0 + i as f32 * (ROW_H + ROW_GAP)
}
/// Width of a trim handle, px.
pub const HANDLE_W: f32 = 10.0;
/// How near (px) a split marker or the playhead must be to be grabbed.
pub const GRAB_PX: f32 = 5.0;
/// Hatch stripe spacing, px (DESIGN §5.5: `line` 1 px stripes at 6 px).
pub const HATCH_SPACING: f32 = 6.0;
/// The most the timeline zooms in on a recording.
pub const MAX_ZOOM: f64 = 64.0;

/// What a stretch of the axis is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SegKind {
    /// Kept, at `speed`; `index` counts kept segments from the left (the split segments).
    Kept {
        index: usize,
        speed: f64,
        keep_audio: bool,
    },
    /// Removed by a cut: hatched, restorable.
    Cut { index: usize },
    /// Before the in-point or after the out-point.
    Trimmed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seg {
    pub span: Span,
    pub kind: SegKind,
}

/// The axis is cut into stretches at the trim points, the cut edges, the splits and the speed
/// edges. Neighbours of the same kind and speed are joined unless a split lies between them.
pub fn segments(list: &EditList, session_end: MediaTime) -> Vec<Seg> {
    let mut edges = vec![MediaTime::ZERO, session_end, list.start, list.end];
    edges.extend(list.splits.iter().copied());
    for c in &list.cuts {
        edges.extend([c.from, c.to]);
    }
    for s in &list.speeds {
        edges.extend([s.span.from, s.span.to]);
    }
    edges.retain(|t| *t >= MediaTime::ZERO && *t <= session_end);
    edges.sort();
    edges.dedup();

    let mut out: Vec<Seg> = Vec::new();
    for w in edges.windows(2) {
        let span = Span::new(w[0], w[1]);
        let kind = if span.from < list.start || span.to > list.end {
            SegKind::Trimmed
        } else if let Some(i) = list
            .cuts
            .iter()
            .position(|c| c.from <= span.from && span.to <= c.to)
        {
            SegKind::Cut { index: i }
        } else {
            let (speed, keep_audio) = list
                .speeds
                .iter()
                .find(|s| s.span.from <= span.from && span.to <= s.span.to)
                .map_or((list.base_speed, false), |s| (s.factor, s.keep_audio));
            SegKind::Kept {
                index: 0,
                speed,
                keep_audio,
            }
        };
        let split_between = list.splits.contains(&span.from);
        match out.last_mut() {
            Some(prev) if !split_between && same_run(&prev.kind, &kind) => prev.span.to = span.to,
            _ => out.push(Seg { span, kind }),
        }
    }
    let mut n = 0;
    for s in &mut out {
        if let SegKind::Kept { index, .. } = &mut s.kind {
            *index = n;
            n += 1;
        }
    }
    out
}

fn same_run(a: &SegKind, b: &SegKind) -> bool {
    match (a, b) {
        (SegKind::Trimmed, SegKind::Trimmed) => true,
        (SegKind::Cut { index: x }, SegKind::Cut { index: y }) => x == y,
        (
            SegKind::Kept {
                speed: s1,
                keep_audio: k1,
                ..
            },
            SegKind::Kept {
                speed: s2,
                keep_audio: k2,
                ..
            },
        ) => (s1 - s2).abs() < 1e-9 && k1 == k2,
        _ => false,
    }
}

/// `2×`, `1.5×`, `0.5×`: the speed badge text.
pub fn speed_label(factor: f64) -> String {
    let s = cm_i18n::fmt::decimal(factor, 2);
    let sep = cm_i18n::fmt::decimal_separator(cm_i18n::language());
    let s = s.trim_end_matches('0').trim_end_matches(sep);
    format!("{s}\u{00D7}")
}

/// The timeline's window on the axis and its width on screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geom {
    /// Width of the track area, px.
    pub width: f32,
    /// Length of the axis (the end of the recording).
    pub end: MediaTime,
    /// 1 shows everything; larger zooms in.
    pub zoom: f64,
    /// Session time at the left edge.
    pub scroll: MediaTime,
}

impl Geom {
    pub fn new(width: f32, end: MediaTime) -> Geom {
        Geom {
            width: width.max(1.0),
            end: end.max(MediaTime::from_ticks(1)),
            zoom: 1.0,
            scroll: MediaTime::ZERO,
        }
    }

    /// Ticks the whole width shows.
    fn visible_ticks(&self) -> f64 {
        self.end.ticks() as f64 / self.zoom
    }

    pub fn x_of(&self, t: MediaTime) -> f32 {
        ((t.ticks() - self.scroll.ticks()) as f64 / self.visible_ticks() * f64::from(self.width))
            as f32
    }

    /// The time under pixel `x`, clamped to the recording.
    pub fn t_of(&self, x: f32) -> MediaTime {
        let t = self.scroll.ticks() as f64
            + f64::from(x) / f64::from(self.width) * self.visible_ticks();
        MediaTime::from_ticks(t.round() as i64).clamp(MediaTime::ZERO, self.end)
    }

    /// Time per pixel, for snapping thresholds.
    pub fn ticks_per_px(&self) -> f64 {
        self.visible_ticks() / f64::from(self.width)
    }

    fn clamp_scroll(&mut self) {
        let max = (self.end.ticks() as f64 - self.visible_ticks()).max(0.0);
        self.scroll = MediaTime::from_ticks((self.scroll.ticks() as f64).clamp(0.0, max) as i64);
    }

    /// Zooms by `factor` keeping the time under pixel `anchor_x` where it is.
    pub fn zoom_at(&mut self, factor: f64, anchor_x: f32) {
        let anchor = self.t_of(anchor_x);
        self.zoom = (self.zoom * factor).clamp(1.0, MAX_ZOOM);
        let left = anchor.ticks() as f64
            - f64::from(anchor_x) / f64::from(self.width) * self.visible_ticks();
        self.scroll = MediaTime::from_ticks(left.round() as i64);
        self.clamp_scroll();
    }

    /// Scrolls by `dx` pixels.
    pub fn pan(&mut self, dx: f32) {
        let d = f64::from(dx) * self.ticks_per_px();
        self.scroll = MediaTime::from_ticks(self.scroll.ticks() + d.round() as i64);
        self.clamp_scroll();
    }

    /// Scrolls so `t` is visible (playback follows the playhead).
    pub fn reveal(&mut self, t: MediaTime) {
        let x = self.x_of(t);
        if x < 0.0 || x > self.width {
            let half = (self.visible_ticks() / 2.0) as i64;
            self.scroll = MediaTime::from_ticks(t.ticks() - half);
            self.clamp_scroll();
        }
    }

    pub fn resize(&mut self, width: f32) {
        self.width = width.max(1.0);
        self.clamp_scroll();
    }
}

/// Ruler tick spacing: the first of 1, 2, 5, 10, ... seconds that leaves at least `min_px`
/// between ticks.
pub fn tick_step_secs(geom: &Geom, min_px: f32) -> f64 {
    let px_per_sec =
        f64::from(geom.width) / (geom.visible_ticks() / cm_core::TICKS_PER_SECOND as f64);
    for step in [
        0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0,
    ] {
        if step * px_per_sec >= f64::from(min_px) {
            return step;
        }
    }
    3600.0
}

/// Diagonal hatch lines filling `[x0, x1] x [0, h]`, as `((xa, ya), (xb, yb))`, clipped to the
/// rectangle. Lines run bottom-left to top-right, `spacing` px apart horizontally.
pub fn hatch_lines(x0: f32, x1: f32, h: f32, spacing: f32) -> Vec<((f32, f32), (f32, f32))> {
    let mut lines = Vec::new();
    if x1 <= x0 || h <= 0.0 || spacing <= 0.0 {
        return lines;
    }
    // A line through (c, h) rising at 45 degrees: x = c + (h - y).
    let mut c = (x0 - h / spacing * spacing).floor();
    c -= c.rem_euclid(spacing);
    while c <= x1 {
        // Bottom point (y = h) at x = c, top point (y = 0) at x = c + h; clip to [x0, x1].
        let (mut xa, mut ya) = (c, h);
        let (mut xb, mut yb) = (c + h, 0.0);
        if xa < x0 {
            ya = h - (x0 - xa);
            xa = x0;
        }
        if xb > x1 {
            yb = h - (x1 - c);
            xb = x1;
        }
        if xb > xa && ya >= yb {
            lines.push(((xa, ya), (xb, yb)));
        }
        c += spacing;
    }
    lines
}

/// What is under a point of the timeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Ruler(MediaTime),
    Playhead,
    InHandle,
    OutHandle,
    /// Index into `EditList::splits`.
    Split(usize),
    /// Index into `EditList::cuts`.
    Cut(usize),
    /// A kept segment: its index among the kept segments.
    Segment(usize),
    Trimmed,
    Nothing,
}

/// Hit test at (`x`, `y`) in the timeline's own coordinates (the ruler on top, the track below).
/// Handles and the playhead win over the content under them.
pub fn hit_test(
    geom: &Geom,
    list: &EditList,
    segs: &[Seg],
    playhead: MediaTime,
    rows: usize,
    x: f32,
    y: f32,
) -> Hit {
    if y < 0.0 || y > body_height(rows) || x < -HANDLE_W || x > geom.width + HANDLE_W {
        return Hit::Nothing;
    }
    if y < RULER_H {
        return Hit::Ruler(geom.t_of(x));
    }
    if (x - geom.x_of(playhead)).abs() <= GRAB_PX {
        return Hit::Playhead;
    }
    let (xin, xout) = (geom.x_of(list.start), geom.x_of(list.end));
    // The in handle hangs to the left of the in-point, the out handle to the right of the
    // out-point, so a short recording cannot cover one with the other.
    if x >= xin - HANDLE_W && x <= xin + 2.0 {
        return Hit::InHandle;
    }
    if x >= xout - 2.0 && x <= xout + HANDLE_W {
        return Hit::OutHandle;
    }
    for (i, s) in list.splits.iter().enumerate() {
        if (x - geom.x_of(*s)).abs() <= GRAB_PX {
            return Hit::Split(i);
        }
    }
    let t = geom.t_of(x);
    for s in segs {
        if s.span.from <= t && t < s.span.to {
            return match s.kind {
                SegKind::Kept { index, .. } => Hit::Segment(index),
                SegKind::Cut { index } => Hit::Cut(index),
                SegKind::Trimmed => Hit::Trimmed,
            };
        }
    }
    Hit::Nothing
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::{EditContext, SpeedSpan};

    /// R3: the row constants move together: every row is one pitch below the last, the body ends
    /// below the last row, the label column is 100 px, and a click on a row's middle is a hit on
    /// the track (not the ruler or nothing).
    #[test]
    fn row_constants_agree_with_each_other_and_the_hit_test() {
        assert_eq!(LABEL_W, 100.0);
        for rows in 1..=3usize {
            for i in 1..rows {
                assert_eq!(row_top(i) - row_top(i - 1), ROW_H + ROW_GAP);
            }
            assert!(row_top(rows - 1) + ROW_H <= body_height(rows));
            assert!(row_top(0) >= RULER_H);
        }
        let list = EditList::full(&EditContext {
            end: secs(10),
            live_at: None,
            gaps: &[],
        });
        let g = Geom::new(800.0, secs(10));
        let segs = segments(&list, secs(10));
        for i in 0..3 {
            let y = row_top(i) + ROW_H / 2.0;
            assert!(matches!(
                hit_test(&g, &list, &segs, secs(1), 3, 400.0, y),
                Hit::Segment(0)
            ));
        }
        assert!(matches!(
            hit_test(&g, &list, &segs, secs(1), 3, 400.0, RULER_H / 2.0),
            Hit::Ruler(_)
        ));
    }

    fn secs(s: i64) -> MediaTime {
        MediaTime::from_secs(s)
    }

    fn sixty() -> EditList {
        EditList::full(&EditContext {
            end: secs(60),
            live_at: None,
            gaps: &[],
        })
    }

    /// EDT-1.AC2: a cut shows as its own hatched stretch between two kept ones.
    #[test]
    fn edt1_ac2_cut_is_a_stretch_between_kept_segments() {
        let l = sixty().cut(&[Span::new(secs(10), secs(15))]).unwrap();
        let segs = segments(&l, secs(60));
        assert_eq!(segs.len(), 3);
        assert!(matches!(segs[0].kind, SegKind::Kept { index: 0, .. }));
        assert_eq!(segs[1].kind, SegKind::Cut { index: 0 });
        assert_eq!(segs[1].span, Span::new(secs(10), secs(15)));
        assert!(matches!(segs[2].kind, SegKind::Kept { index: 1, .. }));
    }

    /// EDT-1.AC1: trimming to 2-58 s leaves trimmed stretches at both ends.
    #[test]
    fn edt1_ac1_trim_marks_both_ends() {
        let l = sixty().trim(secs(2), secs(58), &ctx()).unwrap();
        let segs = segments(&l, secs(60));
        assert_eq!(segs.first().unwrap().kind, SegKind::Trimmed);
        assert_eq!(segs.last().unwrap().kind, SegKind::Trimmed);
        assert_eq!(segs[1].span, Span::new(secs(2), secs(58)));
    }

    fn ctx() -> EditContext<'static> {
        EditContext {
            end: secs(60),
            live_at: None,
            gaps: &[],
        }
    }

    /// EDT-2: a split makes two segments even at the same speed; a speed change shows its factor.
    #[test]
    fn edt2_split_and_speed_segments() {
        let l = sixty().split(secs(30)).unwrap();
        assert_eq!(segments(&l, secs(60)).len(), 2);
        let l = l
            .set_speed(Span::new(secs(30), secs(60)), 2.0, false)
            .unwrap();
        let segs = segments(&l, secs(60));
        assert_eq!(
            segs[1].kind,
            SegKind::Kept {
                index: 1,
                speed: 2.0,
                keep_audio: false
            }
        );
        assert_eq!(speed_label(2.0), "2\u{00D7}");
        assert_eq!(speed_label(1.5), "1.5\u{00D7}");
        assert_eq!(speed_label(0.25), "0.25\u{00D7}");
        // I18N P2: the decimal separator follows the language.
        cm_i18n::with_language(cm_i18n::Lang::Vi, || {
            assert_eq!(speed_label(1.5), "1,5\u{00D7}");
            assert_eq!(speed_label(2.0), "2\u{00D7}");
            assert_eq!(speed_label(0.25), "0,25\u{00D7}");
        });
    }

    #[test]
    fn speed_span_alignment_keeps_segments_separate() {
        let mut l = sixty();
        l.speeds.push(SpeedSpan {
            span: Span::new(secs(0), secs(20)),
            factor: 4.0,
            keep_audio: false,
        });
        let segs = segments(&l, secs(60));
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn x_and_t_are_inverse_and_zoom_keeps_the_anchor() {
        let mut g = Geom::new(600.0, secs(60));
        assert_eq!(g.x_of(secs(30)), 300.0);
        assert_eq!(g.t_of(300.0), secs(30));
        let before = g.t_of(450.0);
        g.zoom_at(4.0, 450.0);
        assert!((g.t_of(450.0) - before).abs() <= MediaTime::from_ticks(2));
        assert!(g.scroll > MediaTime::ZERO);
        g.zoom_at(0.0001, 450.0);
        assert_eq!(g.zoom, 1.0);
        assert_eq!(g.scroll, MediaTime::ZERO);
    }

    #[test]
    fn pan_and_reveal_stay_inside_the_recording() {
        let mut g = Geom::new(600.0, secs(60));
        g.zoom_at(4.0, 0.0);
        g.pan(100000.0);
        assert!(
            g.scroll + MediaTime::from_ticks(g.visible_ticks() as i64)
                <= g.end + MediaTime::from_ticks(2)
        );
        g.reveal(secs(1));
        assert!(g.x_of(secs(1)) >= 0.0);
    }

    #[test]
    fn ruler_ticks_leave_room() {
        let g = Geom::new(600.0, secs(60));
        let step = tick_step_secs(&g, 60.0);
        assert!(step * 10.0 >= 60.0, "{step}");
    }

    /// DESIGN §5.5: hatching is diagonal 1 px stripes 6 px apart, clipped to the cut.
    #[test]
    fn hatch_lines_are_spaced_and_clipped() {
        let lines = hatch_lines(10.0, 70.0, 56.0, HATCH_SPACING);
        assert!(!lines.is_empty());
        for ((xa, ya), (xb, yb)) in &lines {
            assert!(*xa >= 10.0 && *xb <= 70.0);
            assert!((0.0..=56.0).contains(ya) && (0.0..=56.0).contains(yb));
            assert!(((xb - xa) - (ya - yb)).abs() < 1e-3, "45 degrees");
        }
        assert!(hatch_lines(5.0, 5.0, 56.0, 6.0).is_empty());
    }

    #[test]
    fn hit_tests_find_handles_playhead_cuts_and_segments() {
        let g = Geom::new(600.0, secs(60));
        let l = sixty()
            .trim(secs(6), secs(54), &ctx())
            .unwrap()
            .cut(&[Span::new(secs(20), secs(30))])
            .unwrap()
            .split(secs(40))
            .unwrap();
        let segs = segments(&l, secs(60));
        let y = RULER_H + 10.0;
        let ph = secs(10);
        let rows = 2;
        assert_eq!(
            hit_test(&g, &l, &segs, ph, rows, 300.0, 5.0),
            Hit::Ruler(secs(30))
        );
        assert_eq!(hit_test(&g, &l, &segs, ph, rows, 100.0, y), Hit::Playhead);
        assert_eq!(hit_test(&g, &l, &segs, ph, rows, 56.0, y), Hit::InHandle);
        assert_eq!(hit_test(&g, &l, &segs, ph, rows, 545.0, y), Hit::OutHandle);
        assert_eq!(hit_test(&g, &l, &segs, ph, rows, 400.0, y), Hit::Split(0));
        assert_eq!(hit_test(&g, &l, &segs, ph, rows, 250.0, y), Hit::Cut(0));
        assert_eq!(hit_test(&g, &l, &segs, ph, rows, 150.0, y), Hit::Segment(0));
        assert_eq!(hit_test(&g, &l, &segs, ph, rows, 350.0, y), Hit::Segment(1));
        assert_eq!(hit_test(&g, &l, &segs, ph, rows, 560.0, y), Hit::Trimmed);
        assert_eq!(
            hit_test(&g, &l, &segs, ph, rows, 300.0, 500.0),
            Hit::Nothing
        );
    }
}
