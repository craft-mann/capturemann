//! The Windows recording sources that are plain logic. They compile on every system so that the
//! development Mac tests them; the parts that need Windows (Windows.Graphics.Capture, Media
//! Foundation, WASAPI) are the `cm-capture`, `cm-encode`, `cm-audio` and `cm-camera` backends that
//! [`SystemSources`](crate::recording::SystemSources) builds on Windows.
//!
//! * **Screen**: `WgcSource` into the hardware H.264 encoder; no software fallback, so a computer
//!   without one fails the start with `hardware_encoder_unavailable`.
//! * **Permissions**: Windows has no screen-capture prompt, and the microphone and camera consent
//!   is read, never asked for.
//! * **Cursor**: the global space is the virtual screen in physical px (the process is per-monitor
//!   DPI aware), so the target scale is 1.0.

use cm_capture::record::CapturePlan;
use cm_capture::{CaptureTarget, Targets};
use cm_core::Rect;
use cm_cursor::{CursorTarget, TargetKind};

fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
    Rect::new(f64::from(x), f64::from(y), f64::from(w), f64::from(h))
}

/// The cursor's recorded area for `plan`: display, region or window in virtual-screen physical px.
pub fn cursor_target(plan: &CapturePlan, targets: &Targets) -> CursorTarget {
    let display = plan
        .display_id
        .and_then(|id| targets.displays.iter().find(|d| d.id == id));
    let whole = |kind| {
        CursorTarget::from_global_rect(
            kind,
            plan.display_id,
            rect(0, 0, plan.source_size.width, plan.source_size.height),
            1.0,
        )
    };
    match (&plan.target, display) {
        (CaptureTarget::Display { display_id }, _) => {
            match display.or_else(|| targets.displays.iter().find(|d| d.id == *display_id)) {
                Some(d) => CursorTarget::display(
                    d.id,
                    rect(
                        d.bounds.origin.x,
                        d.bounds.origin.y,
                        d.bounds.size.width,
                        d.bounds.size.height,
                    ),
                    1.0,
                ),
                None => whole(TargetKind::Display),
            }
        }
        (CaptureTarget::Region { .. }, Some(d)) => {
            let crop = plan.crop.unwrap_or(d.bounds);
            CursorTarget::from_global_rect(
                TargetKind::Region,
                Some(d.id),
                rect(
                    d.bounds.origin.x + crop.origin.x,
                    d.bounds.origin.y + crop.origin.y,
                    crop.size.width,
                    crop.size.height,
                ),
                1.0,
            )
        }
        (CaptureTarget::Window { window_id }, _) => {
            match targets.windows.iter().find(|w| w.id == *window_id) {
                Some(w) => CursorTarget::from_global_rect(
                    TargetKind::Window,
                    plan.display_id,
                    rect(
                        w.bounds.origin.x,
                        w.bounds.origin.y,
                        w.bounds.size.width,
                        w.bounds.size.height,
                    ),
                    1.0,
                ),
                None => whole(TargetKind::Window),
            }
        }
        (_, None) => whole(TargetKind::Region),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_capture::record::{plan_capture, ScreenSettings};
    use cm_capture::{DisplayInfo, RegionRect, WindowInfo};
    use cm_core::{PhysicalPoint, PhysicalRect, PhysicalSize, ScaleFactor};

    fn prect(x: i32, y: i32, w: u32, h: u32) -> PhysicalRect {
        PhysicalRect {
            origin: PhysicalPoint { x, y },
            size: PhysicalSize {
                width: w,
                height: h,
            },
        }
    }

    /// A second monitor to the right of a 1920x1080 primary, at 150 %.
    fn targets() -> Targets {
        Targets {
            displays: vec![DisplayInfo {
                id: 2,
                name: "ext".into(),
                bounds: prect(1920, 0, 2560, 1440),
                scale: ScaleFactor::new(1.5).unwrap(),
                is_primary: false,
                is_builtin: false,
            }],
            windows: vec![WindowInfo {
                id: 7,
                title: "w".into(),
                app: "a".into(),
                pid: 1,
                bounds: prect(2120, 100, 800, 600),
                scale: ScaleFactor::new(1.5).unwrap(),
                display_id: Some(2),
                minimized: false,
            }],
        }
    }

    /// The origin is in virtual-screen physical px and the scale stays 1.0, whatever the monitor's
    /// DPI scale is.
    #[test]
    fn display_region_and_window_targets_are_in_physical_px() {
        let t = targets();
        let display = plan_capture(
            &ScreenSettings::new(CaptureTarget::Display { display_id: 2 }),
            &t,
        )
        .unwrap();
        let c = cursor_target(&display, &t);
        assert_eq!(c.kind, TargetKind::Display);
        assert_eq!(c.origin, [1920.0, 0.0]);
        assert_eq!((c.size.width, c.size.height), (2560, 1440));
        assert!((c.scale - 1.0).abs() < 1e-9);

        let region = plan_capture(
            &ScreenSettings::new(CaptureTarget::Region {
                display_id: 2,
                rect: RegionRect::Physical(prect(100, 80, 1280, 720)),
            }),
            &t,
        )
        .unwrap();
        let c = cursor_target(&region, &t);
        assert_eq!(c.kind, TargetKind::Region);
        assert_eq!(c.origin, [2020.0, 80.0]);
        assert_eq!((c.size.width, c.size.height), (1280, 720));

        let window = plan_capture(
            &ScreenSettings::new(CaptureTarget::Window { window_id: 7 }),
            &t,
        )
        .unwrap();
        let c = cursor_target(&window, &t);
        assert_eq!(c.kind, TargetKind::Window);
        assert_eq!(c.origin, [2120.0, 100.0]);
        assert_eq!((c.size.width, c.size.height), (800, 600));
    }

    /// A cursor on the second monitor maps to its pixels in the display recording.
    #[test]
    fn a_cursor_position_maps_to_the_recorded_pixels() {
        let t = targets();
        let plan = plan_capture(
            &ScreenSettings::new(CaptureTarget::Display { display_id: 2 }),
            &t,
        )
        .unwrap();
        let c = cursor_target(&plan, &t);
        assert_eq!(c.map(1920.0 + 100.0, 50.0), (100, 50, false));
        assert!(
            c.map(10.0, 10.0).2,
            "a point on the primary monitor is outside"
        );
    }

    fn display(id: u32, x: i32, y: i32, w: u32, h: u32, scale: f64) -> DisplayInfo {
        DisplayInfo {
            id,
            name: format!("d{id}"),
            bounds: prect(x, y, w, h),
            scale: ScaleFactor::new(scale).unwrap(),
            is_primary: x == 0 && y == 0,
            is_builtin: false,
        }
    }

    fn layout(displays: Vec<DisplayInfo>) -> Targets {
        Targets {
            displays,
            windows: vec![],
        }
    }

    /// Mixed-DPI desktops (virtual-screen physical px, monitors at negative coordinates): for a display and a
    /// region on each monitor the cursor target is that area's physical rectangle, the area's top-left maps to
    /// (0, 0), its bottom-right corner is outside, and a point on the neighbouring monitor is outside.
    #[test]
    fn region_cursor_targets_on_mixed_dpi_layouts() {
        // (name, monitors, display holding the region, region in that display's own physical px)
        let cases = [
            (
                "100 % at 0,0 + 150 % at 1920,0",
                vec![
                    display(1, 0, 0, 1920, 1080, 1.0),
                    display(2, 1920, 0, 2560, 1440, 1.5),
                ],
                2,
                prect(300, 200, 1280, 720),
            ),
            (
                "200 % primary + 100 % to the left at -1920,0",
                vec![
                    display(1, 0, 0, 3840, 2160, 2.0),
                    display(2, -1920, 0, 1920, 1080, 1.0),
                ],
                2,
                prect(100, 50, 800, 600),
            ),
            (
                "150 % display above a 100 % primary",
                vec![
                    display(1, 0, 0, 1920, 1080, 1.0),
                    display(2, 0, -1440, 2560, 1440, 1.5),
                ],
                2,
                prect(10, 10, 640, 480),
            ),
            (
                "primary 125 % with a 100 % display left and above (-1920,-1080)",
                vec![
                    display(1, 0, 0, 2400, 1350, 1.25),
                    display(2, -1920, -1080, 1920, 1080, 1.0),
                ],
                2,
                prect(0, 0, 1920, 1080),
            ),
        ];
        for (name, monitors, id, region) in cases {
            let t = layout(monitors.clone());
            let d = monitors.iter().find(|d| d.id == id).unwrap().clone();
            let plan = plan_capture(
                &ScreenSettings::new(CaptureTarget::Region {
                    display_id: id,
                    rect: RegionRect::Physical(region),
                }),
                &t,
            )
            .unwrap();
            assert_eq!(plan.display_id, Some(id), "{name}");
            assert_eq!(
                plan.scale, d.scale,
                "{name}: the region uses its own display's scale"
            );
            let c = cursor_target(&plan, &t);
            let (ox, oy) = (
                d.bounds.origin.x + region.origin.x,
                d.bounds.origin.y + region.origin.y,
            );
            assert_eq!(c.origin, [f64::from(ox), f64::from(oy)], "{name}");
            assert_eq!(
                (c.size.width, c.size.height),
                (region.size.width, region.size.height),
                "{name}"
            );
            assert!((c.scale - 1.0).abs() < 1e-9, "{name}");
            let (gx, gy) = (f64::from(ox), f64::from(oy));
            assert_eq!(c.map(gx, gy), (0, 0, false), "{name}");
            assert_eq!(c.map(gx + 10.0, gy + 20.0), (10, 20, false), "{name}");
            let (w, h) = (f64::from(region.size.width), f64::from(region.size.height));
            assert!(c.map(gx + w, gy + h).2, "{name}: the far corner is outside");
            assert!(c.map(gx - 1.0, gy).2, "{name}: one px left is outside");
            // A point on the other monitor is outside the region.
            let other = monitors.iter().find(|m| m.id != id).unwrap();
            let (px_, py_) = (
                f64::from(other.bounds.origin.x) + 5.0,
                f64::from(other.bounds.origin.y) + 5.0,
            );
            assert!(c.map(px_, py_).2, "{name}: other monitor");
        }
    }

    /// A logical region is converted with the scale of the display it is on, never the primary's.
    #[test]
    fn a_logical_region_uses_the_scale_of_its_own_display() {
        let t = layout(vec![
            display(1, 0, 0, 1920, 1080, 1.0),
            display(2, 1920, 0, 2560, 1440, 1.5),
        ]);
        let plan = plan_capture(
            &ScreenSettings::new(CaptureTarget::Region {
                display_id: 2,
                rect: RegionRect::Logical(cm_core::Rect::new(100.0, 40.0, 400.0, 300.0)),
            }),
            &t,
        )
        .unwrap();
        assert_eq!(plan.crop, Some(prect(150, 60, 600, 450)));
        let c = cursor_target(&plan, &t);
        assert_eq!(c.origin, [2070.0, 60.0]);
        assert_eq!((c.size.width, c.size.height), (600, 450));
    }

    /// A region that is on no monitor, or that spans two, is refused at the plan, so no recording and no cursor
    /// target is ever made for it (REC-1: a region lives on one display).
    #[test]
    fn a_region_on_no_monitor_or_on_two_is_refused() {
        let t = layout(vec![
            display(1, 0, 0, 1920, 1080, 1.0),
            display(2, 1920, 0, 2560, 1440, 1.5),
        ]);
        let plan = |display_id, r| {
            plan_capture(
                &ScreenSettings::new(CaptureTarget::Region {
                    display_id,
                    rect: RegionRect::Physical(r),
                }),
                &t,
            )
        };
        // Spans the seam, expressed on either display.
        assert!(plan(1, prect(1800, 100, 400, 300)).is_err());
        assert!(plan(2, prect(-100, 100, 400, 300)).is_err());
        // Beyond the display's own pixels.
        assert!(plan(2, prect(2400, 0, 400, 300)).is_err());
        // A display that is gone (unplugged, or a stale id).
        let e = plan(7, prect(0, 0, 100, 100)).unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::NotFound);
    }

    /// A window on the left or the upper monitor (negative coordinates) maps its own pixels from (0, 0).
    #[test]
    fn a_window_on_a_negative_monitor_maps_from_its_own_corner() {
        let mut t = layout(vec![
            display(1, 0, 0, 3840, 2160, 2.0),
            display(2, -1920, 0, 1920, 1080, 1.0),
        ]);
        t.windows.push(WindowInfo {
            id: 9,
            title: "w".into(),
            app: "a".into(),
            pid: 1,
            bounds: prect(-1500, 120, 800, 600),
            scale: ScaleFactor::ONE,
            display_id: Some(2),
            minimized: false,
        });
        let plan = plan_capture(
            &ScreenSettings::new(CaptureTarget::Window { window_id: 9 }),
            &t,
        )
        .unwrap();
        let c = cursor_target(&plan, &t);
        assert_eq!(c.origin, [-1500.0, 120.0]);
        assert_eq!(c.map(-1500.0, 120.0), (0, 0, false));
        assert_eq!(c.map(-701.0, 719.0), (799, 599, false));
        assert!(c.map(-700.0, 719.0).2);
    }
}
