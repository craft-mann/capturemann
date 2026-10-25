//! Tools, their shortcuts and the per-tool style that persists between uses (ANN-1, ANN-2).

use cm_core::annotation::{
    DEFAULT_BLUR_STRENGTH, DEFAULT_COLOR, DEFAULT_HIGHLIGHT_COLOR, DEFAULT_HIGHLIGHT_OPACITY,
    DEFAULT_PIXELATE_BLOCK, DEFAULT_STEP_SIZE, DEFAULT_TEXT_SIZE, DEFAULT_WIDTH,
};
use cm_core::{Color, OpKind};
use cm_i18n::tr_ctx;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Arrow,
    Line,
    Rect,
    Ellipse,
    Text,
    Highlight,
    Step,
    Blur,
    Pixelate,
    Crop,
}

impl Tool {
    /// Palette order (DESIGN 5.5: left vertical toolbar).
    pub const ALL: [Tool; 11] = [
        Tool::Select,
        Tool::Arrow,
        Tool::Line,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Text,
        Tool::Highlight,
        Tool::Step,
        Tool::Blur,
        Tool::Pixelate,
        Tool::Crop,
    ];

    pub fn label(self) -> String {
        match self {
            Tool::Select => tr_ctx!("tool", "Select"),
            Tool::Arrow => tr_ctx!("tool", "Arrow"),
            Tool::Line => tr_ctx!("tool", "Line"),
            Tool::Rect => tr_ctx!("tool", "Rectangle"),
            Tool::Ellipse => tr_ctx!("tool", "Ellipse"),
            Tool::Text => tr_ctx!("tool", "Text"),
            Tool::Highlight => tr_ctx!("tool", "Highlighter"),
            Tool::Step => tr_ctx!("tool", "Step badge"),
            Tool::Blur => tr_ctx!("tool", "Blur"),
            Tool::Pixelate => tr_ctx!("tool", "Pixelate"),
            Tool::Crop => tr_ctx!("tool", "Crop"),
        }
    }

    /// Single-key shortcut (ANN-1: V A L R E T H N C, ANN-2: B X).
    pub fn key(self) -> char {
        match self {
            Tool::Select => 'v',
            Tool::Arrow => 'a',
            Tool::Line => 'l',
            Tool::Rect => 'r',
            Tool::Ellipse => 'e',
            Tool::Text => 't',
            Tool::Highlight => 'h',
            Tool::Step => 'n',
            Tool::Blur => 'b',
            Tool::Pixelate => 'x',
            Tool::Crop => 'c',
        }
    }

    pub fn from_key(c: char) -> Option<Tool> {
        let c = c.to_ascii_lowercase();
        Tool::ALL.into_iter().find(|t| t.key() == c)
    }

    /// Lucide icon name for the palette button (the name is the accessible label).
    pub fn icon(self) -> &'static str {
        match self {
            Tool::Select => "mouse-pointer-2",
            Tool::Arrow => "move-up-right",
            Tool::Line => "slash",
            Tool::Rect => "square",
            Tool::Ellipse => "circle",
            Tool::Text => "type",
            Tool::Highlight => "highlighter",
            Tool::Step => "hash",
            Tool::Blur => "droplet",
            Tool::Pixelate => "grid-3x3",
            Tool::Crop => "crop",
        }
    }

    /// Key used for the persisted style (`editor.tool_styles`).
    pub fn id(self) -> &'static str {
        match self {
            Tool::Select => "select",
            Tool::Arrow => "arrow",
            Tool::Line => "line",
            Tool::Rect => "rect",
            Tool::Ellipse => "ellipse",
            Tool::Text => "text",
            Tool::Highlight => "highlight",
            Tool::Step => "step",
            Tool::Blur => "blur",
            Tool::Pixelate => "pixelate",
            Tool::Crop => "crop",
        }
    }

    /// The tool that draws ops of this kind.
    pub fn for_kind(k: &OpKind) -> Tool {
        match k {
            OpKind::Arrow { .. } => Tool::Arrow,
            OpKind::Line { .. } => Tool::Line,
            OpKind::Rect { .. } => Tool::Rect,
            OpKind::Ellipse { .. } => Tool::Ellipse,
            OpKind::Text { .. } => Tool::Text,
            OpKind::Highlight { .. } => Tool::Highlight,
            OpKind::Step { .. } => Tool::Step,
            OpKind::Blur { .. } => Tool::Blur,
            OpKind::Pixelate { .. } => Tool::Pixelate,
            OpKind::Crop { .. } => Tool::Crop,
        }
    }

    /// Inspector controls this tool exposes.
    pub fn params(self) -> &'static [Param] {
        match self {
            Tool::Select | Tool::Crop => &[],
            Tool::Arrow | Tool::Line | Tool::Rect | Tool::Ellipse => &[Param::Color, Param::Width],
            Tool::Text => &[Param::Color, Param::TextSize],
            Tool::Highlight => &[Param::Color, Param::Opacity],
            Tool::Step => &[Param::Color, Param::StepSize],
            Tool::Blur => &[Param::Strength],
            Tool::Pixelate => &[Param::Block],
        }
    }
}

