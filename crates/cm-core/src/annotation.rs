//! The annotation op model (`manifest.ops`), shared by the editor UI, `annotate` (MCP),
//! `--annotate ops.json` (CLI), `cm-library` (persistence) and `cm-render` (painting).
//!
//! The JSON shape follows `specs/features/02-annotation.md` exactly: an object with an `id`, an
//! `op` tag and the per-op fields. Points are `[x, y]`, rects are `[x, y, w, h]`, all in
//! physical px of `original.png` (crop does not re-base them); colors are `#RRGGBB`. Numbers are
//! `f64` so sub-pixel geometry is expressible, but whole numbers serialize as JSON integers, so
//! the spec's example list round-trips byte-for-byte in shape (ANN-1.AC6).
//!
//! Conventions:
//! - ops are painted in list order; `crop` is not painted, it selects the visible rect (the
//!   **last** crop op wins);
//! - coordinates outside the image are legal (clipped at render time, not rejected); so are
//!   empty or negative-extent shape rects (they draw nothing / are normalized);
//! - omitted optional fields take the spec/DESIGN defaults (color `#FF3B3B`, width 4, text 24 px
//!   white with a black outline, ...);
//! - unknown fields inside a known op are ignored when reading (and dropped on the next save);
//!   an unknown `op` type is an `invalid_input` error naming the op index (`ops[i]`).

use crate::CmError;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

/// Annotation palette default (DESIGN §2 "Annotation palette").
pub const DEFAULT_COLOR: Color = Color::rgb(0xFF, 0x3B, 0x3B);
/// Default stroke width in physical px.
pub const DEFAULT_WIDTH: f64 = 4.0;
/// Default text size in px (ANN-1).
pub const DEFAULT_TEXT_SIZE: f64 = 24.0;
/// Default step-badge diameter in px.
pub const DEFAULT_STEP_SIZE: f64 = 28.0;
/// Default highlighter color.
pub const DEFAULT_HIGHLIGHT_COLOR: Color = Color::rgb(0xFF, 0xD6, 0x0A);
/// Default highlighter opacity.
pub const DEFAULT_HIGHLIGHT_OPACITY: f64 = 0.35;
/// Default blur strength (ANN-2).
pub const DEFAULT_BLUR_STRENGTH: u32 = 12;
/// Allowed blur strength range (ANN-2), inclusive.
pub const BLUR_STRENGTH_RANGE: std::ops::RangeInclusive<u32> = 4..=32;
/// Default pixelate block size in px (ANN-2).
pub const DEFAULT_PIXELATE_BLOCK: u32 = 12;
/// Allowed pixelate block range (ANN-2), inclusive.
pub const PIXELATE_BLOCK_RANGE: std::ops::RangeInclusive<u32> = 6..=32;
/// Largest accepted text size / step diameter in px.
pub const MAX_SIZE: f64 = 2000.0;
/// Longest accepted text op, in chars (R1-9). The renderer additionally caps the layer area.
pub const MAX_TEXT_CHARS: usize = 2000;

/// An opaque `#RRGGBB` color; serializes as an upper-case `#RRGGBB` string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const BLACK: Color = Color::rgb(0, 0, 0);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b }
    }

    /// Parses `#RRGGBB` (case-insensitive). Nothing else is accepted.
    pub fn parse(s: &str) -> Result<Self, CmError> {
        let bad = || CmError::invalid_input(format!("color `{s}` must look like #RRGGBB"));
        let hex = s.strip_prefix('#').ok_or_else(bad)?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(bad());
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
        Ok(Color::rgb(byte(0)?, byte(2)?, byte(4)?))
    }

    /// `[r, g, b]`.
    pub fn to_rgb(self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }

    /// `#RRGGBB`, upper case.
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl FromStr for Color {
    type Err = CmError;
    fn from_str(s: &str) -> Result<Self, CmError> {
        Color::parse(s)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Color::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Serializes whole numbers as JSON integers (`4`, not `4.0`) so the wire shape matches the spec.
fn ser_num<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        s.serialize_i64(*v as i64)
    } else {
        s.serialize_f64(*v)
    }
}

/// A number that serializes through [`ser_num`] (used inside the array newtypes).
#[derive(Clone, Copy)]
struct Num(f64);

impl Serialize for Num {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        ser_num(&self.0, s)
    }
}

