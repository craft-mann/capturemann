//! Tools, their shortcuts and the per-tool style that persists between uses (ANN-1, ANN-2).

use cm_core::annotation::LineStyle;
use cm_core::annotation::{
    CalloutStyle, MagnifyShape, SpotMode, SpotShapeKind, StepStyle, TextAlign,
    DEFAULT_BLUR_STRENGTH, DEFAULT_CALLOUT_RADIUS, DEFAULT_CALLOUT_SIZE, DEFAULT_COLOR,
    DEFAULT_HIGHLIGHT_COLOR, DEFAULT_HIGHLIGHT_OPACITY, DEFAULT_MAGNIFY_ZOOM,
    DEFAULT_PIXELATE_BLOCK, DEFAULT_SPOTLIGHT_DIM, DEFAULT_STEP_SIZE, DEFAULT_TEXT_SIZE,
    DEFAULT_WIDTH,
};
use cm_core::annotation::{StampKind, DEFAULT_STAMP_SIZE};
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
    /// ANN-5: drag a rectangle, the text in it goes to the clipboard. Draws nothing.
    SelectText,
    /// ANN-4: a speech bubble or box with a pointer tail and text (key K).
    Callout,
    /// ANN-7: dims or blurs everything outside its holes (key S).
    Spotlight,
    /// ANN-4: a check, cross, `!`, `?` or a BEFORE / AFTER label (key U).
    Stamp,
    /// ANN-8: the Zoom Lens, a zoomed copy of a detail with a line back to it (key M).
    Magnify,
    /// ANN-9: change the text in a screenshot (key Y, Touch-up group).
    Retype,
    /// ANN-10: move or remove an area of the picture (key J, Touch-up group).
    MoveArea,
}

