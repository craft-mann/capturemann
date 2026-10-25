//! Design tokens from the design kit (`design/tokens/tokens.json`, `design/CLAUDE.md`), as
//! plain data so they can be unit tested without a display. `ui::paint` converts them to GPUI
//! colors. A test (`tokens_match_design_kit`) fails when this file drifts from the kit.
//!
//! Studio follows the OS (dark by default, the kit's light theme when the OS is light);
//! the floating windows (toasts, confirmation dialog) are dark in both themes.

use serde::{Deserialize, Serialize};

/// `0xRRGGBBAA`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u32);

impl Color {
    /// Opaque `0xRRGGBB`.
    pub const fn rgb(v: u32) -> Self {
        Color((v << 8) | 0xFF)
    }

    /// `0xRRGGBB` with alpha in percent (0..=100), for tokens written as `rgba(.., 0.16)`.
    pub const fn rgb_a(v: u32, alpha_percent: u32) -> Self {
        Color((v << 8) | ((alpha_percent * 255 + 50) / 100))
    }

    /// The same color at `alpha_percent` (0..=100) opacity.
    pub const fn with_alpha(self, alpha_percent: u32) -> Self {
        Color((self.0 & 0xFFFF_FF00) | ((alpha_percent * 255 + 50) / 100))
    }

    pub const fn r(self) -> u8 {
        (self.0 >> 24) as u8
    }
    pub const fn g(self) -> u8 {
        (self.0 >> 16) as u8
    }
    pub const fn b(self) -> u8 {
        (self.0 >> 8) as u8
    }
    pub const fn a(self) -> u8 {
        self.0 as u8
    }

    /// Source-over compositing onto an opaque background.
    pub fn over(self, bg: Color) -> Color {
        let a = f64::from(self.a()) / 255.0;
        let mix = |f: u8, b: u8| (f64::from(f) * a + f64::from(b) * (1.0 - a)).round() as u8;
        Color::rgb(
            (u32::from(mix(self.r(), bg.r())) << 16)
                | (u32::from(mix(self.g(), bg.g())) << 8)
                | u32::from(mix(self.b(), bg.b())),
        )
    }

    /// WCAG 2.x relative luminance.
    pub fn luminance(self) -> f64 {
        let lin = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.r()) + 0.7152 * lin(self.g()) + 0.0722 * lin(self.b())
    }
}

/// WCAG contrast ratio of two opaque colors.
pub fn contrast(a: Color, b: Color) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// What the user picked in Settings -> General (`appearance.theme`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    /// Follow the OS: dark by default, light when the OS is light.
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
}

impl ThemePreference {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "system" => Some(Self::System),
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }

    /// The Studio mode to draw. `os_is_light` is `None` when the OS appearance is unknown
    /// (treated as dark: dark is the default).
    pub fn resolve(self, os_is_light: Option<bool>) -> Mode {
        match self {
            Self::Dark => Mode::Dark,
            Self::Light => Mode::Light,
            Self::System => {
                if os_is_light == Some(true) {
                    Mode::Light
                } else {
                    Mode::Dark
                }
            }
        }
    }
}

