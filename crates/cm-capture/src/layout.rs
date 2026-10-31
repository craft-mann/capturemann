//! The monitor layout of a desktop that hands out one picture of all monitors (the Wayland
//! Screenshot portal). Pure: the compositor's monitor list goes in, [`DisplayInfo`] rows with
//! positions in the picture come out, so the mapping (mixed DPI, negative origins, fractional
//! scales) is tested on every system.
//!
//! # Units
//!
//! The monitor list is in **logical** px (the compositor's layout space, where a 2x monitor
//! covers half as many units as it has pixels). The picture is in **picture px**: its width over
//! the layout's logical width is the picture scale `s` (1 when the compositor shoots the layout
//! at 1x, the largest monitor scale when it shoots at the sharpest scale). A monitor's
//! [`DisplayInfo::bounds`] are its rectangle in picture px, in the global desktop space (the
//! origin may be negative, like a monitor left of the primary); [`DisplayInfo::scale`] is the
//! monitor's own scale as the compositor reports it. On a mixed-DPI desktop a 1x monitor next to
//! a 2x one is therefore enlarged in the picture: the picture, not the panel, sets the pixels.

use cm_core::{PhysicalPoint, PhysicalRect, PhysicalSize, ScaleFactor};

use crate::DisplayInfo;

/// One monitor as the compositor lays it out.
#[derive(Debug, Clone, PartialEq)]
pub struct LogicalMonitor {
    pub name: String,
    /// Top-left corner in logical px (may be negative).
    pub x: i32,
    pub y: i32,
    /// Size in logical px (the current mode's pixels divided by the scale, rotation applied).
    pub width: u32,
    pub height: u32,
    /// The monitor's own scale (1.0, 1.25, 1.5, 2.0, ...).
    pub scale: f64,
    pub primary: bool,
}

/// The displays of a picture, and where the picture sits in the global desktop space.
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopLayout {
    pub displays: Vec<DisplayInfo>,
    /// Global desktop position (picture px) of the picture's top-left pixel.
    pub origin: PhysicalPoint,
    /// Picture px per logical px.
    pub picture_scale: f64,
}

impl DesktopLayout {
    /// The rectangle of display `id` inside the picture (picture px from its top-left).
    pub fn crop_of(&self, id: u32) -> Option<PhysicalRect> {
        let d = self.displays.iter().find(|d| d.id == id)?;
        Some(PhysicalRect {
            origin: PhysicalPoint {
                x: d.bounds.origin.x - self.origin.x,
                y: d.bounds.origin.y - self.origin.y,
            },
            size: d.bounds.size,
        })
    }
}

/// How far the picture's height may differ from the one the width implies (rounding of
/// fractional scales), in picture px.
const SIZE_SLACK_PX: f64 = 3.0;

/// Maps `monitors` onto a picture of `picture` size. `None` when the list is empty or does not
/// fit the picture (the compositor shot something else than this layout): the caller then
/// reports one "Desktop" display.
///
/// Display ids are 1-based; the primary monitor comes first, then the rest top to bottom, left
/// to right, so `1` is the primary one (as on a single-display desktop).
pub fn layout_for_picture(
    monitors: &[LogicalMonitor],
    picture: PhysicalSize,
) -> Option<DesktopLayout> {
    if monitors.is_empty() || picture.width == 0 || picture.height == 0 {
        return None;
    }
    if monitors
        .iter()
        .any(|m| m.width == 0 || m.height == 0 || !m.scale.is_finite() || m.scale <= 0.0)
    {
        return None;
    }
    let min_x = monitors.iter().map(|m| i64::from(m.x)).min()?;
    let min_y = monitors.iter().map(|m| i64::from(m.y)).min()?;
    let max_x = monitors
        .iter()
        .map(|m| i64::from(m.x) + i64::from(m.width))
        .max()?;
    let max_y = monitors
        .iter()
        .map(|m| i64::from(m.y) + i64::from(m.height))
        .max()?;
    let (lw, lh) = ((max_x - min_x) as f64, (max_y - min_y) as f64);
    let s = f64::from(picture.width) / lw;
    if (f64::from(picture.height) - lh * s).abs() > SIZE_SLACK_PX {
        return None;
    }
    // Edges are rounded, not sizes, so neighbours tile without a gap or an overlap.
    let edge = |logical: i64| ((logical as f64) * s).round() as i64;
    let origin = PhysicalPoint {
        x: i32::try_from(edge(min_x)).ok()?,
        y: i32::try_from(edge(min_y)).ok()?,
    };
    let mut order: Vec<&LogicalMonitor> = monitors.iter().collect();
    order.sort_by_key(|m| (!m.primary, m.y, m.x));
    let mut displays = Vec::with_capacity(order.len());
    for (i, m) in order.iter().enumerate() {
        let (x0, y0) = (edge(i64::from(m.x) - min_x), edge(i64::from(m.y) - min_y));
        let (x1, y1) = (
            edge(i64::from(m.x) + i64::from(m.width) - min_x),
            edge(i64::from(m.y) + i64::from(m.height) - min_y),
        );
        displays.push(DisplayInfo {
            id: u32::try_from(i + 1).ok()?,
            name: m.name.clone(),
            bounds: PhysicalRect {
                origin: PhysicalPoint {
                    x: i32::try_from(x0 + i64::from(origin.x)).ok()?,
                    y: i32::try_from(y0 + i64::from(origin.y)).ok()?,
                },
                size: PhysicalSize {
                    width: u32::try_from(x1 - x0).ok()?,
                    height: u32::try_from(y1 - y0).ok()?,
                },
            },
            scale: ScaleFactor::new(m.scale).ok()?,
            is_primary: m.primary || (i == 0 && !monitors.iter().any(|m| m.primary)),
            is_builtin: false,
        });
    }
    Some(DesktopLayout {
        displays,
        origin,
        picture_scale: s,
    })
}