/// Point in physical px; serializes as `[x, y]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(from = "[f64; 2]", into = "[Num; 2]")]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

impl Pt {
    pub const fn new(x: f64, y: f64) -> Self {
        Pt { x, y }
    }

    pub fn to_array(self) -> [f64; 2] {
        [self.x, self.y]
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl From<[f64; 2]> for Pt {
    fn from([x, y]: [f64; 2]) -> Self {
        Pt { x, y }
    }
}
impl From<Pt> for [Num; 2] {
    fn from(p: Pt) -> Self {
        [Num(p.x), Num(p.y)]
    }
}

/// Rect in physical px; serializes as `[x, y, w, h]`. Extents may be negative or zero (shapes
/// normalize or draw nothing); see [`Rc::normalized`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(from = "[f64; 4]", into = "[Num; 4]")]
pub struct Rc {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rc {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Rc { x, y, w, h }
    }

    pub fn to_array(self) -> [f64; 4] {
        [self.x, self.y, self.w, self.h]
    }

    pub fn is_finite(self) -> bool {
        self.to_array().iter().all(|v| v.is_finite())
    }

    /// The same rect with non-negative extents (a drag in any direction).
    pub fn normalized(self) -> Rc {
        let (x0, x1) = (self.x.min(self.x + self.w), self.x.max(self.x + self.w));
        let (y0, y1) = (self.y.min(self.y + self.h), self.y.max(self.y + self.h));
        Rc {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        }
    }
}

impl From<[f64; 4]> for Rc {
    fn from([x, y, w, h]: [f64; 4]) -> Self {
        Rc { x, y, w, h }
    }
}
impl From<Rc> for [Num; 4] {
    fn from(r: Rc) -> Self {
        [Num(r.x), Num(r.y), Num(r.w), Num(r.h)]
    }
}

fn d_color() -> Color {
    DEFAULT_COLOR
}
fn d_width() -> f64 {
    DEFAULT_WIDTH
}
fn d_text_size() -> f64 {
    DEFAULT_TEXT_SIZE
}
fn d_white() -> Color {
    Color::WHITE
}
fn d_outline() -> Option<Color> {
    Some(Color::BLACK)
}
fn d_highlight() -> Color {
    DEFAULT_HIGHLIGHT_COLOR
}
fn d_opacity() -> f64 {
    DEFAULT_HIGHLIGHT_OPACITY
}
fn d_step_size() -> f64 {
    DEFAULT_STEP_SIZE
}
fn d_strength() -> u32 {
    DEFAULT_BLUR_STRENGTH
}
fn d_block() -> u32 {
    DEFAULT_PIXELATE_BLOCK
}

/// One annotation: a stable `id` (`op_<n>`) plus the typed payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationOp {
    /// Stable id such as `op_7`. Optional on input (agents may omit it); empty means "assign
    /// one" when passed to `ShotBundle` edit methods. Omitted on output when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(flatten)]
    pub kind: OpKind,
}

impl AnnotationOp {
    pub fn new(id: impl Into<String>, kind: OpKind) -> Self {
        AnnotationOp {
            id: id.into(),
            kind,
        }
    }
}

/// The op payload, tagged by `"op"` in JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum OpKind {
    Arrow {
        from: Pt,
        to: Pt,
        #[serde(default = "d_color")]
        color: Color,
        #[serde(default = "d_width", serialize_with = "ser_num")]
        width: f64,
    },
    Line {
        from: Pt,
        to: Pt,
        #[serde(default = "d_color")]
        color: Color,
        #[serde(default = "d_width", serialize_with = "ser_num")]
        width: f64,
    },
    Rect {
        rect: Rc,
        #[serde(default = "d_color")]
        color: Color,
        #[serde(default = "d_width", serialize_with = "ser_num")]
        width: f64,
        /// Interior fill; `null`/absent = outline only.
        #[serde(default)]
        fill: Option<Color>,
        /// Corner radius in px (clamped to half the shorter side at render time).
        #[serde(default, serialize_with = "ser_num")]
        radius: f64,
    },
    Ellipse {
        rect: Rc,
        #[serde(default = "d_color")]
        color: Color,
        #[serde(default = "d_width", serialize_with = "ser_num")]
        width: f64,
    },
    Text {
        /// Top-left of the text box (first line).
        at: Pt,
        text: String,
        /// Font size in px (em).
        #[serde(default = "d_text_size", serialize_with = "ser_num")]
        size: f64,
        #[serde(default = "d_white")]
        color: Color,
        /// Outline color; absent = black (DESIGN default), explicit `null` = none.
        #[serde(default = "d_outline")]
        outline: Option<Color>,
        /// Background box color; `null`/absent = none.
        #[serde(default)]
        bg: Option<Color>,
    },
    Highlight {
        rect: Rc,
        #[serde(default = "d_highlight")]
        color: Color,
        #[serde(default = "d_opacity")]
        opacity: f64,
    },
    /// Numbered step badge. Deleting one through `ShotBundle::remove_op` renumbers later ones.
    Step {
        /// Center of the badge.
        at: Pt,
        n: u32,
        /// Badge diameter in px.
        #[serde(default = "d_step_size", serialize_with = "ser_num")]
        size: f64,
        #[serde(default = "d_color")]
        color: Color,
    },
    /// Gaussian blur of the original pixels under `rect`.
    Blur {
        rect: Rc,
        #[serde(default = "d_strength")]
        strength: u32,
    },
    /// Pixelation of the original pixels under `rect`.
    Pixelate {
        rect: Rc,
        #[serde(default = "d_block")]
        block: u32,
    },
    /// Non-destructive crop: sets the visible rect; the last crop op wins.
    Crop { rect: Rc },
}