/// All color tokens of one theme (kit names: ground, sidebar, surface, surface-2, raised, field,
/// border, line, text, text-2, text-3, mist, faint, rec, rec-soft, agent, success, warning).
///
/// **Name mapping with the redesign kit** (`design/redesign/CLAUDE.md`): the kit's `--line`
/// (#222226, card borders) is `border` here and the kit's `--line-strong` (#2E2E34, control
/// borders, switch-off track) is `line` here. The code names are older than the redesign and are
/// kept so no call site changes; `design/tokens` carries both spellings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub mode: Mode,
    /// Window background.
    pub ground: Color,
    /// Sidebar and footers.
    pub sidebar: Color,
    /// Cards and panels.
    pub surface: Color,
    /// Secondary buttons and the second step of the surface ladder (kit `--surface-2`).
    pub surface_2: Color,
    /// Selected nav item, hover, inputs, keycaps.
    pub raised: Color,
    /// Inputs and the segmented-control track (kit `--field`).
    pub field: Color,
    /// Card borders and row dividers (the kit's `--line`).
    pub border: Color,
    /// Strong borders, dividers, the switch-off track (the kit's `--line-strong`).
    pub line: Color,
    pub text: Color,
    pub text_2: Color,
    pub text_3: Color,
    /// Captions: the lowest contrast allowed for small text.
    pub mist: Color,
    /// Below `mist`: disabled labels, placeholders, decorative glyphs. Not for text that must be
    /// read (it does not reach 4.5:1 on dark).
    pub faint: Color,
    /// Record red. Reserved: record/stop, the recording state, the logo bubble and the
    /// primary action on dark. Never an error color.
    pub rec: Color,
    /// Hover state of a `rec` button.
    pub rec_hover: Color,
    /// A rec icon or label drawn on a dark rec tint (kit `--rec-soft`).
    pub rec_soft: Color,
    /// Text on `rec`: always ink, never white.
    pub on_rec: Color,
    /// The kit's ink, also the letterbox behind images.
    pub ink: Color,
    pub agent: Color,
    pub agent_text: Color,
    /// Agent card fill and border (sidebar MCP card, agent toast).
    pub agent_fill: Color,
    pub agent_border: Color,
    pub success: Color,
    pub warning: Color,
    /// Primary button (OD-17): neutral. Dark = `text` fill with ink text; light = ink fill with
    /// white text. Record, Stop and destructive confirmations use `rec`/`on_rec` instead.
    pub primary_bg: Color,
    pub primary_fg: Color,
    pub primary_hover: Color,
    pub scrim: Color,
    /// The tint over the main window while a pop-up sheet or dialog is open (board R9/R20:
    /// `rgba(5,5,6,0.66)`; the board's 3 px blur has no GPUI equivalent).
    pub modal_scrim: Color,
}

pub const INK: Color = Color::rgb(0x0A0A0B);
pub const REC: Color = Color::rgb(0xFF4F3A);

impl Theme {
    pub const DARK: Theme = Theme {
        mode: Mode::Dark,
        ground: INK,
        sidebar: Color::rgb(0x0E0E10),
        surface: Color::rgb(0x111113),
        surface_2: Color::rgb(0x141417),
        raised: Color::rgb(0x1E1E22),
        field: Color::rgb(0x0D0D0F),
        border: Color::rgb(0x222226),
        line: Color::rgb(0x2E2E34),
        text: Color::rgb(0xEDEDEF),
        text_2: Color::rgb(0xC9C9CF),
        text_3: Color::rgb(0xA1A1A9),
        mist: Color::rgb(0x8B8B93),
        faint: Color::rgb(0x6E6E76),
        rec: REC,
        rec_hover: Color::rgb(0xFF6551),
        rec_soft: Color::rgb(0xFF6E5C),
        on_rec: INK,
        ink: INK,
        agent: Color::rgb(0x5B8CFF),
        agent_text: Color::rgb(0x8FB0FF),
        agent_fill: Color::rgb_a(0x5B8CFF, 7),
        agent_border: Color::rgb_a(0x5B8CFF, 35),
        success: Color::rgb(0x3DD68C),
        warning: Color::rgb(0xF5B84A),
        primary_bg: Color::rgb(0xEDEDEF),
        primary_fg: INK,
        primary_hover: Color::rgb(0xD9D9DC),
        scrim: Color::rgb_a(0x000000, 55),
        modal_scrim: Color::rgb_a(0x050506, 66),
    };

