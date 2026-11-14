//! The Make a board sheet (BRD-1, design board S10), drawn inside the Studio window. State and
//! words: `crate::board_flow`; **Finish board** calls `create_board` as the user. The layout lives
//! only in this sheet until Finish, so it stays editable; after Finish the board is an ordinary
//! screenshot capture. Not checked by eye in this build (no display): the state is unit tested in
//! `board_flow`.
//!
//! The canvas is a schematic of the board at its real proportions (the plan the renderer will
//! use, one box per picture with its title); the real picture is made at Finish. Free layout has
//! no drag canvas here yet: it is available to agents through `create_board`.

use cm_i18n::{tr, trn};
use cm_render::compose_board::LayoutKind;
use gpui::{
    div, prelude::*, px, AnyElement, App, Context, Entity, FocusHandle, FontWeight, KeyDownEvent,
    Render, Role, SharedString, Subscription, Window,
};
use serde_json::{json, Value};

use crate::backend::Backend;
use crate::board_flow::{layout_label, BgChoice, BoardSheet, LAYOUTS};
use crate::daemon_text::call_error_text;
use crate::theme::{scale, Theme};
use crate::ui::app::{self, AppModel};
use crate::ui::bulk_sheet::{section, seg};
use crate::ui::components::{button_with_icon, icon, switch, ButtonKind};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::studio::ThemeGlobal;
use crate::ui::text_field::{TextField, TextFieldEvent};

const SHEET_W: f32 = 980.;
/// The canvas box the board is drawn into.
const CANVAS_W: f32 = 520.;
const CANVAS_H: f32 = 360.;

pub struct BoardSheetView {
    app: Entity<AppModel>,
    title_field: Entity<TextField>,
    tint_field: Entity<TextField>,
    seen_epoch: u64,
    focus: FocusHandle,
    focused: bool,
    _subs: Vec<Subscription>,
}

impl BoardSheetView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&app, |_, _, cx| cx.notify());
        let title_field = cx.new(|cx| {
            TextField::new(tr!("Board title"), tr!("Board title"), cx).without_icon()
        });
        let tint_field =
            cx.new(|cx| TextField::new(tr!("Tint colour"), "#DCE6F5", cx).without_icon());
        let subs = vec![
            observe,
            cx.subscribe(&title_field, |this, _, ev: &TextFieldEvent, cx| {
                this.on_field(ev, cx, |s, t| s.title = t.to_owned());
            }),
            cx.subscribe(&tint_field, |this, _, ev: &TextFieldEvent, cx| {
                this.on_field(ev, cx, |s, t| s.tint = t.to_owned());
            }),
        ];
        BoardSheetView {
            app,
            title_field,
            tint_field,
            seen_epoch: u64::MAX,
            focus: cx.focus_handle(),
            focused: false,
            _subs: subs,
        }
    }

    fn on_field(
        &mut self,
        ev: &TextFieldEvent,
        cx: &mut Context<Self>,
        set: fn(&mut BoardSheet, &str),
    ) {
        match ev {
            TextFieldEvent::Changed(text) => {
                let text = text.clone();
                self.app.update(cx, |m, cx| {
                    if let Some(s) = m.bulk.board.as_mut() {
                        set(s, &text);
                    }
                    cx.notify();
                });
            }
            TextFieldEvent::Submitted => finish(&self.app, cx),
            TextFieldEvent::Escaped => close(&self.app, cx),
        }
    }

    fn sync_fields(&mut self, cx: &mut Context<Self>) {
        let (epoch, vals) = {
            let m = self.app.read(cx);
            (
                m.bulk.epoch,
                m.bulk
                    .board
                    .as_ref()
                    .map(|s| (s.title.clone(), s.tint.clone())),
            )
        };
        if epoch == self.seen_epoch {
            return;
        }
        self.seen_epoch = epoch;
        if let Some((title, tint)) = vals {
            self.title_field.update(cx, |f, cx| f.set_text(&title, cx));
            self.tint_field.update(cx, |f, cx| f.set_text(&tint, cx));
        }
    }
}

fn update(app: &Entity<AppModel>, cx: &mut App, f: impl FnOnce(&mut BoardSheet)) {
    app.update(cx, |m, cx| {
        if let Some(s) = m.bulk.board.as_mut() {
            f(s);
        }
        cx.notify();
    });
}

