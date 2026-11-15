//! The Export selected sheet (BLK-1, design board S12), drawn inside the Studio window. State and
//! words: `crate::bulk_flow`; this file draws it and makes the calls to the daemon
//! (`export_images`, `get_job`, `cancel_job`). Not checked by eye in this build (no display): the
//! state and the request are unit tested in `bulk_flow`.
//!
//! Where the design pack has a progress toast and a summary toast, the sheet itself switches to a
//! progress panel and a summary panel (Cancel, Show in folder, Retry skipped): one place to look.

use std::time::Duration;

use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, prelude::*, px, AnyElement, App, Context, ElementId, Entity, FocusHandle, FontWeight,
    KeyDownEvent, Render, Role, SharedString, Stateful, Subscription, Window,
};
use serde_json::json;

use crate::backend::Backend;
use crate::bulk_flow::{
    reason_text, BulkSheet, Format, MarkKind, Run, RunState, SizeMode, POSITIONS, TOKENS,
};
use crate::daemon_text::call_error_text;
use crate::theme::{scale, Theme};
use crate::ui::ai_view::progress_bar;
use crate::ui::app::{self, AppModel};
use crate::ui::components::{button_with_icon, icon, switch, ButtonKind};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::studio::ThemeGlobal;
use crate::ui::text_field::{TextField, TextFieldEvent};

const SHEET_W: f32 = 740.;

pub struct BulkSheetView {
    app: Entity<AppModel>,
    size_field: Entity<TextField>,
    mark_field: Entity<TextField>,
    pattern_field: Entity<TextField>,
    seen_epoch: u64,
    focus: FocusHandle,
    focused: bool,
    _subs: Vec<Subscription>,
}

impl BulkSheetView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&app, |_, _, cx| cx.notify());
        let size_field =
            cx.new(|cx| TextField::new(tr!("Size"), tr_ctx!("field", "1600"), cx).without_icon());
        let mark_field = cx.new(|cx| {
            TextField::new(
                tr!("Watermark text"),
                tr_ctx!("field", "Watermark text"),
                cx,
            )
            .without_icon()
        });
        let pattern_field =
            cx.new(|cx| TextField::new(tr!("File name pattern"), "{name}", cx).without_icon());
        let mut subs = vec![observe];
        subs.push(
            cx.subscribe(&size_field, |this, _, ev: &TextFieldEvent, cx| {
                this.on_field(ev, cx, |s, t| s.size_value = t.to_owned());
            }),
        );
        subs.push(
            cx.subscribe(&mark_field, |this, _, ev: &TextFieldEvent, cx| {
                this.on_field(ev, cx, |s, t| s.mark_text = t.to_owned());
            }),
        );
        subs.push(
            cx.subscribe(&pattern_field, |this, _, ev: &TextFieldEvent, cx| {
                this.on_field(ev, cx, |s, t| s.pattern = t.to_owned());
            }),
        );
        BulkSheetView {
            app,
            size_field,
            mark_field,
            pattern_field,
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
        set: fn(&mut BulkSheet, &str),
    ) {
        match ev {
            TextFieldEvent::Changed(text) => {
                let text = text.clone();
                self.app.update(cx, |m, cx| {
                    if let Some(s) = m.bulk.sheet.as_mut() {
                        set(s, &text);
                    }
                    cx.notify();
                });
            }
            TextFieldEvent::Submitted => start(&self.app, cx),
            TextFieldEvent::Escaped => close(&self.app, cx),
        }
    }

    /// Refills the fields when a sheet opens.
    fn sync_fields(&mut self, cx: &mut Context<Self>) {
        let (epoch, vals) = {
            let m = self.app.read(cx);
            (
                m.bulk.epoch,
                m.bulk
                    .sheet
                    .as_ref()
                    .map(|s| (s.size_value.clone(), s.mark_text.clone(), s.pattern.clone())),
            )
        };
        if epoch == self.seen_epoch {
            return;
        }
        self.seen_epoch = epoch;
        if let Some((size, mark, pattern)) = vals {
            self.size_field.update(cx, |f, cx| f.set_text(&size, cx));
            self.mark_field.update(cx, |f, cx| f.set_text(&mark, cx));
            self.pattern_field
                .update(cx, |f, cx| f.set_text(&pattern, cx));
        }
    }
}