    /// The kit's light theme (`EditorLight`): primary is ink on white, red stays rare. Not
    /// redesigned (OD-18): it keeps working through this token mapping.
    /// The kit gives no light success/warning; the dark ones are reused for dots and the
    /// text on them uses `text`.
    pub const LIGHT: Theme = Theme {
        mode: Mode::Light,
        ground: Color::rgb(0xF6F6F4),
        sidebar: Color::rgb(0xEFEFEC),
        surface: Color::rgb(0xFFFFFF),
        surface_2: Color::rgb(0xFAFAF8),
        raised: Color::rgb(0xF6F6F4),
        field: Color::rgb(0xEFEFEC),
        border: Color::rgb(0xE4E4E0),
        line: Color::rgb(0xD6D6D1),
        text: INK,
        text_2: Color::rgb(0x2A2A30),
        text_3: Color::rgb(0x44444B),
        mist: Color::rgb(0x55555C),
        faint: Color::rgb(0x6B6B72),
        rec: REC,
        rec_hover: Color::rgb(0xFF6551),
        rec_soft: Color::rgb(0xD93A27),
        on_rec: INK,
        ink: INK,
        agent: Color::rgb(0x3B6FE8),
        agent_text: Color::rgb(0x2F5FD0),
        agent_fill: Color::rgb(0xEEF3FF),
        agent_border: Color::rgb(0xC8D6FA),
        success: Color::rgb(0x3DD68C),
        warning: Color::rgb(0xF5B84A),
        primary_bg: INK,
        primary_fg: Color::rgb(0xFFFFFF),
        primary_hover: Color::rgb(0x2A2A30),
        scrim: Color::rgb_a(0x000000, 55),
        modal_scrim: Color::rgb_a(0x000000, 45),
    };

    pub fn for_mode(mode: Mode) -> Theme {
        match mode {
            Mode::Dark => Self::DARK,
            Mode::Light => Self::LIGHT,
        }
    }

    /// Selected segment of a segmented control (kit `#2A2A30`; light: the surface).
    pub fn segment_selected(&self) -> Color {
        match self.mode {
            Mode::Dark => Color::rgb(0x2A2A30),
            Mode::Light => self.surface,
        }
    }

    /// The 34 px icon tile of a settings row (kit `#1A1A1E`; light: `raised`).
    pub fn icon_tile(&self) -> Color {
        match self.mode {
            Mode::Dark => Color::rgb(0x1A1A1E),
            Mode::Light => self.raised,
        }
    }

    /// The 1 px divider between settings rows (kit `#1E1E22`; light: `border`).
    pub fn row_divider(&self) -> Color {
        match self.mode {
            Mode::Dark => Color::rgb(0x1E1E22),
            Mode::Light => self.border,
        }
    }

    /// Floating windows (toasts, confirmation dialog) stay dark in both themes.
    pub fn overlay() -> Theme {
        Self::DARK
    }
}

/// Colors the floating surfaces derive from the dark tokens (kit: "96% surface", "agent at 45%",
/// ...). Every raw color of the UI lives in this file, so a palette change is one edit.
pub mod derived {
    use super::{Color, Theme, REC};

