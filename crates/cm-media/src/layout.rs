//! The camera bubble as stored in the manifest and as pixels on the output frame.
//!
//! The recording engine records the bubble facts (`shape`, `size` preset `s|m|l`, `position`,
//! `mirror`) and at stop persists the *layout at stop* in `edits[0].layout.camera` (technical
//! design, "Data model"):
//!
//! ```json
//! { "visible": true, "shape": "circle", "rect": [0.78, 0.72, 0.16, 0.28],
//!   "mirror": true, "size": "m", "ring": 0.0167, "radius": 0.1 }
//! ```
//!
//! `rect` is `[x, y, w, h]` as **fractions of the recorded area** (which is the output frame, since
//! the quick export keeps the screen master's aspect ratio). `w` is authoritative: the height of a
//! circle equals its width in pixels and a rounded bubble is 4:3, so `h` is informative. `ring` is
//! the rec ring width as a fraction of the bubble width (4 px of a 240 px medium bubble) and
//! `radius` the corner radius of a rounded bubble (24 px of 240).
//!
//! The geometry matches `apps/desktop` (`recording/geometry.rs`): presets are 160, 240 and 320 pt
//! wide in the recorded area's point space, anchors keep a 24 pt margin, a fraction position is
//! the bubble's centre.
//!
//! **Known gap:** the engine does not record bubble moves *during* the recording (the bubble is a
//! window of the desktop app, drawn again at export). The layout is the one at stop, applied to
//! the whole video.

use serde_json::{json, Value};

/// Small, medium and large bubble width in points (CAM-1, OD-1).
pub const SIZE_PRESETS_PT: [(&str, f64); 3] = [("s", 160.0), ("m", 240.0), ("l", 320.0)];
/// Ring width in points; part of the bubble's outer size (DESIGN 5.4).
pub const RING_PT: f64 = 4.0;
/// Margin of a named anchor from the area edge, in points.
pub const MARGIN_PT: f64 = 24.0;
/// Corner radius of a rounded bubble, in points.
pub const ROUNDED_RADIUS_PT: f64 = 24.0;
/// Rec ring colour `#FF4F3A` (DESIGN: the bubble ring is the only place the export uses it).
pub const RING_RGB: [u8; 3] = [0xFF, 0x4F, 0x3A];

/// Bubble shape (CAM-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Circle,
    Rounded,
}

impl Shape {
    pub fn parse(s: &str) -> Option<Shape> {
        match s {
            "circle" => Some(Shape::Circle),
            "rounded" => Some(Shape::Rounded),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Shape::Circle => "circle",
            Shape::Rounded => "rounded",
        }
    }

    /// Height over width of the bubble box.
    pub fn aspect_h_over_w(self) -> f64 {
        match self {
            Shape::Circle => 1.0,
            Shape::Rounded => 0.75,
        }
    }
}

/// `layout.camera` of the edit list.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraLayout {
    pub visible: bool,
    pub shape: Shape,
    /// `[x, y, w, h]`, fractions of the output frame.
    pub rect: [f64; 4],
    /// Overrides the track's `mirror` flag when present.
    pub mirror: Option<bool>,
    /// Ring width as a fraction of the bubble width.
    pub ring: f64,
    /// Corner radius of a rounded bubble as a fraction of the bubble width.
    pub radius: f64,
    /// The size preset it came from (`s|m|l`), informative.
    pub size: Option<String>,
}

pub const DEFAULT_RING: f64 = RING_PT / 240.0;
pub const DEFAULT_RADIUS: f64 = ROUNDED_RADIUS_PT / 240.0;

fn round(x: f64, places: i32) -> f64 {
    let m = 10f64.powi(places);
    (x * m).round() / m
}