impl Tool {
    /// Palette order (design spec 0.3.3 section 2): the tools of each palette group are next to
    /// each other, in the order of the group menu.
    pub const ALL: [Tool; 18] = [
        Tool::Select,
        Tool::SelectText,
        Tool::Arrow,
        Tool::Line,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Text,
        Tool::Callout,
        Tool::Highlight,
        Tool::Step,
        Tool::Stamp,
        Tool::Retype,
        Tool::MoveArea,
        Tool::Spotlight,
        Tool::Magnify,
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
            Tool::SelectText => tr_ctx!("tool", "Select text"),
            Tool::Callout => tr_ctx!("tool", "Callout"),
            Tool::Spotlight => tr_ctx!("tool", "Spotlight"),
            Tool::Stamp => tr_ctx!("tool", "Stamp"),
            Tool::Magnify => tr_ctx!("tool", "Zoom Lens"),
            Tool::Retype => tr_ctx!("tool", "Retype"),
            Tool::MoveArea => tr_ctx!("tool", "Move area"),
        }
    }

    /// Single-key shortcut (ANN-1: V A L R E T H N C, ANN-2: B X, ANN-5: G, ANN-4: K U,
    /// ANN-7: S, ANN-8: M, ANN-9: Y, ANN-10: J).
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
            Tool::SelectText => 'g',
            Tool::Callout => 'k',
            Tool::Spotlight => 's',
            Tool::Stamp => 'u',
            Tool::Magnify => 'm',
            Tool::Retype => 'y',
            Tool::MoveArea => 'j',
        }
    }

    pub fn from_key(c: char) -> Option<Tool> {
        let c = c.to_ascii_lowercase();
        // Only tools the palette offers have a key (a tool left out of its group is off).
        Tool::ALL
            .into_iter()
            .find(|t| t.key() == c && t.in_palette())
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
            Tool::SelectText => "text-select",
            Tool::Callout => "message-square",
            Tool::Spotlight => "focus",
            Tool::Stamp => "stamp",
            Tool::Magnify => "zoom-in",
            Tool::Retype => "text-cursor-input",
            Tool::MoveArea => "move",
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
            Tool::SelectText => "select_text",
            Tool::Callout => "callout",
            Tool::Spotlight => "spotlight",
            Tool::Stamp => "stamp",
            Tool::Magnify => "magnify",
            Tool::Retype => "retype",
            Tool::MoveArea => "move_area",
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
            OpKind::Callout { .. } => Tool::Callout,
            OpKind::Spotlight { .. } => Tool::Spotlight,
            OpKind::Stamp { .. } => Tool::Stamp,
            OpKind::Blur { .. } => Tool::Blur,
            OpKind::Pixelate { .. } => Tool::Pixelate,
            OpKind::Crop { .. } => Tool::Crop,
            OpKind::Magnify { .. } => Tool::Magnify,
            OpKind::Retype { .. } => Tool::Retype,
            OpKind::MoveArea { .. } => Tool::MoveArea,
            // Blueprint is a view-bar command, not a tool: its op selects like the pointer.
            OpKind::Blueprint { .. } => Tool::Select,
        }
    }

    /// Inspector controls this tool exposes.
    pub fn params(self) -> &'static [Param] {
        match self {
            Tool::Callout => &[Param::Color, Param::TextSize, Param::Radius],
            Tool::Spotlight => &[Param::Dim],
            Tool::Stamp => &[Param::Color, Param::StampSize],
            Tool::Magnify => &[Param::Zoom, Param::Color, Param::BorderWidth],
            Tool::Retype => &[Param::Color, Param::TextSize],
            Tool::Select | Tool::Crop | Tool::SelectText | Tool::MoveArea => &[],
            Tool::Arrow | Tool::Line | Tool::Rect | Tool::Ellipse => {
                &[Param::Color, Param::Width, Param::LineStyle]
            }
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
    /// Corner radius of a callout box.
    Radius,
    /// How dark a spotlight makes the outside (0 to 1).
    Dim,
    /// Size of a stamp.
    StampSize,
    /// Zoom Lens magnification (2 to 8).
    Zoom,
    /// Zoom Lens border width (0 to 32 px).
    BorderWidth,
    /// Solid, dashed or dotted line of an arrow, line, rectangle or ellipse (ANN-12).
    LineStyle,
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
            Param::Radius => tr_ctx!("annotation setting", "Corners"),
            Param::Dim => tr_ctx!("annotation setting", "Dim"),
            Param::StampSize => tr_ctx!("annotation setting", "Size"),
            Param::Zoom => tr_ctx!("annotation setting", "Zoom"),
            Param::BorderWidth => tr_ctx!("annotation setting", "Border width"),
            Param::LineStyle => tr_ctx!("annotation setting", "Line style"),
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
    Radius(f64),
    Dim(f64),
    StampSize(f64),
    Zoom(f64),
    BorderWidth(f64),
    LineStyle(LineStyle),
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
            ParamValue::Radius(_) => Param::Radius,
            ParamValue::Dim(_) => Param::Dim,
            ParamValue::StampSize(_) => Param::StampSize,
            ParamValue::Zoom(_) => Param::Zoom,
            ParamValue::BorderWidth(_) => Param::BorderWidth,
            ParamValue::LineStyle(_) => Param::LineStyle,
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

/// The word of a label stamp. It is written into the op in the UI language when the stamp is
/// placed (stored text, never re-translated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StampWord {
    Before,
    After,
}

impl StampWord {
    pub const ALL: [StampWord; 2] = [StampWord::Before, StampWord::After];

    /// The word in the UI language.
    pub fn text(self) -> String {
        match self {
            StampWord::Before => tr_ctx!("stamp label", "Before"),
            StampWord::After => tr_ctx!("stamp label", "After"),
        }
    }
}

/// The usual colour of a stamp kind: green for a check, red for the rest.
pub fn stamp_color(kind: StampKind) -> Color {
    match kind {
        StampKind::Check => Color::rgb(0x2E, 0xB8, 0x72),
        _ => DEFAULT_COLOR,
    }
}

