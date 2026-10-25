//! The window cards of the picker: the grid (thumbnail over a two-line caption) and the list
//! (a row per window). Both show the app's tile, the full title (cut with an ellipsis) and the
//! pixel size in Geist Mono with the display.

use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, img, prelude::*, px, Context, FontWeight, ObjectFit, Role, SharedString, Stateful,
};

use super::{display_name, PickerView};
use crate::theme::{derived, Color, Theme};
use crate::ui::components::icon;
use crate::ui::paint::{rgba_of, TextExt};
use crate::window_picker::session::app_tile;
use crate::window_picker::thumbs::ThumbState;
use crate::window_picker::{format_size, Layout};

/// The thumbnail well of a grid card.
pub const THUMB_H: f32 = 132.;
/// Padding 8 + well + gap 8 + caption 34 + padding 8 + the 1 px borders.
pub const CARD_H: f32 = 8. + THUMB_H + 8. + 34. + 8. + 2.;
pub const GRID_GAP: f32 = 14.;
pub const LIST_ROW_H: f32 = 64.;
pub const LIST_GAP: f32 = 8.;

/// What one card shows, worked out before painting so the text is built once per paint.
struct Card {
    pos: usize,
    id: u32,
    title: String,
    app: String,
    sub: String,
    aria: String,
    selected: bool,
    thumb: ThumbState,
}

/// "1680 × 1050 · Built-in", or just the size when the display is not known.
pub fn sub_text(width: u32, height: u32, display: Option<&str>) -> String {
    let size = format_size(width, height);
    match display {
        Some(d) => tr_ctx!(
            "window picker card",
            "{size} \u{00B7} {display}",
            size = size,
            display = d
        ),
        None => size,
    }
}

fn aria_text(title: &str, app: &str, width: u32, height: u32, display: Option<&str>) -> String {
    match display {
        Some(d) => tr!(
            "{title}, {app}, {width} by {height} pixels, on {display}",
            title = title,
            app = app,
            width = width,
            height = height,
            display = d
        ),
        None => tr!(
            "{title}, {app}, {width} by {height} pixels",
            title = title,
            app = app,
            width = width,
            height = height
        ),
    }
}

/// The neutral tile shown while a thumbnail loads and when it failed.
fn well(t: &Theme, thumb: &ThumbState) -> gpui::Div {
    let base = div()
        .relative()
        .flex_none()
        .overflow_hidden()
        .rounded(px(9.))
        .border_1()
        .border_color(rgba_of(derived::PICKER_THUMB_BORDER))
        .bg(rgba_of(derived::PICKER_THUMB));
    match thumb {
        ThumbState::Ready(image) => base.child(
            img(image.clone())
                .size_full()
                .object_fit(ObjectFit::Contain),
        ),
        ThumbState::Loading | ThumbState::Failed => base
            .flex()
            .items_center()
            .justify_center()
            .child(icon("app-window", 22., t.faint)),
    }
}

fn app_chip(t: &Theme, app: &str, size: f32) -> gpui::Div {
    let (letter, fill) = app_tile(app);
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .bg(rgba_of(Color::rgb(fill)))
        .text_color(rgba_of(t.ink))
        .text_size(px(10.5))
        .font_weight(FontWeight::BOLD)
        .child(SharedString::from(letter.to_string()))
}

fn caption(t: &Theme, c: &Card) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(2.))
        .pb(px(2.))
        .child(app_chip(t, &c.app, 20.))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(
                    div()
                        .min_w(px(0.))
                        .truncate()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(c.title.clone())),
                )
                .child(
                    div()
                        .min_w(px(0.))
                        .truncate()
                        .mono()
                        .text_size(px(11.))
                        .text_color(rgba_of(t.mist))
                        .child(SharedString::from(c.sub.clone())),
                ),
        )
}

fn frame(t: &Theme, c: &Card) -> Stateful<gpui::Div> {
    let (border, bg) = if c.selected {
        (t.text, derived::PICKER_CARD_SELECTED)
    } else {
        (derived::PICKER_CARD_BORDER, derived::PICKER_CARD)
    };
    div()
        .id(SharedString::from(format!("wp-card-{}", c.id)))
        .role(Role::ListBoxOption)
        .aria_label(SharedString::from(c.aria.clone()))
        .aria_selected(c.selected)
        .relative()
        .cursor_pointer()
        .border_1()
        .border_color(rgba_of(border))
        .bg(rgba_of(bg))
}

