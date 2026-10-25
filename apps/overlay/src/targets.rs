//! `list_targets` payload, window hit testing and display <-> OS monitor matching.
//!
//! Display and window bounds are physical px in the global desktop space (daemon contract);
//! the picker works in display-relative px, so hit testing subtracts the display origin.
//! A window's bounds are expressed in the physical frame of the display named by its
//! `display_id` (mixed-DPI desktops make the global spaces of two displays overlap), so a
//! window is only ever hit-tested on its own display (R1-2).

use cm_core::{PhysicalPoint, PhysicalRect, PhysicalSize, ScaleFactor};
use serde::Deserialize;

use crate::selection::{clamp_to_display, rect_contains};

/// A display from `list_targets`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DisplayInfo {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub bounds: PhysicalRect,
    pub scale: ScaleFactor,
    #[serde(default)]
    pub is_primary: bool,
}

/// A top-level window from `list_targets`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WindowInfo {
    pub id: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub pid: u32,
    pub bounds: PhysicalRect,
    /// The display holding the window; `None` when the daemon could not tell.
    #[serde(default)]
    pub display_id: Option<u32>,
    #[serde(default)]
    pub minimized: bool,
}

/// The `list_targets` result. Windows are front to back when the OS provides an order.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct Targets {
    pub displays: Vec<DisplayInfo>,
    #[serde(default)]
    pub windows: Vec<WindowInfo>,
}

/// Windows smaller than this (either side) are ignored when picking: they are almost always
/// helper windows (status items, shadows), not something a user means to capture.
pub const MIN_PICKABLE_WINDOW_PX: u32 = 24;

/// A window the user can pick on `display`: its rect relative to the display, clipped to it.
#[derive(Debug, Clone, PartialEq)]
pub struct PickableWindow {
    pub id: u32,
    pub title: String,
    pub app: String,
    pub rect: PhysicalRect,
}

/// Windows that can be picked on `display`, front to back. Minimized windows, this process's
/// own windows (`own_pid`) and tiny helper windows are left out.
pub fn pickable_windows(
    targets: &Targets,
    display: &DisplayInfo,
    own_pid: u32,
) -> Vec<PickableWindow> {
    let origin = display.bounds.origin;
    targets
        .windows
        .iter()
        .filter(|w| !w.minimized && w.pid != own_pid)
        // R1-2: a window on another display must never land inside this display's rectangle.
        .filter(|w| w.display_id.is_none_or(|id| id == display.id))
        .filter(|w| {
            w.bounds.size.width >= MIN_PICKABLE_WINDOW_PX
                && w.bounds.size.height >= MIN_PICKABLE_WINDOW_PX
        })
        .filter_map(|w| {
            let rel = PhysicalRect {
                origin: PhysicalPoint {
                    x: w.bounds.origin.x - origin.x,
                    y: w.bounds.origin.y - origin.y,
                },
                size: w.bounds.size,
            };
            let clipped = clamp_to_display(rel, display.bounds.size)?;
            Some(PickableWindow {
                id: w.id,
                title: w.title.clone(),
                app: w.app.clone(),
                rect: clipped,
            })
        })
        .collect()
}

/// The front-most pickable window under display pixel `p` (CAP-1.AC5 hover highlight).
pub fn window_at(windows: &[PickableWindow], p: PhysicalPoint) -> Option<&PickableWindow> {
    windows.iter().find(|w| rect_contains(w.rect, p))
}

/// The display a global desktop point is on.
pub fn display_at(displays: &[DisplayInfo], p: PhysicalPoint) -> Option<&DisplayInfo> {
    displays.iter().find(|d| rect_contains(d.bounds, p))
}

/// An OS monitor as seen by the windowing layer (physical px in winit's desktop space).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorGeometry {
    pub position: PhysicalPoint,
    pub size: PhysicalSize,
}