fn row() -> gpui::Div {
    div().flex().flex_wrap().items_center().gap(px(8.))
}

/// The canvas: the board at its real proportions, a box per picture.
fn canvas(t: &Theme, s: &BoardSheet) -> AnyElement {
    let frame = div()
        .w(px(CANVAS_W))
        .h(px(CANVAS_H))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.surface));
    let Ok(plan) = s.plan() else {
        return frame
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(rgba_of(t.warning))
                    .child(SharedString::from(s.plan().err().unwrap_or_default())),
            )
            .into_any_element();
    };
    let k = (CANVAS_W - 40.) / plan.width as f32;
    let k = k.min((CANVAS_H - 40.) / plan.height as f32);
    let (w, h) = (plan.width as f32 * k, plan.height as f32 * k);
    let bg = match s.background {
        BgChoice::Transparent => t.field,
        BgChoice::White => crate::theme::Color::rgb(0xFFFFFF),
        BgChoice::LightGrey => crate::theme::Color::rgb(0xECEDF0),
        BgChoice::Dark => crate::theme::Color::rgb(0x18191C),
        BgChoice::Tint => parse_tint(&s.tint).unwrap_or(t.field),
    };
    let ink = if matches!(s.background, BgChoice::Dark) {
        crate::theme::Color::rgb(0xF2F2F4)
    } else {
        crate::theme::Color::rgb(0x1B1C1F)
    };
    let mut board = div()
        .relative()
        .w(px(w))
        .h(px(h))
        .flex_none()
        .overflow_hidden()
        .bg(rgba_of(bg));
    if let (Some(title), Some((x, y))) = (s.options().ok().and_then(|o| o.title), plan.title_at) {
        board = board.child(
            div()
                .absolute()
                .left(px(x as f32 * k))
                .top(px(y as f32 * k))
                .text_size(px((plan.unit * 1.6 * k).clamp(9., 22.)))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgba_of(ink))
                .child(SharedString::from(title)),
        );
    }
    for (i, (cell, src)) in plan.cells.iter().zip(&s.sources).enumerate() {
        let label = if s.layout == LayoutKind::Pair && s.before_after {
            Some(if i == 0 { tr!("BEFORE") } else { tr!("AFTER") })
        } else if s.layout == LayoutKind::Strip {
            Some((i + 1).to_string())
        } else {
            None
        };
        board = board
            .child(
                div()
                    .absolute()
                    .left(px(cell.x as f32 * k))
                    .top(px(cell.y as f32 * k))
                    .w(px(cell.w as f32 * k))
                    .h(px(cell.h as f32 * k))
                    .rounded(px((s.radius as f32 * k).min(12.)))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.raised))
                    .overflow_hidden()
                    .p(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.))
                    .text_color(rgba_of(t.text_2))
                    .child(SharedString::from(src.title.clone())),
            )
            .children(label.map(|l| {
                div()
                    .absolute()
                    .left(px(cell.x as f32 * k + 4.))
                    .top(px(cell.y as f32 * k + 4.))
                    .px(px(5.))
                    .rounded(px(4.))
                    .bg(rgba_of(crate::theme::Color::rgb(0x1B1C1F)))
                    .text_size(px(10.))
                    .text_color(rgba_of(crate::theme::Color::rgb(0xFFFFFF)))
                    .child(SharedString::from(l))
            }));
    }
    frame.child(board).into_any_element()
}

fn parse_tint(s: &str) -> Option<crate::theme::Color> {
    let h = s.trim().strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some(crate::theme::Color::rgb(n))
}

fn preset(
    t: &Theme,
    app: &Entity<AppModel>,
    id: &'static str,
    values: &[u32],
    current: u32,
    set: fn(&mut BoardSheet, u32),
) -> gpui::Div {
    row().children(values.iter().map(|&v| {
        let a = app.clone();
        seg(
            t,
            SharedString::from(format!("{id}-{v}")),
            v.to_string(),
            current == v,
        )
        .on_click(move |_, _, cx| update(&a, cx, |s| set(s, v)))
    }))
}