impl PickerView {
    fn card_events(
        &self,
        el: Stateful<gpui::Div>,
        pos: usize,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        el.on_mouse_move(
            cx.listener(move |this, ev: &gpui::MouseMoveEvent, window, cx| {
                // Only a pointer that moved selects: a list scrolling under a still pointer (arrow
                // keys) must not take the selection back.
                if this.last_pointer != Some(ev.position) {
                    this.last_pointer = Some(ev.position);
                    this.select(pos, window, cx);
                }
            }),
        )
        .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, window, cx| {
            this.select(pos, window, cx);
            if ev.click_count() >= 2 {
                let o = this.state.enter();
                this.apply(o, window, cx);
            }
        }))
    }

    pub(super) fn list(&mut self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let list_layout = self.state.is_list_layout();
        let selected = self.state.selected_pos();
        let cards: Vec<Card> = self
            .state
            .visible_windows()
            .enumerate()
            .map(|(pos, w)| {
                let display = self.state.display_of(w).map(display_name);
                Card {
                    pos,
                    id: w.id,
                    title: w.title.clone(),
                    app: w.app.clone(),
                    sub: sub_text(w.width, w.height, display.as_deref()),
                    aria: aria_text(&w.title, &w.app, w.width, w.height, display.as_deref()),
                    selected: selected == Some(pos),
                    thumb: self.thumbs.read(cx).state(w.id),
                }
            })
            .collect();
        let cols = match self.state.layout() {
            Layout::Grid { columns } => columns,
            Layout::List => 1,
        };
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        let mut selected_row = 0;
        if list_layout {
            for c in cards {
                if c.selected {
                    selected_row = rows.len();
                }
                let pos = c.pos;
                let el = frame(t, &c)
                    .h(px(LIST_ROW_H))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(12.))
                    .p(px(7.))
                    .rounded(px(12.))
                    .child(well(t, &c.thumb).w(px(76.)).h(px(48.)).rounded(px(7.)))
                    .child(div().flex_1().min_w(px(0.)).child(caption(t, &c)));
                rows.push(self.card_events(el, pos, cx).into_any_element());
            }
        } else {
            let mut it = cards.into_iter().peekable();
            while it.peek().is_some() {
                let mut row = div().flex().flex_none().gap(px(GRID_GAP));
                let mut placed = 0;
                for c in it.by_ref().take(cols) {
                    placed += 1;
                    if c.selected {
                        selected_row = rows.len();
                    }
                    let pos = c.pos;
                    let el = frame(t, &c)
                        .flex_1()
                        .min_w(px(0.))
                        .h(px(CARD_H))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .p(px(8.))
                        .rounded(px(14.))
                        .child(well(t, &c.thumb).w_full().h(px(THUMB_H)))
                        .child(caption(t, &c));
                    row = row.child(self.card_events(el, pos, cx));
                }
                // The last row keeps the column width of the rows above it.
                for _ in placed..cols {
                    row = row.child(div().flex_1().min_w(px(0.)));
                }
                rows.push(row.into_any_element());
            }
        }
        if self.scroll_to_selected {
            self.scroll_to_selected = false;
            self.scroll.scroll_to_item(selected_row);
        }
        div()
            .id("window-picker-list")
            .role(Role::ListBox)
            .aria_label(tr!("Windows"))
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap(px(if list_layout { LIST_GAP } else { GRID_GAP }))
            .pb(px(8.))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(rows)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_line_is_the_board_text() {
        cm_i18n::with_language(cm_i18n::Lang::En, || {
            assert_eq!(
                sub_text(1680, 1050, Some("Built-in")),
                "1680 \u{00D7} 1050 \u{00B7} Built-in"
            );
            assert_eq!(sub_text(980, 1240, None), "980 \u{00D7} 1240");
        });
    }

    #[test]
    fn the_accessible_name_has_every_field() {
        cm_i18n::with_language(cm_i18n::Lang::En, || {
            let s = aria_text("Docs", "Safari", 1680, 1050, Some("LG"));
            assert_eq!(s, "Docs, Safari, 1680 by 1050 pixels, on LG");
            assert_eq!(
                aria_text("Docs", "Safari", 1, 2, None),
                "Docs, Safari, 1 by 2 pixels"
            );
        });
    }

    #[test]
    fn the_card_height_matches_its_parts() {
        assert_eq!(CARD_H, 192.);
    }
}
