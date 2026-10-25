//! The camera card of the inspector (CAM-3), display-free: which layout a range has, the bubble's
//! corner, size, shape and ring, and the arguments of `set_camera_layout` for a change.
//!
//! The card never edits the edit list: [`params`] is the daemon call, and the answer replaces the
//! list (like every other edit). A range with no stored layout reads as the recorder's bubble
//! (`layout.camera`), which is what [`effective`] resolves.

use cm_core::camera_layout::{
    BubbleKind, CameraLayoutSpec, CameraMode, LayoutTransition, DEFAULT_RING,
};
use cm_core::Span;
use cm_i18n::tr_ctx;
use cm_media::layout::{CameraLayout, Shape};
use serde_json::{json, Value};

/// The layouts offered, in the order shown (CAM-3: screen + bubble, screen only, camera full,
/// side by side).
pub const MODES: [CameraMode; 4] = [
    CameraMode::Bubble,
    CameraMode::Screen,
    CameraMode::Camera,
    CameraMode::SideBySide,
];

/// The translated name of a layout (the tile's label).
pub fn mode_label(mode: CameraMode) -> String {
    match mode {
        CameraMode::Bubble => tr_ctx!("camera layout", "Bubble"),
        CameraMode::Screen => tr_ctx!("camera layout", "Screen only"),
        CameraMode::Camera => tr_ctx!("camera layout", "Camera full"),
        CameraMode::SideBySide => tr_ctx!("camera layout", "Side by side"),
    }
}

/// Bubble widths offered as buttons, as a fraction of the frame width.
pub const SIZES: [(&str, f64); 3] = [("S", 0.14), ("M", 0.2), ("L", 0.28)];

/// Distance of a corner bubble from the frame edge, as a fraction of the frame width.
const MARGIN: f64 = 0.03;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub const ALL: [Corner; 4] = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomLeft,
        Corner::BottomRight,
    ];

    pub fn label(self) -> String {
        match self {
            Corner::TopLeft => tr_ctx!("bubble corner", "Top left"),
            Corner::TopRight => tr_ctx!("bubble corner", "Top right"),
            Corner::BottomLeft => tr_ctx!("bubble corner", "Bottom left"),
            Corner::BottomRight => tr_ctx!("bubble corner", "Bottom right"),
        }
    }
}

/// What a layout range draws once the recorder's bubble fills in what it leaves out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Effective {
    pub shape: BubbleKind,
    /// `[x, y, w, h]` as fractions of the frame.
    pub rect: [f64; 4],
    /// Ring width as a fraction of the bubble width; 0 is no ring.
    pub ring: f64,
    pub mirror: bool,
}

fn kind_of(s: Shape) -> BubbleKind {
    match s {
        Shape::Circle => BubbleKind::Circle,
        Shape::Rounded => BubbleKind::Rounded,
    }
}

/// The bubble `spec` draws, with the recorder's settings (`base`, else the defaults) behind it.
pub fn effective(spec: &CameraLayoutSpec, base: Option<&CameraLayout>) -> Effective {
    Effective {
        shape: spec
            .shape
            .or_else(|| base.map(|b| kind_of(b.shape)))
            .unwrap_or(BubbleKind::Circle),
        rect: spec
            .rect
            .or_else(|| base.map(|b| b.rect))
            .unwrap_or([0.78, 0.7, 0.2, 0.2]),
        ring: spec
            .ring
            .or_else(|| base.map(|b| b.ring))
            .unwrap_or(DEFAULT_RING),
        mirror: spec
            .mirror
            .or_else(|| base.and_then(|b| b.mirror))
            .unwrap_or(false),
    }
}

/// The corner nearest to the middle of `rect`.
pub fn corner_of(rect: [f64; 4]) -> Corner {
    let (cx, cy) = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
    match (cx < 0.5, cy < 0.5) {
        (true, true) => Corner::TopLeft,
        (false, true) => Corner::TopRight,
        (true, false) => Corner::BottomLeft,
        (false, false) => Corner::BottomRight,
    }
}

/// The size button a width belongs to, if it is one of the presets.
pub fn size_label(w: f64) -> Option<&'static str> {
    SIZES
        .iter()
        .find(|(_, s)| (s - w).abs() < 0.01)
        .map(|(l, _)| *l)
}