    pub const WHITE: Color = Color::rgb(0xFFFFFF);
    pub const BLACK: Color = Color::rgb(0x000000);
    /// The HUD, the bubble toolbar: surface at 96%.
    pub const HUD_FILL: Color = Theme::DARK.surface.with_alpha(96);
    /// The start sheet: surface at 98%.
    pub const SHEET_FILL: Color = Theme::DARK.surface.with_alpha(98);
    /// The recording dot at the dim end of its blink.
    pub const REC_DIM: Color = REC.with_alpha(45);
    /// Agent chip and agent toast border (`agent` at 45%).
    pub const AGENT_CHIP_BORDER: Color = Theme::DARK.agent.with_alpha(45);
    /// Launcher agent tile (`agent` at 14%, border 40%).
    pub const AGENT_TILE_FILL: Color = Theme::DARK.agent.with_alpha(14);
    pub const AGENT_TILE_BORDER: Color = Theme::DARK.agent.with_alpha(40);
    /// Launcher selected row (kit `#232328`, no token).
    pub const LAUNCHER_SELECTED: Color = Color::rgb(0x232328);
    /// The window picker (R19): the palette fill (`rgba(19,19,22,.97)`), its footer, a card, the
    /// selected card, a card's border, and the thumbnail well with its border. Kit values, no token.
    pub const PICKER_FILL: Color = Color::rgb_a(0x131316, 97);
    pub const PICKER_FOOTER: Color = Color::rgb(0x0F0F11);
    pub const PICKER_CARD: Color = Color::rgb(0x141417);
    pub const PICKER_CARD_SELECTED: Color = Color::rgb(0x1C1C21);
    pub const PICKER_CARD_BORDER: Color = Color::rgb(0x26262B);
    pub const PICKER_THUMB: Color = Color::rgb(0x1A1B22);
    pub const PICKER_THUMB_BORDER: Color = Color::rgb(0x26262E);
    /// The outline drawn on the real window: the 6 px ring (`rgba(237,237,239,.18)`).
    pub const PICKER_OUTLINE_RING: Color = Color::rgb_a(0xEDEDEF, 18);
    /// A warning banner's tint.
    pub const WARNING_TINT: Color = Theme::DARK.warning.with_alpha(14);
    /// The bubble before the camera delivers a frame.
    pub const BUBBLE_PLACEHOLDER: Color = Theme::DARK.line;
    pub const BUBBLE_PLACEHOLDER_ICON: Color = Theme::DARK.mist;
}

/// Spacing, radius and size scales from the kit, in logical px.
pub mod scale {
    /// 4 px grid (`space.1..10`).
    pub const SPACE: [f32; 8] = [4., 8., 12., 16., 20., 24., 32., 40.];
    pub const RADIUS_XS: f32 = 6.;
    pub const RADIUS_SM: f32 = 8.;
    pub const RADIUS_MD: f32 = 10.;
    pub const RADIUS_LG: f32 = 14.;
    pub const RADIUS_XL: f32 = 16.;
    pub const RADIUS_PILL: f32 = 999.;
    /// Default button and input height.
    pub const CONTROL_H: f32 = 40.;
    /// Minimum hit target.
    pub const TARGET: f32 = 44.;
    pub const ROW_H: f32 = 48.;
    /// The app and settings sidebar (redesign, OD-16; was 220).
    pub const SIDEBAR_W: f32 = 248.;
    /// The icon rail on full-window pages.
    pub const RAIL_W: f32 = 64.;
    /// A toast window (redesign; the toast is not restyled yet).
    pub const TOAST_W: f32 = 440.;
    /// Dialog corner radius (confirm dialog, sheets): 18 to 20 px in the kit.
    pub const RADIUS_DIALOG: f32 = 18.;
    pub const RADIUS_DIALOG_LG: f32 = 20.;
}

/// Type scale from the kit (`fontSize`), px. Display text is weight 600; the kit's negative
/// letter-spacing (-0.045em display, -0.03em headings) is not applied because GPUI 0.3.9 has
/// no letter-spacing on text styles.
pub mod type_scale {
    pub const CAPTION: f32 = 12.;
    pub const UI: f32 = 14.;
    pub const BODY: f32 = 15.;
    pub const TITLE: f32 = 17.;
    pub const H2: f32 = 22.;
    pub const H1: f32 = 26.;
    pub const DISPLAY: f32 = 64.;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const TOKENS_JSON: &str = include_str!("../../../design/tokens/tokens.json");
    const TOKENS_CSS: &str = include_str!("../../../design/tokens/tokens.css");

    fn hex(s: &str) -> Color {
        let s = s.trim().trim_start_matches('#');
        assert_eq!(s.len(), 6, "{s}");
        Color::rgb(u32::from_str_radix(s, 16).unwrap_or_else(|_| panic!("bad hex {s}")))
    }

    fn json_color(v: &Value, path: &[&str]) -> Color {
        let mut cur = v;
        for p in path {
            cur = &cur[*p];
        }
        hex(cur["$value"].as_str().unwrap_or_else(|| panic!("{path:?}")))
    }

