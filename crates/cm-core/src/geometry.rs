//! Geometry: logical px (f64), physical px (integers) and normalized layout fractions.
//!
//! Sizes are logical px unless the type says `Physical` (specs/README.md "Units").
//! `NormRect`/`NormPoint`/`PhysicalSize` serialize as JSON arrays to match the manifest
//! (`"rect": [0.78, 0.72, 0.18, 0.18]`, `"size": [3840, 2160]`).

use crate::error::CmError;
use serde::{Deserialize, Serialize};

/// Point in logical px.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Size in logical px.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// Rectangle in logical px.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }
}

impl Size {
    pub const fn new(width: f64, height: f64) -> Self {
        Size { width, height }
    }
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Rect {
            origin: Point { x, y },
            size: Size { width, height },
        }
    }

    pub fn right(&self) -> f64 {
        self.origin.x + self.size.width
    }

    pub fn bottom(&self) -> f64 {
        self.origin.y + self.size.height
    }

    pub fn center(&self) -> Point {
        Point::new(
            self.origin.x + self.size.width / 2.0,
            self.origin.y + self.size.height / 2.0,
        )
    }

    /// Half-open containment: left/top edges inside, right/bottom outside.
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.origin.x && p.x < self.right() && p.y >= self.origin.y && p.y < self.bottom()
    }

    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let x = self.origin.x.max(other.origin.x);
        let y = self.origin.y.max(other.origin.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        (r > x && b > y).then(|| Rect::new(x, y, r - x, b - y))
    }

    pub fn translated(&self, dx: f64, dy: f64) -> Rect {
        Rect::new(
            self.origin.x + dx,
            self.origin.y + dy,
            self.size.width,
            self.size.height,
        )
    }
}

/// Point in physical px.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct PhysicalPoint {
    pub x: i32,
    pub y: i32,
}

/// Size in physical px; serializes as `[width, height]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(from = "[u32; 2]", into = "[u32; 2]")]
pub struct PhysicalSize {
    pub width: u32,
    pub height: u32,
}

impl From<[u32; 2]> for PhysicalSize {
    fn from([width, height]: [u32; 2]) -> Self {
        PhysicalSize { width, height }
    }
}
impl From<PhysicalSize> for [u32; 2] {
    fn from(s: PhysicalSize) -> Self {
        [s.width, s.height]
    }
}

/// Rectangle in physical px.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct PhysicalRect {
    pub origin: PhysicalPoint,
    pub size: PhysicalSize,
}

/// Physical px per logical px (e.g. 2.0 on a 2x display).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct ScaleFactor(f64);

impl ScaleFactor {
    pub const ONE: ScaleFactor = ScaleFactor(1.0);

    pub fn new(scale: f64) -> Result<Self, CmError> {
        if scale.is_finite() && scale > 0.0 {
            Ok(ScaleFactor(scale))
        } else {
            Err(CmError::invalid_field("scale", "must be a positive number"))
        }
    }

    pub fn get(self) -> f64 {
        self.0
    }

    pub fn size_to_physical(self, s: Size) -> PhysicalSize {
        PhysicalSize {
            width: (s.width * self.0).round().max(0.0) as u32,
            height: (s.height * self.0).round().max(0.0) as u32,
        }
    }

    pub fn size_to_logical(self, s: PhysicalSize) -> Size {
        Size::new(f64::from(s.width) / self.0, f64::from(s.height) / self.0)
    }

    pub fn point_to_physical(self, p: Point) -> PhysicalPoint {
        PhysicalPoint {
            x: (p.x * self.0).round() as i32,
            y: (p.y * self.0).round() as i32,
        }
    }

    pub fn point_to_logical(self, p: PhysicalPoint) -> Point {
        Point::new(f64::from(p.x) / self.0, f64::from(p.y) / self.0)
    }

    pub fn rect_to_physical(self, r: Rect) -> PhysicalRect {
        PhysicalRect {
            origin: self.point_to_physical(r.origin),
            size: self.size_to_physical(r.size),
        }
    }