/// A segment of a segmented control.
pub(crate) fn seg(
    t: &Theme,
    id: impl Into<ElementId>,
    label: String,
    active: bool,
) -> Stateful<gpui::Div> {
    let focus = rgba_of(t.text);
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(label.clone()))
        .aria_selected(active)
        .focusable()
        .tab_stop(true)
        .h(px(32.))
        .px(px(12.))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(scale::RADIUS_SM))
        .border_1()
        .border_color(rgba_of(if active { t.line } else { t.border }))
        .bg(rgba_of(if active { t.raised } else { t.field }))
        .text_size(px(13.))
        .text_color(rgba_of(if active { t.text } else { t.text_2 }))
        .when(active, |d| d.font_weight(FontWeight::MEDIUM))
        .cursor_pointer()
        .focus_visible(move |s| s.border_color(focus))
        .child(SharedString::from(label))
}

/// One section: a 130 px label column with a hint under it, then the controls.
pub(crate) fn section(
    t: &Theme,
    label: String,
    hint: Option<String>,
    body: AnyElement,
) -> AnyElement {
    div()
        .px(px(20.))
        .py(px(14.))
        .flex()
        .gap(px(18.))
        .border_b_1()
        .border_color(rgba_of(t.border))
        .child(
            div()
                .w(px(130.))
                .flex_none()
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(label)),
                )
                .children(hint.map(|h| {
                    div()
                        .mt(px(2.))
                        .text_size(px(12.))
                        .text_color(rgba_of(t.mist))
                        .child(SharedString::from(h))
                })),
        )
        .child(div().flex_1().min_w(px(0.)).child(body))
        .into_any_element()
}

fn row() -> gpui::Div {
    div().flex().flex_wrap().items_center().gap(px(8.))
}

fn update(app: &Entity<AppModel>, cx: &mut App, f: impl FnOnce(&mut BulkSheet)) {
    app.update(cx, |m, cx| {
        if let Some(s) = m.bulk.sheet.as_mut() {
            f(s);
        }
        cx.notify();
    });
}

/// The sheet's tallest size: 860 pt, and never more than the window leaves (24 pt each side), so
/// the body scrolls and the footer stays in view (QA L2).
fn sheet_max_h(viewport_h: f32) -> f32 {
    (viewport_h - 48.0).clamp(240.0, 860.0)
}