/// An inspector control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Param {
    Color,
    Width,
    TextSize,
    StepSize,
    Opacity,
    Strength,
    Block,
}

impl Param {
    pub fn label(self) -> String {
        match self {
            Param::Color => tr_ctx!("annotation setting", "Color"),
            Param::Width => tr_ctx!("annotation setting", "Stroke width"),
            Param::TextSize => tr_ctx!("annotation setting", "Text size"),
            Param::StepSize => tr_ctx!("annotation setting", "Badge size"),
            Param::Opacity => tr_ctx!("annotation setting", "Opacity"),
            Param::Strength => tr_ctx!("annotation setting", "Blur strength"),
            Param::Block => tr_ctx!("annotation setting", "Pixel size"),
        }
    }
}

/// A new value for one [`Param`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamValue {
    Color(Color),
    Width(f64),
    TextSize(f64),
    StepSize(f64),
    Opacity(f64),
    Strength(u32),
    Block(u32),
}

impl ParamValue {
    pub fn param(self) -> Param {
        match self {
            ParamValue::Color(_) => Param::Color,
            ParamValue::Width(_) => Param::Width,
            ParamValue::TextSize(_) => Param::TextSize,
            ParamValue::StepSize(_) => Param::StepSize,
            ParamValue::Opacity(_) => Param::Opacity,
            ParamValue::Strength(_) => Param::Strength,
            ParamValue::Block(_) => Param::Block,
        }
    }
}

/// DESIGN section 2 annotation palette.
pub const PALETTE: [(&str, Color); 8] = [
    ("Red", Color::rgb(0xFF, 0x3B, 0x3B)), // i18n-ignore (id, see palette_name)
    ("Orange", Color::rgb(0xFF, 0x8A, 0x00)), // i18n-ignore (id, see palette_name)
    ("Yellow", Color::rgb(0xFF, 0xD6, 0x0A)), // i18n-ignore (id, see palette_name)
    ("Green", Color::rgb(0x2F, 0xD1, 0x58)), // i18n-ignore (id, see palette_name)
    ("Blue", Color::rgb(0x2E, 0x8B, 0xFF)), // i18n-ignore (id, see palette_name)
    ("Purple", Color::rgb(0xB1, 0x5C, 0xFF)), // i18n-ignore (id, see palette_name)
    ("Black", Color::rgb(0x11, 0x11, 0x11)), // i18n-ignore (id, see palette_name)
    ("White", Color::rgb(0xFF, 0xFF, 0xFF)), // i18n-ignore (id, see palette_name)
];

/// The translated name of a palette colour (the swatch's tooltip and accessible label).
pub fn palette_name(id: &str) -> String {
    match id {
        "Red" => tr_ctx!("colour", "Red"),
        "Orange" => tr_ctx!("colour", "Orange"),
        "Yellow" => tr_ctx!("colour", "Yellow"),
        "Green" => tr_ctx!("colour", "Green"),
        "Blue" => tr_ctx!("colour", "Blue"),
        "Purple" => tr_ctx!("colour", "Purple"),
        "Black" => tr_ctx!("colour", "Black"),
        "White" => tr_ctx!("colour", "White"),
        other => other.to_owned(),
    }
}

