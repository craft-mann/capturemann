//! Where the Long Shot HUD and the side preview go (design board S2b, CAP-6 "HUD and preview
//! placement"). Pure geometry in logical px on one display, so the overlay only draws.
//!
//! The HUD (56 px, excluded from the frames) avoids the content being scrolled: above the area
//! when 64 px of room is there (the HUD plus a 12 px gap, rounded), else below, else inside the
//! area at the bottom centre, 16 px up. The side preview goes on the side with more room (right
//! first); when neither side has 200 px it becomes a Preview button on the HUD. The place is
//! chosen once when the capture starts; the HUD always stays on the display.

/// A rectangle in logical px (x, y = top-left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Rect { x, y, w, h }
    }

    fn right(&self) -> f64 {
        self.x + self.w
    }

    fn bottom(&self) -> f64 {
        self.y + self.h
    }
}

pub const HUD_HEIGHT: f64 = 56.0;
pub const HUD_GAP: f64 = 12.0;
/// Room above or below the area that the HUD needs (the HUD and its gap, rounded).
pub const HUD_ROOM: f64 = 64.0;
/// Distance from the area's bottom edge when the HUD sits inside it.
pub const HUD_INSIDE_INSET: f64 = 16.0;
pub const PREVIEW_WIDTH: f64 = 360.0;
/// Least room a side needs to hold the preview.
pub const PREVIEW_MIN_ROOM: f64 = 200.0;

/// Where the HUD sits relative to the area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudSide {
    Above,
    Below,
    /// The area fills the display height: the bottom centre inside the area.
    Inside,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HudPlacement {
    pub side: HudSide,
    pub rect: Rect,
}

/// The HUD of width `hud_w` for `area` on `display`.
pub fn place_hud(display: Rect, area: Rect, hud_w: f64) -> HudPlacement {
    let room_above = area.y - display.y;
    let room_below = display.bottom() - area.bottom();
    let (side, y) = if room_above >= HUD_ROOM {
        (HudSide::Above, area.y - HUD_GAP - HUD_HEIGHT)
    } else if room_below >= HUD_ROOM {
        (HudSide::Below, area.bottom() + HUD_GAP)
    } else {
        (
            HudSide::Inside,
            area.bottom() - HUD_INSIDE_INSET - HUD_HEIGHT,
        )
    };
    let w = hud_w.min(display.w);
    let x = (area.x + (area.w - w) / 2.0).clamp(display.x, display.right() - w);
    let y = y.clamp(display.y, (display.bottom() - HUD_HEIGHT).max(display.y));
    HudPlacement {
        side,
        rect: Rect::new(x, y, w, HUD_HEIGHT),
    }
}

/// The side preview: a panel beside the area, or a Preview button on the HUD.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PreviewPlace {
    Right(Rect),
    Left(Rect),
    /// Neither side has room: the HUD shows a Preview button that opens a floating panel.
    Button,
}

pub fn place_preview(display: Rect, area: Rect) -> PreviewPlace {
    let right = display.right() - area.right();
    let left = area.x - display.x;
    if right >= left && right >= PREVIEW_MIN_ROOM {
        let w = PREVIEW_WIDTH.min(right - HUD_GAP);
        PreviewPlace::Right(Rect::new(area.right() + HUD_GAP, area.y, w, area.h))
    } else if left >= PREVIEW_MIN_ROOM {
        let w = PREVIEW_WIDTH.min(left - HUD_GAP);
        PreviewPlace::Left(Rect::new(area.x - HUD_GAP - w, area.y, w, area.h))
    } else {
        PreviewPlace::Button
    }
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

    #[test]
    fn s2b_room_above_puts_the_hud_above_centred_with_a_12_px_gap() {
        let area = Rect::new(300.0, 120.0, 800.0, 560.0);
        let p = place_hud(D, area, 400.0);
        assert_eq!(p.side, HudSide::Above);
        assert_eq!(p.rect, Rect::new(500.0, 120.0 - 12.0 - 56.0, 400.0, 56.0));
    }

    #[test]
    fn s2b_less_than_64_px_above_goes_below() {
        let area = Rect::new(300.0, 40.0, 800.0, 560.0);
        let p = place_hud(D, area, 400.0);
        assert_eq!(p.side, HudSide::Below);
        assert_eq!(p.rect.y, 40.0 + 560.0 + 12.0);
        // Exactly 64 px above still counts as room.
        let p = place_hud(D, Rect::new(300.0, 64.0, 800.0, 560.0), 400.0);
        assert_eq!(p.side, HudSide::Above);
    }

    #[test]
    fn s2b_full_height_area_puts_the_hud_inside_at_the_bottom_16_px_up() {
        let area = Rect::new(300.0, 0.0, 800.0, 900.0);
        let p = place_hud(D, area, 400.0);
        assert_eq!(p.side, HudSide::Inside);
        assert_eq!(p.rect.y, 900.0 - 16.0 - 56.0);
        assert_eq!(p.rect.x, 500.0);
    }

    #[test]
    fn s2b_the_hud_never_leaves_the_display() {
        let p = place_hud(D, Rect::new(0.0, 200.0, 200.0, 300.0), 400.0);
        assert_eq!(p.rect.x, 0.0);
        let p = place_hud(D, Rect::new(1300.0, 200.0, 140.0, 300.0), 400.0);
        assert_eq!(p.rect.x + p.rect.w, 1440.0);
        // A HUD wider than the display shrinks to it.
        let small = Rect::new(0.0, 0.0, 300.0, 600.0);
        let p = place_hud(small, Rect::new(0.0, 100.0, 300.0, 400.0), 400.0);
        assert_eq!(p.rect.w, 300.0);
    }

    #[test]
    fn s2b_the_preview_takes_the_side_with_more_room_right_first() {
        let area = Rect::new(300.0, 100.0, 600.0, 600.0);
        let PreviewPlace::Right(r) = place_preview(D, area) else {
            panic!()
        };
        assert_eq!((r.x, r.w, r.y, r.h), (912.0, 360.0, 100.0, 600.0));
        let area = Rect::new(800.0, 100.0, 500.0, 600.0);
        let PreviewPlace::Left(r) = place_preview(D, area) else {
            panic!()
        };
        assert_eq!((r.x + r.w, r.w), (788.0, 360.0));
        // Equal room: right first.
        let area = Rect::new(420.0, 100.0, 600.0, 600.0);
        assert!(matches!(place_preview(D, area), PreviewPlace::Right(_)));
    }

    #[test]
    fn s2b_no_side_with_200_px_becomes_a_preview_button() {
        let area = Rect::new(100.0, 100.0, 1240.0, 600.0);
        assert_eq!(place_preview(D, area), PreviewPlace::Button);
        assert_eq!(place_preview(D, D), PreviewPlace::Button);
        // 199 px is not enough, 200 is.
        let area = Rect::new(199.0, 0.0, 1042.0, 900.0);
        assert_eq!(place_preview(D, area), PreviewPlace::Button);
        let area = Rect::new(200.0, 0.0, 1040.0, 900.0);
        assert!(matches!(
            place_preview(D, area),
            PreviewPlace::Right(_) | PreviewPlace::Left(_)
        ));
    }
}
