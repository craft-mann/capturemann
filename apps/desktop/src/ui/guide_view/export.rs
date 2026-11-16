//! The guide export sheet (GDE-3, design board S9) and "Copy as rich text". Both are one
//! `export_guide` call; the sheet only gathers the options.

use std::path::PathBuf;

use gpui::{div, prelude::*, px, Context, Role, SharedString};
use serde_json::{json, Value};

use cm_i18n::tr;

use super::GuideView;
use crate::theme::{scale, type_scale, Theme};
use crate::toast::{ToastKind, ToastSpec};
use crate::ui::app;
use crate::ui::components::{button, ButtonKind};
use crate::ui::paint::{rgba_of, TextExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Markdown,
    Html,
    Pdf,
}

impl Format {
    fn key(self) -> &'static str {
        match self {
            Format::Markdown => "markdown",
            Format::Html => "html",
            Format::Pdf => "pdf",
        }
    }
}

/// The options of the open sheet.
#[derive(Debug, Clone)]
pub struct ExportState {
    pub format: Format,
    pub dest: Option<PathBuf>,
    pub headings: &'static str,
    pub scale: u32,
    pub intro: bool,
    pub notes: bool,
    pub note_steps: bool,
    pub busy: bool,
}

pub fn open(view: &mut GuideView, cx: &mut Context<GuideView>) {
    view.commit(cx);
    view.export = Some(ExportState {
        format: Format::Markdown,
        dest: None,
        headings: "numbered",
        scale: 1,
        intro: true,
        notes: true,
        note_steps: true,
        busy: false,
    });
    cx.notify();
}

fn toast(view: &GuideView, spec: ToastSpec, cx: &mut Context<GuideView>) {
    let app = view.app.clone();
    cx.defer(move |cx| app::push_toast(&app, spec, cx));
}

/// "Copy as rich text": formatted text with the pictures on the clipboard.
pub fn copy_rich_text(view: &mut GuideView, cx: &mut Context<GuideView>) {
    view.commit(cx);
    let Some(id) = view.guide_id().map(str::to_owned) else {
        return;
    };
    view.call(
        "export_guide",
        json!({ "guide_id": id, "format": "rich_text" }),
        cx,
        |this, r, cx| match r {
            Ok(v) if v["copied_to_clipboard"] == true => toast(
                this,
                ToastSpec::new(ToastKind::Success, tr!("Copied the guide")).body(tr!(
                    "Paste it into a ticket or a document: the text and the pictures are on the clipboard."
                )),
                cx,
            ),
            Ok(v) => {
                let why = v["warnings"][0].as_str().unwrap_or_default().to_owned();
                toast(
                    this,
                    ToastSpec::new(ToastKind::Warning, tr!("The guide was not copied")).body(why),
                    cx,
                );
            }
            Err(e) => {
                this.error = Some(e);
                cx.notify();
            }
        },
    );
}

fn choose_folder(view: &mut GuideView, cx: &mut Context<GuideView>) {
    let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some(cm_i18n::tr_ctx!("folder dialog", "Choose folder").into()),
    });
    let _ = view;
    cx.spawn(async move |this, cx| {
        let Ok(Ok(Some(paths))) = rx.await else {
            return;
        };
        if let Some(p) = paths.into_iter().next() {
            let _ = this.update(cx, |this, cx| {
                if let Some(ex) = &mut this.export {
                    ex.dest = Some(p);
                }
                cx.notify();
            });
        }
    })
    .detach();
}

fn run(view: &mut GuideView, cx: &mut Context<GuideView>) {
    let (Some(ex), Some(id)) = (view.export.clone(), view.guide_id().map(str::to_owned)) else {
        return;
    };
    let Some(dest) = ex.dest.clone() else {
        choose_folder(view, cx);
        return;
    };
    if let Some(e) = &mut view.export {
        e.busy = true;
    }
    // Markdown goes into the chosen folder when it is empty, else into a folder of the guide's
    // name inside it; HTML and PDF are one file in the chosen folder. Nothing is overwritten.
    let params = json!({
        "guide_id": id,
        "format": ex.format.key(),
        "dest": dest,
        "headings": ex.headings,
        "image_scale": ex.scale,
        "intro": ex.intro,
        "notes": ex.notes,
        "note_steps": ex.note_steps,
    });
    cx.notify();
    view.call("export_guide", params, cx, move |this, r, cx| {
        this.export = None;
        match r {
            Ok(v) => {
                let n = v["steps"].as_u64().unwrap_or(0);
                // Where it really went: the daemon never writes over what is there, so a folder
                // that held files gets a folder of the guide's name inside it (or `(2)`).
                let path = v["path"]
                    .as_str()
                    .map_or_else(|| dest.display().to_string(), str::to_owned);
                let mut body = tr!("{n} steps written to {path}", n = n, path = path);
                if let Some(w) = v["warnings"][0].as_str() {
                    body = format!("{body}\n{w}");
                }
                toast(
                    this,
                    ToastSpec::new(ToastKind::Success, tr!("Guide exported")).body(body),
                    cx,
                );
            }
            Err(e) => this.error = Some(e),
        }
        cx.notify();
    });
}

fn tile(
    t: &Theme,
    id: &'static str,
    label: String,
    on: bool,
    disabled: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(label.clone()))
        .px(px(14.))
        .h(px(44.))
        .flex()
        .items_center()
        .rounded(px(scale::RADIUS_MD))
        .border_1()
        .border_color(rgba_of(if on { t.text } else { t.line }))
        .text_size(px(type_scale::UI))
        .text_color(rgba_of(if disabled { t.faint } else { t.text }))
        .when(!disabled, |d| d.cursor_pointer())
        .child(SharedString::from(label))
}

