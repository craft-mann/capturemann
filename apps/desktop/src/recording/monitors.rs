//! Monitors in physical pixels and how they map to GPUI's window coordinates, as pure
//! arithmetic (the Windows code in `platform_window` only fills [`Monitor`] from the Win32
//! monitor APIs; everything that decides is here and runs on any host).
//!
//! # The two coordinate spaces
//!
//! * **Physical px**: the Windows virtual desktop. The engine reports a recorded area here
//!   (`indicator.area.bounds`), and `EnumDisplayMonitors` / `GetCursorPos` use it.
//! * **GPUI points**: GPUI 0.3.9 on Windows places a monitor at `physical origin / that
//!   monitor's own scale` and gives it `physical size / scale`; a window opened with bounds `b`
//!   on a display lands at physical `b * that display's scale`, and the bounds are **absolute**
//!   in this space (macOS takes them relative to the display). On a mixed-DPI desktop the point
//!   rectangles of two monitors can overlap (a 100 % monitor at physical 0..1920 and a 150 %
//!   monitor at physical 1920.. is points 1280..), so a point does not always name one monitor.
//!   Everything that must be exact (which monitor was recorded, which one holds the pointer)
//!   is therefore decided in physical px, and only the result is converted to points.
//!
//! This is the REC-5 / REC-4 placement contract on Windows: the countdown disc is centered on
//! the recorded area on its monitor, the area brackets cover exactly the recorded area's
//! monitor in the space the windows use, and the HUD sits at the bottom centre of that monitor's
//! work area.

use super::geometry::Rect;

/// One monitor, as the platform reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Monitor {
    /// The platform handle GPUI names the display with (`HMONITOR` on Windows).
    pub id: u32,
    /// The monitor in physical px (`rcMonitor`).
    pub frame_px: Rect,
    /// The monitor without the taskbar, in physical px (`rcWork`).
    pub work_px: Rect,
    /// DPI / 96 of this monitor.
    pub scale: f64,
}

fn safe_scale(s: f64) -> f64 {
    if s.is_finite() && s > 0.0 {
        s
    } else {
        1.0
    }
}

fn div(r: &Rect, s: f64) -> Rect {
    Rect::new(r.x / s, r.y / s, r.w / s, r.h / s)
}

fn mul(r: &Rect, s: f64) -> Rect {
    Rect::new(r.x * s, r.y * s, r.w * s, r.h * s)
}

impl Monitor {
    /// The monitor in GPUI points: what `Display::bounds()` is.
    pub fn frame_points(&self) -> Rect {
        div(&self.frame_px, safe_scale(self.scale))
    }

    /// The work area in GPUI points.
    pub fn work_points(&self) -> Rect {
        div(&self.work_px, safe_scale(self.scale))
    }

    /// A rectangle in GPUI points, as a window opened on this monitor puts it, in physical px.
    pub fn to_physical(&self, points: &Rect) -> Rect {
        mul(points, safe_scale(self.scale))
    }
}

/// The monitor holding the physical point `(x, y)`.
pub fn index_at_px(ms: &[Monitor], x: f64, y: f64) -> Option<usize> {
    ms.iter().position(|m| m.frame_px.contains(x, y))
}

/// The monitor a recorded area (`[x, y, w, h]` in physical px) belongs to: the one holding its
/// center, else the one it overlaps most. Physical rectangles never overlap, so this is exact.
pub fn index_for_area_px(ms: &[Monitor], bounds: [i64; 4]) -> Option<usize> {
    let area = Rect::new(
        bounds[0] as f64,
        bounds[1] as f64,
        bounds[2] as f64,
        bounds[3] as f64,
    );
    let (cx, cy) = area.center();
    if let Some(i) = index_at_px(ms, cx, cy) {
        return Some(i);
    }
    let overlap = |m: &Monitor| {
        let d = &m.frame_px;
        let w = (d.right().min(area.right()) - d.x.max(area.x)).max(0.0);
        let h = (d.bottom().min(area.bottom()) - d.y.max(area.y)).max(0.0);
        w * h
    };
    ms.iter()
        .enumerate()
        .filter(|(_, m)| overlap(m) > 0.0)
        .max_by(|a, b| overlap(a.1).total_cmp(&overlap(b.1)))
        .map(|(i, _)| i)
}