/// A bubble of width `w` in `corner`, `margin` in from the edges. `aspect` is the frame's width
/// over height: a circle is as tall as it is wide in pixels, a rounded bubble is 4:3.
pub fn corner_rect(corner: Corner, w: f64, aspect: f64, shape: BubbleKind) -> [f64; 4] {
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        16.0 / 9.0
    };
    let w = w.clamp(0.05, 0.5);
    let h = match shape {
        BubbleKind::Circle => w * aspect,
        BubbleKind::Rounded => w * aspect * 0.75,
    }
    .min(0.9);
    let (mx, my) = (MARGIN, MARGIN * aspect);
    let x = match corner {
        Corner::TopLeft | Corner::BottomLeft => mx,
        Corner::TopRight | Corner::BottomRight => 1.0 - mx - w,
    };
    let y = match corner {
        Corner::TopLeft | Corner::TopRight => my,
        Corner::BottomLeft | Corner::BottomRight => 1.0 - my - h,
    };
    [x, y, w, h].map(|v| (v * 10_000.0).round() / 10_000.0)
}

/// Another layout. A layout other than the bubble carries no bubble settings (the daemon refuses
/// them); the transition is kept.
pub fn with_mode(spec: &CameraLayoutSpec, mode: CameraMode) -> CameraLayoutSpec {
    let mut s = spec.clone();
    s.mode = mode;
    if mode != CameraMode::Bubble {
        s.shape = None;
        s.rect = None;
        s.ring = None;
    }
    s
}

/// The bubble in another corner, at its current width.
pub fn with_corner(
    spec: &CameraLayoutSpec,
    base: Option<&CameraLayout>,
    corner: Corner,
    aspect: f64,
) -> CameraLayoutSpec {
    let e = effective(spec, base);
    let mut s = spec.clone();
    s.rect = Some(corner_rect(corner, e.rect[2], aspect, e.shape));
    s
}

/// The bubble at another width, keeping its corner.
pub fn with_size(
    spec: &CameraLayoutSpec,
    base: Option<&CameraLayout>,
    w: f64,
    aspect: f64,
) -> CameraLayoutSpec {
    let e = effective(spec, base);
    let mut s = spec.clone();
    s.rect = Some(corner_rect(corner_of(e.rect), w, aspect, e.shape));
    s
}

/// The bubble in another shape, in the same corner and at the same width.
pub fn with_shape(
    spec: &CameraLayoutSpec,
    base: Option<&CameraLayout>,
    shape: BubbleKind,
    aspect: f64,
) -> CameraLayoutSpec {
    let e = effective(spec, base);
    let mut s = spec.clone();
    s.shape = Some(shape);
    s.rect = Some(corner_rect(corner_of(e.rect), e.rect[2], aspect, shape));
    s
}

/// The ring on (the recorder's default width) or off.
pub fn with_ring(spec: &CameraLayoutSpec, on: bool) -> CameraLayoutSpec {
    let mut s = spec.clone();
    s.ring = Some(if on { DEFAULT_RING } else { 0.0 });
    s
}

pub fn with_mirror(spec: &CameraLayoutSpec, mirror: bool) -> CameraLayoutSpec {
    let mut s = spec.clone();
    s.mirror = Some(mirror);
    s
}

/// A smooth move from the layout before (300 ms), or a cut.
pub fn with_smooth(spec: &CameraLayoutSpec, smooth: bool) -> CameraLayoutSpec {
    let mut s = spec.clone();
    s.transition = if smooth {
        LayoutTransition::Smooth
    } else {
        LayoutTransition::Cut
    };
    s
}