/// Pairs each display with a monitor: same size first, then nearest position, each monitor
/// used at most once. `None` means the display has no monitor any more (it was unplugged).
pub fn match_monitors(
    displays: &[DisplayInfo],
    monitors: &[MonitorGeometry],
) -> Vec<Option<usize>> {
    let mut taken = vec![false; monitors.len()];
    let mut out = vec![None; displays.len()];
    // Greedy over (display, monitor) pairs by cost, so two identical monitors pair by position.
    let mut pairs: Vec<(i64, usize, usize)> = Vec::new();
    for (di, d) in displays.iter().enumerate() {
        for (mi, m) in monitors.iter().enumerate() {
            if m.size != d.bounds.size {
                continue;
            }
            let cost = i64::from((m.position.x - d.bounds.origin.x).abs())
                + i64::from((m.position.y - d.bounds.origin.y).abs());
            pairs.push((cost, di, mi));
        }
    }
    pairs.sort_unstable();
    for (_, di, mi) in pairs {
        if out[di].is_none() && !taken[mi] {
            out[di] = Some(mi);
            taken[mi] = true;
        }
    }
    out
}

/// Displays whose monitor is gone (CAP-1: close that display's overlay, keep the others).
pub fn lost_displays(displays: &[DisplayInfo], monitors: &[MonitorGeometry]) -> Vec<u32> {
    match_monitors(displays, monitors)
        .iter()
        .zip(displays)
        .filter(|(m, _)| m.is_none())
        .map(|(_, d)| d.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rect(x: i32, y: i32, w: u32, h: u32) -> PhysicalRect {
        PhysicalRect {
            origin: PhysicalPoint { x, y },
            size: PhysicalSize {
                width: w,
                height: h,
            },
        }
    }

    fn display(id: u32, x: i32, y: i32, w: u32, h: u32) -> DisplayInfo {
        DisplayInfo {
            id,
            name: format!("d{id}"),
            bounds: rect(x, y, w, h),
            scale: ScaleFactor::ONE,
            is_primary: id == 1,
        }
    }

    fn window(id: u32, pid: u32, b: PhysicalRect) -> WindowInfo {
        WindowInfo {
            id,
            title: format!("w{id}"),
            app: "app".into(),
            pid,
            bounds: b,
            display_id: None,
            minimized: false,
        }
    }

    fn window_on(id: u32, display_id: u32, b: PhysicalRect) -> WindowInfo {
        WindowInfo {
            display_id: Some(display_id),
            ..window(id, 5, b)
        }
    }

    #[test]
    fn parses_the_daemon_list_targets_shape() {
        let v = json!({
            "displays": [{"id": 1, "name": "Built-in", "bounds": {"origin": {"x": 0, "y": 0},
                "size": [3024, 1964]}, "scale": 2.0, "is_primary": true}],
            "windows": [{"id": 7, "title": "t", "app": "a", "pid": 12, "bounds":
                {"origin": {"x": 10, "y": 20}, "size": [300, 200]}, "scale": 2.0,
                "display_id": 1, "minimized": false}]
        });
        let t: Targets = serde_json::from_value(v).unwrap();
        assert_eq!(t.windows[0].display_id, Some(1));
        assert_eq!(t.displays[0].bounds.size.width, 3024);
        assert_eq!(t.windows[0].bounds.origin, PhysicalPoint { x: 10, y: 20 });
    }

    /// CAP-1.AC5: the hovered window is the front-most one under the pointer.
    #[test]
    fn cap1_ac5_hover_picks_the_front_most_window() {
        let d = display(1, 0, 0, 1000, 800);
        let t = Targets {
            displays: vec![d.clone()],
            windows: vec![
                window(1, 5, rect(100, 100, 300, 300)),
                window(2, 5, rect(50, 50, 600, 600)),
            ],
        };
        let ws = pickable_windows(&t, &d, 999);
        assert_eq!(
            window_at(&ws, PhysicalPoint { x: 150, y: 150 }).map(|w| w.id),
            Some(1)
        );
        assert_eq!(
            window_at(&ws, PhysicalPoint { x: 60, y: 60 }).map(|w| w.id),
            Some(2)
        );
        assert!(window_at(&ws, PhysicalPoint { x: 900, y: 700 }).is_none());
    }

    #[test]
    fn own_minimized_and_tiny_windows_are_not_pickable() {
        let d = display(1, 0, 0, 1000, 800);
        let mut min = window(3, 5, rect(0, 0, 200, 200));
        min.minimized = true;
        let t = Targets {
            displays: vec![d.clone()],
            windows: vec![
                window(1, 42, rect(0, 0, 200, 200)),
                min,
                window(2, 5, rect(0, 0, 10, 10)),
                window(4, 5, rect(0, 0, 200, 200)),
            ],
        };
        let ids: Vec<u32> = pickable_windows(&t, &d, 42).iter().map(|w| w.id).collect();
        assert_eq!(ids, vec![4]);
    }

    /// Window rects are display-relative and clipped, so a window on a second display maps to
    /// that display's own pixels (CAP-1.AC7).
    #[test]
    fn cap1_ac7_windows_are_relative_to_their_display_and_clipped() {
        let d2 = display(2, 3024, 0, 1920, 1080);
        let t = Targets {
            displays: vec![display(1, 0, 0, 3024, 1964), d2.clone()],
            windows: vec![window(9, 5, rect(3000, -10, 400, 300))],
        };
        let ws = pickable_windows(&t, &d2, 0);
        assert_eq!(ws[0].rect, rect(0, 0, 376, 290));
        let ws1 = pickable_windows(&t, &t.displays[0], 0);
        assert_eq!(ws1[0].rect, rect(3000, 0, 24, 290));
        // A window wholly on the other display is not pickable here.
        let t2 = Targets {
            displays: t.displays.clone(),
            windows: vec![window(9, 5, rect(3100, 100, 400, 300))],
        };
        assert!(pickable_windows(&t2, &t2.displays[0], 0).is_empty());
    }

    /// R1-2: a 2x display (0..3024 px) next to a 1x display whose physical origin is 1512 (its
    /// own scale applied to its point origin), so the two global spaces overlap. A window on
    /// the 1x display, at 1612..2012, must not show up on the 2x display, and vice versa.
    #[test]
    fn r1_2_mixed_dpi_windows_only_belong_to_their_own_display() {
        let a = display(1, 0, 0, 3024, 1964);
        let b = display(2, 1512, 0, 1920, 1080);
        let t = Targets {
            displays: vec![a.clone(), b.clone()],
            windows: vec![
                window_on(10, 2, rect(1612, 50, 400, 300)),
                window_on(11, 1, rect(200, 100, 800, 600)),
            ],
        };
        let on_a: Vec<u32> = pickable_windows(&t, &a, 0).iter().map(|w| w.id).collect();
        assert_eq!(on_a, vec![11]);
        let on_b = pickable_windows(&t, &b, 0);
        assert_eq!(on_b.len(), 1);
        assert_eq!(on_b[0].id, 10);
        assert_eq!(on_b[0].rect, rect(100, 50, 400, 300));
        // The ghost position: window 10 hovered at a 2x pixel is not hit on display 1.
        let ws = pickable_windows(&t, &a, 0);
        assert!(window_at(&ws, PhysicalPoint { x: 1700, y: 100 }).is_none());
    }

    #[test]
    fn display_at_finds_the_display_in_global_space() {
        let ds = vec![display(1, 0, 0, 1000, 800), display(2, 1000, 0, 1000, 800)];
        assert_eq!(
            display_at(&ds, PhysicalPoint { x: 1500, y: 10 }).map(|d| d.id),
            Some(2)
        );
        assert!(display_at(&ds, PhysicalPoint { x: 5000, y: 10 }).is_none());
    }

    fn mon(x: i32, y: i32, w: u32, h: u32) -> MonitorGeometry {
        MonitorGeometry {
            position: PhysicalPoint { x, y },
            size: PhysicalSize {
                width: w,
                height: h,
            },
        }
    }

    #[test]
    fn identical_monitors_pair_by_position() {
        let ds = vec![
            display(1, 0, 0, 1920, 1080),
            display(2, 1920, 0, 1920, 1080),
        ];
        // The OS lists them in the opposite order.
        let ms = vec![mon(1920, 0, 1920, 1080), mon(0, 0, 1920, 1080)];
        assert_eq!(match_monitors(&ds, &ms), vec![Some(1), Some(0)]);
    }

    /// CAP-1 edge case: a display that disappears loses its overlay; the others stay.
    #[test]
    fn unplugged_display_is_reported_lost() {
        let ds = vec![
            display(1, 0, 0, 3024, 1964),
            display(2, 3024, 0, 1920, 1080),
        ];
        let ms = vec![mon(0, 0, 3024, 1964)];
        assert_eq!(lost_displays(&ds, &ms), vec![2]);
        assert!(lost_displays(&ds, &[ms[0], mon(3024, 0, 1920, 1080)]).is_empty());
    }
}