    fn json_px(v: &Value, path: &[&str]) -> f32 {
        let mut cur = v;
        for p in path {
            cur = &cur[*p];
        }
        cur["$value"]
            .as_str()
            .and_then(|s| s.trim_end_matches("px").parse().ok())
            .unwrap_or_else(|| panic!("{path:?}"))
    }

    /// The value of `--cm-<name>` in the `[data-theme="light"]` block of tokens.css.
    fn css_light(name: &str) -> Color {
        let start = TOKENS_CSS
            .find("[data-theme=\"light\"] {")
            .expect("light block");
        let block = &TOKENS_CSS[start..];
        let block = &block[..block.find('}').expect("end of block")];
        let key = format!("--cm-{name}:");
        let at = block.find(&key).unwrap_or_else(|| panic!("{name}"));
        let rest = &block[at + key.len()..];
        hex(rest[..rest.find(';').expect(";")].trim())
    }

    /// Fails when the constants here drift from the design kit. Dark and brand values come
    /// from tokens.json; the light theme is read from tokens.css (the kit's generated light
    /// block, which is complete; tokens.json only lists part of it and its light `text-2`
    /// disagrees with the CSS and CLAUDE.md table).
    #[test]
    fn tokens_match_design_kit() {
        let j: Value = serde_json::from_str(TOKENS_JSON).expect("tokens.json parses");
        let c = |n: &str| json_color(&j, &["color", n]);
        let d = Theme::DARK;
        assert_eq!(d.ground, c("ink"));
        assert_eq!(d.sidebar, c("sidebar"));
        assert_eq!(d.surface, c("surface"));
        assert_eq!(d.surface_2, c("surface-2"));
        assert_eq!(d.raised, c("raised"));
        assert_eq!(d.field, c("field"));
        assert_eq!(d.faint, c("faint"));
        assert_eq!(d.rec_hover, c("rec-hover"));
        assert_eq!(d.rec_soft, c("rec-soft"));
        assert_eq!(d.primary_bg, c("primary"));
        assert_eq!(d.primary_hover, c("primary-hover"));
        assert_eq!(d.border, c("border"));
        assert_eq!(d.line, c("line"));
        assert_eq!(d.text, c("text"));
        assert_eq!(d.text_2, c("text-2"));
        assert_eq!(d.text_3, c("text-3"));
        assert_eq!(d.mist, c("mist"));
        assert_eq!(d.rec, c("rec"));
        assert_eq!(d.on_rec, c("ink"));
        assert_eq!(d.agent, c("agent"));
        assert_eq!(d.agent_text, c("agent-text"));
        assert_eq!(d.success, c("success"));
        assert_eq!(d.warning, c("warning"));
        assert_eq!(Theme::LIGHT.ground, c("paper"));
        assert_eq!(Theme::LIGHT.rec, c("rec"));
        assert_eq!(
            Theme::LIGHT.primary_bg,
            json_color(&j, &["color", "light", "primary"])
        );

        let l = Theme::LIGHT;
        for (name, v) in [
            ("ground", l.ground),
            ("sidebar", l.sidebar),
            ("surface", l.surface),
            ("surface-2", l.surface_2),
            ("raised", l.raised),
            ("field", l.field),
            ("faint", l.faint),
            ("rec-soft", l.rec_soft),
            ("primary-hover", l.primary_hover),
            ("border", l.border),
            ("line", l.line),
            ("text", l.text),
            ("text-2", l.text_2),
            ("text-3", l.text_3),
            ("mist", l.mist),
            ("agent", l.agent),
            ("agent-text", l.agent_text),
            ("primary-bg", l.primary_bg),
            ("primary-fg", l.primary_fg),
        ] {
            assert_eq!(v, css_light(name), "light {name}");
        }
        // The JSON light entries that do exist must agree with the CSS too.
        for (n, css) in [
            ("ground", "ground"),
            ("sidebar", "sidebar"),
            ("surface", "surface"),
            ("border", "border"),
            ("line", "line"),
            ("text", "text"),
            ("mist", "mist"),
            ("agent", "agent"),
            ("surface-2", "surface-2"),
            ("field", "field"),
            ("faint", "faint"),
            ("rec-soft", "rec-soft"),
        ] {
            assert_eq!(
                json_color(&j, &["color", "light", n]),
                css_light(css),
                "{n}"
            );
        }

        assert_eq!(scale::RADIUS_XS, json_px(&j, &["radius", "xs"]));
        assert_eq!(scale::RADIUS_SM, json_px(&j, &["radius", "sm"]));
        assert_eq!(scale::RADIUS_MD, json_px(&j, &["radius", "md"]));
        assert_eq!(scale::RADIUS_LG, json_px(&j, &["radius", "lg"]));
        assert_eq!(scale::RADIUS_XL, json_px(&j, &["radius", "xl"]));
        assert_eq!(scale::RADIUS_PILL, json_px(&j, &["radius", "pill"]));
        assert_eq!(scale::CONTROL_H, json_px(&j, &["size", "control"]));
        assert_eq!(scale::TARGET, json_px(&j, &["size", "target"]));
        assert_eq!(scale::SIDEBAR_W, json_px(&j, &["size", "sidebar"]));
        assert_eq!(scale::SIDEBAR_W, 248.);
        assert_eq!(scale::RAIL_W, json_px(&j, &["size", "rail"]));
        assert_eq!(scale::TOAST_W, json_px(&j, &["size", "toast"]));
        assert_eq!(scale::RADIUS_DIALOG, json_px(&j, &["radius", "dialog"]));
        assert_eq!(scale::RADIUS_DIALOG_LG, json_px(&j, &["radius", "2xl"]));
        // The same sizes in tokens.css.
        for (name, px) in [("sidebar-w", 248), ("rail-w", 64), ("toast-w", 440)] {
            assert!(
                TOKENS_CSS.contains(&format!("--cm-{name}: {px}px")),
                "{name}"
            );
        }
        for (i, k) in ["1", "2", "3", "4", "5", "6", "8", "10"].iter().enumerate() {
            assert_eq!(scale::SPACE[i], json_px(&j, &["space", k]), "space {k}");
        }
        assert!(scale::SPACE.iter().all(|s| s % 4. == 0.));
        assert_eq!(type_scale::CAPTION, json_px(&j, &["fontSize", "caption"]));
        assert_eq!(type_scale::UI, json_px(&j, &["fontSize", "ui"]));
        assert_eq!(type_scale::BODY, json_px(&j, &["fontSize", "body"]));
        assert_eq!(type_scale::TITLE, json_px(&j, &["fontSize", "title"]));
        assert_eq!(type_scale::H2, json_px(&j, &["fontSize", "h2"]));
        assert_eq!(type_scale::H1, json_px(&j, &["fontSize", "h1"]));
        assert_eq!(type_scale::DISPLAY, json_px(&j, &["fontSize", "display"]));
        // The agent card tint is the kit's rgba(91,140,255,.07) / .35 border.
        assert_eq!(d.agent_fill, Color::rgb_a(0x5B8CFF, 7));
        assert_eq!(d.agent_border, Color::rgb_a(0x5B8CFF, 35));
    }