    pub fn rect_to_logical(self, r: PhysicalRect) -> Rect {
        Rect {
            origin: self.point_to_logical(r.origin),
            size: self.size_to_logical(r.size),
        }
    }
}

impl TryFrom<f64> for ScaleFactor {
    type Error = CmError;
    fn try_from(v: f64) -> Result<Self, CmError> {
        ScaleFactor::new(v)
    }
}
impl From<ScaleFactor> for f64 {
    fn from(s: ScaleFactor) -> f64 {
        s.0
    }
}

const EPS: f64 = 1e-9;

fn unit(v: f64) -> bool {
    v.is_finite() && (0.0..=1.0).contains(&v)
}

/// Point as fractions of the output frame; serializes as `[x, y]`, each in 0..=1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "[f64; 2]", into = "[f64; 2]")]
pub struct NormPoint {
    x: f64,
    y: f64,
}

impl NormPoint {
    pub fn new(x: f64, y: f64) -> Result<Self, CmError> {
        if unit(x) && unit(y) {
            Ok(NormPoint { x, y })
        } else {
            Err(CmError::invalid_input(
                "normalized point must be within 0..=1",
            ))
        }
    }
    pub fn x(self) -> f64 {
        self.x
    }
    pub fn y(self) -> f64 {
        self.y
    }
    pub fn to_point(self, frame: Size) -> Point {
        Point::new(self.x * frame.width, self.y * frame.height)
    }
}

impl TryFrom<[f64; 2]> for NormPoint {
    type Error = CmError;
    fn try_from([x, y]: [f64; 2]) -> Result<Self, CmError> {
        NormPoint::new(x, y)
    }
}
impl From<NormPoint> for [f64; 2] {
    fn from(p: NormPoint) -> Self {
        [p.x, p.y]
    }
}

/// Rectangle as fractions of the output frame; serializes as `[x, y, w, h]`.
/// One edit list serves 16:9, 9:16 and 1:1 exports because layouts are fractions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "[f64; 4]", into = "[f64; 4]")]
pub struct NormRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl NormRect {
    /// Requires all values in 0..=1 and the rect to lie inside the frame.
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Result<Self, CmError> {
        if unit(x) && unit(y) && unit(w) && unit(h) && x + w <= 1.0 + EPS && y + h <= 1.0 + EPS {
            Ok(NormRect { x, y, w, h })
        } else {
            Err(CmError::invalid_input(
                "normalized rect must lie within the 0..=1 frame",
            ))
        }
    }

    pub fn x(self) -> f64 {
        self.x
    }
    pub fn y(self) -> f64 {
        self.y
    }
    pub fn w(self) -> f64 {
        self.w
    }
    pub fn h(self) -> f64 {
        self.h
    }

    /// Maps onto a concrete output frame (logical px).
    pub fn to_rect(self, frame: Size) -> Rect {
        Rect::new(
            self.x * frame.width,
            self.y * frame.height,
            self.w * frame.width,
            self.h * frame.height,
        )
    }

    /// Normalizes `r` against `frame`; errors if `frame` is empty or `r` falls outside it.
    pub fn from_rect(r: Rect, frame: Size) -> Result<Self, CmError> {
        if frame.width <= 0.0 || frame.height <= 0.0 {
            return Err(CmError::invalid_input("frame size must be positive"));
        }
        NormRect::new(
            r.origin.x / frame.width,
            r.origin.y / frame.height,
            r.size.width / frame.width,
            r.size.height / frame.height,
        )
    }
}

