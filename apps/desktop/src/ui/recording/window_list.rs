//! The Window kind of the Record sheet (board R20): the search bar with "Pick on screen", the
//! scrolling window list ("Last used", then "Front to back", the Privacy note), and the Include
//! chips. The state and every decision are in `recording::window_rows` and `recording::sheet`;
//! this file only draws them. Not seen by eye yet (tested with fakes: the model and the layout
//! guards).

use std::sync::Arc;

use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, img, prelude::*, px, Context, Entity, FontWeight, ObjectFit, Role, ScrollHandle,
    SharedString, Stateful,
};

use crate::recording::sheet::IncludeChip;
use crate::recording::window_rows::{
    app_badge, offset_to_reveal, row_label, row_title, size_text, visible_range, Group, ListItem,
    WindowList, BADGE_PALETTE, LIST_VIEWPORT_H,
};
use crate::theme::{scale, type_scale, Color, Theme};
use crate::ui::components::{ellipsis_text, icon, keycaps, label_fill};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::recording::sheet::StartSheet;
use crate::ui::scroll::nested_scroll;
use crate::ui::text_field::TextField;
use crate::window_picker::thumbs::{ThumbState, Thumbs};

/// The search field, its shortcut hint and "Pick on screen" (the old hover mode, unchanged).
pub fn search_bar(
    t: &Theme,
    search: &Entity<TextField>,
    find_keys: &[String],
    picking: bool,
    cx: &mut Context<StartSheet>,
) -> gpui::Div {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(div().flex_1().min_w(px(0.)).child(search.clone()))
        .child(keycaps(t, find_keys))
        .child(
            div()
                .id("pick-on-screen")
                .role(Role::Button)
                .aria_label(SharedString::from(tr!("Pick on screen")))
                .focusable()
                .tab_stop(true)
                .flex_none()
                .h(px(34.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(7.))
                .rounded(px(scale::RADIUS_MD))
                .border_1()
                .border_color(rgba_of(t.line))
                .bg(rgba_of(t.surface_2))
                .text_size(px(12.5))
                .text_color(rgba_of(t.text_2))
                .when(picking, |d| d.opacity(0.5).cursor_default())
                .when(!picking, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
                .focus_visible(move |s| s.border_color(focus))
                .child(icon("mouse-pointer-2", 14., t.text_2))
                .child(ellipsis_text(tr!("Pick on screen")))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.pick_on_screen(cx);
                })),
        )
}

fn group_title(g: Group) -> String {
    match g {
        Group::LastUsed => tr_ctx!("window list", "Last used"),
        Group::FrontToBack => tr_ctx!("window list", "Front to back"),
    }
    .to_uppercase()
}

fn header_view(t: &Theme, g: Group) -> gpui::Div {
    div()
        .flex_none()
        .h(px(crate::recording::window_rows::HEADER_H))
        .px(px(14.))
        .pt(px(9.))
        .text_size(px(11.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgba_of(t.mist))
        .child(ellipsis_text(group_title(g)))
}

fn badge_color(app: &str) -> Color {
    Color::rgb(BADGE_PALETTE[app_badge(app).1])
}

/// The 64 x 40 thumbnail tile: the window's picture when there is one, else the board's wireframe
/// placeholder.
fn thumb_tile(t: &Theme, image: Option<Arc<gpui::RenderImage>>) -> gpui::Div {
    let tile = div()
        .relative()
        .flex_none()
        .w(px(64.))
        .h(px(40.))
        .rounded(px(6.))
        .overflow_hidden()
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.field));
    if let Some(image) = image {
        return tile.child(img(image).size_full().object_fit(ObjectFit::Cover));
    }
    tile.child(
        div()
            .absolute()
            .left_0()
            .right_0()
            .top_0()
            .h(px(7.))
            .bg(rgba_of(t.raised)),
    )
    .child(
        div()
            .absolute()
            .left(px(5.))
            .top(px(11.))
            .w(px(13.))
            .bottom(px(4.))
            .rounded(px(2.))
            .bg(rgba_of(t.surface_2)),
    )
    .child(
        div()
            .absolute()
            .left(px(22.))
            .top(px(11.))
            .right(px(5.))
            .bottom(px(4.))
            .rounded(px(2.))
            .bg(rgba_of(t.surface)),
    )
}

