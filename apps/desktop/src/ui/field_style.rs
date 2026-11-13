//! Text field tokens (spec `specs/capturemann-text-fields/text-field-spec.md`, boards T0 to T2): the one
//! set of colours and sizes every place you type uses, so a field, the canvas text box, a step row
//! and a transcript row all show focus, caret and selection the same way.
//!
//! Dark values are the spec's; light values follow its "Light theme" line (same blue, selection
//! 30 %, caret `#18181B`). Nothing here draws: callers read the tokens with [`FieldTokens::of`].

use crate::theme::{Color, Mode, Theme};

/// The blue of focus, selection handles on the canvas and text selection.
pub const FOCUS_BLUE: Color = Color::rgb(0x5B8CFF);
/// The 3 px ring outside a focused field's border (rgba(91,140,255,.22)).
pub const FOCUS_RING: Color = Color::rgb_a(0x5B8CFF, 22);
/// Field height, radius and horizontal padding (px).
pub const FIELD_H: f32 = 36.;
pub const FIELD_RADIUS: f32 = 9.;
pub const FIELD_PAD_X: f32 = 11.;
/// Font size of text in a field (px).
pub const FIELD_TEXT: f32 = 13.5;
/// Width of the focus ring outside the border (px).
pub const RING_W: f32 = 3.;
/// Caret width (px), blink period (ms) and how long it stays solid after a key (ms).
pub const CARET_W: f32 = 2.;
pub const CARET_BLINK_MS: u64 = 530;
pub const CARET_SOLID_MS: u64 = 500;
/// Multi-line fields: minimum and maximum height (px) and line height (em).
pub const MULTI_MIN_H: f32 = 64.;
pub const MULTI_MAX_H: f32 = 160.;
pub const MULTI_LINE_HEIGHT: f32 = 1.5;

/// The colours of one theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldTokens {
    pub bg: Color,
    pub bg_hover: Color,
    pub border: Color,
    pub border_hover: Color,
    pub text: Color,
    pub placeholder: Color,
    pub caret: Color,
    /// Selected text, window active.
    pub selection: Color,
    pub selection_text: Color,
    /// Selected text, window inactive (the focus ring is removed too).
    pub selection_inactive: Color,
    pub error: Color,
    pub error_ring: Color,
    pub error_text: Color,
    /// Read-only: dashed border, no background.
    pub readonly_border: Color,
    /// Disabled: draw at this opacity (percent).
    pub disabled_opacity: u32,
}

impl FieldTokens {
    pub fn of(t: &Theme) -> FieldTokens {
        match t.mode {
            Mode::Dark => FieldTokens {
                bg: Color::rgb(0x0D0D0F),
                bg_hover: Color::rgb(0x101013),
                border: Color::rgb(0x2E2E34),
                border_hover: Color::rgb(0x45454E),
                text: Color::rgb(0xEDEDEF),
                placeholder: Color::rgb(0x5E5E66),
                caret: Color::rgb(0xEDEDEF),
                selection: Color::rgb_a(0x5B8CFF, 42),
                selection_text: Color::rgb(0xFFFFFF),
                selection_inactive: Color::rgb_a(0xFFFFFF, 14),
                error: Color::rgb(0xF87171),
                error_ring: Color::rgb_a(0xF87171, 18),
                error_text: Color::rgb(0xFCA5A5),
                readonly_border: Color::rgb(0x2A2A30),
                disabled_opacity: 45,
            },
            Mode::Light => FieldTokens {
                bg: Color::rgb(0xFFFFFF),
                bg_hover: Color::rgb(0xFAFAFB),
                border: Color::rgb(0xD4D4DB),
                border_hover: Color::rgb(0xB4B4BE),
                text: Color::rgb(0x18181B),
                placeholder: Color::rgb(0x8A8A94),
                caret: Color::rgb(0x18181B),
                selection: Color::rgb_a(0x5B8CFF, 30),
                selection_text: Color::rgb(0x18181B),
                selection_inactive: Color::rgb_a(0x000000, 12),
                error: Color::rgb(0xDC2626),
                error_ring: Color::rgb_a(0xDC2626, 18),
                error_text: Color::rgb(0xB91C1C),
                readonly_border: Color::rgb(0xDDDDE3),
                disabled_opacity: 45,
            },
        }
    }
}