/// DESIGN 5.5: stroke widths 2, 4, 6, 8 px.
pub const WIDTHS: [f64; 4] = [2.0, 4.0, 6.0, 8.0];
pub const TEXT_SIZES: [f64; 5] = [16.0, 24.0, 32.0, 48.0, 64.0];
pub const STEP_SIZES: [f64; 4] = [20.0, 28.0, 40.0, 56.0];
pub const OPACITIES: [f64; 4] = [0.2, 0.35, 0.5, 0.7];
/// Callout text sizes (px) and corner radii offered in the inspector.
pub const CALLOUT_SIZES: [f64; 5] = [16.0, 20.0, 24.0, 32.0, 40.0];
pub const CORNER_RADII: [f64; 4] = [0.0, 6.0, 12.0, 20.0];
/// Spotlight dim levels offered in the inspector (default 60 %).
pub const DIM_LEVELS: [f64; 4] = [0.2, 0.4, 0.6, 0.8];
/// Stamp sizes offered in the inspector (default 34).
pub const STAMP_SIZES: [f64; 4] = [24.0, 34.0, 48.0, 64.0];
/// Zoom Lens magnifications offered in the inspector (default 3).
pub const ZOOMS: [f64; 5] = [2.0, 3.0, 4.0, 6.0, 8.0];
/// Zoom Lens border widths offered in the inspector, px (default 3; 0 is no border).
pub const BORDER_WIDTHS: [f64; 6] = [0.0, 2.0, 3.0, 4.0, 6.0, 8.0];
/// Retype font weights offered in the inspector: Regular, Medium, Bold.
pub const WEIGHTS: [u16; 3] = [400, 500, 700];

/// The values a tool draws with. Fields a tool does not use keep their defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolStyle {
    pub color: Color,
    pub width: f64,
    /// Solid, dashed or dotted line of new arrows, lines, rectangles and ellipses (ANN-12).
    pub line_style: LineStyle,
    pub size: f64,
    pub opacity: f64,
    pub strength: u32,
    pub block: u32,
    /// Callout corner radius (ANN-4).
    pub radius: f64,
    /// Callout look: bubble or box (ANN-4).
    pub callout_style: CalloutStyle,
    /// Spotlight: how dark the outside is (ANN-7).
    pub dim: f64,
    /// Spotlight: dim or blur the outside (ANN-7).
    pub spot_mode: SpotMode,
    /// Spotlight: the shape the next drag makes (ANN-7).
    pub spot_shape: SpotShapeKind,
    /// Step badges: numbering style and first number of new steps (ANN-6).
    pub step_style: StepStyle,
    pub step_start: u32,
    /// Stamp: what the next click places (ANN-4).
    pub stamp_kind: StampKind,
    /// Stamp: which word a label stamp carries.
    pub stamp_word: StampWord,
    /// Zoom Lens: magnification, loupe shape, connector line and zoom badge (ANN-8). The
    /// border colour is `color` and the border width is `width`.
    pub zoom: f64,
    pub lens_shape: MagnifyShape,
    pub connector: bool,
    pub badge: bool,
    /// Retype: font weight and alignment of the new text (ANN-9).
    pub weight: u16,
    pub align: TextAlign,
    /// Text tool only (ANN-1.AC8): the colour of a new text is picked from the picture (Auto).
    /// Picking a colour by hand turns it off until the user chooses Auto again.
    pub auto_color: bool,
}

