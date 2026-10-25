//! Magnifier geometry (CAP-3): placement with edge flipping, source-pixel mapping, hex color.
//!
//! The loupe is 128 logical px across at 8x: each source (physical) pixel is drawn as an
//! `8 * scale` px cell, and the pixel under the pointer is the center cell.

use cm_core::{PhysicalPoint, PhysicalRect, PhysicalSize};

/// Loupe diameter in logical px (CAP-3).
pub const LOUPE_LOGICAL_PX: f64 = 128.0;
/// Zoom: device px per source px at scale 1.
pub const ZOOM: f64 = 8.0;
/// Within this many logical px of the right or bottom edge the loupe flips (CAP-3.AC3).
pub const EDGE_FLIP_LOGICAL_PX: f64 = 160.0;
const GAP_LOGICAL_PX: f64 = 20.0;
const LABEL_HEIGHT_LOGICAL_PX: f64 = 22.0;
const LABEL_GAP_LOGICAL_PX: f64 = 6.0;

/// Where the loupe circle and its label go, in display px.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoupeLayout {
    /// Bounding square of the circle.
    pub circle: PhysicalRect,
    /// Slot for the `x, y  #RRGGBB` label under the circle (width is the circle's; the painter
    /// centers a pill of its own width in it).
    pub label: PhysicalRect,
}

fn px(logical: f64, scale: f64) -> i32 {
    (logical * scale).round() as i32
}

/// Circle diameter in device px.
pub fn diameter(scale: f64) -> i32 {
    px(LOUPE_LOGICAL_PX, scale)
}

/// Cell size in device px: one source pixel.
pub fn cell(scale: f64) -> i32 {
    px(ZOOM, scale).max(1)
}

/// Places the loupe down-right of the pointer, flips it to the left and/or above near the
/// right and bottom edges, then clamps it so it stays fully on the display (CAP-3.AC3).
pub fn loupe_layout(pointer: PhysicalPoint, display: PhysicalSize, scale: f64) -> LoupeLayout {
    let d = diameter(scale);
    let gap = px(GAP_LOGICAL_PX, scale);
    let label_h = px(LABEL_HEIGHT_LOGICAL_PX, scale);
    let label_gap = px(LABEL_GAP_LOGICAL_PX, scale);
    let total_h = d + label_gap + label_h;
    let (w, h) = (display.width as i32, display.height as i32);
    let edge = px(EDGE_FLIP_LOGICAL_PX, scale);

    let flip_x = w - pointer.x < edge;
    let flip_y = h - pointer.y < edge.max(total_h + gap);
    let x = if flip_x {
        pointer.x - gap - d
    } else {
        pointer.x + gap
    };
    let y = if flip_y {
        pointer.y - gap - total_h
    } else {
        pointer.y + gap
    };
    let x = x.clamp(0, (w - d).max(0));
    let y = y.clamp(0, (h - total_h).max(0));
    LoupeLayout {
        circle: PhysicalRect {
            origin: PhysicalPoint { x, y },
            size: PhysicalSize {
                width: d as u32,
                height: d as u32,
            },
        },
        label: PhysicalRect {
            origin: PhysicalPoint {
                x,
                y: y + d + label_gap,
            },
            size: PhysicalSize {
                width: d as u32,
                height: label_h as u32,
            },
        },
    }
}

/// Source pixel offset from the center pixel for a device-px offset `d` from the loupe
/// center. The center cell spans `[-cell/2, cell/2)`; cells tile outward.
pub fn source_offset(d: i32, cell: i32) -> i32 {
    (d + cell / 2).div_euclid(cell.max(1))
}

/// `#RRGGBB` from a `0x00RRGGBB` pixel (CAP-3.AC4).
pub fn hex_color(rgb: u32) -> String {
    format!("#{:06X}", rgb & 0x00FF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISPLAY: PhysicalSize = PhysicalSize {
        width: 1920,
        height: 1080,
    };

    fn on_screen(l: &LoupeLayout) -> bool {
        let r = l.circle;
        let lab = l.label;
        r.origin.x >= 0
            && r.origin.y >= 0
            && r.origin.x + r.size.width as i32 <= DISPLAY.width as i32
            && lab.origin.y + lab.size.height as i32 <= DISPLAY.height as i32
    }

    #[test]
    fn loupe_goes_down_right_of_the_pointer_by_default() {
        let l = loupe_layout(PhysicalPoint { x: 500, y: 400 }, DISPLAY, 1.0);
        assert_eq!(l.circle.origin, PhysicalPoint { x: 520, y: 420 });
        assert_eq!(l.circle.size.width, 128);
        assert!(on_screen(&l));
    }

    /// CAP-3.AC3: within 160 px of the right edge the loupe is on the left, fully on screen.
    #[test]
    fn cap3_ac3_flips_left_near_the_right_edge() {
        for scale in [1.0, 2.0] {
            let edge = (160.0 * scale) as i32;
            let p = PhysicalPoint {
                x: DISPLAY.width as i32 - edge + 1,
                y: 300,
            };
            let l = loupe_layout(p, DISPLAY, scale);
            assert!(
                l.circle.origin.x + l.circle.size.width as i32 <= p.x,
                "scale {scale}"
            );
            let at_edge = loupe_layout(
                PhysicalPoint {
                    x: DISPLAY.width as i32 - 1,
                    y: 300,
                },
                DISPLAY,
                scale,
            );
            assert!(
                at_edge.circle.origin.x + at_edge.circle.size.width as i32 <= DISPLAY.width as i32
            );
        }
    }

    #[test]
    fn flips_up_near_the_bottom_and_stays_on_screen_in_every_corner() {
        for (x, y) in [(0, 0), (1919, 0), (0, 1079), (1919, 1079), (960, 1079)] {
            for scale in [1.0, 2.0] {
                let l = loupe_layout(PhysicalPoint { x, y }, DISPLAY, scale);
                assert!(on_screen(&l), "({x},{y}) scale {scale}: {l:?}");
            }
        }
        let bottom = loupe_layout(PhysicalPoint { x: 500, y: 1070 }, DISPLAY, 1.0);
        assert!(bottom.label.origin.y + bottom.label.size.height as i32 <= 1070);
    }

    /// CAP-3.AC1: the center cell is the pixel under the pointer; cells tile outward.
    #[test]
    fn cap3_ac1_center_cell_is_the_pointer_pixel() {
        for cell in [8, 12, 16] {
            assert_eq!(source_offset(0, cell), 0);
            assert_eq!(source_offset(cell / 2 - 1, cell), 0);
            assert_eq!(source_offset(-cell / 2, cell), 0);
            assert_eq!(source_offset(cell / 2, cell), 1);
            assert_eq!(source_offset(-cell / 2 - 1, cell), -1);
        }
    }

    #[test]
    fn cell_scales_with_the_display() {
        assert_eq!(cell(1.0), 8);
        assert_eq!(cell(2.0), 16);
        assert_eq!(diameter(2.0), 256);
    }

    /// CAP-3.AC4: `#RRGGBB`, upper case, zero padded.
    #[test]
    fn cap3_ac4_hex_color_format() {
        assert_eq!(hex_color(0x00A1B2C3), "#A1B2C3");
        assert_eq!(hex_color(0x0000000F), "#00000F");
        assert_eq!(hex_color(0xFFFF_FFFF), "#FFFFFF");
    }
}