/// DESIGN 5.5: stroke widths 2, 4, 6, 8 px.
pub const WIDTHS: [f64; 4] = [2.0, 4.0, 6.0, 8.0];
pub const TEXT_SIZES: [f64; 5] = [16.0, 24.0, 32.0, 48.0, 64.0];
pub const STEP_SIZES: [f64; 4] = [20.0, 28.0, 40.0, 56.0];
pub const OPACITIES: [f64; 4] = [0.2, 0.35, 0.5, 0.7];

/// The values a tool draws with. Fields a tool does not use keep their defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolStyle {
    pub color: Color,
    pub width: f64,
    pub size: f64,
    pub opacity: f64,
    pub strength: u32,
    pub block: u32,
}

impl ToolStyle {
    pub fn defaults(tool: Tool) -> ToolStyle {
        let base = ToolStyle {
            color: DEFAULT_COLOR,
            width: DEFAULT_WIDTH,
            size: DEFAULT_TEXT_SIZE,
            opacity: DEFAULT_HIGHLIGHT_OPACITY,
            strength: DEFAULT_BLUR_STRENGTH,
            block: DEFAULT_PIXELATE_BLOCK,
        };
        match tool {
            Tool::Text => ToolStyle {
                color: Color::WHITE,
                ..base
            },
            Tool::Highlight => ToolStyle {
                color: DEFAULT_HIGHLIGHT_COLOR,
                ..base
            },
            Tool::Step => ToolStyle {
                size: DEFAULT_STEP_SIZE,
                ..base
            },
            _ => base,
        }
    }

    pub fn apply(&mut self, v: ParamValue) {
        match v {
            ParamValue::Color(c) => self.color = c,
            ParamValue::Width(w) => self.width = w,
            ParamValue::TextSize(s) | ParamValue::StepSize(s) => self.size = s,
            ParamValue::Opacity(o) => self.opacity = o,
            ParamValue::Strength(s) => self.strength = s,
            ParamValue::Block(b) => self.block = b,
        }
    }

    pub fn value(&self, p: Param) -> ParamValue {
        match p {
            Param::Color => ParamValue::Color(self.color),
            Param::Width => ParamValue::Width(self.width),
            Param::TextSize => ParamValue::TextSize(self.size),
            Param::StepSize => ParamValue::StepSize(self.size),
            Param::Opacity => ParamValue::Opacity(self.opacity),
            Param::Strength => ParamValue::Strength(self.strength),
            Param::Block => ParamValue::Block(self.block),
        }
    }
}

/// Every tool's style (ANN-1: the last used color and width persist per tool).
#[derive(Debug, Clone, PartialEq)]
pub struct Styles {
    by_tool: Vec<(Tool, ToolStyle)>,
}

impl Default for Styles {
    fn default() -> Self {
        Styles {
            by_tool: Tool::ALL
                .iter()
                .map(|t| (*t, ToolStyle::defaults(*t)))
                .collect(),
        }
    }
}

impl Styles {
    pub fn get(&self, tool: Tool) -> ToolStyle {
        self.by_tool
            .iter()
            .find(|(t, _)| *t == tool)
            .map(|(_, s)| *s)
            .unwrap_or_else(|| ToolStyle::defaults(tool))
    }

    pub fn apply(&mut self, tool: Tool, v: ParamValue) {
        if let Some((_, s)) = self.by_tool.iter_mut().find(|(t, _)| *t == tool) {
            s.apply(v);
        }
    }

    /// JSON for the `editor.tool_styles` setting.
    pub fn to_json(&self) -> Value {
        let mut m = serde_json::Map::new();
        for (t, s) in &self.by_tool {
            m.insert(
                t.id().to_owned(),
                json!({
                    "color": s.color.to_hex(), "width": s.width, "size": s.size,
                    "opacity": s.opacity, "strength": s.strength, "block": s.block,
                }),
            );
        }
        Value::Object(m)
    }