impl CameraLayout {
    /// The manifest JSON.
    pub fn to_json(&self) -> Value {
        let mut v = json!({
            "visible": self.visible,
            "shape": self.shape.as_str(),
            "rect": self.rect.map(|x| round(x, 5)),
            "ring": round(self.ring, 5),
            "radius": round(self.radius, 5),
        });
        if let (Some(m), Some(o)) = (self.mirror, v.as_object_mut()) {
            o.insert("mirror".into(), json!(m));
        }
        if let (Some(s), Some(o)) = (&self.size, v.as_object_mut()) {
            o.insert("size".into(), json!(s));
        }
        v
    }

    /// Reads `layout.camera`. Returns `None` when it has no usable `rect` (callers then derive
    /// the layout from the recording settings).
    pub fn from_json(v: &Value) -> Option<CameraLayout> {
        let rect = v.get("rect")?.as_array()?;
        if rect.len() != 4 {
            return None;
        }
        let mut r = [0.0; 4];
        for (slot, x) in r.iter_mut().zip(rect) {
            *slot = x.as_f64().filter(|n| n.is_finite())?;
        }
        if r[2] <= 0.0 {
            return None;
        }
        Some(CameraLayout {
            visible: v.get("visible").and_then(Value::as_bool).unwrap_or(true),
            shape: v
                .get("shape")
                .and_then(Value::as_str)
                .and_then(Shape::parse)
                .unwrap_or(Shape::Circle),
            rect: r,
            mirror: v.get("mirror").and_then(Value::as_bool),
            ring: v
                .get("ring")
                .and_then(Value::as_f64)
                .filter(|n| (0.0..0.5).contains(n))
                .unwrap_or(DEFAULT_RING),
            radius: v
                .get("radius")
                .and_then(Value::as_f64)
                .filter(|n| (0.0..=0.5).contains(n))
                .unwrap_or(DEFAULT_RADIUS),
            size: v.get("size").and_then(Value::as_str).map(str::to_owned),
        })
    }
}

/// Width in points of a size preset (`m` for anything unknown).
pub fn preset_width_pt(size: &str) -> f64 {
    SIZE_PRESETS_PT
        .iter()
        .find(|(k, _)| *k == size)
        .map_or(240.0, |(_, w)| *w)
}

/// Builds the layout from the engine's bubble facts (`indicator.camera` / `settings.camera`).
///
/// `position` is a named anchor (`bottom-right`, ...) or `[x, y]` = the bubble centre as fractions
/// of the recorded area; `area_pt` is the recorded area in points (source pixels over scale).
pub fn camera_layout(
    shape: &str,
    size: &str,
    position: &Value,
    mirror: Option<bool>,
    area_pt: (f64, f64),
) -> CameraLayout {
    let shape_v = Shape::parse(shape).unwrap_or(Shape::Circle);
    let (aw, ah) = (area_pt.0.max(1.0), area_pt.1.max(1.0));
    let w = preset_width_pt(size).min(aw);
    let h = if shape_v == Shape::Circle {
        w
    } else {
        (w * shape_v.aspect_h_over_w()).round()
    }
    .min(ah);
    let (left, right) = (MARGIN_PT, aw - MARGIN_PT - w);
    let (top, bottom) = (MARGIN_PT, ah - MARGIN_PT - h);
    let (mid_x, mid_y) = ((aw - w) / 2.0, (ah - h) / 2.0);
    let (x, y) = match position {
        Value::Array(a) if a.len() == 2 => {
            let cx = a[0].as_f64().unwrap_or(1.0).clamp(0.0, 1.0);
            let cy = a[1].as_f64().unwrap_or(1.0).clamp(0.0, 1.0);
            (cx * aw - w / 2.0, cy * ah - h / 2.0)
        }
        Value::String(s) => match s.as_str() {
            "top-left" => (left, top),
            "top" => (mid_x, top),
            "top-right" => (right, top),
            "left" => (left, mid_y),
            "right" => (right, mid_y),
            "bottom-left" => (left, bottom),
            "bottom" => (mid_x, bottom),
            _ => (right, bottom),
        },
        _ => (right, bottom),
    };
    let x = x.clamp(0.0, (aw - w).max(0.0));
    let y = y.clamp(0.0, (ah - h).max(0.0));
    CameraLayout {
        visible: true,
        shape: shape_v,
        rect: [x / aw, y / ah, w / aw, h / ah],
        mirror,
        ring: RING_PT / w.max(1.0),
        radius: ROUNDED_RADIUS_PT / w.max(1.0),
        size: Some(size.to_owned()),
    }
}

