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
/// Default callout text size in px (ANN-4).
pub const DEFAULT_CALLOUT_SIZE: f64 = 24.0;
/// Default callout corner radius in px (ANN-4).
pub const DEFAULT_CALLOUT_RADIUS: f64 = 12.0;
/// Default spotlight dimming, 0 (none) to 1 (black) (ANN-7): the area outside the holes keeps 40 % of its brightness.
pub const DEFAULT_SPOTLIGHT_DIM: f64 = 0.6;
/// Most holes one spotlight can have (ANN-7).
pub const MAX_SPOTLIGHT_SHAPES: usize = 8;
/// Default stamp size in px (ANN-4).
pub const DEFAULT_STAMP_SIZE: f64 = 34.0;
/// Longest step label and stamp label, in chars (ANN-6, ANN-4).
pub const MAX_LABEL_CHARS: usize = 200;

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
///
/// Fields this build does not know inside a known op (written by a newer version) are kept in
/// `extra` and written back, so a later version never loses them when an older one saves. A key
/// the typed payload writes always wins over the same key in `extra`.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationOp {
    /// Stable id such as `op_7`. Optional on input (agents may omit it); empty means "assign
    /// one" when passed to `ShotBundle` edit methods. Omitted on output when empty.
    pub id: String,
    pub kind: OpKind,
    /// Unknown fields of a known op, kept verbatim (pass-through, 0.3.3).
    pub extra: serde_json::Map<String, Value>,
}

impl AnnotationOp {
    pub fn new(id: impl Into<String>, kind: OpKind) -> Self {
        AnnotationOp {
            id: id.into(),
            kind,
            extra: serde_json::Map::new(),
        }
    }
}

impl Serialize for AnnotationOp {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::Error;
        let Value::Object(typed) = serde_json::to_value(&self.kind).map_err(S::Error::custom)?
        else {
            return Err(S::Error::custom("op must serialize as an object"));
        };
        let mut out = serde_json::Map::new();
        if !self.id.is_empty() {
            out.insert("id".into(), Value::String(self.id.clone()));
        }
        out.extend(typed);
        for (k, v) in &self.extra {
            if k != "id" && !out.contains_key(k) {
                out.insert(k.clone(), v.clone());
            }
        }
        Value::Object(out).serialize(s)
    }
}

impl<'de> Deserialize<'de> for AnnotationOp {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let Value::Object(map) = Value::deserialize(d)? else {
            return Err(D::Error::custom("an op must be a JSON object"));
        };
        let id = match map.get("id") {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) => s.clone(),
            Some(_) => return Err(D::Error::custom("id must be a string")),
        };
        let kind: OpKind =
            serde_json::from_value(Value::Object(map.clone())).map_err(D::Error::custom)?;
        // Whatever the typed payload does not write back is kept as it came.
        let Value::Object(typed) = serde_json::to_value(&kind).map_err(D::Error::custom)? else {
            return Err(D::Error::custom("op must serialize as an object"));
        };
        let extra = map
            .into_iter()
            .filter(|(k, _)| k != "id" && !typed.contains_key(k))
            .collect();
        Ok(AnnotationOp { id, kind, extra })
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
        /// Numbering style (ANN-6): the label is `step_label(style, start, n)`.
        #[serde(default, skip_serializing_if = "StepStyle::is_default")]
        style: StepStyle,
        /// Number the first step gets (ANN-6), at least 1.
        #[serde(default = "d_start", skip_serializing_if = "is_one")]
        start: u32,
        /// Label drawn next to the badge (ANN-6); absent = none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Speech-bubble or box callout with a pointer tail (ANN-4).
    ///
    /// `rect` is the **minimum** box; the renderer draws `max(rect, text size + padding)` and the
    /// editor writes the grown rectangle back. `tail` is the tip of the pointer in image px; moving
    /// the box does not move it.
    Callout {
        rect: Rc,
        tail: Pt,
        #[serde(default)]
        text: String,
        /// Font size in px (em).
        #[serde(default = "d_callout_size", serialize_with = "ser_num")]
        size: f64,
        /// Text color.
        #[serde(default = "d_white")]
        color: Color,
        /// Box fill.
        #[serde(default = "d_color")]
        bg: Color,
        /// Corner radius in px.
        #[serde(default = "d_callout_radius", serialize_with = "ser_num")]
        radius: f64,
        #[serde(default)]
        style: CalloutStyle,
    },
    /// Dims (or blurs) everything outside its holes (ANN-7). At most one per image, at most 8 holes.
    Spotlight {
        shapes: Vec<SpotShape>,
        /// 0 = no dimming, 1 = black.
        #[serde(default = "d_dim", serialize_with = "ser_num")]
        dim: f64,
        #[serde(default)]
        mode: SpotMode,
        /// Blur strength for `mode: "blur"` (a look, not a redaction).
        #[serde(default = "d_strength")]
        strength: u32,
    },
    /// A symbol drawn as vector paths, or a short word (ANN-4).
    Stamp {
        /// Center.
        at: Pt,
        kind: StampKind,
        #[serde(default = "d_stamp_size", serialize_with = "ser_num")]
        size: f64,
        #[serde(default = "d_color")]
        color: Color,
        /// The word of a `label` stamp ("Before", "After"), written in the UI language at creation.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
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

fn d_start() -> u32 {
    1
}
fn is_one(v: &u32) -> bool {
    *v == 1
}
fn d_callout_size() -> f64 {
    DEFAULT_CALLOUT_SIZE
}
fn d_callout_radius() -> f64 {
    DEFAULT_CALLOUT_RADIUS
}
fn d_dim() -> f64 {
    DEFAULT_SPOTLIGHT_DIM
}
fn d_stamp_size() -> f64 {
    DEFAULT_STAMP_SIZE
}

/// How a step badge is numbered (ANN-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepStyle {
    /// 1, 2, 3
    #[default]
    Number,
    /// A, B, C ... Z, AA, AB
    Upper,
    /// a, b, c ... z, aa, ab
    Lower,
}