/// The single "Desktop" display of a picture whose layout is unknown.
pub fn single_desktop(picture: PhysicalSize) -> DesktopLayout {
    DesktopLayout {
        displays: vec![DisplayInfo {
            id: 1,
            name: "Desktop".to_owned(),
            bounds: PhysicalRect {
                origin: PhysicalPoint { x: 0, y: 0 },
                size: picture,
            },
            scale: ScaleFactor::ONE,
            is_primary: true,
            is_builtin: false,
        }],
        origin: PhysicalPoint { x: 0, y: 0 },
        picture_scale: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(
        name: &str,
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        scale: f64,
        primary: bool,
    ) -> LogicalMonitor {
        LogicalMonitor {
            name: name.to_owned(),
            x,
            y,
            width: w,
            height: h,
            scale,
            primary,
        }
    }

    fn size(width: u32, height: u32) -> PhysicalSize {
        PhysicalSize { width, height }
    }

    fn bounds(d: &DisplayInfo) -> (i32, i32, u32, u32) {
        (
            d.bounds.origin.x,
            d.bounds.origin.y,
            d.bounds.size.width,
            d.bounds.size.height,
        )
    }

    #[test]
    fn one_monitor_is_the_whole_picture() {
        // The headless GNOME session: one 1280x720 virtual monitor.
        let l = layout_for_picture(
            &[mon("Virtual-1", 0, 0, 1280, 720, 1.0, true)],
            size(1280, 720),
        )
        .unwrap();
        assert_eq!(l.displays.len(), 1);
        assert_eq!(bounds(&l.displays[0]), (0, 0, 1280, 720));
        assert_eq!(l.displays[0].id, 1);
        assert!(l.displays[0].is_primary);
        assert_eq!(l.displays[0].name, "Virtual-1");
        assert_eq!(l.crop_of(1).unwrap().size, size(1280, 720));
    }

    #[test]
    fn two_monitors_side_by_side_tile_the_picture() {
        let l = layout_for_picture(
            &[
                mon("DP-2", 1920, 0, 2560, 1440, 1.0, false),
                mon("eDP-1", 0, 0, 1920, 1080, 1.0, true),
            ],
            size(4480, 1440),
        )
        .unwrap();
        // The primary is display 1 whatever order the compositor listed it in.
        assert_eq!(l.displays[0].name, "eDP-1");
        assert_eq!(bounds(&l.displays[0]), (0, 0, 1920, 1080));
        assert_eq!(bounds(&l.displays[1]), (1920, 0, 2560, 1440));
        assert_eq!(l.displays[1].id, 2);
        assert_eq!(
            l.crop_of(2).unwrap().origin,
            PhysicalPoint { x: 1920, y: 0 }
        );
    }

    #[test]
    fn mixed_dpi_is_shot_at_the_sharpest_scale() {
        // A 2x laptop panel (1440x900 logical) next to a 1x monitor (1920x1080), picture at 2x.
        let l = layout_for_picture(
            &[
                mon("eDP-1", 0, 0, 1440, 900, 2.0, true),
                mon("HDMI-1", 1440, 0, 1920, 1080, 1.0, false),
            ],
            size(6720, 2160),
        )
        .unwrap();
        assert!((l.picture_scale - 2.0).abs() < 1e-9);
        assert_eq!(bounds(&l.displays[0]), (0, 0, 2880, 1800));
        assert_eq!(l.displays[0].scale.get(), 2.0);
        // The 1x monitor is enlarged to the picture's pixels; its scale stays 1.
        assert_eq!(bounds(&l.displays[1]), (2880, 0, 3840, 2160));
        assert_eq!(l.displays[1].scale.get(), 1.0);
    }

    #[test]
    fn mixed_dpi_shot_at_one_to_one_keeps_logical_pixels() {
        let l = layout_for_picture(
            &[
                mon("eDP-1", 0, 0, 1440, 900, 2.0, true),
                mon("HDMI-1", 1440, 0, 1920, 1080, 1.0, false),
            ],
            size(3360, 1080),
        )
        .unwrap();
        assert!((l.picture_scale - 1.0).abs() < 1e-9);
        assert_eq!(bounds(&l.displays[0]), (0, 0, 1440, 900));
        assert_eq!(bounds(&l.displays[1]), (1440, 0, 1920, 1080));
    }

    #[test]
    fn negative_origins_become_picture_relative_crops() {
        // A monitor left of and above the primary: the layout starts at (-1920, -180).
        let l = layout_for_picture(
            &[
                mon("DP-1", -1920, -180, 1920, 1080, 1.0, false),
                mon("eDP-1", 0, 0, 1536, 864, 1.25, true),
            ],
            size(3456, 1080),
        )
        .unwrap();
        // Global space keeps the negative corner; the picture starts there.
        assert_eq!(l.origin, PhysicalPoint { x: -1920, y: -180 });
        assert_eq!(bounds(&l.displays[0]), (0, 0, 1536, 864));
        assert_eq!(bounds(&l.displays[1]), (-1920, -180, 1920, 1080));
        let crop = l.crop_of(2).unwrap();
        assert_eq!((crop.origin.x, crop.origin.y), (0, 0));
        let crop = l.crop_of(1).unwrap();
        assert_eq!((crop.origin.x, crop.origin.y), (1920, 180));
    }

    #[test]
    fn fractional_scale_picture_rounds_edges_so_neighbours_tile() {
        // 1.5x everywhere, the picture shot at 1.5: logical 1280x720 monitors, 1920x1080 px each.
        let l = layout_for_picture(
            &[
                mon("A", 0, 0, 1280, 720, 1.5, true),
                mon("B", 1280, 0, 1281, 720, 1.5, false),
            ],
            size(3842, 1080),
        )
        .unwrap();
        let (a, b) = (&l.displays[0], &l.displays[1]);
        assert_eq!(
            a.bounds.origin.x + a.bounds.size.width as i32,
            b.bounds.origin.x
        );
        assert_eq!(
            b.bounds.origin.x + b.bounds.size.width as i32,
            3842,
            "the last edge is the picture's edge"
        );
    }

    #[test]
    fn a_picture_that_does_not_fit_gives_no_layout() {
        let m = [mon("A", 0, 0, 1280, 720, 1.0, true)];
        assert!(layout_for_picture(&m, size(1280, 1000)).is_none());
        assert!(layout_for_picture(&[], size(1280, 720)).is_none());
        assert!(
            layout_for_picture(&[mon("A", 0, 0, 0, 720, 1.0, true)], size(1280, 720)).is_none()
        );
        assert!(
            layout_for_picture(&[mon("A", 0, 0, 100, 100, f64::NAN, true)], size(100, 100))
                .is_none()
        );
    }

    #[test]
    fn without_a_primary_the_first_display_is_primary() {
        let l = layout_for_picture(
            &[
                mon("B", 100, 0, 100, 100, 1.0, false),
                mon("A", 0, 0, 100, 100, 1.0, false),
            ],
            size(200, 100),
        )
        .unwrap();
        assert!(l.displays[0].is_primary && !l.displays[1].is_primary);
        assert_eq!(l.displays[0].name, "A");
    }

    #[test]
    fn single_desktop_is_display_one() {
        let l = single_desktop(size(800, 600));
        assert_eq!(l.displays[0].name, "Desktop");
        assert_eq!(l.crop_of(1).unwrap().size, size(800, 600));
        assert!(l.crop_of(2).is_none());
    }
}