/// The bubble on the output frame, in pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct BubbleGeom {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub ring_px: u32,
    /// Corner radius of the outer shape in px (`min(w,h)/2` for a circle).
    pub radius_px: f64,
}

fn even(x: f64) -> u32 {
    let n = x.round().max(2.0) as u32;
    n - n % 2
}

/// Pixel geometry of the bubble on an `out`-sized frame (even sizes, inside the frame).
pub fn bubble_geom(layout: &CameraLayout, out: (u32, u32)) -> BubbleGeom {
    let (ow, oh) = (f64::from(out.0), f64::from(out.1));
    let max_w = out.0 - out.0 % 2;
    let mut w = even(layout.rect[2] * ow).clamp(16.min(max_w), max_w.max(2));
    let mut h = if layout.shape == Shape::Circle {
        w
    } else {
        even(f64::from(w) * layout.shape.aspect_h_over_w())
    };
    let max_h = out.1 - out.1 % 2;
    if h > max_h {
        // Too tall for the frame (tiny recordings): shrink both, keeping the aspect.
        h = max_h.max(2);
        w = if layout.shape == Shape::Circle {
            h
        } else {
            even(f64::from(h) / layout.shape.aspect_h_over_w())
        };
    }
    let x = (layout.rect[0] * ow)
        .round()
        .clamp(0.0, f64::from(out.0.saturating_sub(w))) as u32;
    let y = (layout.rect[1] * oh)
        .round()
        .clamp(0.0, f64::from(out.1.saturating_sub(h))) as u32;
    let ring_px = ((layout.ring * f64::from(w)).round() as u32).clamp(2, w / 4);
    let radius_px = match layout.shape {
        Shape::Circle => f64::from(w.min(h)) / 2.0,
        Shape::Rounded => (layout.radius * f64::from(w)).clamp(0.0, f64::from(w.min(h)) / 2.0),
    };
    BubbleGeom {
        x,
        y,
        w,
        h,
        ring_px,
        radius_px,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CAM-1: the medium circle sits on the bottom-right anchor with a 24 pt margin; sizes are
    /// the 160/240/320 pt presets as fractions of the area width.
    #[test]
    fn presets_and_anchors_become_fractions() {
        let area = (1512.0, 982.0);
        let l = camera_layout("circle", "m", &json!("bottom-right"), Some(true), area);
        assert!((l.rect[2] - 240.0 / 1512.0).abs() < 1e-9);
        assert!((l.rect[0] - (1512.0 - 24.0 - 240.0) / 1512.0).abs() < 1e-9);
        assert!((l.rect[1] - (982.0 - 24.0 - 240.0) / 982.0).abs() < 1e-9);
        assert!((l.ring - 4.0 / 240.0).abs() < 1e-9);
        let s = camera_layout("circle", "s", &json!("top-left"), None, area);
        assert!((s.rect[2] - 160.0 / 1512.0).abs() < 1e-9);
        assert!((s.rect[0] - 24.0 / 1512.0).abs() < 1e-9);
        assert!((s.rect[1] - 24.0 / 982.0).abs() < 1e-9);
        let big = camera_layout("circle", "l", &json!("top"), None, area);
        assert!((big.rect[2] - 320.0 / 1512.0).abs() < 1e-9);
        assert!((big.rect[0] - (1512.0 - 320.0) / 2.0 / 1512.0).abs() < 1e-9);
        // Rounded is 4:3.
        let r = camera_layout("rounded", "m", &json!("bottom"), None, area);
        assert!((r.rect[3] * 982.0 - 180.0).abs() < 1e-6);
        assert_eq!(r.shape, Shape::Rounded);
    }

    #[test]
    fn a_fraction_position_is_the_bubble_centre_and_is_kept_inside() {
        let area = (1000.0, 500.0);
        let l = camera_layout("circle", "m", &json!([0.5, 0.5]), None, area);
        assert!((l.rect[0] - (500.0 - 120.0) / 1000.0).abs() < 1e-9);
        assert!((l.rect[1] - (250.0 - 120.0) / 500.0).abs() < 1e-9);
        let edge = camera_layout("circle", "m", &json!([1.0, 1.0]), None, area);
        assert!((edge.rect[0] + edge.rect[2] - 1.0).abs() < 1e-9);
        assert!((edge.rect[1] + edge.rect[3] - 1.0).abs() < 1e-9);
        // A bubble wider than the area is clamped to it.
        let tiny = camera_layout("circle", "l", &json!("top-left"), None, (200.0, 150.0));
        assert!(tiny.rect[2] <= 1.0 && tiny.rect[3] <= 1.0);
    }

    #[test]
    fn layout_json_round_trips_and_rejects_garbage() {
        let l = camera_layout(
            "rounded",
            "l",
            &json!("top-right"),
            Some(false),
            (1920.0, 1080.0),
        );
        let back = CameraLayout::from_json(&l.to_json()).unwrap();
        assert_eq!(back.shape, Shape::Rounded);
        assert_eq!(back.mirror, Some(false));
        assert!((back.rect[2] - l.rect[2]).abs() < 1e-4);
        assert!(CameraLayout::from_json(&json!({"visible": true})).is_none());
        assert!(CameraLayout::from_json(&json!({"rect": [0, 0, 0, 0]})).is_none());
        assert!(CameraLayout::from_json(&json!({"rect": [0, 0, 0.1]})).is_none());
        // The spec's sketch has no ring or radius: defaults apply.
        let spec = CameraLayout::from_json(
            &json!({"visible": true, "shape": "circle", "rect": [0.78, 0.72, 0.18, 0.18]}),
        )
        .unwrap();
        assert!((spec.ring - DEFAULT_RING).abs() < 1e-12);
    }

    #[test]
    fn pixel_geometry_is_even_inside_the_frame_and_has_a_ring() {
        let l = CameraLayout {
            visible: true,
            shape: Shape::Circle,
            rect: [0.78, 0.72, 0.16, 0.28],
            mirror: None,
            ring: DEFAULT_RING,
            radius: DEFAULT_RADIUS,
            size: None,
        };
        let g = bubble_geom(&l, (1280, 720));
        assert_eq!((g.w, g.h), (204, 204));
        // y = 0.72 * 720 = 518 would cross the bottom edge: clamped to 720 - 204.
        assert_eq!((g.x, g.y), (998, 516));
        assert!(g.x + g.w <= 1280 && g.y + g.h <= 720);
        assert_eq!(g.ring_px, 3);
        assert!((g.radius_px - 102.0).abs() < 1e-9);
        // Rounded: 4:3, radius from the fraction.
        let r = CameraLayout {
            shape: Shape::Rounded,
            ..l.clone()
        };
        let g = bubble_geom(&r, (1280, 720));
        assert_eq!((g.w, g.h), (204, 152));
        assert!((g.radius_px - 20.4).abs() < 0.01);
        // Pushed over the right edge: clamped back inside.
        let edge = CameraLayout {
            rect: [0.99, 0.99, 0.16, 0.16],
            ..l
        };
        let g = bubble_geom(&edge, (1280, 720));
        assert!(g.x + g.w <= 1280 && g.y + g.h <= 720);
    }
}