impl Render for BulkSheetView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        self.sync_fields(cx);
        let Some(sheet) = self.app.read(cx).bulk.sheet.clone() else {
            self.focused = false;
            return div().into_any_element();
        };
        if !self.focused {
            self.focused = true;
            window.focus(&self.focus, cx);
        }
        let app = self.app.clone();
        let body = match &sheet.run {
            Some(run) => run_panel(&t, &sheet, run, &app),
            None => form(&t, &sheet, self, &app),
        };
        let footer = match &sheet.run {
            Some(run) => run_footer(&t, run, &app),
            None => form_footer(&t, &sheet, &app),
        };
        let a_close = app.clone();
        let panel = div()
            .id("bulk-sheet")
            .role(Role::Dialog)
            .aria_label(SharedString::from(sheet.title()))
            .w(px(SHEET_W))
            .max_h(px(sheet_max_h(f32::from(window.viewport_size().height))))
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
                    .flex_none()
                    .px(px(24.))
                    .pt(px(22.))
                    .pb(px(14.))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .child(
                        div()
                            .size(px(44.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(12.))
                            .bg(rgba_of(t.raised))
                            .child(icon("folder-output", 22., t.text)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .child(
                                div()
                                    .text_size(px(22.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(SharedString::from(sheet.title())),
                            )
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .text_color(rgba_of(t.text_3))
                                    .child(tr!(
                                        "New files are written; the originals are never changed. Annotations and redactions are drawn into every file."
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .id("bulk-close")
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
                    .id("bulk-body")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .px(px(24.))
                    .pb(px(18.))
                    .child(body)
                    .children(sheet.error.clone().map(|e| {
                        div()
                            .mt(px(14.))
                            .text_size(px(13.))
                            .text_color(rgba_of(t.warning))
                            .child(SharedString::from(e))
                    })),
            )
            .child(div().flex_none().child(footer));
        let key_app = app.clone();
        div()
            .id("bulk-scrim")
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

fn form(t: &Theme, s: &BulkSheet, v: &BulkSheetView, app: &Entity<AppModel>) -> AnyElement {
    let frame = |rows: Vec<AnyElement>| {
        div()
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .children(rows)
    };

    // Size.
    let a = app.clone();
    let size_on = s.size_on;
    let size = section(
        t,
        tr!("Size"),
        Some(tr!("Never larger than the original unless you allow it")),
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                row()
                    .child({
                        let a = a.clone();
                        seg(t, "bulk-size-off", tr!("Keep size"), !size_on)
                            .on_click(move |_, _, cx| update(&a, cx, |s| s.size_on = false))
                    })
                    .children(SizeMode::ALL.iter().map(|&m| {
                        let a = a.clone();
                        seg(
                            t,
                            SharedString::from(format!("bulk-size-{m:?}")),
                            m.label(),
                            size_on && s.size_mode == m,
                        )
                        .on_click(move |_, _, cx| {
                            update(&a, cx, |s| {
                                s.size_on = true;
                                s.size_mode = m;
                            })
                        })
                    })),
            )
            .children(size_on.then(|| {
                row()
                    .child(div().w(px(120.)).h(px(32.)).child(v.size_field.clone()))
                    .child(div().text_size(px(13.)).text_color(rgba_of(t.mist)).child(
                        if s.size_mode == SizeMode::Percent {
                            "%"
                        } else {
                            "px"
                        },
                    ))
                    .child({
                        let a = a.clone();
                        let on = s.allow_larger;
                        row()
                            .child(
                                switch(
                                    t,
                                    "bulk-larger",
                                    tr!("Allow larger than the original"),
                                    on,
                                    true,
                                )
                                .on_click(move |_, _, cx| {
                                    update(&a, cx, |s| s.allow_larger = !s.allow_larger)
                                }),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .child(tr!("Allow larger than the original")),
                            )
                    })
            }))
            .into_any_element(),
    );

    // Format.
    let a = app.clone();
    let format = section(
        t,
        tr!("Format"),
        None,
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(row().children(Format::ALL.iter().map(|&f| {
                let a = a.clone();
                seg(
                    t,
                    SharedString::from(format!("bulk-format-{f:?}")),
                    f.label(),
                    s.format == f,
                )
                .on_click(move |_, _, cx| update(&a, cx, |s| s.format = f))
            })))
            .children((s.format == Format::Jpeg).then(|| {
                row()
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgba_of(t.mist))
                            .child(tr!("Quality")),
                    )
                    .children([60u8, 70, 80, 90, 100].map(|q| {
                        let a = a.clone();
                        seg(
                            t,
                            SharedString::from(format!("bulk-quality-{q}")),
                            q.to_string(),
                            s.quality == q,
                        )
                        .on_click(move |_, _, cx| update(&a, cx, |s| s.quality = q))
                    }))
            }))
            .into_any_element(),
    );

    // Watermark.
    let a = app.clone();
    let mark = section(
        t,
        tr!("Watermark"),
        Some(tr!("Drawn on each file, not stored")),
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                row().children(
                    [
                        (MarkKind::None, tr!("None")),
                        (MarkKind::Text, tr!("Text")),
                        (MarkKind::Image, tr!("Image")),
                    ]
                    .map(|(k, label)| {
                        let a = a.clone();
                        seg(
                            t,
                            SharedString::from(format!("bulk-mark-{k:?}")),
                            label,
                            s.mark == k,
                        )
                        .on_click(move |_, _, cx| update(&a, cx, |s| s.mark = k))
                    }),
                ),
            )
            .children(
                (s.mark == MarkKind::Text).then(|| div().h(px(32.)).child(v.mark_field.clone())),
            )
            .children((s.mark == MarkKind::Image).then(|| {
                let a = a.clone();
                row()
                    .child(
                        button_with_icon(
                            t,
                            "bulk-mark-pick",
                            tr!("Choose picture"),
                            ButtonKind::Secondary,
                            Some("image"),
                        )
                        .on_click(move |_, _, cx| pick_mark_image(&a, cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_size(px(12.5))
                            .text_color(rgba_of(t.mist))
                            .mono()
                            .child(SharedString::from(
                                s.mark_image
                                    .clone()
                                    .unwrap_or_else(|| tr!("No picture yet")),
                            )),
                    )
            }))
            .children((s.mark != MarkKind::None).then(|| {
                let (a1, a2) = (a.clone(), a.clone());
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(row().child(small_label(t, tr!("Opacity"))).children(
                        [20u8, 35, 50, 75, 100].map(|o| {
                            let a = a1.clone();
                            seg(
                                t,
                                SharedString::from(format!("bulk-opacity-{o}")),
                                format!("{o}%"),
                                s.mark_opacity == o,
                            )
                            .on_click(move |_, _, cx| update(&a, cx, |s| s.mark_opacity = o))
                        }),
                    ))
                    .child(row().child(small_label(t, tr!("Size"))).children(
                        [10u8, 15, 25, 40, 60].map(|z| {
                            let a = a2.clone();
                            seg(
                                t,
                                SharedString::from(format!("bulk-marksize-{z}")),
                                format!("{z}%"),
                                s.mark_size == z,
                            )
                            .on_click(move |_, _, cx| update(&a, cx, |s| s.mark_size = z))
                        }),
                    ))
                    .child(position_grid(t, s.mark_pos, &a))
            }))
            .into_any_element(),
    );

    // Blueprint (ANN-11): every text line of each picture becomes a bar in the file; the
    // screenshots themselves are not changed.
    let a = app.clone();
    let blueprint = section(
        t,
        tr!("Blueprint"),
        Some(tr!(
            "Replaces every line of text with a bar in the files; the screenshots stay as they are"
        )),
        row()
            .child(
                switch(t, "bulk-blueprint", tr!("Blueprint"), s.blueprint, true)
                    .on_click(move |_, _, cx| update(&a, cx, |s| s.blueprint = !s.blueprint)),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .child(tr!("Replace text with bars")),
            )
            .into_any_element(),
    );

    // File names.
    let a = app.clone();
    let names = section(
        t,
        tr!("File names"),
        Some(tr!(
            "A name that exists gets a number; nothing is overwritten"
        )),
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().h(px(32.)).child(v.pattern_field.clone()))
            .child(row().children(TOKENS.iter().map(|tok| {
                let a = a.clone();
                let token = (*tok).to_owned();
                let shown = (*tok).to_owned();
                seg(
                    t,
                    SharedString::from(format!("bulk-token-{tok}")),
                    shown,
                    false,
                )
                .mono()
                .on_click(move |_, _, cx| {
                    let token = token.clone();
                    a.update(cx, |m, cx| {
                        if let Some(s) = m.bulk.sheet.as_mut() {
                            s.insert_token(&token);
                        }
                        // The field shows the new pattern.
                        m.bulk.epoch += 1;
                        cx.notify();
                    });
                })
            })))
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(rgba_of(t.mist))
                    .mono()
                    .child(SharedString::from(tr!(
                        "Example: {name}",
                        name = s.example()
                    ))),
            )
            .into_any_element(),
    );

    // Save to.
    let a = app.clone();
    let dest = section(
        t,
        tr!("Save to"),
        None,
        row()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .text_size(px(13.))
                    .mono()
                    .text_color(rgba_of(if s.dest.is_some() { t.text } else { t.mist }))
                    .child(SharedString::from(
                        s.dest.clone().unwrap_or_else(|| tr!("No folder chosen")),
                    )),
            )
            .child(
                button_with_icon(
                    t,
                    "bulk-choose",
                    tr!("Choose\u{2026}"),
                    ButtonKind::Secondary,
                    Some("folder-open"),
                )
                .on_click(move |_, _, cx| pick_folder(&a, cx)),
            )
            .into_any_element(),
    );
    frame(vec![size, format, mark, blueprint, names, dest]).into_any_element()
}

fn small_label(t: &Theme, text: String) -> gpui::Div {
    div()
        .w(px(64.))
        .text_size(px(13.))
        .text_color(rgba_of(t.mist))
        .child(SharedString::from(text))
}

/// The 3 x 3 grid of the nine positions.
fn position_grid(t: &Theme, current: usize, app: &Entity<AppModel>) -> AnyElement {
    let t = *t;
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .children((0..3).map(|r| {
            div().flex().gap(px(4.)).children((0..3).map(|c| {
                let ix = r * 3 + c;
                let a = app.clone();
                let active = ix == current;
                div()
                    .id(SharedString::from(format!("bulk-pos-{}", POSITIONS[ix])))
                    .role(Role::Button)
                    .aria_label(SharedString::from(POSITIONS[ix].replace('_', " ")))
                    .aria_selected(active)
                    .focusable()
                    .tab_stop(true)
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .border_1()
                    .border_color(rgba_of(if active { t.text } else { t.border }))
                    .bg(rgba_of(if active { t.raised } else { t.field }))
                    .cursor_pointer()
                    .child(div().size(px(6.)).rounded(px(3.)).bg(rgba_of(if active {
                        t.text
                    } else {
                        t.mist
                    })))
                    .on_click(move |_, _, cx| update(&a, cx, |s| s.mark_pos = ix))
            }))
        }))
        .into_any_element()
}

fn form_footer(t: &Theme, s: &BulkSheet, app: &Entity<AppModel>) -> AnyElement {
    let (a_cancel, a_go) = (app.clone(), app.clone());
    let can = s.can_start();
    div()
        .px(px(24.))
        .py(px(16.))
        .flex()
        .items_center()
        .gap(px(14.))
        .border_t_1()
        .border_color(rgba_of(t.border))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_size(px(12.5))
                .text_color(rgba_of(t.text_3))
                .child(SharedString::from(s.problem().unwrap_or_default())),
        )
        .child(
            button_with_icon(t, "bulk-cancel", tr!("Cancel"), ButtonKind::Secondary, None)
                .on_click(move |_, _, cx| close(&a_cancel, cx)),
        )
        .child({
            let b = button_with_icon(
                t,
                "bulk-go",
                s.button_label(),
                ButtonKind::Primary,
                Some("folder-output"),
            );
            if can {
                b.on_click(move |_, _, cx| start(&a_go, cx))
            } else {
                b.opacity(0.45)
            }
        })
        .into_any_element()
}