/// The monitor whose point frame is `frame` (a display rectangle taken from [`Monitor::frame_points`]),
/// else the first whose point frame holds `frame`'s center, preferring `frame`'s own match.
pub fn index_for_frame(ms: &[Monitor], frame: &Rect) -> Option<usize> {
    let same = |a: f64, b: f64| (a - b).abs() < 0.5;
    if let Some(i) = ms.iter().position(|m| {
        let f = m.frame_points();
        same(f.x, frame.x) && same(f.y, frame.y) && same(f.w, frame.w) && same(f.h, frame.h)
    }) {
        return Some(i);
    }
    let (cx, cy) = frame.center();
    ms.iter().position(|m| m.frame_points().contains(cx, cy))
}

/// The monitor holding the GPUI point `(x, y)`; on overlapping point rectangles (mixed DPI) the
/// monitor `prefer` wins if it holds the point.
pub fn index_for_point(ms: &[Monitor], x: f64, y: f64, prefer: Option<usize>) -> Option<usize> {
    if let Some(i) = prefer.filter(|i| ms.get(*i).is_some_and(|m| m.frame_points().contains(x, y)))
    {
        return Some(i);
    }
    ms.iter().position(|m| m.frame_points().contains(x, y))
}

/// The point on a monitor for a physical position on it.
pub fn px_to_point(m: &Monitor, x: f64, y: f64) -> (f64, f64) {
    let s = safe_scale(m.scale);
    (x / s, y / s)
}