impl TryFrom<[f64; 4]> for NormRect {
    type Error = CmError;
    fn try_from([x, y, w, h]: [f64; 4]) -> Result<Self, CmError> {
        NormRect::new(x, y, w, h)
    }
}
impl From<NormRect> for [f64; 4] {
    fn from(r: NormRect) -> Self {
        [r.x, r.y, r.w, r.h]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_1_ac1_physical_size_on_2x_display() {
        // CAP-1.AC1: 3024x1964 physical on a 2x display is 1512x982 logical.
        let s = ScaleFactor::new(2.0).unwrap();
        let logical = s.size_to_logical(PhysicalSize {
            width: 3024,
            height: 1964,
        });
        assert_eq!(logical, Size::new(1512.0, 982.0));
        assert_eq!(
            s.size_to_physical(logical),
            PhysicalSize {
                width: 3024,
                height: 1964
            }
        );
    }

    #[test]
    fn rect_logical_physical_round_trip() {
        let s = ScaleFactor::new(1.5).unwrap();
        let r = Rect::new(10.0, 20.0, 300.0, 200.0);
        let p = s.rect_to_physical(r);
        assert_eq!(p.origin, PhysicalPoint { x: 15, y: 30 });
        assert_eq!(
            p.size,
            PhysicalSize {
                width: 450,
                height: 300
            }
        );
        assert_eq!(s.rect_to_logical(p), r);
    }

    #[test]
    fn scale_must_be_positive_and_finite() {
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(ScaleFactor::new(bad).is_err());
        }
        assert!(serde_json::from_str::<ScaleFactor>("0").is_err());
        assert_eq!(
            serde_json::from_str::<ScaleFactor>("2.0").unwrap().get(),
            2.0
        );
    }

    #[test]
    fn rect_helpers() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert_eq!(a.intersection(&b), Some(Rect::new(5.0, 5.0, 5.0, 5.0)));
        assert_eq!(a.intersection(&Rect::new(10.0, 0.0, 5.0, 5.0)), None);
        assert!(a.contains(Point::new(0.0, 0.0)));
        assert!(!a.contains(Point::new(10.0, 5.0)));
        assert_eq!(a.center(), Point::new(5.0, 5.0));
        assert_eq!(a.translated(1.0, 2.0), Rect::new(1.0, 2.0, 10.0, 10.0));
    }

    #[test]
    fn norm_rect_matches_manifest_json() {
        // Technical design manifest: "rect": [0.78, 0.72, 0.18, 0.18]
        let r: NormRect = serde_json::from_str("[0.78, 0.72, 0.18, 0.18]").unwrap();
        assert_eq!(serde_json::to_string(&r).unwrap(), "[0.78,0.72,0.18,0.18]");
        let out = r.to_rect(Size::new(1920.0, 1080.0));
        assert!((out.origin.x - 1497.6).abs() < 1e-9);
        let tall = r.to_rect(Size::new(1080.0, 1920.0));
        assert!((tall.size.height - 345.6).abs() < 1e-9);
    }

    #[test]
    fn norm_rect_validation() {
        assert!(NormRect::new(0.9, 0.0, 0.2, 0.5).is_err());
        assert!(NormRect::new(-0.1, 0.0, 0.2, 0.5).is_err());
        assert!(NormRect::new(f64::NAN, 0.0, 0.2, 0.5).is_err());
        assert!(NormRect::new(0.0, 0.0, 1.0, 1.0).is_ok());
        assert!(serde_json::from_str::<NormRect>("[2, 0, 1, 1]").is_err());
        let r = Rect::new(96.0, 54.0, 192.0, 108.0);
        let n = NormRect::from_rect(r, Size::new(1920.0, 1080.0)).unwrap();
        assert!((n.x() - 0.05).abs() < 1e-12 && (n.h() - 0.1).abs() < 1e-12);
        assert!(NormRect::from_rect(r, Size::new(0.0, 10.0)).is_err());
    }

    #[test]
    fn norm_point_and_physical_size_serde() {
        let p: NormPoint = serde_json::from_str("[0.62, 0.31]").unwrap();
        assert_eq!(p.to_point(Size::new(100.0, 200.0)), Point::new(62.0, 62.0));
        assert!(serde_json::from_str::<NormPoint>("[1.5, 0]").is_err());
        let s: PhysicalSize = serde_json::from_str("[3840, 2160]").unwrap();
        assert_eq!(
            s,
            PhysicalSize {
                width: 3840,
                height: 2160
            }
        );
        assert_eq!(serde_json::to_string(&s).unwrap(), "[3840,2160]");
    }
}