/// The progress panel and, when the job ended, the summary with the skipped files.
fn run_panel(t: &Theme, _s: &BulkSheet, run: &Run, _app: &Entity<AppModel>) -> AnyElement {
    match &run.state {
        RunState::Running => div()
            .py(px(20.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(run.progress_line())),
            )
            .child(progress_bar(
                t,
                &tr!("Export progress"),
                run.done as f32 / run.total.max(1) as f32,
            ))
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(rgba_of(t.mist))
                    .mono()
                    .truncate()
                    .child(SharedString::from(run.current.clone())),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(rgba_of(t.text_3))
                    .child(tr!("Cancel keeps the files already written.")),
            )
            .into_any_element(),
        RunState::Done => div()
            .py(px(20.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(icon(
                        if run.skipped.is_empty() {
                            "circle-check"
                        } else {
                            "circle-alert"
                        },
                        20.,
                        if run.skipped.is_empty() {
                            t.success
                        } else {
                            t.warning
                        },
                    ))
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(run.summary_line())),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(rgba_of(t.mist))
                    .mono()
                    .truncate()
                    .child(SharedString::from(run.dest.clone())),
            )
            .children(run.skipped.iter().map(|k| {
                div()
                    .flex()
                    .gap(px(10.))
                    .text_size(px(13.))
                    .child(icon("circle-slash", 14., t.warning))
                    .child(
                        div()
                            .w(px(220.))
                            .truncate()
                            .child(SharedString::from(k.title.clone())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgba_of(t.warning))
                            .child(SharedString::from(reason_text(&k.reason))),
                    )
            }))
            .into_any_element(),
        RunState::Failed(reason) => div()
            .py(px(20.))
            .text_size(px(14.))
            .text_color(rgba_of(t.warning))
            .child(SharedString::from(tr!(
                "The export stopped: {reason}",
                reason = reason
            )))
            .into_any_element(),
        RunState::Cancelled => div()
            .py(px(20.))
            .text_size(px(14.))
            .child(SharedString::from(tr!(
                "Export cancelled. The files already written are in the folder."
            )))
            .into_any_element(),
    }
}