    /// Red is reserved; text on it is ink (design/CLAUDE.md rule 2).
    #[test]
    fn text_on_rec_is_ink_and_passes_contrast() {
        for t in [Theme::DARK, Theme::LIGHT] {
            assert_eq!(t.on_rec, INK);
            assert!(contrast(t.on_rec, t.rec) >= 5.5);
        }
        // White on rec would fail AA for normal text.
        assert!(contrast(Color::rgb(0xFFFFFF), REC) < 3.5);
        assert_eq!(Theme::DARK.primary_fg, Theme::DARK.on_rec);
    }

    /// OD-17: the primary button is neutral. Dark = the text colour with ink text, and the
    /// hover is dimmer; rec red is a separate pair.
    #[test]
    fn primary_is_neutral_and_rec_is_separate() {
        let d = Theme::DARK;
        assert_eq!(d.primary_bg, d.text);
        assert_eq!(d.primary_fg, INK);
        assert_ne!(d.primary_bg, d.rec);
        assert!(d.primary_hover.luminance() < d.primary_bg.luminance());
        assert!(contrast(d.primary_fg, d.primary_hover) >= 7.0);
        let l = Theme::LIGHT;
        assert_eq!((l.primary_bg, l.primary_fg), (INK, Color::rgb(0xFFFFFF)));
        assert_ne!(l.primary_bg, l.rec);
        // The rec pair for Record / Stop / destructive confirms.
        for t in [d, l] {
            assert_eq!(t.on_rec, INK);
            assert!(contrast(t.on_rec, t.rec_hover) >= 5.5);
            assert!(contrast(t.rec_soft, t.surface) >= 3.0, "{:?}", t.mode);
        }
    }

