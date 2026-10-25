//! The app rail of the window picker: "All windows" and one row per app with its window count,
//! in the order of each app's front-most window. Cmd+1..9 jumps to the n-th app (the rail does not
//! number its rows; the footer names the keys).

use cm_i18n::trn;
use gpui::{div, prelude::*, px, Context, FontWeight, Role, SharedString, Stateful, Toggled};

use super::{all_windows, PickerView};
use crate::theme::{derived, Color, Theme};
use crate::ui::paint::{rgba_of, TextExt};
use crate::window_picker::session::app_tile;
use crate::window_picker::AppFilter;

/// The fill of the "All windows" tile (kit `#3A3A42`).
const ALL_TILE: Color = Color::rgb(0x3A3A42);

fn row(
    t: &Theme,
    id: SharedString,
    label: String,
    count: usize,
    tile: (char, Color),
    active: bool,
    aria: String,
) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(aria))
        .aria_toggled(if active {
            Toggled::True
        } else {
            Toggled::False
        })
        .focusable()
        .tab_stop(true)
        .h(px(36.))
        .px(px(10.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .rounded(px(9.))
        .text_size(px(13.5))
        .text_color(rgba_of(if active { t.text } else { t.text_2 }))
        .cursor_pointer()
        .when(active, |d| d.bg(rgba_of(derived::LAUNCHER_SELECTED)))
        .hover(|s| s.bg(rgba_of(derived::LAUNCHER_SELECTED)))
        .child(
            div()
                .size(px(22.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .bg(rgba_of(tile.1))
                .text_color(rgba_of(t.ink))
                .text_size(px(11.))
                .font_weight(FontWeight::BOLD)
                .child(SharedString::from(tile.0.to_string())),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .truncate()
                .child(SharedString::from(label)),
        )
        .child(
            div()
                .flex_none()
                .mono()
                .text_size(px(11.5))
                .text_color(rgba_of(t.faint))
                .child(SharedString::from(count.to_string())),
        )
}

impl PickerView {
    pub(super) fn rail(&mut self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let groups = self.state.groups();
        let current = self.state.app_filter().clone();
        let mut rows: Vec<gpui::AnyElement> = Vec::with_capacity(groups.groups.len() + 1);
        rows.push(
            row(
                t,
                "wp-app-all".into(),
                all_windows(),
                groups.total,
                ('\u{2217}', ALL_TILE),
                current == AppFilter::All,
                trn!(
                    "All apps, {n} window",
                    "All apps, {n} windows",
                    groups.total
                ),
            )
            .on_click(
                cx.listener(|this, _, window, cx| this.set_app_filter(AppFilter::All, window, cx)),
            )
            .into_any_element(),
        );
        for (i, g) in groups.groups.iter().enumerate() {
            let filter = AppFilter::App(g.app.clone());
            let (letter, fill) = app_tile(&g.app);
            let active = current == filter;
            rows.push(
                row(
                    t,
                    SharedString::from(format!("wp-app-{i}")),
                    g.app.clone(),
                    g.count,
                    (letter, Color::rgb(fill)),
                    active,
                    trn!(
                        "{app}, {n} window",
                        "{app}, {n} windows",
                        g.count,
                        app = g.app
                    ),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.set_app_filter(filter.clone(), window, cx)
                }))
                .into_any_element(),
            );
        }
        div()
            .id("window-picker-apps")
            .role(Role::Navigation)
            .aria_label(cm_i18n::tr!("Apps"))
            .w(px(super::RAIL_W))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(10.))
            .py(px(12.))
            .border_r_1()
            .border_color(rgba_of(t.border))
            .overflow_y_scroll()
            .children(rows)
            .into_any_element()
    }
}
