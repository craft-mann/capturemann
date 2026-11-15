//! Where the Long Shot HUD and side preview go (design S2b), from what the overlay reported.
//! Pure arithmetic: the rules themselves are `cm_core::long_shot_hud`, shared with the overlay.

use cm_core::long_shot_hud::{place_hud, place_preview, HudPlacement, PreviewPlace, Rect as HRect};

use crate::recording::geometry::{area_points, Rect};
use crate::recording::monitors::EngineDisplay;

/// The widest the HUD gets, points (S20: 640, and the display minus 48; `bar::plan` decides the
/// width of the window from what it shows).
pub const HUD_W: f64 = super::bar::BAR_MAX;

/// What to open.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// The HUD, global points.
    pub hud: Rect,
    /// The side preview panel, global points, or `None` when the HUD gets a Preview button.
    pub preview: Option<Rect>,
    pub preview_button: bool,
}

fn h(r: &Rect) -> HRect {
    HRect::new(r.x, r.y, r.w, r.h)
}

fn r(x: HRect) -> Rect {
    Rect::new(x.x, x.y, x.w, x.h)
}

/// The picked area (`[x, y, w, h]` physical px relative to display `display_id`) in global
/// physical px, from the engine's own display list. `None` when the display is not listed.
pub fn area_global_px(
    engine: &[EngineDisplay],
    display_id: u32,
    area: [i64; 4],
) -> Option<[i64; 4]> {
    let d = engine.iter().find(|d| d.id == display_id)?;
    Some([
        d.rect_px.x.round() as i64 + area[0],
        d.rect_px.y.round() as i64 + area[1],
        area[2],
        area[3],
    ])
}

/// The area in global points (the space GPUI windows use on this platform; see
/// `recording::monitors`).
pub fn area_in_points(global_px: [i64; 4], scale: f64) -> Rect {
    area_points(global_px, scale)
}

/// The HUD (`hud_w` wide) and preview for `area` on `display` (both global points; `display`
/// should be the part a window may use, without the menu bar).
pub fn layout(display: &Rect, area: &Rect, hud_w: f64) -> Layout {
    let HudPlacement { rect, .. } = place_hud(h(display), h(area), hud_w);
    let (preview, preview_button) = match place_preview(h(display), h(area)) {
        PreviewPlace::Right(p) | PreviewPlace::Left(p) => (Some(r(p)), false),
        PreviewPlace::Button => (None, true),
    };
    Layout {
        hud: r(rect),
        preview,
        preview_button,
    }
}

/// The floating panel the Preview button opens when the area leaves no room beside it (S2b):
/// over the far side of the display, 360 x 480 at most, kept inside the display.
pub fn floating_preview(display: &Rect, area: &Rect) -> Rect {
    let (w, hh) = (360.0_f64.min(display.w), 480.0_f64.min(display.h));
    let area_mid = area.x + area.w / 2.0;
    let on_left_half = area_mid < display.x + display.w / 2.0;
    let x = if on_left_half {
        display.x + display.w - w - 16.0
    } else {
        display.x + 16.0
    };
    let y = display.y + (display.h - hh) / 2.0;
    Rect::new(
        x.clamp(display.x, (display.x + display.w - w).max(display.x)),
        y,
        w,
        hh,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1440.0,
        h: 900.0,
    };

    fn engine() -> Vec<EngineDisplay> {
        vec![
            EngineDisplay {
                id: 1,
                rect_px: Rect::new(0.0, 0.0, 2880.0, 1800.0),
                primary: true,
            },
            EngineDisplay {
                id: 2,
                rect_px: Rect::new(2880.0, -200.0, 1920.0, 1080.0),
                primary: false,
            },
        ]
    }

    #[test]
    fn the_area_is_moved_to_the_displays_place_in_the_desktop() {
        assert_eq!(
            area_global_px(&engine(), 2, [100, 50, 800, 600]),
            Some([2980, -150, 800, 600])
        );
        assert_eq!(area_global_px(&engine(), 9, [0, 0, 1, 1]), None);
        let p = area_in_points([2980, -150, 800, 600], 2.0);
        assert_eq!((p.x, p.y, p.w, p.h), (1490.0, -75.0, 400.0, 300.0));
    }

    /// S2b: above the area when there is room, centred, and the preview on the right.
    #[test]
    fn s2b_the_hud_goes_above_and_the_preview_to_the_right() {
        let area = Rect::new(300.0, 120.0, 600.0, 560.0);
        let l = layout(&D, &area, HUD_W);
        assert_eq!(l.hud.y, 120.0 - 12.0 - 56.0);
        assert_eq!(l.hud.x + l.hud.w / 2.0, 600.0);
        assert_eq!(l.hud.w, HUD_W);
        let p = l.preview.unwrap();
        assert_eq!((p.x, p.y, p.h), (912.0, 120.0, 560.0));
        assert!(!l.preview_button);
    }

    /// S2b: an area touching the top goes below; a full-height area takes the HUD inside at the
    /// bottom; a full-display area has no room for the preview and gets the Preview button.
    #[test]
    fn s2b_top_edge_full_height_and_full_display() {
        let l = layout(&D, &Rect::new(300.0, 10.0, 600.0, 500.0), HUD_W);
        assert_eq!(l.hud.y, 10.0 + 500.0 + 12.0);
        let l = layout(&D, &Rect::new(300.0, 0.0, 800.0, 900.0), HUD_W);
        assert_eq!(l.hud.y, 900.0 - 16.0 - 56.0);
        let l = layout(&D, &D, HUD_W);
        assert_eq!(l.hud.y, 900.0 - 16.0 - 56.0);
        assert!(l.preview.is_none() && l.preview_button);
    }

    #[test]
    fn the_floating_preview_sits_over_the_far_side_inside_the_display() {
        let left_area = Rect::new(0.0, 0.0, 1440.0, 900.0);
        let p = floating_preview(&D, &left_area);
        assert!(p.x >= 0.0 && p.x + p.w <= 1440.0 && p.y >= 0.0 && p.y + p.h <= 900.0);
        let right_area = Rect::new(900.0, 0.0, 540.0, 900.0);
        assert!(floating_preview(&D, &right_area).x < 100.0);
        assert!(floating_preview(&D, &Rect::new(0.0, 0.0, 400.0, 900.0)).x > 900.0);
        // A tiny display still gives a panel inside it.
        let tiny = Rect::new(0.0, 0.0, 300.0, 200.0);
        let p = floating_preview(&tiny, &tiny);
        assert!(p.w <= 300.0 && p.h <= 200.0);
    }
}