impl StepStyle {
    pub fn is_default(&self) -> bool {
        *self == StepStyle::Number
    }
}

/// The label of a step badge: `style` applied to `start + n - 1` (ANN-6). Letters continue past Z
/// the way spreadsheet columns do (AA, AB); `start` and `n` below 1 count as 1.
pub fn step_label(style: StepStyle, start: u32, n: u32) -> String {
    let v = u64::from(start.max(1)) + u64::from(n.max(1)) - 1;
    match style {
        StepStyle::Number => v.to_string(),
        StepStyle::Upper | StepStyle::Lower => {
            let base = if style == StepStyle::Upper {
                b'A'
            } else {
                b'a'
            };
            let mut v = v;
            let mut out = Vec::new();
            while v > 0 {
                v -= 1;
                out.push(base + (v % 26) as u8);
                v /= 26;
            }
            out.reverse();
            String::from_utf8(out).unwrap_or_default()
        }
    }
}

/// Callout look (ANN-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CalloutStyle {
    /// Rounded speech bubble with a curved tail.
    #[default]
    Bubble,
    /// Rectangle with a straight triangular tail.
    Box,
}

/// What the area outside a spotlight's holes looks like (ANN-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpotMode {
    #[default]
    Dim,
    /// Blurs the outside. A look only: it is **not** a redaction (use `blur` or `pixelate`).
    Blur,
}

/// Shape of one spotlight hole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpotShapeKind {
    #[default]
    Rect,
    Ellipse,
}

/// One hole of a spotlight (ANN-7).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpotShape {
    #[serde(default)]
    pub kind: SpotShapeKind,
    pub rect: Rc,
    /// Corner radius of a rectangle hole, in px.
    #[serde(default, serialize_with = "ser_num")]
    pub radius: f64,
}