/// Text on the canvas (spec "Text on the canvas", board T1): the outline and its halo, which flips
/// with the image under it so the box shows on a light screenshot and on a dark one.
pub mod canvas {
    use super::*;

    /// The outline of a text box being edited, and the selected box.
    pub const OUTLINE: Color = FOCUS_BLUE;
    pub const OUTLINE_W: f32 = 1.5;
    /// The halo around the outline (px) and its two colours.
    pub const HALO_W: f32 = 3.;
    pub const HALO_ON_LIGHT: Color = Color::rgb(0xFFFFFF);
    pub const HALO_ON_DARK: Color = Color::rgb_a(0x0A0A0B, 75);
    /// Selected text inside a canvas text box (reads on light and dark images).
    pub const SELECTION: Color = Color::rgb_a(0x5B8CFF, 35);
    /// "Type text": the text colour at 38 % on light images, 45 % on dark ones.
    pub const PLACEHOLDER_ON_LIGHT_PCT: u32 = 38;
    pub const PLACEHOLDER_ON_DARK_PCT: u32 = 45;
    /// Selection handles of a selected (not editing) box.
    pub const HANDLE: f32 = 8.;
    /// The hover outline of a box that is not selected: thin and dashed.
    pub const HOVER_W: f32 = 1.;

    /// Whether the image under a box is light, from its luminance (0..=1).
    pub fn is_light_image(luminance: f64) -> bool {
        luminance > 0.5
    }

    pub fn halo(light_image: bool) -> Color {
        if light_image {
            HALO_ON_LIGHT
        } else {
            HALO_ON_DARK
        }
    }
}

/// Rows of lists you edit in place: the Steps list and the transcript (spec boards T1, T2).
pub mod row {
    use super::*;

    pub const STEP_H: f32 = 44.;
    pub const STEP_RADIUS: f32 = 10.;
    pub const STEP_GAP: f32 = 8.;
    pub const STEP_HOVER_BG: Color = Color::rgb(0x16161A);
    pub const STEP_SELECTED_BG: Color = Color::rgb(0x1B1B20);
    pub const STEP_SELECTED_BORDER: Color = Color::rgb(0x34343C);
    pub const LABEL_H: f32 = 30.;
    pub const LABEL_RADIUS: f32 = 7.;
    pub const LABEL_TEXT: f32 = 13.;
    /// The label field's outline while the row is hovered (not focused).
    pub const LABEL_HOVER_BG: Color = Color::rgb(0x0D0D0F);
    pub const LABEL_HOVER_BORDER: Color = Color::rgb(0x2A2A30);
    /// The badge: red marks the step itself and is never used for focus.
    pub const BADGE: Color = Color::rgb(0xFF4F3A);
    pub const BADGE_D: f32 = 22.;
    /// The ring of a selected badge: 1.5 px blue with a 2 px gap (same as the canvas selection).
    pub const BADGE_RING_W: f32 = 1.5;
    pub const BADGE_RING_GAP: f32 = 2.;
    pub const GRIP_REST_OPACITY: u32 = 55;
    pub const DELETE_SIZE: f32 = 24.;
    pub const DELETE_HOVER: Color = Color::rgb(0xF87171);
    /// The drop line while reordering: 2 px blue with a ring.
    pub const DROP_LINE_W: f32 = 2.;
    pub const DROP_RING_W: f32 = 8.;
    /// Transcript row: hover background.
    pub const TRANSCRIPT_HOVER_BG: Color = Color::rgb(0x141417);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_blue_and_the_spec_alphas() {
        assert_eq!(FOCUS_BLUE, Color(0x5B8CFFFF));
        assert_eq!(FOCUS_RING.a(), 56); // .22 of 255
        let d = FieldTokens::of(&Theme::for_mode(Mode::Dark));
        assert_eq!(d.selection.a(), 107); // .42
        assert_eq!(d.selection_inactive.a(), 36); // .14
        let l = FieldTokens::of(&Theme::for_mode(Mode::Light));
        assert_eq!(l.selection.a(), 77); // .30
        assert_eq!(l.caret, Color::rgb(0x18181B));
    }

    #[test]
    fn the_canvas_halo_follows_the_image() {
        assert_eq!(canvas::halo(true), canvas::HALO_ON_LIGHT);
        assert_eq!(canvas::halo(false), canvas::HALO_ON_DARK);
        assert!(canvas::is_light_image(0.9));
        assert!(!canvas::is_light_image(0.1));
    }
}
