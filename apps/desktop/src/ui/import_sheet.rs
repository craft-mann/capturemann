//! The import sheet (board I2) and the drop overlay (board I1), drawn inside the Studio window.
//! State and words: `crate::import_flow`; the calls to the daemon: `ui::import_actions`.
//! Not checked by eye in this build (no display): the view models are unit tested.

use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, prelude::*, px, AnyElement, Context, Entity, FontWeight, Render, Role, SharedString,
    Subscription, Window,
};

use crate::import_flow::{self as f, Kind, Row, Status};
use crate::theme::{scale, Theme};
use crate::ui::app::AppModel;
use crate::ui::components::{button_with_icon, icon, shrink_text, switch, ButtonKind};
use crate::ui::import_actions as act;
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::studio::ThemeGlobal;
use crate::ui::text_field::{TextField, TextFieldEvent};

/// Width of the sheet (board: about 770 px at 1x).
const SHEET_W: f32 = 770.;

pub struct ImportSheetView {
    app: Entity<AppModel>,
    fields: Vec<Entity<TextField>>,
    seen_epoch: u64,
    observe: Subscription,
    field_subs: Vec<Subscription>,
}

impl ImportSheetView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&app, |_, _, cx| cx.notify());
        ImportSheetView {
            app,
            fields: Vec::new(),
            seen_epoch: u64::MAX,
            observe,
            field_subs: Vec::new(),
        }
    }

    /// One title field per row, rebuilt when the rows change.
    fn sync_fields(&mut self, cx: &mut Context<Self>) {
        let _ = &self.observe;
        let (epoch, titles): (u64, Vec<String>) = {
            let m = self.app.read(cx);
            (
                m.import.epoch,
                m.import
                    .sheet
                    .as_ref()
                    .map(|s| s.rows.iter().map(|r| r.title.clone()).collect())
                    .unwrap_or_default(),
            )
        };
        if epoch == self.seen_epoch && titles.len() == self.fields.len() {
            return;
        }
        self.seen_epoch = epoch;
        self.fields.clear();
        self.field_subs.clear();
        for (i, title) in titles.iter().enumerate() {
            let field = cx.new(|cx| {
                let mut t =
                    TextField::new(tr!("Title"), tr_ctx!("field", "Title"), cx).without_icon();
                t.set_text(title, cx);
                t
            });
            let sub = cx.subscribe(&field, move |this, _, ev: &TextFieldEvent, cx| {
                if let TextFieldEvent::Changed(text) = ev {
                    let text = text.clone();
                    this.app.update(cx, |m, _| {
                        if let Some(s) = m.import.sheet.as_mut() {
                            s.set_title(i, &text);
                        }
                    });
                }
            });
            self.field_subs.push(sub);
            self.fields.push(field);
        }
    }
}