/// The rectangle to give `WindowBounds::Windowed` for a window that should cover `global` (GPUI
/// points) on `display`: relative to the display where the platform wants that (macOS), absolute
/// where it takes global points (Windows).
pub fn window_bounds(global: &Rect, display: &Rect, absolute: bool) -> Rect {
    if absolute {
        *global
    } else {
        global.relative_to(display)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::countdown;
    use crate::recording::geometry::{area_points, brackets, hud_default, HUD_BOTTOM_MARGIN};

    fn mon(id: u32, frame: [f64; 4], work: [f64; 4], scale: f64) -> Monitor {
        Monitor {
            id,
            frame_px: Rect::new(frame[0], frame[1], frame[2], frame[3]),
            work_px: Rect::new(work[0], work[1], work[2], work[3]),
            scale,
        }
    }

    /// Everything the recording UI computes for a recording of `bounds` (physical, with the
    /// engine's `scale`), the way `ui::recording` does it on Windows, with its windows in
    /// physical px at the end (where GPUI puts them).
    struct Placed {
        monitor: usize,
        countdown_px: Rect,
        indicator_px: Rect,
        brackets_px: [Rect; 4],
        hud_px: Rect,
    }

    const HUD: (f64, f64) = (739.0, 150.0);

    fn place(ms: &[Monitor], bounds: [i64; 4], scale: f64) -> Placed {
        let i = index_for_area_px(ms, bounds).expect("a monitor holds the area");
        let m = &ms[i];
        let display = m.frame_points();
        let area = area_points(bounds, scale);
        let cd = countdown::window_rect(&area, &display);
        let hud = hud_default(&m.work_points(), HUD);
        let b = brackets(&area, &display);
        Placed {
            monitor: i,
            countdown_px: m.to_physical(&window_bounds(&cd, &display, true)),
            indicator_px: m.to_physical(&display),
            brackets_px: b.map(|r| m.to_physical(&r)),
            hud_px: m.to_physical(&hud),
        }
    }

    fn inside(outer: &Rect, r: &Rect) -> bool {
        r.x >= outer.x - 0.01
            && r.y >= outer.y - 0.01
            && r.right() <= outer.right() + 0.01
            && r.bottom() <= outer.bottom() + 0.01
    }

    /// The lab laptop of Nov 5, 2026: panel 1920x1080 at 150 % (primary) and a Samsung
    /// 3840x2160 at 150 % to its right, taskbar 48 px at the bottom of the panel.
    fn lab() -> Vec<Monitor> {
        vec![
            mon(0x10001, [0., 0., 1920., 1080.], [0., 0., 1920., 1032.], 1.5),
            mon(
                0x20001,
                [1920., 0., 3840., 2160.],
                [1920., 0., 3840., 2160.],
                1.5,
            ),
        ]
    }

    #[test]
    fn the_lab_laptop_external_monitor_gets_every_window_on_it() {
        let ms = lab();
        // The recording of display 131073: bounds [1920,0,3840,2160], scale 1.5.
        let p = place(&ms, [1920, 0, 3840, 2160], 1.5);
        assert_eq!(p.monitor, 1, "the external monitor, not the panel");
        // The area brackets' window covers exactly the recorded pixels: (1920,0) 3840x2160, not
        // the panel's origin (the 3862x2171 at -11,-5 of the report).
        assert_eq!(p.indicator_px, Rect::new(1920., 0., 3840., 2160.));
        for b in p.brackets_px {
            assert!(inside(&Rect::new(1920., 0., 3840., 2160.), &b), "{b:?}");
        }
        // The countdown disc is centered on the recorded display.
        let (cx, cy) = p.countdown_px.center();
        assert!((cx - 3840.0).abs() < 40.0, "{cx}");
        assert!((cy - 1080.0).abs() < 40.0, "{cy}");
        assert!(inside(&ms[1].frame_px, &p.countdown_px));
        // The HUD: bottom centre of the external monitor, wholly on it (not straddling the seam
        // at x = 1920, which is where the report found it).
        assert!(inside(&ms[1].work_px, &p.hud_px), "{:?}", p.hud_px);
        let (hx, _) = p.hud_px.center();
        assert!((hx - 3840.0).abs() < 1.0, "{hx}");
        assert!((p.hud_px.bottom() - (2160.0 - 1.5 * HUD_BOTTOM_MARGIN)).abs() < 1.0);
    }

    #[test]
    fn the_lab_laptop_panel_recording_stays_on_the_panel() {
        let ms = lab();
        let p = place(&ms, [0, 0, 1920, 1080], 1.5);
        assert_eq!(p.monitor, 0);
        assert_eq!(p.indicator_px, Rect::new(0., 0., 1920., 1080.));
        // The HUD sits above the taskbar (work area), still centred.
        assert!(inside(&ms[0].work_px, &p.hud_px));
        assert!((p.hud_px.center().0 - 960.0).abs() < 1.0);
        assert!(inside(&ms[0].frame_px, &p.countdown_px));
    }

    #[test]
    fn a_window_opened_at_global_points_lands_on_the_physical_pixels() {
        // The bug of Nov 5: the origin was made relative to the display, which is wrong where
        // GPUI takes absolute points.
        let ms = lab();
        let d = ms[1].frame_points();
        assert_eq!(d, Rect::new(1280., 0., 2560., 1440.));
        let rel = window_bounds(&d, &d, false);
        assert_eq!((rel.x, rel.y), (0., 0.));
        let abs = window_bounds(&d, &d, true);
        assert_eq!(ms[1].to_physical(&abs).x, 1920.0);
    }

    #[test]
    fn an_external_at_100_percent_to_the_right_of_a_150_percent_panel() {
        let ms = vec![
            mon(1, [0., 0., 1920., 1080.], [0., 0., 1920., 1032.], 1.5),
            mon(2, [1920., 0., 2560., 1440.], [1920., 0., 2560., 1400.], 1.0),
        ];
        // The external's points (1920..4480) and the panel's (0..1280) differ in scale; the
        // monitor is still decided in physical px.
        let p = place(&ms, [1920, 0, 2560, 1440], 1.0);
        assert_eq!(p.monitor, 1);
        assert_eq!(p.indicator_px, Rect::new(1920., 0., 2560., 1440.));
        assert!(inside(&ms[1].work_px, &p.hud_px));
        assert!((p.hud_px.center().0 - (1920.0 + 1280.0)).abs() < 1.0);
        let (cx, cy) = p.countdown_px.center();
        assert!((cx - 3200.0).abs() < 100.0 && (cy - 720.0).abs() < 100.0);
    }

    #[test]
    fn a_150_percent_monitor_whose_points_overlap_a_100_percent_one() {
        // 100 % at physical 0..1920 is points 0..1920; a 150 % monitor at physical 1920..4480
        // is points 1280..2987: they overlap on 1280..1920.
        let ms = vec![
            mon(1, [0., 0., 1920., 1080.], [0., 0., 1920., 1040.], 1.0),
            mon(2, [1920., 0., 2560., 1440.], [1920., 0., 2560., 1400.], 1.5),
        ];
        // A region in the left part of the second monitor: its center in points (about 1500)
        // lies in both point frames; physical px decide.
        let bounds = [2000, 100, 600, 400];
        let (cx, cy) = area_points(bounds, 1.5).center();
        assert!(ms[0].frame_points().contains(cx, cy) && ms[1].frame_points().contains(cx, cy));
        assert_eq!(index_for_area_px(&ms, bounds), Some(1));
        let p = place(&ms, bounds, 1.5);
        assert_eq!(p.monitor, 1);
        assert!(inside(&ms[1].frame_px, &p.countdown_px));
        assert!(inside(&ms[1].work_px, &p.hud_px));
        // A frame taken from the monitor names that monitor, though the other holds its center.
        assert_eq!(index_for_frame(&ms, &ms[1].frame_points()), Some(1));
        assert_eq!(index_for_frame(&ms, &ms[0].frame_points()), Some(0));
    }

    #[test]
    fn a_100_percent_monitor_left_of_a_200_percent_primary() {
        let ms = vec![
            mon(1, [0., 0., 2880., 1800.], [0., 0., 2880., 1752.], 2.0),
            mon(
                2,
                [-1920., 0., 1920., 1080.],
                [-1920., 0., 1920., 1040.],
                1.0,
            ),
        ];
        let p = place(&ms, [-1920, 0, 1920, 1080], 1.0);
        assert_eq!(p.monitor, 1);
        assert_eq!(p.indicator_px, Rect::new(-1920., 0., 1920., 1080.));
        assert!(inside(&ms[1].work_px, &p.hud_px), "{:?}", p.hud_px);
        assert!((p.hud_px.center().0 + 960.0).abs() < 1.0);
        assert!(inside(&ms[1].frame_px, &p.countdown_px));
        // The primary at 200 %.
        let q = place(&ms, [0, 0, 2880, 1800], 2.0);
        assert_eq!(q.monitor, 0);
        assert_eq!(q.indicator_px, Rect::new(0., 0., 2880., 1800.));
        assert!(inside(&ms[0].work_px, &q.hud_px));
    }

    #[test]
    fn a_monitor_above_the_primary() {
        let ms = vec![
            mon(1, [0., 0., 1920., 1080.], [0., 0., 1920., 1040.], 1.0),
            mon(
                2,
                [0., -1080., 1920., 1080.],
                [0., -1080., 1920., 1080.],
                1.0,
            ),
        ];
        let p = place(&ms, [0, -1080, 1920, 1080], 1.0);
        assert_eq!(p.monitor, 1);
        assert_eq!(p.indicator_px, Rect::new(0., -1080., 1920., 1080.));
        assert!(inside(&ms[1].work_px, &p.hud_px));
        assert!(
            p.hud_px.y < 0.0,
            "the HUD is above the primary, at negative y"
        );
        assert!(inside(&ms[1].frame_px, &p.countdown_px));
        // A region and a window on the upper monitor.
        let r = place(&ms, [400, -700, 800, 500], 1.0);
        assert_eq!(r.monitor, 1);
        assert!(inside(&ms[1].frame_px, &r.countdown_px));
    }

    #[test]
    fn the_pointer_picks_the_monitor_in_physical_px() {
        let ms = lab();
        assert_eq!(index_at_px(&ms, 100., 100.), Some(0));
        assert_eq!(index_at_px(&ms, 2000., 50.), Some(1));
        assert_eq!(index_at_px(&ms, 1919., 50.), Some(0));
        assert_eq!(index_at_px(&ms, 1920., 50.), Some(1));
        assert_eq!(index_at_px(&ms, 9000., 50.), None);
        let (x, y) = px_to_point(&ms[1], 3000., 300.);
        assert_eq!((x, y), (2000., 200.));
        assert_eq!(index_for_point(&ms, x, y, None), Some(1));
        assert_eq!(index_for_point(&ms, x, y, Some(0)), Some(1));
    }

    #[test]
    fn an_area_outside_every_monitor_has_none() {
        assert_eq!(index_for_area_px(&lab(), [9000, 9000, 100, 100]), None);
        // Straddling the seam: the larger part wins.
        assert_eq!(index_for_area_px(&lab(), [1800, 0, 400, 200]), Some(1));
        assert_eq!(index_for_area_px(&lab(), [1500, 0, 400, 200]), Some(0));
    }
}