/// The symbol of a stamp (ANN-4). Drawn as paths, never as font glyphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StampKind {
    Check,
    Cross,
    Exclaim,
    Question,
    /// A word on a pill (`text`).
    Label,
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
            OpKind::Callout { .. } => "callout",
            OpKind::Spotlight { .. } => "spotlight",
            OpKind::Stamp { .. } => "stamp",
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
            OpKind::Step {
                at,
                n,
                size: s,
                start,
                text,
                ..
            } => {
                finite_pt("at", &[at])?;
                if *n < 1 {
                    return Err("n must be >= 1".into());
                }
                if *start < 1 {
                    return Err("start must be >= 1".into());
                }
                if let Some(t) = text {
                    if t.chars().count() > MAX_LABEL_CHARS {
                        return Err(format!("text must be at most {MAX_LABEL_CHARS} characters"));
                    }
                }
                size(*s)
            }
            OpKind::Callout {
                rect,
                tail,
                text,
                size: s,
                radius,
                ..
            } => {
                finite_rect(rect)?;
                finite_pt("tail", &[tail])?;
                if text.chars().count() > MAX_TEXT_CHARS {
                    return Err(format!("text must be at most {MAX_TEXT_CHARS} characters"));
                }
                if !(radius.is_finite() && (0.0..=MAX_SIZE).contains(radius)) {
                    return Err(format!("radius must be in 0..={MAX_SIZE}"));
                }
                size(*s)
            }
            OpKind::Spotlight {
                shapes,
                dim,
                strength,
                ..
            } => {
                if shapes.is_empty() || shapes.len() > MAX_SPOTLIGHT_SHAPES {
                    return Err(format!(
                        "a spotlight needs 1 to {MAX_SPOTLIGHT_SHAPES} shapes"
                    ));
                }
                for sh in shapes {
                    finite_rect(&sh.rect)?;
                    has_area(&sh.rect, "spotlight shape")?;
                    if !(sh.radius.is_finite() && (0.0..=MAX_SIZE).contains(&sh.radius)) {
                        return Err(format!("shape radius must be in 0..={MAX_SIZE}"));
                    }
                }
                if !(0.0..=1.0).contains(dim) {
                    return Err("dim must be in 0..=1".into());
                }
                if BLUR_STRENGTH_RANGE.contains(strength) {
                    Ok(())
                } else {
                    Err(format!("strength must be in {BLUR_STRENGTH_RANGE:?}"))
                }
            }
            OpKind::Stamp {
                at,
                kind,
                size: s,
                text,
                ..
            } => {
                finite_pt("at", &[at])?;
                if *kind == StampKind::Label && text.as_deref().is_none_or(|t| t.trim().is_empty())
                {
                    return Err("a label stamp needs text".into());
                }
                if let Some(t) = text {
                    if t.chars().count() > MAX_LABEL_CHARS {
                        return Err(format!("text must be at most {MAX_LABEL_CHARS} characters"));
                    }
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

/// Checks rules that span the list: at most one `spotlight` (ANN-7). The error is `invalid_input`
/// naming the second spotlight (`ops[i]`).
pub fn validate_list(ops: &[AnnotationOp]) -> Result<(), CmError> {
    let mut first = None;
    for (i, op) in ops.iter().enumerate() {
        if matches!(op.kind, OpKind::Spotlight { .. }) {
            if let Some(f) = first {
                return Err(CmError::invalid_field(
                    &format!("ops[{i}]"),
                    format!("only one spotlight per image: add the shapes to ops[{f}]"),
                ));
            }
            first = Some(i);
        }
    }
    Ok(())
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
    validate_list(&out)?;
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

    /// The 0.3.3 ops (ANN-4, ANN-6, ANN-7): the frozen JSON shape round-trips exactly.
    fn pack_ops() -> Value {
        json!([
            { "id": "op_1", "op": "callout", "rect": [100, 80, 220, 70], "tail": [90, 200], "text": "Click here",
              "size": 24, "color": "#FFFFFF", "bg": "#FF3B3B", "radius": 12, "style": "bubble" },
            { "id": "op_2", "op": "spotlight",
              "shapes": [ { "kind": "rect", "rect": [10, 10, 200, 100], "radius": 0 },
                          { "kind": "ellipse", "rect": [300, 40, 120, 120], "radius": 0 } ],
              "dim": 0.6, "mode": "dim", "strength": 12 },
            { "id": "op_3", "op": "step", "at": [400, 200], "n": 2, "size": 28, "color": "#FF3B3B",
              "style": "upper", "start": 3, "text": "Open Settings" },
            { "id": "op_4", "op": "stamp", "at": [50, 50], "kind": "check", "size": 34, "color": "#2EB872" },
            { "id": "op_5", "op": "stamp", "at": [90, 50], "kind": "label", "size": 34, "color": "#FF3B3B", "text": "Before" }
        ])
    }

    #[test]
    fn ann_4_6_7_ops_round_trip() {
        let ops = parse_ops(&pack_ops()).unwrap();
        assert_eq!(serde_json::to_value(&ops).unwrap(), pack_ops());
        assert_eq!(ops[0].kind.name(), "callout");
        assert_eq!(ops[1].kind.name(), "spotlight");
        assert_eq!(ops[4].kind.name(), "stamp");
    }

    #[test]
    fn old_step_keeps_its_wire_shape() {
        // A 0.3.2 step has no style, start or text and must serialize without them.
        let ops = parse_ops(&json!([{ "id": "s", "op": "step", "at": [1, 1], "n": 1, "size": 28, "color": "#FF3B3B" }])).unwrap();
        let back = serde_json::to_value(&ops).unwrap();
        assert!(
            back[0].get("style").is_none()
                && back[0].get("start").is_none()
                && back[0].get("text").is_none()
        );
        assert!(matches!(
            &ops[0].kind,
            OpKind::Step {
                style: StepStyle::Number,
                start: 1,
                text: None,
                ..
            }
        ));
    }

    #[test]
    fn ann_6_step_labels() {
        use StepStyle::*;
        assert_eq!(step_label(Number, 1, 3), "3");
        assert_eq!(step_label(Number, 5, 1), "5");
        assert_eq!(step_label(Upper, 1, 1), "A");
        assert_eq!(step_label(Upper, 3, 1), "C");
        assert_eq!(step_label(Upper, 3, 3), "E");
        assert_eq!(step_label(Upper, 1, 26), "Z");
        assert_eq!(step_label(Upper, 1, 27), "AA");
        assert_eq!(step_label(Upper, 1, 28), "AB");
        assert_eq!(step_label(Upper, 1, 52), "AZ");
        assert_eq!(step_label(Upper, 1, 53), "BA");
        assert_eq!(step_label(Lower, 1, 27), "aa");
        assert_eq!(step_label(Lower, 0, 0), "a");
    }

    #[test]
    fn ann_7_one_spotlight_per_image() {
        let sp = |r: Value| json!({ "op": "spotlight", "shapes": [{ "rect": r }] });
        parse_ops(&json!([sp(json!([0, 0, 5, 5]))])).unwrap();
        let e = parse_ops(&json!([{ "op": "crop", "rect": [0,0,9,9] }, sp(json!([0, 0, 5, 5])), sp(json!([1, 1, 5, 5]))])).unwrap_err();
        assert_eq!(e.code(), crate::ErrorCode::InvalidInput);
        assert!(e.to_string().contains("ops[2]"), "{e}");
        // 9 holes, no holes, a hole with no area, dim out of range.
        let nine: Vec<Value> = (0..9).map(|_| json!({ "rect": [0, 0, 5, 5] })).collect();
        for bad in [
            json!({ "op": "spotlight", "shapes": nine }),
            json!({ "op": "spotlight", "shapes": [] }),
            json!({ "op": "spotlight", "shapes": [{ "rect": [0, 0, 0, 5] }] }),
            json!({ "op": "spotlight", "shapes": [{ "rect": [0, 0, 5, 5] }], "dim": 1.5 }),
        ] {
            let e = parse_ops(&json!([bad.clone()])).unwrap_err();
            assert!(e.to_string().contains("ops[0]"), "{bad}: {e}");
        }
        let eight: Vec<Value> = (0..8).map(|_| json!({ "rect": [0, 0, 5, 5] })).collect();
        parse_ops(&json!([{ "op": "spotlight", "shapes": eight }])).unwrap();
    }

    #[test]
    fn ann_4_callout_and_stamp_validation() {
        for bad in [
            json!({ "op": "callout", "rect": [0, 0, 10, 10] }),
            json!({ "op": "callout", "rect": [0, 0, 10, 10], "tail": [1, 1], "size": 0 }),
            json!({ "op": "callout", "rect": [0, 0, 10, 10], "tail": [1, 1], "style": "cloud" }),
            json!({ "op": "stamp", "at": [1, 1], "kind": "label" }),
            json!({ "op": "stamp", "at": [1, 1], "kind": "smile" }),
        ] {
            let e = parse_ops(&json!([bad.clone()])).unwrap_err();
            assert!(e.to_string().contains("ops[0]"), "{bad}: {e}");
        }
        let ops = parse_ops(&json!([{ "op": "callout", "rect": [0, 0, 10, 10], "tail": [1, 1] }]))
            .unwrap();
        assert!(
            matches!(&ops[0].kind, OpKind::Callout { size, bg, color, radius, style: CalloutStyle::Bubble, text, .. }
            if *size == 24.0 && *bg == DEFAULT_COLOR && *color == Color::WHITE && *radius == 12.0 && text.is_empty())
        );
    }

    /// Unknown fields inside a known op survive a read and a write (0.3.3 pass-through).
    #[test]
    fn unknown_fields_in_known_ops_pass_through() {
        let input = json!([
            { "id": "a", "op": "arrow", "from": [0, 0], "to": [5, 5], "color": "#FF3B3B", "width": 4, "glow": { "r": 3 }, "z": 9 },
            { "id": "b", "op": "step", "at": [1, 1], "n": 1, "size": 28, "color": "#FF3B3B", "style": "number", "future": [1, 2] }
        ]);
        let ops = parse_ops(&input).unwrap();
        assert_eq!(ops[0].extra.len(), 2);
        assert_eq!(serde_json::to_value(&ops).unwrap(), input);
        // The typed value wins when it is set.
        let mut ops = ops;
        if let OpKind::Step { style, .. } = &mut ops[1].kind {
            *style = StepStyle::Upper;
        }
        let back = serde_json::to_value(&ops).unwrap();
        assert_eq!(back[1]["style"], "upper");
        assert_eq!(back[1]["future"], json!([1, 2]));
    }
}