fn status_chip(t: &Theme, s: &Status) -> AnyElement {
    let (color, glyph) = match s {
        Status::Ready => (t.success, "circle-check"),
        Status::Prepared(_) => (t.agent_text, "refresh-cw"),
        Status::Cant(r, _) => (
            t.warning,
            if r == "protected_media" {
                "lock-keyhole"
            } else {
                "circle-slash"
            },
        ),
    };
    div()
        .h(px(30.))
        .px(px(12.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .rounded(px(scale::RADIUS_PILL))
        .border_1()
        .border_color(rgba_of(color.with_alpha(40)))
        .text_size(px(13.))
        .text_color(rgba_of(color))
        .child(icon(glyph, 14., color))
        .child(SharedString::from(f::chip_label(s)))
        .into_any_element()
}

fn thumb(t: &Theme, r: &Row) -> AnyElement {
    let glyph = match (&r.status, r.kind) {
        (Status::Cant(reason, _), _) if reason == "protected_media" => "lock-keyhole",
        (Status::Cant(..), _) => "file-x",
        (_, Kind::Audio) => "cm-import-audio",
        (_, Kind::Video) => "cm-import-video",
    };
    div()
        .w(px(122.))
        .h(px(72.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_SM))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.field))
        .child(icon(glyph, 22., t.mist))
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn after_row(
    t: &Theme,
    id: &'static str,
    glyph: &'static str,
    title: String,
    hint: String,
    on: bool,
    enabled: bool,
    app: &Entity<AppModel>,
    toggle: fn(&mut f::Sheet),
) -> AnyElement {
    let app = app.clone();
    div()
        .px(px(16.))
        .py(px(12.))
        .flex()
        .items_center()
        .gap(px(14.))
        .border_b_1()
        .border_color(rgba_of(t.border))
        .child(
            div()
                .size(px(34.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(scale::RADIUS_MD))
                .bg(rgba_of(t.raised))
                .child(icon(glyph, 18., t.text_2)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .when(!enabled, |d| d.opacity(0.5))
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(title)),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(rgba_of(t.mist))
                        .child(SharedString::from(hint)),
                ),
        )
        .child(
            switch(t, id, tr!("On"), on, enabled).on_click(move |_, _, cx| {
                if enabled {
                    app.update(cx, |m, cx| {
                        if let Some(s) = m.import.sheet.as_mut() {
                            toggle(s);
                        }
                        cx.notify();
                    });
                }
            }),
        )
        .into_any_element()
}

impl Render for ImportSheetView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        self.sync_fields(cx);
        let Some(sheet) = self.app.read(cx).import.sheet.clone() else {
            return div().into_any_element();
        };
        let app = self.app.clone();

        let rows: Vec<AnyElement> = sheet
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let app = app.clone();
                let field = self.fields.get(i).cloned();
                div()
                    .px(px(16.))
                    .py(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .border_b_1()
                    .border_color(rgba_of(t.border))
                    .child(thumb(&t, r))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(match (field, r.importable()) {
                                (Some(fl), true) => div().h(px(32.)).child(fl).into_any_element(),
                                _ => div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(SharedString::from(r.name.clone()))
                                    .into_any_element(),
                            })
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgba_of(t.mist))
                                    .mono()
                                    .overflow_hidden()
                                    .child(SharedString::from(r.meta.clone())),
                            ),
                    )
                    .child(status_chip(&t, &r.status))
                    .child(
                        div()
                            .id(SharedString::from(format!("import-remove-{i}")))
                            .role(Role::Button)
                            .aria_label(tr!("Remove from the list"))
                            .focusable()
                            .tab_stop(true)
                            .size(px(32.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(scale::RADIUS_SM))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgba_of(t.raised)))
                            .child(icon("x", 16., t.text_3))
                            .on_click(move |_, _, cx| {
                                app.update(cx, |m, cx| {
                                    if let Some(s) = m.import.sheet.as_mut() {
                                        s.remove(i);
                                    }
                                    m.import.epoch += 1;
                                    cx.notify();
                                })
                            }),
                    )
                    .into_any_element()
            })
            .collect();

        let note = sheet.prepared_note().map(|n| {
            div()
                .mt(px(14.))
                .px(px(16.))
                .py(px(12.))
                .flex()
                .gap(px(10.))
                .rounded(px(scale::RADIUS_MD))
                .border_1()
                .border_color(rgba_of(t.agent.with_alpha(40)))
                .bg(rgba_of(t.agent.with_alpha(10)))
                .text_size(px(13.5))
                .child(icon("info", 16., t.agent_text))
                .child(div().flex_1().min_w(px(0.)).child(SharedString::from(n)))
                .into_any_element()
        });

        let after = div()
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .child(after_row(
                &t,
                "import-transcribe",
                "captions",
                tr!("Transcribe"),
                tr!("Language detected automatically; uses the model in use"),
                sheet.transcribe,
                true,
                &app,
                |s| s.transcribe = !s.transcribe,
            ))
            .child(after_row(
                &t,
                "import-diarize",
                "users",
                tr!("Find speakers"),
                if sheet.speaker_model {
                    tr!("For a call or an interview. Runs after the transcript")
                } else {
                    tr!("Needs the speaker model (AI page)")
                },
                sheet.diarize && sheet.transcribe,
                sheet.speaker_model && sheet.transcribe,
                &app,
                |s| s.diarize = !s.diarize,
            ))
            .children(sheet.has_audio().then(|| {
                div()
                    .px(px(16.))
                    .py(px(10.))
                    .text_size(px(12.5))
                    .text_color(rgba_of(t.mist))
                    .child(tr!(
                        "A report for an audio file is offered after its transcript is ready."
                    ))
            }));

        let remove = {
            let app = app.clone();
            let on = sheet.remove_originals;
            div()
                .mt(px(14.))
                .px(px(16.))
                .py(px(14.))
                .flex()
                .gap(px(12.))
                .rounded(px(scale::RADIUS_LG))
                .border_1()
                .border_color(rgba_of(t.border))
                .bg(rgba_of(t.surface))
                .child(
                    div()
                        .id("import-remove-originals")
                        .role(Role::CheckBox)
                        .aria_label(tr!("Remove the originals after importing"))
                        .aria_toggled(if on {
                            gpui::Toggled::True
                        } else {
                            gpui::Toggled::False
                        })
                        .focusable()
                        .tab_stop(true)
                        .mt(px(2.))
                        .size(px(20.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .border_1()
                        .border_color(rgba_of(t.line))
                        .cursor_pointer()
                        .when(on, |d| d.bg(rgba_of(t.text)))
                        .children(on.then(|| icon("check", 14., t.ground)))
                        .on_click(move |_, _, cx| {
                            app.update(cx, |m, cx| {
                                if let Some(s) = m.import.sheet.as_mut() {
                                    s.remove_originals = !s.remove_originals;
                                }
                                cx.notify();
                            })
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .child(
                            div()
                                .text_size(px(14.5))
                                .font_weight(FontWeight::MEDIUM)
                                .child(tr!("Remove the originals after importing")),
                        )
                        .child(
                            div()
                                .mt(px(3.))
                                .text_size(px(12.5))
                                .text_color(rgba_of(t.mist))
                                .child(SharedString::from(sheet.remove_hint())),
                        ),
                )
        };

        let can = sheet.can_import();
        let (a_start, a_close, a_add, a_cancel) =
            (app.clone(), app.clone(), app.clone(), app.clone());
        let panel = div()
            .id("import-sheet")
            .role(Role::Dialog)
            .aria_label(SharedString::from(sheet.heading()))
            .w(px(SHEET_W))
            .max_h(px(860.))
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
                            .child(icon("cm-import", 22., t.text)),
                    )
                    .child(
                        div()
                            .flex_1().min_w(px(0.))
                            .child(
                                div()
                                    .text_size(px(22.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(SharedString::from(sheet.heading())),
                            )
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .text_color(rgba_of(t.text_3))
                                    .child(tr!(
                                        "Pick one or more files. Edit, transcribe and write reports as if you'd recorded them here."
                                    )),
                            ),
                    )
                    .child(
                        button_with_icon(
                            &t,
                            "import-add",
                            tr!("Add files"),
                            ButtonKind::Secondary,
                            Some("plus"),
                        )
                        .on_click(move |_, _, cx| act::pick(&a_add, cx)),
                    )
                    .child(
                        div()
                            .id("import-close")
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
                            .on_click(move |_, _, cx| act::close_sheet(&a_close, cx)),
                    ),
            )
            .child(
                div()
                    .id("import-body")
                    .flex_1().min_w(px(0.))
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .px(px(24.))
                    .pb(px(18.))
                    .child(
                        div()
                            .rounded(px(scale::RADIUS_LG))
                            .border_1()
                            .border_color(rgba_of(t.border))
                            .bg(rgba_of(t.surface))
                            .children(rows),
                    )
                    .children(note)
                    .child(
                        div()
                            .mt(px(18.))
                            .mb(px(8.))
                            .text_size(px(13.))
                            .text_color(rgba_of(t.text_3))
                            .child(tr!("After import")),
                    )
                    .child(after)
                    .child(remove)
                    .children((sheet.folders_skipped > 0).then(|| {
                        div()
                            .mt(px(14.))
                            .flex()
                            .gap(px(8.))
                            .text_size(px(13.))
                            .text_color(rgba_of(t.warning))
                            .child(icon("folder-input", 16., t.warning))
                            .child(shrink_text(format!(
                                "{} {}",
                                f::folder_skipped_text(sheet.folders_skipped),
                                f::folder_hint()
                            )))
                    }))
                    .children(sheet.error.clone().map(|e| {
                        div()
                            .mt(px(14.))
                            .text_size(px(13.))
                            .text_color(rgba_of(t.warning))
                            .child(SharedString::from(e))
                    })),
            )
            .child(
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
                            .flex_1().min_w(px(0.))
                            .text_size(px(12.5))
                            .text_color(rgba_of(t.text_3))
                            .child(div().child(SharedString::from(sheet.space_line())))
                            .child(div().child(SharedString::from(sheet.originals_line()))),
                    )
                    .child(
                        button_with_icon(
                            &t,
                            "import-cancel",
                            tr!("Cancel"),
                            ButtonKind::Secondary,
                            None,
                        )
                        .on_click(move |_, _, cx| act::close_sheet(&a_cancel, cx)),
                    )
                    .child({
                        let b = button_with_icon(
                            &t,
                            "import-go",
                            sheet.button_label(),
                            ButtonKind::Primary,
                            Some("cm-import"),
                        );
                        if can {
                            b.on_click(move |_, _, cx| act::start(&a_start, cx))
                        } else {
                            b.opacity(0.45)
                        }
                    }),
            );
        div()
            .id("import-scrim")
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

/// The drop overlay (board I1): a dashed full-area zone, the 84 px tile, the title, the counts by
/// kind and the supported formats. Always in the window, shown only while files are dragged over
/// it (`drag_over`); `hover` carries the counts.
pub fn drop_overlay(t: &Theme, hover: Option<&f::DropCount>) -> gpui::Stateful<gpui::Div> {
    let counts = hover.map(f::drop_counts_line).unwrap_or_default();
    let folders = hover.map_or(0, |h| h.folders);
    div()
        .id("import-drop")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .invisible()
        .p(px(24.))
        .child(
            div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(14.))
                .rounded(px(20.))
                .border_2()
                .border_dashed()
                .border_color(rgba_of(t.agent.with_alpha(60)))
                .bg(rgba_of(t.ground.with_alpha(92)))
                .child(
                    div()
                        .size(px(84.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(22.))
                        .bg(rgba_of(t.raised))
                        .child(icon("cm-import", 40., t.text)),
                )
                .child(
                    div()
                        .text_size(px(24.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgba_of(t.text))
                        .child(SharedString::from(f::drop_title())),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(rgba_of(t.text_2))
                        .child(SharedString::from(counts)),
                )
                .children((folders > 0).then(|| {
                    div()
                        .text_size(px(13.))
                        .text_color(rgba_of(t.warning))
                        .child(SharedString::from(format!(
                            "{}. {}",
                            f::folder_skipped_text(folders),
                            f::folder_hint()
                        )))
                }))
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(rgba_of(t.mist))
                        .child(SharedString::from(f::drop_formats_line())),
                ),
        )
}