impl OpKind {
    /// The JSON `op` tag (`"arrow"`, `"rect"`, ...).
    pub fn name(&self) -> &'static str {
        match self {
            OpKind::Arrow { .. } => "arrow",
            OpKind::Line { .. } => "line",
            OpKind::Rect { .. } => "rect",
            OpKind::Ellipse { .. } => "ellipse",
            OpKind::Text { .. } => "text",
            OpKind::Highlight { .. } => "highlight",
            OpKind::Step { .. } => "step",
            OpKind::Blur { .. } => "blur",
            OpKind::Pixelate { .. } => "pixelate",
            OpKind::Crop { .. } => "crop",
        }
    }

    /// True for ops that destroy pixel information (blur, pixelate).
    pub fn is_redaction(&self) -> bool {
        matches!(self, OpKind::Blur { .. } | OpKind::Pixelate { .. })
    }

    /// Checks value ranges. Out-of-image coordinates, empty text and empty or negative-extent
    /// shape rects are fine; non-finite numbers, non-positive widths/sizes, out-of-range
    /// strengths and an area-less crop are not. Errors are `invalid_input` without an index
    /// (see [`validate_op`]).
    pub fn validate(&self) -> Result<(), CmError> {
        self.check().map_err(CmError::invalid_input)
    }

    fn check(&self) -> Result<(), String> {
        fn finite_rect(r: &Rc) -> Result<(), String> {
            if r.is_finite() {
                Ok(())
            } else {
                Err("rect must be finite numbers".into())
            }
        }
        /// A redaction over zero pixels would silently redact nothing (R1-13).
        fn has_area(r: &Rc, what: &str) -> Result<(), String> {
            let a = r.to_array();
            if a[2] == 0.0 || a[3] == 0.0 {
                Err(format!("{what} rect must have area"))
            } else {
                Ok(())
            }
        }
        fn finite_pt(name: &str, p: &[&Pt]) -> Result<(), String> {
            if p.iter().all(|p| p.is_finite()) {
                Ok(())
            } else {
                Err(format!("{name} must be finite numbers"))
            }
        }
        fn positive(name: &str, v: f64) -> Result<(), String> {
            if v.is_finite() && v > 0.0 {
                Ok(())
            } else {
                Err(format!("{name} must be > 0"))
            }
        }
        fn size(v: f64) -> Result<(), String> {
            positive("size", v)?;
            if v > MAX_SIZE {
                return Err(format!("size must be <= {MAX_SIZE}"));
            }
            Ok(())
        }
        match self {
            OpKind::Arrow {
                from, to, width, ..
            }
            | OpKind::Line {
                from, to, width, ..
            } => {
                finite_pt("from/to", &[from, to])?;
                positive("width", *width)
            }
            OpKind::Rect {
                rect,
                width,
                radius,
                ..
            } => {
                finite_rect(rect)?;
                positive("width", *width)?;
                if radius.is_finite() && *radius >= 0.0 {
                    Ok(())
                } else {
                    Err("radius must be >= 0".into())
                }
            }
            OpKind::Ellipse { rect, width, .. } => {
                finite_rect(rect)?;
                positive("width", *width)
            }
            OpKind::Text {
                at, size: s, text, ..
            } => {
                finite_pt("at", &[at])?;
                if text.chars().count() > MAX_TEXT_CHARS {
                    return Err(format!("text must be at most {MAX_TEXT_CHARS} characters"));
                }
                size(*s)
            }
            OpKind::Highlight { rect, opacity, .. } => {
                finite_rect(rect)?;
                if (0.0..=1.0).contains(opacity) {
                    Ok(())
                } else {
                    Err("opacity must be in 0..=1".into())
                }
            }
            OpKind::Step { at, n, size: s, .. } => {
                finite_pt("at", &[at])?;
                if *n < 1 {
                    return Err("n must be >= 1".into());
                }
                size(*s)
            }
            OpKind::Blur { rect, strength } => {
                finite_rect(rect)?;
                has_area(rect, "blur")?;
                if BLUR_STRENGTH_RANGE.contains(strength) {
                    Ok(())
                } else {
                    Err(format!("blur strength must be in {BLUR_STRENGTH_RANGE:?}"))
                }
            }
            OpKind::Pixelate { rect, block } => {
                finite_rect(rect)?;
                has_area(rect, "pixelate")?;
                if PIXELATE_BLOCK_RANGE.contains(block) {
                    Ok(())
                } else {
                    Err(format!(
                        "pixelate block must be in {PIXELATE_BLOCK_RANGE:?}"
                    ))
                }
            }
            OpKind::Crop { rect } => {
                finite_rect(rect)?;
                if rect.w == 0.0 || rect.h == 0.0 {
                    Err("crop rect must have area".into())
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// Validates the op at `index`; errors are `invalid_input` naming `ops[index]`.
pub fn validate_op(index: usize, op: &AnnotationOp) -> Result<(), CmError> {
    op.kind
        .check()
        .map_err(|e| CmError::invalid_field(&format!("ops[{index}]"), e))
}

/// Parses and validates an `ops` JSON array (from a manifest, `annotate` or `--ops ops.json`).
///
/// Errors are `invalid_input` and name the offending index (`ops[3]: ...`), also for an unknown
/// op type (ANN-1 edge cases). Ids may be empty or missing (callers assign them); non-empty ids
/// must be unique.
pub fn parse_ops(value: &Value) -> Result<Vec<AnnotationOp>, CmError> {
    let arr = value
        .as_array()
        .ok_or_else(|| CmError::invalid_field("ops", "must be an array"))?;
    let mut out = Vec::with_capacity(arr.len());
    let mut seen = HashSet::new();
    for (i, v) in arr.iter().enumerate() {
        let field = format!("ops[{i}]");
        let op: AnnotationOp = serde_json::from_value(v.clone()).map_err(|e| {
            let tag = v.get("op").and_then(Value::as_str).unwrap_or("?");
            CmError::invalid_field(&field, format!("invalid `{tag}` op: {e}"))
        })?;
        validate_op(i, &op)?;
        if !op.id.is_empty() && !seen.insert(op.id.clone()) {
            return Err(CmError::invalid_field(
                &field,
                format!("duplicate id `{}`", op.id),
            ));
        }
        out.push(op);
    }
    Ok(out)
}

/// [`parse_ops`] from a JSON string (the `--ops ops.json` file contents).
pub fn parse_ops_str(json: &str) -> Result<Vec<AnnotationOp>, CmError> {
    let v: Value = serde_json::from_str(json)
        .map_err(|e| CmError::invalid_field("ops", format!("invalid JSON: {e}")))?;
    parse_ops(&v)
}

/// The effective crop rect (the last `crop` op, normalized to non-negative extents), or `None`
/// for the full image.
pub fn visible_rect(ops: &[AnnotationOp]) -> Option<Rc> {
    ops.iter().rev().find_map(|o| match o.kind {
        OpKind::Crop { rect } => Some(rect.normalized()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The exact example list from specs/features/02-annotation.md.
    fn spec_ops() -> Value {
        json!([
            { "id": "op_7", "op": "arrow", "from": [120, 300], "to": [380, 210], "color": "#FF3B3B", "width": 4 },
            { "id": "op_8", "op": "rect", "rect": [400, 180, 240, 60], "color": "#FF3B3B", "width": 4, "fill": null, "radius": 6 },
            { "id": "op_9", "op": "ellipse", "rect": [400, 180, 240, 60], "color": "#2E8BFF", "width": 4 },
            { "id": "op_10", "op": "line", "from": [10, 10], "to": [200, 10], "color": "#111111", "width": 2 },
            { "id": "op_11", "op": "text", "at": [130, 320], "text": "Click Save", "size": 24, "color": "#FFFFFF", "outline": "#000000", "bg": null },
            { "id": "op_12", "op": "highlight", "rect": [90, 400, 300, 28], "color": "#FFD60A", "opacity": 0.35 },
            { "id": "op_13", "op": "step", "at": [400, 200], "n": 1, "size": 28, "color": "#FF3B3B" },
            { "id": "op_14", "op": "blur", "rect": [420, 88, 260, 24], "strength": 12 },
            { "id": "op_15", "op": "pixelate", "rect": [420, 120, 260, 24], "block": 12 },
            { "id": "op_16", "op": "crop", "rect": [0, 0, 1200, 800] }
        ])
    }

    #[test]
    fn spec_example_round_trips_ann_1_ac6() {
        let ops = parse_ops(&spec_ops()).unwrap();
        assert_eq!(ops.len(), 10);
        assert_eq!(ops[0].kind.name(), "arrow");
        let back = serde_json::to_value(&ops).unwrap();
        // Exact wire shape: integers stay integers, the tag and arrays are as in the spec.
        assert_eq!(back, spec_ops());
        assert_eq!(parse_ops(&back).unwrap(), ops);
    }

    #[test]
    fn fractional_numbers_survive() {
        let ops = parse_ops(&json!([{ "op": "blur", "rect": [10.4, 10.5, 3, 3], "strength": 4 }]))
            .unwrap();
        let back = serde_json::to_value(&ops).unwrap();
        assert_eq!(back[0]["rect"], json!([10.4, 10.5, 3, 3]));
    }

    #[test]
    fn unknown_op_names_index_ann_1_edge() {
        let err = parse_ops(&json!([
            { "id": "a", "op": "crop", "rect": [0, 0, 5, 5] },
            { "id": "b", "op": "squiggle" }
        ]))
        .unwrap_err();
        assert!(err.to_string().contains("ops[1]"), "{err}");
    }

    #[test]
    fn validation() {
        let bad = [
            json!({ "op": "blur", "rect": [0, 0, 10, 10], "strength": 2 }),
            json!({ "op": "pixelate", "rect": [0, 0, 10, 10], "block": 40 }),
            json!({ "op": "rect", "rect": [0, 0, 10, 10], "width": 0 }),
            json!({ "op": "arrow", "from": [0, 0], "to": [1, 1], "color": "red" }),
            json!({ "op": "highlight", "rect": [0, 0, 5, 5], "color": "#FFFF00", "opacity": 2.0 }),
            json!({ "op": "crop", "rect": [0, 0, 0, 5] }),
            json!({ "op": "step", "at": [0, 0], "n": 0 }),
            json!({ "op": "text", "at": [0, 0], "text": "x", "size": 5000 }),
        ];
        for b in bad {
            let e = parse_ops(&json!([b.clone()])).unwrap_err();
            assert!(e.to_string().contains("ops[0]"), "{b}: {e}");
        }
        assert!(parse_ops(&json!([{"id":"x","op":"crop","rect":[0,0,1,1]},{"id":"x","op":"crop","rect":[0,0,1,1]}])).is_err());
        // Out-of-image coordinates, a crop origin outside the image and degenerate shapes are
        // allowed (clipped at render time).
        assert!(parse_ops(&json!([
            { "op": "arrow", "from": [-50, 99999], "to": [1, 1], "color": "#abcdef" },
            { "op": "crop", "rect": [-1, 0, 5, 5] },
            { "op": "rect", "rect": [5, 5, 0, 0] },
            { "op": "text", "at": [5, 5], "text": "" },
        ]))
        .is_ok());
    }

    #[test]
    fn color_normalizes() {
        let ops = parse_ops(
            &json!([{ "id": "1", "op": "line", "from": [0,0], "to": [1,1], "color": "#abcdef" }]),
        )
        .unwrap();
        match &ops[0].kind {
            OpKind::Line { color, .. } => {
                assert_eq!(color.to_hex(), "#ABCDEF");
                assert_eq!(color.to_rgb(), [0xAB, 0xCD, 0xEF]);
            }
            _ => panic!(),
        }
        assert!(Color::parse("#ABC").is_err());
        assert!(Color::parse("ABCDEF1").is_err());
    }

    #[test]
    fn defaults_apply_when_fields_are_omitted() {
        let ops = parse_ops(&json!([
            { "op": "arrow", "from": [0, 0], "to": [1, 1] },
            { "op": "text", "at": [5, 5], "text": "Hi" },
            { "op": "text", "at": [5, 5], "text": "Hi", "outline": null },
            { "op": "highlight", "rect": [0, 0, 5, 5] },
            { "op": "step", "at": [1, 1], "n": 1 },
            { "op": "blur", "rect": [0, 0, 5, 5] },
            { "op": "pixelate", "rect": [0, 0, 5, 5] },
        ]))
        .unwrap();
        assert!(matches!(&ops[0].kind, OpKind::Arrow { color, width, .. }
            if *color == DEFAULT_COLOR && *width == 4.0));
        assert!(
            matches!(&ops[1].kind, OpKind::Text { size, color, outline, bg, .. }
            if *size == 24.0 && *color == Color::WHITE && *outline == Some(Color::BLACK) && bg.is_none())
        );
        assert!(matches!(&ops[2].kind, OpKind::Text { outline: None, .. }));
        assert!(
            matches!(&ops[3].kind, OpKind::Highlight { color, opacity, .. }
            if *color == DEFAULT_HIGHLIGHT_COLOR && *opacity == 0.35)
        );
        assert!(matches!(&ops[4].kind, OpKind::Step { size, .. } if *size == 28.0));
        assert!(matches!(&ops[5].kind, OpKind::Blur { strength: 12, .. }));
        assert!(matches!(&ops[6].kind, OpKind::Pixelate { block: 12, .. }));
    }

    #[test]
    fn empty_id_is_not_serialized() {
        let op = AnnotationOp::new(
            "",
            OpKind::Crop {
                rect: Rc::new(0.0, 0.0, 1.0, 1.0),
            },
        );
        assert!(serde_json::to_value(&op).unwrap().get("id").is_none());
    }

    #[test]
    fn last_crop_wins() {
        let ops = parse_ops(&json!([
            { "id": "a", "op": "crop", "rect": [0, 0, 10, 10] },
            { "id": "b", "op": "crop", "rect": [1, 1, 5, 5] }
        ]))
        .unwrap();
        assert_eq!(visible_rect(&ops), Some(Rc::new(1.0, 1.0, 5.0, 5.0)));
        assert_eq!(visible_rect(&[]), None);
        let neg = parse_ops(&json!([{ "op": "crop", "rect": [10, 10, -4, -2] }])).unwrap();
        assert_eq!(visible_rect(&neg), Some(Rc::new(6.0, 8.0, 4.0, 2.0)));
    }

    /// R1-9: over-long text is refused with the op index; R1-13: zero-area redactions too.
    #[test]
    fn r1_9_r1_13_text_length_and_zero_area_redactions_are_invalid() {
        let long = "x".repeat(MAX_TEXT_CHARS + 1);
        let e = parse_ops(&json!([{ "op": "text", "at": [0, 0], "text": long }])).unwrap_err();
        assert_eq!(e.code(), crate::ErrorCode::InvalidInput);
        assert!(e.to_string().contains("ops[0]"), "{e}");
        let ok = "x".repeat(MAX_TEXT_CHARS);
        parse_ops(&json!([{ "op": "text", "at": [0, 0], "text": ok }])).unwrap();
        // Counted in chars, not bytes.
        let cjk = "字".repeat(MAX_TEXT_CHARS);
        parse_ops(&json!([{ "op": "text", "at": [0, 0], "text": cjk }])).unwrap();
        for op in ["blur", "pixelate"] {
            for rect in [json!([5, 5, 0, 10]), json!([5, 5, 10, 0])] {
                let e = parse_ops(&json!([{ "op": op, "rect": rect }])).unwrap_err();
                assert!(e.to_string().contains("ops[0]"), "{e}");
                assert!(e.to_string().contains("area"), "{e}");
            }
            // Negative extents are normalized, not empty.
            parse_ops(&json!([{ "op": op, "rect": [5, 5, -10, 10] }])).unwrap();
        }
    }
}
