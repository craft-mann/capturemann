//! Where a Quick Capture screenshot was taken, in the terms the after-capture bar needs (QC-3):
//! the display, the area on it in physical px, and the display's scale. Read from a
//! `list_targets` answer. Pure.

use cm_capture::Targets;
use serde_json::Value;

use super::run::Area;

/// An area placed on an engine display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub display_id: u32,
    pub window_id: Option<u32>,
    /// `[x, y, w, h]`, physical px relative to the display.
    pub rect: [i64; 4],
    /// The rectangle the user picked (a region); absent for a window or a display.
    pub picked: Option<[i64; 4]>,
    pub scale: f64,
    /// The display's origin and size in global physical px.
    pub display_px: [i64; 4],
}

/// Reads a `list_targets` answer (with windows when the area is a window).
pub fn parse_targets(v: &Value) -> Option<Targets> {
    serde_json::from_value(v.clone()).ok()
}

/// Puts `area` on the displays of `targets`. A display that is not known (a delayed display
/// capture) is the primary one. `None` when nothing matches.
pub fn place(targets: &Targets, area: &Area) -> Option<Placed> {
    let primary = || {
        targets
            .displays
            .iter()
            .find(|d| d.is_primary)
            .or(targets.displays.first())
    };
    let on = |id: u32| targets.displays.iter().find(|d| d.id == id);
    let display_px = |d: &cm_capture::DisplayInfo| {
        [
            i64::from(d.bounds.origin.x),
            i64::from(d.bounds.origin.y),
            i64::from(d.bounds.size.width),
            i64::from(d.bounds.size.height),
        ]
    };
    match *area {
        Area::Region { display_id, rect } => {
            let d = on(display_id)?;
            Some(Placed {
                display_id,
                window_id: None,
                rect,
                picked: Some(rect),
                scale: d.scale.get(),
                display_px: display_px(d),
            })
        }
        Area::Display { display_id } => {
            let d = display_id.and_then(on).or_else(primary)?;
            Some(Placed {
                display_id: d.id,
                window_id: None,
                rect: [
                    0,
                    0,
                    i64::from(d.bounds.size.width),
                    i64::from(d.bounds.size.height),
                ],
                picked: None,
                scale: d.scale.get(),
                display_px: display_px(d),
            })
        }
        Area::Window { window_id } => {
            let w = targets.windows.iter().find(|w| w.id == window_id)?;
            let d = w.display_id.and_then(on).or_else(primary)?;
            Some(Placed {
                display_id: d.id,
                window_id: Some(window_id),
                rect: [
                    i64::from(w.bounds.origin.x),
                    i64::from(w.bounds.origin.y),
                    i64::from(w.bounds.size.width),
                    i64::from(w.bounds.size.height),
                ],
                picked: None,
                scale: w.scale.get(),
                display_px: display_px(d),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_capture::{DisplayInfo, WindowInfo};
    use cm_core::geometry::{PhysicalPoint, PhysicalRect, PhysicalSize, ScaleFactor};

    fn rect(x: i32, y: i32, w: u32, h: u32) -> PhysicalRect {
        PhysicalRect {
            origin: PhysicalPoint { x, y },
            size: PhysicalSize {
                width: w,
                height: h,
            },
        }
    }

    fn targets() -> Targets {
        let display = |id, primary, x, scale| DisplayInfo {
            id,
            name: format!("D{id}"),
            bounds: rect(x, 0, 2880, 1800),
            scale: ScaleFactor::new(scale).unwrap(),
            is_primary: primary,
            is_builtin: false,
        };
        Targets {
            displays: vec![display(1, true, 0, 2.0), display(2, false, 2880, 1.0)],
            windows: vec![WindowInfo {
                id: 77,
                title: "t".into(),
                app: "a".into(),
                pid: 1,
                bounds: rect(100, 200, 800, 600),
                scale: ScaleFactor::new(1.0).unwrap(),
                display_id: Some(2),
                minimized: false,
            }],
        }
    }

    #[test]
    fn a_region_keeps_its_rectangle_and_takes_the_display_scale() {
        let p = place(
            &targets(),
            &Area::Region {
                display_id: 1,
                rect: [10, 20, 300, 200],
            },
        )
        .unwrap();
        assert_eq!(
            (p.display_id, p.rect, p.scale),
            (1, [10, 20, 300, 200], 2.0)
        );
        assert_eq!(p.picked, Some([10, 20, 300, 200]));
        assert_eq!(p.display_px, [0, 0, 2880, 1800]);
    }

    #[test]
    fn an_unknown_display_is_the_primary_one() {
        let p = place(&targets(), &Area::Display { display_id: None }).unwrap();
        assert_eq!(p.display_id, 1);
        assert_eq!(p.rect, [0, 0, 2880, 1800]);
        assert!(p.picked.is_none());
        let q = place(
            &targets(),
            &Area::Display {
                display_id: Some(2),
            },
        )
        .unwrap();
        assert_eq!((q.display_id, q.scale), (2, 1.0));
    }

    #[test]
    fn a_window_is_placed_on_its_display() {
        let p = place(&targets(), &Area::Window { window_id: 77 }).unwrap();
        assert_eq!((p.display_id, p.window_id), (2, Some(77)));
        assert_eq!(p.rect, [100, 200, 800, 600]);
        assert!(place(&targets(), &Area::Window { window_id: 1 }).is_none());
    }

    /// The reading of a real `list_targets` answer works (the scale is a plain number).
    #[test]
    fn a_serialized_answer_reads_back() {
        let v = serde_json::to_value(targets()).unwrap();
        let t = parse_targets(&v).unwrap();
        assert_eq!(t.displays[0].scale.get(), 2.0);
    }
}
