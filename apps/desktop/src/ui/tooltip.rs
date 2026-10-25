//! The shared tooltip: the control's name and its keycaps (DESIGN 4: icon buttons have a
//! tooltip). Promoted from the editor's `KeyTip` so any element can use it:
//!
//! ```ignore
//! div().id("x").tooltip(tooltip::tip("Snap", vec!["S".into()]))
//! ```

use gpui::{div, prelude::*, px, AnyView, App, Context, Render, SharedString, Window};

use crate::theme::{scale, type_scale};
use crate::ui::components::{keycaps, shrink_text};
use crate::ui::paint::rgba_of;
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