fn run_footer(t: &Theme, run: &Run, app: &Entity<AppModel>) -> AnyElement {
    let mut f = div()
        .px(px(24.))
        .py(px(16.))
        .flex()
        .items_center()
        .justify_end()
        .gap(px(12.))
        .border_t_1()
        .border_color(rgba_of(t.border));
    match &run.state {
        RunState::Running => {
            let a = app.clone();
            f = f.child(
                button_with_icon(t, "bulk-stop", tr!("Cancel"), ButtonKind::Secondary, None)
                    .on_click(move |_, _, cx| cancel(&a, cx)),
            );
        }
        _ => {
            let (a_show, a_retry, a_done) = (app.clone(), app.clone(), app.clone());
            let dest = run.dest.clone();
            if matches!(run.state, RunState::Done) {
                f = f.child(
                    button_with_icon(
                        t,
                        "bulk-show",
                        tr!("Show in folder"),
                        ButtonKind::Secondary,
                        Some("folder-open"),
                    )
                    .on_click(move |_, _, cx| {
                        let _ = &a_show;
                        cx.reveal_path(std::path::Path::new(&dest));
                    }),
                );
            }
            if !run.skipped.is_empty() {
                f = f.child(
                    button_with_icon(
                        t,
                        "bulk-retry",
                        tr!("Retry skipped"),
                        ButtonKind::Secondary,
                        Some("refresh-cw"),
                    )
                    .on_click(move |_, _, cx| retry_skipped(&a_retry, cx)),
                );
            }
            f = f.child(
                button_with_icon(t, "bulk-done", tr!("Done"), ButtonKind::Primary, None)
                    .on_click(move |_, _, cx| close(&a_done, cx)),
            );
        }
    }
    f.into_any_element()
}