    /// Body text meets WCAG AA on every surface in both themes; mist is the floor for small
    /// text (>= 4.5:1 on the grounds it is used on).
    #[test]
    fn text_contrast() {
        for t in [Theme::DARK, Theme::LIGHT] {
            for bg in [
                t.ground,
                t.sidebar,
                t.surface,
                t.surface_2,
                t.raised,
                t.field,
            ] {
                assert!(contrast(t.text, bg) >= 7.0);
                assert!(contrast(t.text_2, bg) >= 7.0);
                assert!(contrast(t.text_3, bg) >= 4.5);
                assert!(contrast(t.mist, bg) >= 4.5, "{:?}", t.mode);
            }
            assert!(contrast(t.primary_fg, t.primary_bg) >= 4.5);
            // faint is for disabled and decorative marks, not for text that must be read.
            for bg in [t.ground, t.surface, t.field] {
                assert!(contrast(t.faint, bg) >= 3.0, "{:?}", t.mode);
                assert!(contrast(t.faint, bg) < contrast(t.mist, bg));
            }
        }
    }

    #[test]
    fn theme_resolution() {
        assert_eq!(ThemePreference::System.resolve(None), Mode::Dark);
        assert_eq!(ThemePreference::System.resolve(Some(false)), Mode::Dark);
        assert_eq!(ThemePreference::System.resolve(Some(true)), Mode::Light);
        assert_eq!(ThemePreference::Dark.resolve(Some(true)), Mode::Dark);
        assert_eq!(ThemePreference::Light.resolve(Some(false)), Mode::Light);
        assert_eq!(Theme::overlay().mode, Mode::Dark);
        assert_eq!(
            ThemePreference::parse("light"),
            Some(ThemePreference::Light)
        );
        assert_eq!(ThemePreference::parse("x"), None);
    }

    #[test]
    fn derived_colors_follow_the_tokens() {
        assert_eq!(derived::HUD_FILL, Color::rgb_a(0x111113, 96));
        assert_eq!(derived::SHEET_FILL, Color::rgb_a(0x111113, 98));
        assert_eq!(derived::REC_DIM, Color::rgb_a(0xFF4F3A, 45));
        assert_eq!(derived::AGENT_CHIP_BORDER, Color::rgb_a(0x5B8CFF, 45));
        assert_eq!(derived::AGENT_TILE_FILL, Color::rgb_a(0x5B8CFF, 14));
        assert_eq!(derived::WARNING_TINT, Color::rgb_a(0xF5B84A, 14));
    }

    #[test]
    fn compositing() {
        let c = Color::rgb_a(0xFFFFFF, 100).over(Color::rgb(0));
        assert_eq!(c, Color::rgb(0xFFFFFF));
        assert_eq!(Theme::DARK.agent_fill.a(), 18); // 7% of 255
    }
}