/// The arguments of `set_camera_layout` for `spec` over `span` (without `capture_id`).
pub fn params(span: Span, spec: &CameraLayoutSpec) -> Value {
    let mut p = json!({
        "range": [format!("t:{}", span.from.ticks()), format!("t:{}", span.to.ticks())],
        "layout": spec.mode.as_str(),
        "transition": spec.transition.as_str(),
    });
    if let Some(s) = spec.shape {
        p["shape"] = json!(s.as_str());
    }
    if let Some(r) = spec.rect {
        p["rect"] = json!(r);
    }
    if let Some(g) = spec.ring {
        p["ring"] = json!(g);
    }
    if let Some(m) = spec.mirror {
        p["mirror"] = json!(m);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::MediaTime;

    fn base() -> CameraLayout {
        CameraLayout {
            visible: true,
            shape: Shape::Circle,
            rect: [0.78, 0.7, 0.16, 0.28],
            mirror: Some(true),
            ring: 0.0167,
            radius: 0.1,
            size: None,
        }
    }

    fn bubble() -> CameraLayoutSpec {
        CameraLayoutSpec::new(CameraMode::Bubble)
    }

    /// A range with nothing stored reads as the recorder's bubble.
    #[test]
    fn the_recorders_bubble_fills_in_what_a_range_leaves_out() {
        let e = effective(&bubble(), Some(&base()));
        assert_eq!(e.rect, [0.78, 0.7, 0.16, 0.28]);
        assert_eq!(e.shape, BubbleKind::Circle);
        assert!(e.mirror && e.ring > 0.0);
        assert_eq!(corner_of(e.rect), Corner::BottomRight);
        let mut s = bubble();
        s.shape = Some(BubbleKind::Rounded);
        s.mirror = Some(false);
        let e = effective(&s, Some(&base()));
        assert_eq!((e.shape, e.mirror), (BubbleKind::Rounded, false));
    }

    /// CAM-3.AC1: dragging the bubble to the top left is the rect `corner_rect` gives, inside
    /// the frame with the margin, and the call carries it.
    #[test]
    fn cam3_ac1_top_left_is_a_rect_inside_the_frame() {
        let s = with_corner(&bubble(), Some(&base()), Corner::TopLeft, 16.0 / 9.0);
        let r = s.rect.unwrap();
        assert_eq!(corner_of(r), Corner::TopLeft);
        assert!(r[0] > 0.0 && r[1] > 0.0 && r[0] + r[2] < 0.5 && r[1] + r[3] < 0.5);
        // A circle is as tall as it is wide in pixels: h = w * (16 / 9).
        assert!((r[3] - r[2] * 16.0 / 9.0).abs() < 1e-3);
        let span = Span::new(MediaTime::ZERO, MediaTime::from_secs(15));
        let p = params(span, &s);
        assert_eq!(p["layout"], "bubble");
        assert_eq!(p["range"][1], "t:1800000");
        assert_eq!(p["rect"].as_array().unwrap().len(), 4);
        // Every corner stays inside the frame in a 9:16 frame too.
        for c in Corner::ALL {
            let r = corner_rect(c, 0.28, 9.0 / 16.0, BubbleKind::Rounded);
            assert!(
                r[0] >= 0.0 && r[1] >= 0.0 && r[0] + r[2] <= 1.0 && r[1] + r[3] <= 1.0,
                "{c:?} {r:?}"
            );
        }
    }

    /// A layout other than the bubble carries no bubble settings, so the daemon accepts it.
    #[test]
    fn other_layouts_drop_the_bubble_settings() {
        let s = with_corner(&bubble(), Some(&base()), Corner::TopLeft, 16.0 / 9.0);
        let s = with_ring(&s, false);
        let cam = with_mode(&s, CameraMode::Camera);
        assert_eq!((cam.shape, cam.rect, cam.ring), (None, None, None));
        let p = params(Span::new(MediaTime::ZERO, MediaTime::from_secs(1)), &cam);
        assert!(p.get("rect").is_none() && p.get("ring").is_none());
        assert_eq!(p["layout"], "camera");
        // Mirror and the transition survive a change of layout.
        let m = with_smooth(&with_mirror(&s, true), true);
        let back = with_mode(&m, CameraMode::SideBySide);
        assert_eq!(back.mirror, Some(true));
        assert_eq!(back.transition, LayoutTransition::Smooth);
    }

    /// Size and shape keep the corner; the ring switch is 0 or the default.
    #[test]
    fn size_shape_and_ring_keep_the_corner() {
        let s = with_corner(&bubble(), Some(&base()), Corner::TopRight, 16.0 / 9.0);
        let big = with_size(&s, Some(&base()), 0.28, 16.0 / 9.0);
        assert_eq!(corner_of(big.rect.unwrap()), Corner::TopRight);
        assert_eq!(size_label(big.rect.unwrap()[2]), Some("L"));
        let rounded = with_shape(&big, Some(&base()), BubbleKind::Rounded, 16.0 / 9.0);
        let r = rounded.rect.unwrap();
        assert_eq!(corner_of(r), Corner::TopRight);
        assert!((r[3] - r[2] * 16.0 / 9.0 * 0.75).abs() < 1e-3);
        assert_eq!(with_ring(&s, false).ring, Some(0.0));
        assert_eq!(with_ring(&s, true).ring, Some(DEFAULT_RING));
    }
}