impl Render for BoardSheetView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        self.sync_fields(cx);
        let Some(s) = self.app.read(cx).bulk.board.clone() else {
            self.focused = false;
            return div().into_any_element();
        };
        if !self.focused {
            self.focused = true;
            window.focus(&self.focus, cx);
        }
        let app = self.app.clone();

        // Left column: layouts and the source list.
        let layouts = div().flex().flex_col().gap(px(6.)).children(LAYOUTS.map(|k| {
            let a = app.clone();
            let enabled = s.layout_enabled(k);
            let b = seg(
                &t,
                SharedString::from(format!("board-layout-{}", k.as_str())),
                layout_label(k),
                s.layout == k,
            );
            if enabled {
                b.on_click(move |_, _, cx| update(&a, cx, |s| s.set_layout(k)))
            } else {
                b.opacity(0.45)
            }
        }));
        let n = s.sources.len();
        let sources = div().flex().flex_col().gap(px(4.)).children(
            s.sources.iter().enumerate().map(|(i, src)| {
                let (a_up, a_down, a_rm) = (app.clone(), app.clone(), app.clone());
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .h(px(32.))
                    .px(px(6.))
                    .rounded(px(scale::RADIUS_SM))
                    .border_1()
                    .border_color(rgba_of(t.border))
                    .bg(rgba_of(t.field))
                    .child(
                        div()
                            .w(px(16.))
                            .text_size(px(11.))
                            .text_color(rgba_of(t.mist))
                            .mono()
                            .child((i + 1).to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_size(px(12.5))
                            .child(SharedString::from(src.title.clone())),
                    )
                    .child(small_icon(&t, SharedString::from(format!("board-up-{i}")), "arrow-up", tr!("Move up"), i > 0)
                        .on_click(move |_, _, cx| update(&a_up, cx, |s| s.move_source(i, -1))))
                    .child(small_icon(&t, SharedString::from(format!("board-down-{i}")), "arrow-down", tr!("Move down"), i + 1 < n)
                        .on_click(move |_, _, cx| update(&a_down, cx, |s| s.move_source(i, 1))))
                    .child(small_icon(&t, SharedString::from(format!("board-rm-{i}")), "x", tr!("Remove from the board"), n > 2)
                        .on_click(move |_, _, cx| update(&a_rm, cx, |s| s.remove_source(i))))
            }),
        );
        let left = div()
            .w(px(210.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(layouts)
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(trn!(
                        "{n} screenshot, drag order = list order",
                        "{n} screenshots, drag order = list order",
                        n
                    ))),
            )
            .child(sources);

        // Inspector.
        let a = app.clone();
        let arrangement = (matches!(s.layout, LayoutKind::Pair | LayoutKind::Strip)).then(|| {
            let (a1, a2) = (a.clone(), a.clone());
            section(
                &t,
                tr!("Arrangement"),
                None,
                row()
                    .child(
                        seg(&t, "board-side", tr!("Side by side"), !s.stacked)
                            .on_click(move |_, _, cx| update(&a1, cx, |s| s.stacked = false)),
                    )
                    .child(
                        seg(&t, "board-stack", tr!("Stacked"), s.stacked)
                            .on_click(move |_, _, cx| update(&a2, cx, |s| s.stacked = true)),
                    )
                    .into_any_element(),
            )
        });
        let columns = (s.layout == LayoutKind::Grid).then(|| {
            let a = a.clone();
            section(
                &t,
                tr!("Columns"),
                None,
                row()
                    .children([0u32, 2, 3, 4].map(|c| {
                        let a = a.clone();
                        seg(
                            &t,
                            SharedString::from(format!("board-cols-{c}")),
                            if c == 0 { tr!("Auto") } else { c.to_string() },
                            s.columns.unwrap_or(0) == c,
                        )
                        .on_click(move |_, _, cx| {
                            update(&a, cx, |s| s.columns = (c != 0).then_some(c))
                        })
                    }))
                    .into_any_element(),
            )
        });
        let labels = (s.layout == LayoutKind::Pair).then(|| {
            let a = a.clone();
            section(
                &t,
                tr!("Labels"),
                None,
                row()
                    .child(
                        switch(&t, "board-ba", tr!("BEFORE and AFTER labels"), s.before_after, true)
                            .on_click(move |_, _, cx| update(&a, cx, |s| s.before_after = !s.before_after)),
                    )
                    .child(div().text_size(px(13.)).child(tr!("BEFORE and AFTER labels")))
                    .into_any_element(),
            )
        });
        let gap = section(
            &t,
            tr!("Gap"),
            None,
            preset(&t, &a, "board-gap", &[0, 8, 16, 24, 40], s.gap, |s, v| s.gap = v).into_any_element(),
        );
        let margin = section(
            &t,
            tr!("Outer margin"),
            None,
            preset(&t, &a, "board-margin", &[0, 16, 32, 48, 64], s.margin, |s, v| s.margin = v)
                .into_any_element(),
        );
        let background = section(
            &t,
            tr!("Background"),
            Some(tr!("Transparent is for PNG; a JPEG export puts it on white")),
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(row().children(BgChoice::ALL.map(|b| {
                    let a = a.clone();
                    seg(
                        &t,
                        SharedString::from(format!("board-bg-{b:?}")),
                        b.label(),
                        s.background == b,
                    )
                    .on_click(move |_, _, cx| update(&a, cx, |s| s.background = b))
                })))
                .children((s.background == BgChoice::Tint).then(|| {
                    div().w(px(160.)).h(px(32.)).child(self.tint_field.clone())
                }))
                .into_any_element(),
        );
        let radius = section(
            &t,
            tr!("Corner radius"),
            None,
            preset(&t, &a, "board-radius", &[0, 8, 16, 24], s.radius, |s, v| s.radius = v)
                .into_any_element(),
        );
        let toggles = {
            let (a1, a2) = (a.clone(), a.clone());
            section(
                &t,
                tr!("Extras"),
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        row()
                            .child(
                                switch(&t, "board-shadow", tr!("Shadow"), s.shadow, true)
                                    .on_click(move |_, _, cx| update(&a1, cx, |s| s.shadow = !s.shadow)),
                            )
                            .child(div().text_size(px(13.)).child(tr!("Shadow"))),
                    )
                    .child(
                        row()
                            .child(
                                switch(&t, "board-captions", tr!("Captions"), s.captions, true)
                                    .on_click(move |_, _, cx| update(&a2, cx, |s| s.captions = !s.captions)),
                            )
                            .child(div().text_size(px(13.)).child(tr!("Captions"))),
                    )
                    .into_any_element(),
            )
        };
        let title = section(
            &t,
            tr!("Title"),
            None,
            div().h(px(32.)).child(self.title_field.clone()).into_any_element(),
        );
        let inspector = div()
            .w(px(250.))
            .flex_none()
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .children(arrangement)
            .children(columns)
            .children(labels)
            .child(gap)
            .child(margin)
            .child(background)
            .child(radius)
            .child(toggles)
            .child(title);

        let centre = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.))
            .child(canvas(&t, &s))
            .child(
                div()
                    .px(px(10.))
                    .py(px(3.))
                    .rounded(px(scale::RADIUS_PILL))
                    .border_1()
                    .border_color(rgba_of(t.border))
                    .text_size(px(12.))
                    .mono()
                    .text_color(rgba_of(t.text_2))
                    .child(SharedString::from(s.size_chip())),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(rgba_of(t.mist))
                    .child(tr!("Finish copies the screenshots into the board, with their annotations drawn in. The originals are not changed.")),
            );

        let (a_cancel, a_finish, a_close) = (app.clone(), app.clone(), app.clone());
        let can = s.can_finish();
        let panel = div()
            .id("board-sheet")
            .role(Role::Dialog)
            .aria_label(SharedString::from(s.title_text()))
            .w(px(SHEET_W))
            .max_h(px(780.))
            .flex()
            .flex_col()
            .rounded(px(18.))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.sidebar))
            .text_color(rgba_of(t.text))
            .occlude()
            .child(
                div()
                    .px(px(24.))
                    .pt(px(20.))
                    .pb(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(icon("layout-panel-left", 22., t.text))
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr!("New board")),
                    )
                    .child(
                        div()
                            .px(px(10.))
                            .py(px(3.))
                            .rounded(px(scale::RADIUS_PILL))
                            .border_1()
                            .border_color(rgba_of(t.border))
                            .text_size(px(12.))
                            .text_color(rgba_of(t.text_2))
                            .child(tr!("Layout editable until Finish")),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("board-close")
                            .role(Role::Button)
                            .aria_label(tr!("Close"))
                            .focusable()
                            .tab_stop(true)
                            .size(px(36.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgba_of(t.raised)))
                            .child(icon("x", 18., t.text_2))
                            .on_click(move |_, _, cx| close(&a_close, cx)),
                    ),
            )
            .child(
                div()
                    .id("board-body")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .px(px(24.))
                    .pb(px(16.))
                    .flex()
                    .gap(px(18.))
                    .child(left)
                    .child(centre)
                    .child(inspector),
            )
            .children(s.error.clone().map(|e| {
                div()
                    .px(px(24.))
                    .pb(px(8.))
                    .text_size(px(13.))
                    .text_color(rgba_of(t.warning))
                    .child(SharedString::from(e))
            }))
            .child(
                div()
                    .px(px(24.))
                    .py(px(16.))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(12.))
                    .border_t_1()
                    .border_color(rgba_of(t.border))
                    .child(
                        button_with_icon(&t, "board-cancel", tr!("Cancel"), ButtonKind::Secondary, None)
                            .on_click(move |_, _, cx| close(&a_cancel, cx)),
                    )
                    .child({
                        let b = button_with_icon(
                            &t,
                            "board-finish",
                            if s.busy { tr!("Making the board\u{2026}") } else { tr!("Finish board") },
                            ButtonKind::Primary,
                            None,
                        );
                        if can {
                            b.on_click(move |_, _, cx| finish(&a_finish, cx))
                        } else {
                            b.opacity(0.45)
                        }
                    }),
            );
        let key_app = app.clone();
        div()
            .id("board-scrim")
            .track_focus(&self.focus)
            .on_key_down(move |ev: &KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" {
                    close(&key_app, cx);
                }
            })
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba_of(t.modal_scrim))
            .occlude()
            .child(panel)
            .into_any_element()
    }
}