#[allow(clippy::too_many_arguments)]
fn row_view(
    t: &Theme,
    list: &WindowList,
    window: usize,
    external: bool,
    chosen: bool,
    cursor: bool,
    thumb: Option<Arc<gpui::RenderImage>>,
    cx: &mut Context<StartSheet>,
) -> Stateful<gpui::Div> {
    let w = &list.windows[window];
    let id = w.id;
    let hover = rgba_of(t.raised.with_alpha(60));
    let (letter, _) = app_badge(&w.app);
    div()
        .id(SharedString::from(format!("window-{id}")))
        .role(Role::RadioButton)
        .aria_label(SharedString::from(row_label(w, external)))
        .aria_toggled(if chosen {
            gpui::Toggled::True
        } else {
            gpui::Toggled::False
        })
        .flex_none()
        .h(px(crate::recording::window_rows::ROW_H))
        .mx(px(6.))
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(12.))
        .rounded(px(10.))
        .border_1()
        .border_color(rgba_of(if chosen {
            t.faint
        } else if cursor {
            t.mist
        } else {
            Color(0)
        }))
        .when(chosen, |d| d.bg(rgba_of(t.raised)))
        .when(!chosen, |d| d.hover(move |s| s.bg(hover)))
        .cursor_pointer()
        .child(thumb_tile(t, thumb))
        .child(
            div()
                .flex_none()
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .bg(rgba_of(badge_color(&w.app)))
                .text_color(rgba_of(t.ink))
                .text_size(px(10.5))
                .font_weight(FontWeight::BOLD)
                .child(SharedString::from(letter.to_string())),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(
                    ellipsis_text(row_title(w))
                        .text_size(px(13.5))
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(
                    ellipsis_text(w.app.clone())
                        .text_size(px(12.))
                        .text_color(rgba_of(t.mist)),
                ),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(11.5))
                .text_color(rgba_of(t.mist))
                .whitespace_nowrap()
                .mono()
                .child(SharedString::from(size_text(w, external))),
        )
        .child(radio(t, chosen))
        .on_click(cx.listener(move |this, _, _, cx| this.choose_window(id, cx)))
}

/// The 18 px radio: a red ring with a hole when chosen, else an empty ring.
fn radio(t: &Theme, on: bool) -> gpui::Div {
    div()
        .flex_none()
        .size(px(18.))
        .rounded_full()
        .border(px(if on { 5. } else { 1.5 }))
        .border_color(rgba_of(if on { t.rec } else { t.line }))
}

fn note_view(t: &Theme, text: String) -> gpui::Div {
    div()
        .flex_none()
        .min_h(px(crate::recording::window_rows::NOTE_H))
        .mx(px(6.))
        .px(px(12.))
        .py(px(10.))
        .flex()
        .items_start()
        .gap(px(8.))
        .text_size(px(type_scale::CAPTION))
        .text_color(rgba_of(t.mist))
        .child(div().pt(px(1.)).child(icon("eye-off", 13., t.mist)))
        .child(label_fill(text))
}