// ---- the calls ------------------------------------------------------------------------------

/// Opens the sheet for these captures `(id, title)`.
pub fn open(app: &Entity<AppModel>, captures: Vec<(String, String)>, cx: &mut App) {
    app.update(cx, |m, cx| {
        let mut sheet = BulkSheet::new(captures);
        // The last folder is a good start.
        if let Some(prev) = m.bulk.sheet.as_ref().and_then(|s| s.dest.clone()) {
            sheet.dest = Some(prev);
        }
        m.bulk.board = None;
        m.bulk.sheet = Some(sheet);
        m.bulk.epoch += 1;
        cx.notify();
    });
}

pub fn close(app: &Entity<AppModel>, cx: &mut App) {
    // A running export keeps running; closing the sheet only hides it.
    app.update(cx, |m, cx| {
        m.bulk.sheet = None;
        cx.notify();
    });
}

fn pick_folder(app: &Entity<AppModel>, cx: &mut App) {
    let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some(tr_ctx!("folder dialog", "Choose folder").into()),
    });
    let app = app.clone();
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = rx.await else {
            return;
        };
        let Some(path) = paths.into_iter().next() else {
            return;
        };
        cx.update(|cx| {
            update(&app, cx, |s| {
                s.dest = Some(path.to_string_lossy().into_owned());
                s.error = None;
            })
        });
    })
    .detach();
}

fn pick_mark_image(app: &Entity<AppModel>, cx: &mut App) {
    let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some(tr_ctx!("file dialog", "Choose picture").into()),
    });
    let app = app.clone();
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = rx.await else {
            return;
        };
        let Some(path) = paths.into_iter().next() else {
            return;
        };
        cx.update(|cx| {
            update(&app, cx, |s| {
                s.mark_image = Some(path.to_string_lossy().into_owned())
            })
        });
    })
    .detach();
}