fn option_row(t: &Theme, id: &'static str, label: String, on: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(Role::CheckBox)
        .aria_label(SharedString::from(label.clone()))
        .flex()
        .items_center()
        .gap(px(8.))
        .cursor_pointer()
        .text_size(px(type_scale::UI))
        .child(
            // On: a filled box with a tick; off: an empty outline (G4: the fill alone was easy to misread).
            div()
                .size(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .border_1()
                .border_color(rgba_of(if on { t.text } else { t.text_3 }))
                .when(on, |d| {
                    d.bg(rgba_of(t.text))
                        .child(crate::ui::components::icon("check", 12., t.ground))
                }),
        )
        .child(SharedString::from(label))
}

/// The sheet, drawn over the guide editor.
pub fn sheet(t: &Theme, ex: &ExportState, cx: &mut Context<GuideView>) -> gpui::AnyElement {
    let set = |f: fn(&mut ExportState)| {
        cx.listener(move |this: &mut GuideView, _, _, cx| {
            if let Some(e) = &mut this.export {
                f(e);
            }
            cx.notify();
        })
    };
    let headings = |label: String, key: &'static str, id: &'static str| {
        tile(t, id, label, ex.headings == key, false).on_click(cx.listener(
            move |this: &mut GuideView, _, _, cx| {
                if let Some(e) = &mut this.export {
                    e.headings = key;
                }
                cx.notify();
            },
        ))
    };
    let dest_text = ex
        .dest
        .as_ref()
        .map_or_else(|| tr!("Choose a folder"), |p| p.display().to_string());
    let card = div()
        .id("guide-export-sheet")
        .role(Role::Dialog)
        .aria_label(tr!("Export guide"))
        .w(px(620.))
        .max_h_full()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(px(14.))
        .p(px(24.))
        .rounded(px(scale::RADIUS_DIALOG))
        .border_1()
        .border_color(rgba_of(t.line))
        .bg(rgba_of(t.surface))
        .child(
            div()
                .text_size(px(type_scale::TITLE))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(tr!("Export guide")),
        )
        .child(
            div()
                .flex()
                .gap(px(8.))
                .child(
                    tile(t, "fmt-md", tr!("Markdown"), ex.format == Format::Markdown, false)
                        .on_click(set(|e| e.format = Format::Markdown)),
                )
                .child(
                    tile(t, "fmt-html", tr!("HTML"), ex.format == Format::Html, false)
                        .on_click(set(|e| e.format = Format::Html)),
                )
                .child(
                    tile(t, "fmt-pdf", tr!("PDF"), ex.format == Format::Pdf, false)
                        .on_click(set(|e| e.format = Format::Pdf)),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(div().text_color(rgba_of(t.text_3)).child(tr!("Save to")))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .overflow_hidden()
                        .mono()
                        .text_size(px(type_scale::CAPTION))
                        .child(SharedString::from(dest_text)),
                )
                .child(
                    button(t, "guide-choose", tr!("Choose\u{2026}"), ButtonKind::Secondary)
                        .on_click(cx.listener(|this: &mut GuideView, _, _, cx| choose_folder(this, cx))),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(div().text_color(rgba_of(t.text_3)).child(tr!("Step headings")))
                .child(headings(tr!("1. Title"), "numbered", "hd-num"))
                .child(headings(tr!("Step 1 \u{b7} Title"), "step", "hd-step"))
                .child(headings(tr!("Title only"), "title_only", "hd-title")),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(div().text_color(rgba_of(t.text_3)).child(tr!("Image size")))
                .child(tile(t, "sc-1", "1\u{d7}".to_owned(), ex.scale == 1, false).on_click(set(|e| e.scale = 1)))
                .child(tile(t, "sc-2", "2\u{d7}".to_owned(), ex.scale == 2, false).on_click(set(|e| e.scale = 2))),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(16.))
                .child(option_row(t, "op-intro", tr!("Intro"), ex.intro).on_click(set(|e| e.intro = !e.intro)))
                .child(option_row(t, "op-notes", tr!("Step notes"), ex.notes).on_click(set(|e| e.notes = !e.notes)))
                .child(
                    option_row(t, "op-notesteps", tr!("Note steps without an image"), ex.note_steps)
                        .on_click(set(|e| e.note_steps = !e.note_steps)),
                ),
        )
        .child(
            div()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.text_3))
                .child(tr!(
                    "Pictures are rendered with your annotations and blurs. Links in the Markdown are relative, so it pastes into a wiki or a docs repo as it is."
                )),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.))
                .child(
                    button(t, "guide-export-cancel", tr!("Cancel"), ButtonKind::Secondary).on_click(
                        cx.listener(|this: &mut GuideView, _, _, cx| {
                            this.export = None;
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    button(
                        t,
                        "guide-export-run",
                        match ex.format {
                            Format::Markdown => tr!("Export Markdown"),
                            Format::Html => tr!("Export HTML"),
                            Format::Pdf => tr!("Export PDF"),
                        },
                        ButtonKind::Primary,
                    )
                    .when(ex.busy, |d| d.opacity(0.5))
                    .on_click(cx.listener(|this: &mut GuideView, _, _, cx| run(this, cx))),
                ),
        );
    let _: Option<Value> = None;
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba_of(t.modal_scrim))
        // The scrim takes hover and clicks, so the guide editor behind stays still.
        .occlude()
        .child(card)
        .into_any_element()
}