impl ToolStyle {
    pub fn defaults(tool: Tool) -> ToolStyle {
        let base = ToolStyle {
            color: DEFAULT_COLOR,
            width: DEFAULT_WIDTH,
            line_style: LineStyle::Solid,
            size: DEFAULT_TEXT_SIZE,
            opacity: DEFAULT_HIGHLIGHT_OPACITY,
            strength: DEFAULT_BLUR_STRENGTH,
            block: DEFAULT_PIXELATE_BLOCK,
            radius: DEFAULT_CALLOUT_RADIUS,
            callout_style: CalloutStyle::Bubble,
            dim: DEFAULT_SPOTLIGHT_DIM,
            spot_mode: SpotMode::Dim,
            spot_shape: SpotShapeKind::Rect,
            step_style: StepStyle::Number,
            step_start: 1,
            stamp_kind: StampKind::Check,
            stamp_word: StampWord::Before,
            zoom: DEFAULT_MAGNIFY_ZOOM,
            lens_shape: MagnifyShape::Rounded,
            connector: true,
            badge: true,
            weight: 400,
            align: TextAlign::Left,
            auto_color: false,
        };
        match tool {
            Tool::Magnify => ToolStyle {
                color: Color::WHITE,
                width: 3.0,
                ..base
            },
            Tool::Callout => ToolStyle {
                size: DEFAULT_CALLOUT_SIZE,
                ..base
            },
            Tool::Stamp => ToolStyle {
                size: DEFAULT_STAMP_SIZE,
                color: stamp_color(StampKind::Check),
                ..base
            },
            Tool::Text => ToolStyle {
                color: Color::WHITE,
                auto_color: true,
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
            ParamValue::TextSize(s) | ParamValue::StepSize(s) | ParamValue::StampSize(s) => {
                self.size = s
            }
            ParamValue::Opacity(o) => self.opacity = o,
            ParamValue::Strength(s) => self.strength = s,
            ParamValue::Block(b) => self.block = b,
            ParamValue::Radius(r) => self.radius = r,
            ParamValue::Dim(d) => self.dim = d,
            ParamValue::Zoom(z) => self.zoom = z,
            ParamValue::BorderWidth(w) => self.width = w,
            ParamValue::LineStyle(l) => self.line_style = l,
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
            Param::Radius => ParamValue::Radius(self.radius),
            Param::Dim => ParamValue::Dim(self.dim),
            Param::StampSize => ParamValue::StampSize(self.size),
            Param::Zoom => ParamValue::Zoom(self.zoom),
            Param::BorderWidth => ParamValue::BorderWidth(self.width),
            Param::LineStyle => ParamValue::LineStyle(self.line_style),
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
            // A colour picked by hand is the Text tool's remembered colour; Auto is off.
            if tool == Tool::Text && matches!(v, ParamValue::Color(_)) {
                s.auto_color = false;
            }
        }
    }

    /// Auto on (a new text picks its colour from the picture) or off, for the Text tool.
    pub fn set_text_auto(&mut self, on: bool) {
        if let Some((_, s)) = self.by_tool.iter_mut().find(|(t, _)| *t == Tool::Text) {
            s.auto_color = on;
        }
    }

    /// What new spotlights are made of: the outside look and the shape of the next hole.
    pub fn set_spotlight(&mut self, mode: Option<SpotMode>, shape: Option<SpotShapeKind>) {
        if let Some((_, s)) = self.by_tool.iter_mut().find(|(t, _)| *t == Tool::Spotlight) {
            if let Some(m) = mode {
                s.spot_mode = m;
            }
            if let Some(k) = shape {
                s.spot_shape = k;
            }
        }
    }

    /// What the next stamp is (and its colour: each kind has its usual one).
    pub fn set_stamp_kind(&mut self, kind: StampKind, word: StampWord) {
        if let Some((_, s)) = self.by_tool.iter_mut().find(|(t, _)| *t == Tool::Stamp) {
            s.stamp_kind = kind;
            s.stamp_word = word;
            s.color = stamp_color(kind);
        }
    }

    /// The numbering new step badges get.
    pub fn set_step_numbering(&mut self, style: StepStyle, start: u32) {
        if let Some((_, s)) = self.by_tool.iter_mut().find(|(t, _)| *t == Tool::Step) {
            s.step_style = style;
            s.step_start = start;
        }
    }

    /// The look new callouts get (bubble or box).
    pub fn set_callout_style(&mut self, style: CalloutStyle) {
        if let Some((_, s)) = self.by_tool.iter_mut().find(|(t, _)| *t == Tool::Callout) {
            s.callout_style = style;
        }
    }

    /// What new Zoom Lenses are made of: shape, connector and badge (None keeps the field).
    pub fn set_lens(
        &mut self,
        shape: Option<MagnifyShape>,
        connector: Option<bool>,
        badge: Option<bool>,
    ) {
        if let Some((_, s)) = self.by_tool.iter_mut().find(|(t, _)| *t == Tool::Magnify) {
            if let Some(v) = shape {
                s.lens_shape = v;
            }
            if let Some(v) = connector {
                s.connector = v;
            }
            if let Some(v) = badge {
                s.badge = v;
            }
        }
    }

    /// What new Retype text is made of: weight and alignment (None keeps the field).
    pub fn set_retype(&mut self, weight: Option<u16>, align: Option<TextAlign>) {
        if let Some((_, s)) = self.by_tool.iter_mut().find(|(t, _)| *t == Tool::Retype) {
            if let Some(v) = weight {
                s.weight = v;
            }
            if let Some(v) = align {
                s.align = v;
            }
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
                    "line_style": match s.line_style {
                        LineStyle::Solid => "solid",
                        LineStyle::Dashed => "dashed",
                        LineStyle::Dotted => "dotted",
                    },
                    "opacity": s.opacity, "strength": s.strength, "block": s.block,
                    "radius": s.radius, "dim": s.dim,
                    "spot_mode": match s.spot_mode {
                        SpotMode::Dim => "dim",
                        SpotMode::Blur => "blur",
                    },
                    "step_style": match s.step_style {
                        StepStyle::Number => "number",
                        StepStyle::Upper => "upper",
                        StepStyle::Lower => "lower",
                    },
                    "step_start": s.step_start,
                    "stamp_word": match s.stamp_word {
                        StampWord::Before => "before",
                        StampWord::After => "after",
                    },
                    "stamp": match s.stamp_kind {
                        StampKind::Check => "check",
                        StampKind::Cross => "cross",
                        StampKind::Exclaim => "exclaim",
                        StampKind::Question => "question",
                        StampKind::Label => "label",
                    },
                    "spot_shape": match s.spot_shape {
                        SpotShapeKind::Rect => "rect",
                        SpotShapeKind::Ellipse => "ellipse",
                    },
                    "callout": match s.callout_style {
                        CalloutStyle::Bubble => "bubble",
                        CalloutStyle::Box => "box",
                    },
                    "zoom": s.zoom,
                    "lens_shape": match s.lens_shape {
                        MagnifyShape::Circle => "circle",
                        MagnifyShape::Rounded => "rounded",
                        MagnifyShape::Rect => "rect",
                    },
                    "connector": s.connector,
                    "badge": s.badge,
                    "auto": s.auto_color,
                    "weight": s.weight,
                    "align": match s.align {
                        TextAlign::Left => "left",
                        TextAlign::Center => "center",
                        TextAlign::Right => "right",
                    },
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
            if let Some(w) = num("width", 0.0, 64.0) {
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
            if let Some(w) = num("radius", 0.0, 2000.0) {
                s.radius = w;
            }
            if let Some(w) = num("dim", 0.0, 1.0) {
                s.dim = w;
            }
            if *t == Tool::Text {
                if let Some(b) = o.get("auto").and_then(Value::as_bool) {
                    s.auto_color = b;
                }
            }
            match o.get("line_style").and_then(Value::as_str) {
                Some("solid") => s.line_style = LineStyle::Solid,
                Some("dashed") => s.line_style = LineStyle::Dashed,
                Some("dotted") => s.line_style = LineStyle::Dotted,
                _ => {}
            }
            match o.get("stamp_word").and_then(Value::as_str) {
                Some("before") => s.stamp_word = StampWord::Before,
                Some("after") => s.stamp_word = StampWord::After,
                _ => {}
            }
            match o.get("stamp").and_then(Value::as_str) {
                Some("check") => s.stamp_kind = StampKind::Check,
                Some("cross") => s.stamp_kind = StampKind::Cross,
                Some("exclaim") => s.stamp_kind = StampKind::Exclaim,
                Some("question") => s.stamp_kind = StampKind::Question,
                Some("label") => s.stamp_kind = StampKind::Label,
                _ => {}
            }
            match o.get("step_style").and_then(Value::as_str) {
                Some("number") => s.step_style = StepStyle::Number,
                Some("upper") => s.step_style = StepStyle::Upper,
                Some("lower") => s.step_style = StepStyle::Lower,
                _ => {}
            }
            if let Some(w) = num("step_start", 1.0, 9999.0) {
                s.step_start = w as u32;
            }
            match o.get("spot_mode").and_then(Value::as_str) {
                Some("dim") => s.spot_mode = SpotMode::Dim,
                Some("blur") => s.spot_mode = SpotMode::Blur,
                _ => {}
            }
            match o.get("spot_shape").and_then(Value::as_str) {
                Some("rect") => s.spot_shape = SpotShapeKind::Rect,
                Some("ellipse") => s.spot_shape = SpotShapeKind::Ellipse,
                _ => {}
            }
            match o.get("callout").and_then(Value::as_str) {
                Some("bubble") => s.callout_style = CalloutStyle::Bubble,
                Some("box") => s.callout_style = CalloutStyle::Box,
                _ => {}
            }
            if let Some(z) = num("zoom", 2.0, 8.0) {
                s.zoom = z;
            }
            match o.get("lens_shape").and_then(Value::as_str) {
                Some("circle") => s.lens_shape = MagnifyShape::Circle,
                Some("rounded") => s.lens_shape = MagnifyShape::Rounded,
                Some("rect") => s.lens_shape = MagnifyShape::Rect,
                _ => {}
            }
            if let Some(b) = o.get("connector").and_then(Value::as_bool) {
                s.connector = b;
            }
            if let Some(b) = o.get("badge").and_then(Value::as_bool) {
                s.badge = b;
            }
            if let Some(w) = num("weight", 100.0, 900.0) {
                s.weight = w as u16;
            }
            match o.get("align").and_then(Value::as_str) {
                Some("left") => s.align = TextAlign::Left,
                Some("center") => s.align = TextAlign::Center,
                Some("right") => s.align = TextAlign::Right,
                _ => {}
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
            ('k', Tool::Callout),
            ('s', Tool::Spotlight),
            ('g', Tool::SelectText),
            ('m', Tool::Magnify),
            ('y', Tool::Retype),
            ('j', Tool::MoveArea),
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

    /// ANN-12: the line style is remembered per tool, defaults to solid, only the four shape
    /// tools offer it, and it survives the settings round trip.
    #[test]
    fn line_style_persists_per_tool() {
        let mut s = Styles::default();
        assert_eq!(s.get(Tool::Rect).line_style, LineStyle::Solid);
        s.apply(Tool::Rect, ParamValue::LineStyle(LineStyle::Dotted));
        assert_eq!(s.get(Tool::Rect).line_style, LineStyle::Dotted);
        assert_eq!(s.get(Tool::Arrow).line_style, LineStyle::Solid);
        let back = Styles::from_json(&s.to_json());
        assert_eq!(back, s);
        for t in [Tool::Arrow, Tool::Line, Tool::Rect, Tool::Ellipse] {
            assert!(t.params().contains(&Param::LineStyle));
        }
        assert!(!Tool::Highlight.params().contains(&Param::LineStyle));
        assert!(!Tool::Callout.params().contains(&Param::LineStyle));
        assert_eq!(Param::LineStyle.label(), "Line style");
        let bad = serde_json::json!({ "rect": { "line_style": "wavy" } });
        assert_eq!(
            Styles::from_json(&bad).get(Tool::Rect).line_style,
            LineStyle::Solid
        );
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

    /// ANN-8 / ANN-9: the Zoom Lens and Retype settings are remembered per tool and survive the
    /// settings round trip; the tool is named Zoom Lens in the UI (the op stays `magnify`).
    #[test]
    fn lens_and_retype_styles_persist_and_the_names_are_the_ui_names() {
        let mut s = Styles::default();
        s.apply(Tool::Magnify, ParamValue::Zoom(6.0));
        s.apply(Tool::Magnify, ParamValue::BorderWidth(8.0));
        s.set_lens(Some(MagnifyShape::Circle), Some(false), Some(false));
        s.set_retype(Some(700), Some(TextAlign::Center));
        let back = Styles::from_json(&s.to_json());
        assert_eq!(back, s);
        let m = back.get(Tool::Magnify);
        assert_eq!(
            (m.zoom, m.width, m.lens_shape, m.connector, m.badge),
            (6.0, 8.0, MagnifyShape::Circle, false, false)
        );
        let r = back.get(Tool::Retype);
        assert_eq!((r.weight, r.align), (700, TextAlign::Center));
        assert_eq!(Tool::Magnify.label(), "Zoom Lens");
        assert_eq!(Tool::Magnify.id(), "magnify");
        assert_eq!(
            Tool::for_kind(&OpKind::Magnify {
                source: cm_core::Rc::new(0.0, 0.0, 10.0, 10.0),
                at: cm_core::Pt::new(5.0, 5.0),
                zoom: 3.0,
                shape: MagnifyShape::Rounded,
                border: Color::WHITE,
                border_width: 3.0,
                connector: true,
                badge: true,
            }),
            Tool::Magnify
        );
        // A border width of 0 (no border) is a valid stored value.
        let none = json!({"magnify": {"width": 0}});
        assert_eq!(Styles::from_json(&none).get(Tool::Magnify).width, 0.0);
    }

    #[test]
    fn every_drawing_tool_has_inspector_controls_except_select_and_crop() {
        for t in Tool::ALL {
            let empty = t.params().is_empty();
            assert_eq!(
                empty,
                matches!(
                    t,
                    Tool::Select | Tool::Crop | Tool::SelectText | Tool::MoveArea
                ),
                "{t:?}"
            );
        }
    }
}
