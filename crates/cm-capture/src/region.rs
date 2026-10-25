use crate::{DisplayInfo, RegionRect};
use cm_core::{CmError, PhysicalPoint, PhysicalRect, PhysicalSize};

/// Resolves a display-relative region to a physical-px crop rectangle inside `display`.
///
/// Logical rects are converted edge by edge (`round(x*s)`, `round((x+w)*s)`), so the size never
/// drifts by a pixel from rounding the origin and size separately and neighbouring rects tile
/// without gaps. Returns `invalid_input` for non-finite or negative values, empty regions and
/// regions that leave the display.
pub fn resolve_region(display: &DisplayInfo, rect: RegionRect) -> Result<PhysicalRect, CmError> {
    let phys = match rect {
        RegionRect::Physical(r) => r,
        RegionRect::Logical(r) => {
            let vals = [r.origin.x, r.origin.y, r.size.width, r.size.height];
            if vals.iter().any(|v| !v.is_finite()) {
                return Err(CmError::invalid_input("Region has a non-finite value"));
            }
            if r.origin.x < 0.0 || r.origin.y < 0.0 || r.size.width < 0.0 || r.size.height < 0.0 {
                return Err(CmError::invalid_input(
                    "Region must have a non-negative origin and size",
                ));
            }
            let s = display.scale.get();
            let edge = |v: f64| (v * s).round();
            let (l, t) = (edge(r.origin.x), edge(r.origin.y));
            let (rt, b) = (edge(r.right()), edge(r.bottom()));
            if rt > f64::from(u32::MAX) || b > f64::from(u32::MAX) {
                return Err(CmError::invalid_input("Region is out of range"));
            }
            PhysicalRect {
                origin: PhysicalPoint {
                    x: l as i32,
                    y: t as i32,
                },
                size: PhysicalSize {
                    width: (rt - l) as u32,
                    height: (b - t) as u32,
                },
            }
        }
    };
    if phys.size.width == 0 || phys.size.height == 0 {
        return Err(CmError::invalid_input("Region is empty"));
    }
    let (x, y) = (i64::from(phys.origin.x), i64::from(phys.origin.y));
    let (w, h) = (i64::from(phys.size.width), i64::from(phys.size.height));
    let (dw, dh) = (
        i64::from(display.bounds.size.width),
        i64::from(display.bounds.size.height),
    );
    if x < 0 || y < 0 || x + w > dw || y + h > dh {
        return Err(CmError::invalid_input(format!(
            "Region {w}x{h} at ({x}, {y}) is outside display {} ({dw}x{dh})",
            display.id
        )));
    }
    Ok(phys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::{Rect, ScaleFactor};

    fn display(scale: f64, w: u32, h: u32) -> DisplayInfo {
        DisplayInfo {
            id: 1,
            name: "d".into(),
            bounds: PhysicalRect {
                origin: PhysicalPoint { x: 0, y: 0 },
                size: PhysicalSize {
                    width: w,
                    height: h,
                },
            },
            scale: ScaleFactor::new(scale).unwrap(),
            is_primary: true,
            is_builtin: false,
        }
    }

    #[test]
    fn logical_2x_doubles() {
        // CAP-1.AC2 geometry expressed in logical px on a 2x display.
        let d = display(2.0, 3024, 1964);
        let r = resolve_region(
            &d,
            RegionRect::Logical(Rect::new(1103.5, 334.5, 210.0, 324.5)),
        )
        .unwrap();
        assert_eq!((r.origin.x, r.origin.y), (2207, 669));
        assert_eq!((r.size.width, r.size.height), (420, 649));
    }

    #[test]
    fn logical_fractional_scale_rounds_edges() {
        // 150%: x=1,w=1 -> edges 1.5 -> 2 and 3.0 -> 3: width 1 (not round(1.5)=2).
        let d = display(1.5, 300, 300);
        let r = resolve_region(&d, RegionRect::Logical(Rect::new(1.0, 0.0, 1.0, 2.0))).unwrap();
        assert_eq!((r.origin.x, r.size.width), (2, 1));
        assert_eq!((r.origin.y, r.size.height), (0, 3));
    }

    #[test]
    fn rejects_bad_regions() {
        let d = display(2.0, 100, 100);
        let phys = |x, y, w, h| {
            RegionRect::Physical(PhysicalRect {
                origin: PhysicalPoint { x, y },
                size: PhysicalSize {
                    width: w,
                    height: h,
                },
            })
        };
        assert!(resolve_region(&d, phys(0, 0, 100, 100)).is_ok());
        assert!(resolve_region(&d, phys(1, 0, 100, 100)).is_err());
        assert!(resolve_region(&d, phys(-1, 0, 5, 5)).is_err());
        assert!(resolve_region(&d, phys(0, 0, 0, 5)).is_err());
        assert!(resolve_region(&d, RegionRect::Logical(Rect::new(-1.0, 0.0, 5.0, 5.0))).is_err());
        assert!(
            resolve_region(&d, RegionRect::Logical(Rect::new(f64::NAN, 0.0, 5.0, 5.0))).is_err()
        );
        assert!(resolve_region(&d, RegionRect::Logical(Rect::new(0.0, 0.0, 0.1, 5.0))).is_err());
    }
}