fn small_icon(
    t: &Theme,
    id: SharedString,
    glyph: &'static str,
    aria: String,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let hover = rgba_of(t.raised);
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(aria))
        .focusable()
        .tab_stop(enabled)
        .size(px(22.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .when(enabled, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
        .when(!enabled, |d| d.opacity(0.3))
        .child(icon(glyph, 13., t.text_2))
}

// ---- the calls ------------------------------------------------------------------------------

/// Opens the sheet for these screenshots `(id, title, width, height)`.
pub fn open(app: &Entity<AppModel>, shots: Vec<(String, String, u32, u32)>, cx: &mut App) {
    use crate::board_flow::BoardSource;
    app.update(cx, |m, cx| {
        m.bulk.sheet = None;
        m.bulk.board = Some(BoardSheet::new(
            shots
                .into_iter()
                .map(|(id, title, width, height)| BoardSource {
                    id,
                    title,
                    width,
                    height,
                })
                .collect(),
        ));
        m.bulk.epoch += 1;
        cx.notify();
    });
}

pub fn close(app: &Entity<AppModel>, cx: &mut App) {
    app.update(cx, |m, cx| {
        m.bulk.board = None;
        cx.notify();
    });
}

/// Finish board: `create_board` as the user, then the new board opens in the editor.
pub fn finish(app: &Entity<AppModel>, cx: &mut App) {
    let params: Value = {
        let m = app.read(cx);
        let Some(s) = m.bulk.board.as_ref().filter(|s| s.can_finish()) else {
            return;
        };
        match s.request() {
            Ok(p) => p,
            Err(e) => {
                let app = app.clone();
                cx.defer(move |cx| update(&app, cx, |s| s.error = Some(e)));
                return;
            }
        }
    };
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        update(app, cx, |s| {
            s.error = Some(tr!("Making a board needs the CaptureMann engine, which is not running."))
        });
        return;
    };
    update(app, cx, |s| {
        s.busy = true;
        s.error = None;
    });
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = backend
            .rt
            .spawn(async move { Backend::call_coded(&daemon, "create_board", params).await })
            .await;
        cx.update(|cx| match res {
            Ok(Ok(v)) => {
                let id = v["capture_id"].as_str().unwrap_or("").to_owned();
                close(&app2, cx);
                app::push_toast(
                    &app2,
                    crate::toast::ToastSpec::new(
                        crate::toast::ToastKind::Success,
                        tr!("Board created"),
                    ),
                    cx,
                );
                if !id.is_empty() {
                    app::set_route(&app2, app::open_route(&id), cx);
                }
            }
            Ok(Err(e)) => {
                let msg = call_error_text(&e);
                update(&app2, cx, |s| {
                    s.busy = false;
                    s.error = Some(msg);
                });
            }
            Err(e) => update(&app2, cx, |s| {
                s.busy = false;
                s.error = Some(e.to_string());
            }),
        });
        let _ = json!(null);
    })
    .detach();
}