/// The Export button: `export_images` as the user (the sheet is the consent), then the job is
/// followed until it ends.
pub fn start(app: &Entity<AppModel>, cx: &mut App) {
    let (params, captures, dest) = {
        let m = app.read(cx);
        let Some(s) = m.bulk.sheet.as_ref().filter(|s| s.can_start()) else {
            return;
        };
        match s.request() {
            Ok(p) => (p, s.captures.clone(), s.dest.clone().unwrap_or_default()),
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
            s.error = Some(tr!(
                "Exporting needs the CaptureMann engine, which is not running."
            ))
        });
        return;
    };
    update(app, cx, |s| {
        s.busy = true;
        s.error = None;
    });
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let d = daemon.clone();
        let res = backend
            .rt
            .spawn(async move { Backend::call_coded(&d, "export_images", params).await })
            .await;
        let job_id = match res {
            Ok(Ok(v)) => v["job_id"].as_str().unwrap_or("").to_owned(),
            Ok(Err(e)) => {
                let msg = call_error_text(&e);
                cx.update(|cx| {
                    update(&app2, cx, |s| {
                        s.busy = false;
                        s.error = Some(msg);
                    })
                });
                return;
            }
            Err(e) => {
                cx.update(|cx| {
                    update(&app2, cx, |s| {
                        s.busy = false;
                        s.error = Some(e.to_string());
                    })
                });
                return;
            }
        };
        cx.update(|cx| {
            update(&app2, cx, |s| {
                s.busy = false;
                s.run = Some(Run {
                    job_id: job_id.clone(),
                    total: captures.len(),
                    done: 0,
                    state: RunState::Running,
                    exported: 0,
                    skipped: Vec::new(),
                    current: String::new(),
                    dest,
                });
            })
        });
        // Follow the job until it ends, even when the sheet was closed meanwhile.
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            let (d, id) = (daemon.clone(), job_id.clone());
            let job = backend
                .rt
                .spawn(async move { Backend::call(&d, "get_job", json!({ "job_id": id })).await })
                .await;
            let Ok(Ok(job)) = job else {
                continue;
            };
            let ended = matches!(job["state"].as_str(), Some("done" | "failed" | "cancelled"));
            let titles = captures.clone();
            let (a, ended2) = (app2.clone(), ended);
            let alive = cx.update(|cx| {
                let mut still = true;
                a.update(cx, |m, cx| {
                    match m.bulk.sheet.as_mut().and_then(|s| s.run.as_mut()) {
                        Some(run) if run.job_id == job_id => run.apply_job(&job, &titles),
                        // The sheet was closed or replaced.
                        _ => still = false,
                    }
                    cx.notify();
                });
                if ended2 && !still {
                    // A finished export with the sheet closed: say so with a toast.
                    app::push_toast(
                        &a,
                        crate::toast::ToastSpec::new(
                            crate::toast::ToastKind::Success,
                            crate::bulk_flow::summary(
                                job["result"]["exported"].as_u64().unwrap_or(0) as usize,
                                job["result"]["skipped"].as_u64().unwrap_or(0) as usize,
                            ),
                        ),
                        cx,
                    );
                }
                still
            });
            if ended || !alive {
                break;
            }
        }
    })
    .detach();
}

/// Cancel: `cancel_job`; the files already written stay.
fn cancel(app: &Entity<AppModel>, cx: &mut App) {
    let Some(job_id) = app
        .read(cx)
        .bulk
        .sheet
        .as_ref()
        .and_then(|s| s.run.as_ref())
        .map(|r| r.job_id.clone())
    else {
        return;
    };
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    cx.spawn(async move |_| {
        let _ = backend
            .rt
            .spawn(async move {
                Backend::call(&daemon, "cancel_job", json!({ "job_id": job_id })).await
            })
            .await;
    })
    .detach();
}

/// Retry skipped: the same settings, only the skipped files, a new job.
fn retry_skipped(app: &Entity<AppModel>, cx: &mut App) {
    let next = app
        .read(cx)
        .bulk
        .sheet
        .as_ref()
        .and_then(BulkSheet::retry_skipped);
    if let Some(next) = next {
        app.update(cx, |m, cx| {
            m.bulk.sheet = Some(next);
            m.bulk.epoch += 1;
            cx.notify();
        });
        start(app, cx);
    }
}

#[cfg(test)]
mod sheet_height_tests {
    use super::sheet_max_h;

    #[test]
    fn the_sheet_fits_the_window() {
        assert_eq!(sheet_max_h(2000.0), 860.0);
        assert_eq!(sheet_max_h(700.0), 652.0);
        assert_eq!(sheet_max_h(100.0), 240.0);
    }
}
