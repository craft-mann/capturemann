//! The timeline's zoomed range (board R3c, "Timeline zoomed to 0:00-0:30"): the visible window as
//! start and end times, the label, zooming that keeps the playhead in view, and showing a range.
//! The mapping between time and pixels stays in [`Geom`], so lanes, ruler, playhead, caption
//! blocks and every pointer calculation share one source.

use super::geom::{Geom, MAX_ZOOM};
use cm_core::MediaTime;

/// The visible window: session time at the left and right edges, inside the recording.
pub fn visible_range(g: &Geom) -> (MediaTime, MediaTime) {
    let len = (g.end.ticks() as f64 / g.zoom).round() as i64;
    let from = g.scroll.max(MediaTime::ZERO);
    (from, MediaTime::from_ticks(from.ticks() + len).min(g.end))
}

/// True when the window is narrower than the whole recording.
pub fn is_zoomed(g: &Geom) -> bool {
    g.zoom > 1.0 + 1e-9
}

/// `m:ss` for a time, rounded to the nearest second.
pub fn clock(t: MediaTime) -> String {
    let s = t.as_secs_f64().max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The two ends of the label, `("0:00", "0:30")`.
pub fn range_label_parts(g: &Geom) -> (String, String) {
    let (a, b) = visible_range(g);
    (clock(a), clock(b))
}

/// Zooms by `factor` around the playhead when it is on screen, else around the middle, and keeps
/// the playhead visible afterwards.
pub fn zoom_keeping_playhead(g: &mut Geom, factor: f64, playhead: MediaTime) {
    let x = g.x_of(playhead);
    let anchor = if (0.0..=g.width).contains(&x) {
        x
    } else {
        g.width / 2.0
    };
    g.zoom_at(factor, anchor);
    g.reveal(playhead);
}

/// Shows `from` to `to` (clamped to the recording, never narrower than 1 / `MAX_ZOOM` of it).
pub fn show_range(g: &mut Geom, from: MediaTime, to: MediaTime) {
    let from = from.clamp(MediaTime::ZERO, g.end);
    let span = (to.ticks() - from.ticks()).max(1) as f64;
    g.zoom = (g.end.ticks() as f64 / span).clamp(1.0, MAX_ZOOM);
    g.scroll = from;
    g.pan(0.0); // clamps the scroll
}

/// Scrolls the window so the span `from..to` is at least partly visible, centring `from` when it
/// is not. A no-op when the span already overlaps the window.
pub fn reveal_span(g: &mut Geom, from: MediaTime, to: MediaTime) {
    let (a, b) = visible_range(g);
    if to < a || from > b {
        g.reveal(from);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: i64) -> MediaTime {
        MediaTime::from_ticks(s * cm_core::TICKS_PER_SECOND)
    }

    fn geom() -> Geom {
        Geom::new(1000.0, secs(120))
    }

    fn len(g: &Geom) -> i64 {
        let (a, b) = visible_range(g);
        b.ticks() - a.ticks()
    }

    #[test]
    fn whole_recording_is_not_zoomed() {
        let g = geom();
        assert!(!is_zoomed(&g));
        assert_eq!(visible_range(&g), (secs(0), secs(120)));
    }

    #[test]
    fn show_range_sets_the_window_and_the_label() {
        let mut g = geom();
        show_range(&mut g, secs(0), secs(30));
        assert!(is_zoomed(&g));
        assert_eq!(visible_range(&g), (secs(0), secs(30)));
        assert_eq!(range_label_parts(&g), ("0:00".into(), "0:30".into()));
        assert!((g.x_of(secs(15)) - 500.0).abs() < 0.01);
        assert_eq!(g.t_of(500.0), secs(15));
    }

    #[test]
    fn show_range_clamps_to_the_recording() {
        let mut g = geom();
        show_range(&mut g, secs(100), secs(160));
        let (a, b) = visible_range(&g);
        assert_eq!(b, secs(120));
        assert!(a >= secs(60) && a <= secs(100));
        show_range(&mut g, secs(-5), secs(500));
        assert_eq!(visible_range(&g), (secs(0), secs(120)));
        show_range(&mut g, secs(10), secs(10));
        assert!((g.zoom - MAX_ZOOM).abs() < 1e-9);
    }

    #[test]
    fn zoom_around_the_playhead_keeps_it_where_it_is() {
        let mut g = geom();
        show_range(&mut g, secs(0), secs(60));
        let before = g.x_of(secs(20));
        zoom_keeping_playhead(&mut g, 2.0, secs(20));
        assert!((g.x_of(secs(20)) - before).abs() < 1.0);
        assert!((len(&g) - secs(30).ticks()).abs() <= 1);
    }

    #[test]
    fn zoom_with_the_playhead_off_screen_brings_it_back() {
        let mut g = geom();
        show_range(&mut g, secs(0), secs(30));
        zoom_keeping_playhead(&mut g, 2.0, secs(90));
        let x = g.x_of(secs(90));
        assert!((0.0..=g.width).contains(&x));
    }

    #[test]
    fn zoom_out_never_goes_past_the_whole_recording() {
        let mut g = geom();
        show_range(&mut g, secs(10), secs(40));
        for _ in 0..10 {
            zoom_keeping_playhead(&mut g, 1.0 / 1.5, secs(20));
        }
        assert_eq!(visible_range(&g), (secs(0), secs(120)));
    }

    #[test]
    fn reveal_span_moves_only_when_the_span_is_out_of_view() {
        let mut g = geom();
        show_range(&mut g, secs(0), secs(30));
        reveal_span(&mut g, secs(10), secs(12));
        assert_eq!(visible_range(&g).0, secs(0));
        reveal_span(&mut g, secs(80), secs(82));
        let (a, b) = visible_range(&g);
        assert!(a <= secs(80) && secs(80) <= b);
    }

    #[test]
    fn clock_formats_minutes_and_seconds() {
        assert_eq!(clock(secs(0)), "0:00");
        assert_eq!(clock(secs(75)), "1:15");
        assert_eq!(clock(MediaTime::from_ticks(-5)), "0:00");
    }
}