/// The list card: a fixed-height area that scrolls inside.
pub fn list_card(
    t: &Theme,
    list: &WindowList,
    chosen: Option<u32>,
    scroll: &ScrollHandle,
    thumbs: &Entity<Thumbs>,
    cx: &mut Context<StartSheet>,
) -> gpui::Div {
    let items = list.items();
    let empty = list.empty_text();
    let heights: Vec<f32> = items.iter().map(ListItem::height).collect();
    let offset = (-f32::from(scroll.offset().y)).max(0.0);
    // Only the rows on screen (plus one more each side) may ask for a thumbnail.
    let on_screen = visible_range(&heights, offset, LIST_VIEWPORT_H, 1);
    // Ask for exactly the thumbnails of those rows (the service keeps at most three in flight and
    // forgets what is no longer wanted); the rest of the list shows the placeholder tile.
    let wanted = list.visible_window_ids(on_screen.clone());
    thumbs.update(cx, |th, cx| th.request(&wanted, cx));
    let area = div().relative().h(px(LIST_VIEWPORT_H)).overflow_hidden();
    let body: gpui::AnyElement = if items.is_empty() {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .px(px(24.))
            .text_size(px(type_scale::CAPTION))
            .text_color(rgba_of(t.mist))
            .text_center()
            .child(SharedString::from(empty.unwrap_or_default()))
            .into_any_element()
    } else {
        let mut rows: Vec<gpui::AnyElement> = Vec::with_capacity(items.len() + 1);
        if let Some(e) = empty {
            rows.push(
                div()
                    .flex_none()
                    .px(px(20.))
                    .py(px(14.))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(e))
                    .into_any_element(),
            );
        }
        for (i, item) in items.into_iter().enumerate() {
            rows.push(match item {
                ListItem::Header(g) => header_view(t, g).into_any_element(),
                ListItem::Row { window, external } => {
                    let id = list.windows[window].id;
                    let thumb = if on_screen.contains(&i) {
                        match thumbs.read(cx).state(id) {
                            ThumbState::Ready(image) => Some(image),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    row_view(
                        t,
                        list,
                        window,
                        external,
                        chosen == Some(id),
                        list.cursor == Some(id),
                        thumb,
                        cx,
                    )
                    .into_any_element()
                }
                ListItem::Note(text) => note_view(t, text).into_any_element(),
            });
        }
        nested_scroll(
            t,
            scroll,
            LIST_VIEWPORT_H,
            div()
                .id("window-list")
                .role(Role::RadioGroup)
                .aria_label(SharedString::from(tr!("Windows")))
                .flex()
                .flex_col()
                .pb(px(6.))
                .children(rows),
        )
    };
    div()
        .flex_none()
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.field))
        .overflow_hidden()
        .child(area.child(body))
}

/// Scrolls the list so the cursor row is visible (after an arrow key).
pub fn reveal_cursor(list: &WindowList, scroll: &ScrollHandle) {
    let Some(index) = list.cursor_item_index() else {
        return;
    };
    let heights: Vec<f32> = list.items().iter().map(ListItem::height).collect();
    let offset = (-f32::from(scroll.offset().y)).max(0.0);
    let want = offset_to_reveal(&heights, index, offset, LIST_VIEWPORT_H);
    if (want - offset).abs() > 0.5 {
        let mut o = scroll.offset();
        o.y = px(-want);
        scroll.set_offset(o);
    }
}

/// "Include" and the three chips (R20). A chip is a switch: check when on, x when off.
pub fn include_row(t: &Theme, chips: [IncludeChip; 3], cx: &mut Context<StartSheet>) -> gpui::Div {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .flex_none()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .mr(px(4.))
                .text_size(px(12.5))
                .text_color(rgba_of(t.mist))
                .child(tr_ctx!("record sheet", "Include")),
        )
        .children(chips.into_iter().map(|c| {
            let source = c.source;
            let fg = if c.on { t.text } else { t.mist };
            div()
                .id(SharedString::from(format!("include-{source:?}")))
                .role(Role::Switch)
                .aria_label(SharedString::from(c.label.clone()))
                .aria_toggled(if c.on {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .focusable()
                .tab_stop(true)
                .h(px(34.))
                .max_w_full()
                .pl(px(10.))
                .pr(px(12.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(scale::RADIUS_PILL))
                .border_1()
                .border_color(rgba_of(if c.on { t.line } else { t.border }))
                .when(c.on, |d| d.bg(rgba_of(t.icon_tile())))
                .text_size(px(13.))
                .text_color(rgba_of(fg))
                .when(c.enabled, |d| d.cursor_pointer())
                .when(c.enabled && !c.on, |d| d.hover(move |s| s.bg(hover)))
                .when(!c.enabled, |d| d.cursor_default().opacity(0.5))
                .focus_visible(move |s| s.border_color(focus))
                .child(icon(c.icon, 15., fg))
                .child(ellipsis_text(c.label))
                .child(icon(
                    if c.on { "check" } else { "x" },
                    13.,
                    if c.on { t.success } else { t.faint },
                ))
                .when(c.enabled, |d| {
                    d.on_click(cx.listener(move |this, _, _, cx| this.toggle_chip(source, cx)))
                })
        }))
}
