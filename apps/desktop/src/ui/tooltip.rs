//! The shared tooltip: the control's name and its keycaps (DESIGN 4: icon buttons have a
//! tooltip). Promoted from the editor's `KeyTip` so any element can use it:
//!
//! ```ignore
//! div().id("x").tooltip(tooltip::tip("Snap", vec!["S".into()]))
//! ```

use gpui::{div, prelude::*, px, AnyView, App, Context, Render, SharedString, Window};

use crate::theme::{scale, type_scale};
use crate::ui::components::{keycaps, shrink_text};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::studio::ThemeGlobal;

/// A tooltip never grows wider than this; a longer name wraps.
pub const MAX_W: f32 = 360.;

/// Label plus optional keycaps.
pub struct Tooltip {
    label: SharedString,
    keys: Vec<String>,
}

impl Tooltip {
    pub fn new(label: impl Into<SharedString>, keys: Vec<String>) -> Self {
        Tooltip {
            label: label.into(),
            keys,
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn keys(&self) -> &[String] {
        &self.keys
    }
}

impl Render for Tooltip {
    fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .max_w(px(MAX_W))
            .px(px(8.))
            .py(px(4.))
            .rounded(px(scale::RADIUS_SM))
            .bg(rgba_of(t.raised))
            .border_1()
            .border_color(rgba_of(t.line))
            .text_color(rgba_of(t.text))
            .text_size(px(type_scale::CAPTION))
            .child(shrink_text(self.label.clone()))
            .when(!self.keys.is_empty(), |d| d.child(keycaps(&t, &self.keys)))
    }
}

/// The builder `InteractiveElement::tooltip` takes.
pub fn tip(
    label: impl Into<SharedString>,
    keys: Vec<String>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let label: SharedString = label.into();
    move |_, cx| {
        let (label, keys) = (label.clone(), keys.clone());
        cx.new(|_| Tooltip::new(label, keys)).into()
    }
}

// ---- the rich tooltip (board S14) ---------------------------------------------------------------

/// One line under a rich tooltip's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TipLine {
    /// A sentence (11.5 px, `text-3`).
    Body(String),
    /// A hint such as a chord or the last call (mono 10.5 px, `mist`).
    Hint(String),
    /// A status line: the status shape, then the text.
    Status(String, crate::agent_status::StatusMark),
}

/// A tooltip with a name and several lines: a tile's "what it does and the shortcut", the Agents
/// & CLI status. 200 to 280 px wide, so a long line wraps.
pub struct RichTooltip {
    title: SharedString,
    lines: Vec<TipLine>,
    width: f32,
}

impl Render for RichTooltip {
    fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        let lines = self.lines.iter().map(|l| match l {
            TipLine::Body(s) => div()
                .text_size(px(11.5))
                .text_color(rgba_of(t.text_3))
                .child(s.clone())
                .into_any_element(),
            TipLine::Hint(s) => div()
                .text_size(px(10.5))
                .text_color(rgba_of(t.mist))
                .mono()
                .child(s.clone())
                .into_any_element(),
            TipLine::Status(s, mark) => div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .text_color(rgba_of(t.text))
                .child(crate::ui::components::status_mark(&t, *mark, 9., None))
                .child(crate::ui::components::label_fill(s.clone()))
                .into_any_element(),
        });
        div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .w(px(self.width))
            .max_w(px(MAX_W))
            .px(px(12.))
            .py(px(8.))
            .rounded(px(scale::RADIUS_SM))
            .bg(rgba_of(t.raised))
            .border_1()
            .border_color(rgba_of(t.line))
            .text_color(rgba_of(t.text))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(self.title.clone()),
            )
            .children(lines)
    }
}

/// The builder for a [`RichTooltip`] of `width` px.
pub fn rich_tip(
    title: impl Into<SharedString>,
    lines: Vec<TipLine>,
    width: f32,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let title: SharedString = title.into();
    move |_, cx| {
        let (title, lines) = (title.clone(), lines.clone());
        cx.new(|_| RichTooltip {
            title,
            lines,
            width,
        })
        .into()
    }
}

/// The lines of a tile's tooltip (board S14): the sentence, then the chord hint when it is not
/// the reason of a disabled tile.
pub fn tile_lines(tip: &crate::nav::TileTip) -> Vec<TipLine> {
    let mut v = vec![TipLine::Body(tip.line.clone())];
    v.extend(tip.footer().map(TipLine::Hint));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_keeps_label_and_keys() {
        let t = Tooltip::new("Split", vec!["S".to_owned()]);
        assert_eq!(t.label(), "Split");
        assert_eq!(t.keys(), ["S".to_owned()]);
        assert!(Tooltip::new("Undo", Vec::new()).keys().is_empty());
    }
}