    /// Reads a stored value; anything malformed or out of range falls back to the default
    /// for that field.
    pub fn from_json(v: &Value) -> Styles {
        let mut out = Styles::default();
        for (t, s) in &mut out.by_tool {
            let Some(o) = v.get(t.id()) else { continue };
            if let Some(c) = o
                .get("color")
                .and_then(Value::as_str)
                .and_then(|c| Color::parse(c).ok())
            {
                s.color = c;
            }
            let num = |k: &str, lo: f64, hi: f64| {
                o.get(k)
                    .and_then(Value::as_f64)
                    .filter(|x| x.is_finite() && (lo..=hi).contains(x))
            };
            if let Some(w) = num("width", 1.0, 64.0) {
                s.width = w;
            }
            if let Some(w) = num("size", 6.0, 400.0) {
                s.size = w;
            }
            if let Some(w) = num("opacity", 0.05, 1.0) {
                s.opacity = w;
            }
            if let Some(w) = num("strength", 4.0, 32.0) {
                s.strength = w as u32;
            }
            if let Some(w) = num("block", 6.0, 32.0) {
                s.block = w as u32;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ANN-1 / ANN-2: the documented shortcut keys, one per tool, all distinct.
    #[test]
    fn shortcut_keys_match_the_spec() {
        let spec = [
            ('v', Tool::Select),
            ('a', Tool::Arrow),
            ('l', Tool::Line),
            ('r', Tool::Rect),
            ('e', Tool::Ellipse),
            ('t', Tool::Text),
            ('h', Tool::Highlight),
            ('n', Tool::Step),
            ('c', Tool::Crop),
            ('b', Tool::Blur),
            ('x', Tool::Pixelate),
        ];
        for (k, t) in spec {
            assert_eq!(Tool::from_key(k), Some(t));
            assert_eq!(Tool::from_key(k.to_ascii_uppercase()), Some(t));
            assert_eq!(t.key(), k);
        }
        assert_eq!(Tool::from_key('q'), None);
        let mut keys: Vec<char> = Tool::ALL.iter().map(|t| t.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), Tool::ALL.len());
    }

    /// ANN-1 defaults: red, 4 px; text white; highlighter yellow at 35%.
    #[test]
    fn defaults_follow_the_spec() {
        let s = Styles::default();
        assert_eq!(s.get(Tool::Arrow).color, DEFAULT_COLOR);
        assert_eq!(s.get(Tool::Arrow).width, 4.0);
        assert_eq!(s.get(Tool::Text).color, Color::WHITE);
        assert_eq!(s.get(Tool::Text).size, 24.0);
        assert_eq!(s.get(Tool::Highlight).color, DEFAULT_HIGHLIGHT_COLOR);
        assert_eq!(s.get(Tool::Blur).strength, 12);
        assert_eq!(s.get(Tool::Pixelate).block, 12);
        assert_eq!(s.get(Tool::Step).size, 28.0);
    }

    /// ANN-1: last used color and width persist per tool (and survive the settings round trip).
    #[test]
    fn styles_persist_per_tool() {
        let mut s = Styles::default();
        s.apply(Tool::Rect, ParamValue::Color(PALETTE[4].1));
        s.apply(Tool::Rect, ParamValue::Width(8.0));
        assert_eq!(
            s.get(Tool::Arrow).color,
            DEFAULT_COLOR,
            "other tools unchanged"
        );
        let back = Styles::from_json(&s.to_json());
        assert_eq!(back, s);
        assert_eq!(back.get(Tool::Rect).width, 8.0);
        // Garbage in the settings file never breaks the editor.
        let junk = json!({"rect": {"color": "red", "width": -3, "strength": 99}});
        assert_eq!(Styles::from_json(&junk), Styles::default());
    }

    /// I18N P2: labels are built when asked (they follow the language); English is unchanged.
    #[test]
    fn labels_in_english() {
        assert_eq!(Tool::Rect.label(), "Rectangle");
        assert_eq!(Tool::Step.label(), "Step badge");
        assert_eq!(Param::Width.label(), "Stroke width");
        assert_eq!(palette_name("Orange"), "Orange");
        assert_eq!(
            palette_name("Teal"),
            "Teal",
            "an unknown name is shown as is"
        );
        for (name, _) in PALETTE {
            assert_eq!(palette_name(name), name);
        }
    }

    #[test]
    fn every_drawing_tool_has_inspector_controls_except_select_and_crop() {
        for t in Tool::ALL {
            let empty = t.params().is_empty();
            assert_eq!(empty, matches!(t, Tool::Select | Tool::Crop), "{t:?}");
        }
    }
}
