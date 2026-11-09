//! The Export popover (EXP-5, DESIGN 5.13, boards E1 to E4): choose the format and the size, see
//! the estimated file size first, then export. The state and its rules are `crate::export_flow`
//! (tested with fakes); this file draws it and makes the daemon calls (`estimate_export` on every
//! change, off the UI thread, `export_video` on the button). Not seen by eye yet
//! (`tests/manual/EXP-5.md`).
//!
//! Decisions of Nov 9, 2026: the button always reads "Export…" and opens the popover on MP4 at the
//! recording's size (nothing is remembered); the estimate is an upper bound ("Up to 290 MB"); no
//! presets; GIF has no estimate. 460 px wide at the top right of the page under the button; under
//! 900 px window width a centred dialog over the scrim, same content. The anchor is the page's
//! toolbar corner, not a measured button position (no arrow).

use std::sync::atomic::{AtomicU64, Ordering};

use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, prelude::*, px, AnyElement, App, Context, ElementId, Entity, FocusHandle, FontWeight,
    KeyDownEvent, MouseButton, Render, Role, SharedString, Stateful, Subscription, Window,
};
use serde_json::Value;

use crate::backend::Backend;
use crate::daemon_text;
use crate::export_flow::{
    self as f, ButtonText, ClipLine, Format, HeaderLine, Look, Popover, Refusal, Size,
};
use crate::shortcuts::Platform;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::AppModel;
use crate::ui::components::{button_with_icon, icon, label_fill, shrink_text, ButtonKind};
use crate::ui::export_ui;
use crate::ui::kit::{self, Tone};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::widgets::{segment, segmented};
use crate::ui::studio::ThemeGlobal;
use crate::ui::text_field::{TextField, TextFieldEvent};
use crate::ui::tooltip::tip;

/// Window width under which the popover is a centred dialog.
pub const NARROW_UNDER: f32 = 900.;
/// The popover's width.
const WIDTH: f32 = 460.;
/// Distance from the window's top to the popover (the page toolbar's height).
const TOP: f32 = 72.;

/// Bumped on every open: the MB field starts again from 10 then.
static OPENED: AtomicU64 = AtomicU64::new(0);

// ---- the entry point ---------------------------------------------------------------------------

/// What a page tells the popover about the recording it exports.
#[derive(Debug, Clone)]
pub struct ExportContext {
    pub capture_id: String,
    pub title: String,
    pub captions: Option<bool>,
    pub range: Option<(f64, f64)>,
}

/// Opens the popover on the defaults and asks for the first estimate.
pub fn open(app: &Entity<AppModel>, c: ExportContext, cx: &mut App) {
    OPENED.fetch_add(1, Ordering::Relaxed);
    app.update(cx, |m, cx| {
        m.export_popover = Some(Popover::new(&c.capture_id, &c.title, c.captions, c.range));
        cx.notify();
    });
    estimate(app, cx);
}

pub fn close(app: &Entity<AppModel>, cx: &mut App) {
    app.update(cx, |m, cx| {
        if m.export_popover.take().is_some() {
            cx.notify();
        }
    });
}

/// The key hint of the button's tooltip: `["\u{2318}", "E"]` or `["Ctrl", "E"]`.
pub fn shortcut_keys() -> Vec<String> {
    match Platform::current() {
        Platform::Mac => vec!["\u{2318}".to_owned(), "E".to_owned()],
        Platform::Windows | Platform::Linux => vec!["Ctrl".to_owned(), "E".to_owned()],
    }
}

/// The text of the button for the running export of `capture_id`.
fn button_label(capture_id: &str) -> (String, bool) {
    let running = export_ui::running_progress(capture_id);
    match f::button_text(running) {
        ButtonText::Export => (tr_ctx!("button", "Export\u{2026}"), false),
        ButtonText::Exporting(pct) => (tr_ctx!("button", "Exporting {pct}%", pct = pct), true),
    }
}

/// The one Export button: "Export…", or "Exporting 42%" while a file exports (it still opens the
/// popover, to queue another export). Used by the recording page and the recording editor.
pub fn export_button(
    t: &Theme,
    id: impl Into<ElementId>,
    app: &Entity<AppModel>,
    c: ExportContext,
) -> Stateful<gpui::Div> {
    let (label, busy) = button_label(&c.capture_id);
    let app = app.clone();
    button_with_icon(
        t,
        id,
        label.clone(),
        ButtonKind::Secondary,
        Some(if busy { "loader-circle" } else { "download" }),
    )
    .tooltip(tip(label, shortcut_keys()))
    .on_click(move |_, _, cx| {
        let (app, c) = (app.clone(), c.clone());
        cx.defer(move |cx| open(&app, c, cx));
    })
}

// ---- calls to the daemon -------------------------------------------------------------------------

fn refusal_of(e: &crate::backend::CallError) -> Refusal {
    Refusal {
        code: e.code,
        message: e.message.clone(),
    }
}

/// Asks `estimate_export` for the current choice, off the UI thread; a reply for an older choice
/// is dropped by the state.
pub fn estimate(app: &Entity<AppModel>, cx: &mut App) {
    let Some((seq, params)) = app.update(cx, |m, cx| {
        let r = m.export_popover.as_mut().map(Popover::begin_estimate);
        cx.notify();
        r
    }) else {
        return;
    };
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = backend
            .rt
            .spawn(async move { Backend::call_coded(&daemon, "estimate_export", params).await })
            .await;
        let reply: Result<Value, Refusal> = match res {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(refusal_of(&e)),
            Err(e) => Err(Refusal {
                code: None,
                message: e.to_string(),
            }),
        };
        cx.update(|cx| {
            app2.update(cx, |m, cx| {
                if let Some(p) = m.export_popover.as_mut() {
                    if p.apply_estimate(seq, reply) {
                        cx.notify();
                    }
                }
            })
        });
    })
    .detach();
}

/// Runs `change` on the open popover; an estimate follows when it says the choice changed.
fn choose(app: &Entity<AppModel>, cx: &mut App, change: impl FnOnce(&mut Popover) -> bool) {
    let changed = app.update(cx, |m, cx| {
        let changed = m.export_popover.as_mut().is_some_and(change);
        cx.notify();
        changed
    });
    if changed {
        estimate(app, cx);
    }
}

/// The footer button: `export_video` with the choice. The popover closes when the daemon accepted
/// the job (it runs in the background, and the button shows the progress); a refusal stays in the
/// popover.
fn start(app: &Entity<AppModel>, cx: &mut App) {
    let Some(params) = app.update(cx, |m, cx| {
        let p = m.export_popover.as_mut().filter(|p| p.can_export())?;
        p.busy = true;
        p.error = None;
        cx.notify();
        Some(p.export_params())
    }) else {
        return;
    };
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = backend
            .rt
            .spawn(async move { Backend::call_coded(&daemon, "export_video", params).await })
            .await;
        cx.update(|cx| {
            let failure = match res {
                Ok(Ok(_)) => None,
                Ok(Err(e)) => Some(refusal_of(&e)),
                Err(e) => Some(Refusal {
                    code: None,
                    message: e.to_string(),
                }),
            };
            match failure {
                None => close(&app2, cx),
                Some(r) => app2.update(cx, |m, cx| {
                    if let Some(p) = m.export_popover.as_mut() {
                        p.busy = false;
                        p.error = Some(r);
                    }
                    cx.notify();
                }),
            }
        });
    })
    .detach();
}

// ---- the view ----------------------------------------------------------------------------------

/// The overlay drawn by the Studio window while the popover is open.
pub struct ExportPopoverView {
    app: Entity<AppModel>,
    /// The "Keep the file under" number.
    mb: Entity<TextField>,
    seen_open: u64,
    /// Takes the keys (Esc) while the popover is open.
    focus: FocusHandle,
    focused: bool,
    _subs: Vec<Subscription>,
}

impl ExportPopoverView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        let mb = cx.new(|cx| {
            let mut t = TextField::new(tr!("Megabytes"), tr_ctx!("field", "MB"), cx)
                .without_icon()
                .height(32.);
            t.set_text(&f::DEFAULT_MAX_MB.to_string(), cx);
            t
        });
        let sub = cx.subscribe(&mb, |this, _, ev: &TextFieldEvent, cx| match ev {
            TextFieldEvent::Changed(text) => {
                let text = text.clone();
                this.app.update(cx, |m, cx| {
                    if let Some(p) = m.export_popover.as_mut() {
                        p.set_max_mb_text(&text);
                    }
                    cx.notify();
                });
            }
            TextFieldEvent::Submitted => {
                let app = this.app.clone();
                cx.defer(move |cx| start(&app, cx));
            }
            TextFieldEvent::Escaped => {
                let app = this.app.clone();
                cx.defer(move |cx| close(&app, cx));
            }
        });
        let observe = cx.observe(&app, |_, _, cx| cx.notify());
        ExportPopoverView {
            app,
            mb,
            seen_open: u64::MAX,
            focus: cx.focus_handle(),
            focused: false,
            _subs: vec![sub, observe],
        }
    }
}

fn label_text(t: &Theme, text: String) -> gpui::Div {
    div()
        .text_size(px(type_scale::CAPTION))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba_of(t.mist))
        .child(SharedString::from(text))
}

/// A format card: a radio dot and the name over a muted line; the chosen one has the strong
/// border. The text wraps; no fixed widths.
fn format_card(t: &Theme, app: &Entity<AppModel>, fmt: Format, active: bool) -> AnyElement {
    let desc = match fmt {
        Format::Mp4 => tr!("Plays almost everywhere"),
        Format::Webm => tr!("Smaller, for the web"),
        Format::Gif => tr!("Short loop for docs and PRs"),
    };
    let app = app.clone();
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .id(SharedString::from(format!("ex-fmt-{}", fmt.as_str())))
        .role(Role::RadioButton)
        .aria_label(SharedString::from(fmt.label()))
        .aria_description(SharedString::from(desc.clone()))
        .aria_toggled(if active {
            gpui::Toggled::True
        } else {
            gpui::Toggled::False
        })
        .focusable()
        .tab_stop(true)
        .flex_1()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .items_start()
        .gap(px(4.))
        .px(px(12.))
        .py(px(11.))
        .rounded(px(12.))
        .border_1()
        .border_color(rgba_of(if active { t.line } else { t.border }))
        .bg(rgba_of(if active { t.raised } else { t.surface_2 }))
        .cursor_pointer()
        .when(!active, |d| d.hover(move |s| s.bg(hover)))
        .focus_visible(move |s| s.border_color(focus))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(radio_dot(t, active))
                .child(shrink_text(fmt.label())),
        )
        .child(
            div()
                .text_size(px(12.))
                .line_height(px(17.))
                .text_color(rgba_of(t.mist))
                .child(SharedString::from(desc)),
        )
        .on_click(move |_, _, cx| {
            let app = app.clone();
            cx.defer(move |cx| choose(&app, cx, |p| p.set_format(fmt)));
        })
        .into_any_element()
}

/// The 16 px radio mark: a ring, with a dot when chosen.
fn radio_dot(t: &Theme, on: bool) -> gpui::Div {
    div()
        .size(px(16.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .border_1()
        .border_color(rgba_of(if on { t.text } else { t.line }))
        .children(on.then(|| div().size(px(8.)).rounded(px(4.)).bg(rgba_of(t.text))))
}

/// One row of the Size list: the radio mark, the name, and the real pixels in mono on the right.
fn size_row(
    t: &Theme,
    app: &Entity<AppModel>,
    o: f::SizeOption,
    active: bool,
    first: bool,
) -> AnyElement {
    let name = match o.size {
        Size::Source => tr!("Recording size"),
        // i18n-ignore (the step's own name: 1080p)
        Size::Step(h) => format!("{h}p"),
    };
    let pixels = format!("{} \u{00D7} {}", o.width, o.height);
    let app = app.clone();
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .id(SharedString::from(format!(
            "ex-size-{}",
            match o.size {
                Size::Source => "source".to_owned(),
                Size::Step(h) => h.to_string(),
            }
        )))
        .role(Role::RadioButton)
        .aria_label(SharedString::from(format!("{name}, {pixels}")))
        .aria_toggled(if active {
            gpui::Toggled::True
        } else {
            gpui::Toggled::False
        })
        .focusable()
        .tab_stop(true)
        .min_h(px(40.))
        .px(px(14.))
        .py(px(6.))
        .flex()
        .items_center()
        .gap(px(12.))
        .when(!first, |d| {
            d.border_t_1().border_color(rgba_of(t.row_divider()))
        })
        .when(active, |d| d.bg(rgba_of(t.raised)))
        .cursor_pointer()
        .when(!active, |d| d.hover(move |s| s.bg(hover)))
        .focus_visible(move |s| s.border_color(focus))
        .child(radio_dot(t, active))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_size(px(14.))
                .text_color(rgba_of(if active { t.text } else { t.text_2 }))
                .child(SharedString::from(name)),
        )
        .child(
            div()
                .flex_none()
                .mono()
                .tnum()
                .text_size(px(12.5))
                .text_color(rgba_of(t.mist))
                .child(SharedString::from(pixels)),
        )
        .on_click(move |_, _, cx| {
            let app = app.clone();
            cx.defer(move |cx| choose(&app, cx, |p| p.set_size(o.size)));
        })
        .into_any_element()
}

/// The size block: the one fixed block above the footer, in its four looks.
fn size_block(t: &Theme, p: &Popover) -> AnyElement {
    let block = |tone: Tone, glyph: &'static str, lines: Vec<AnyElement>| {
        let c = kit::tone_colors(t, tone);
        div()
            .id("ex-size-block")
            .px(px(16.))
            .py(px(14.))
            .flex()
            .items_center()
            .gap(px(14.))
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(c.border))
            .bg(rgba_of(if tone == Tone::Neutral {
                t.surface
            } else {
                c.bg
            }))
            .child(
                div()
                    .size(px(40.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.))
                    .bg(rgba_of(if tone == Tone::Neutral {
                        t.raised
                    } else {
                        c.border
                    }))
                    .child(icon(
                        glyph,
                        20.,
                        if tone == Tone::Neutral {
                            t.agent_text
                        } else {
                            c.icon
                        },
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .children(lines),
            )
    };
    let small = |t: &Theme, text: String| {
        div()
            .text_size(px(12.5))
            .line_height(px(17.))
            .text_color(rgba_of(t.mist))
            .child(SharedString::from(text))
            .into_any_element()
    };
    let big = |t: &Theme, text: String| {
        div()
            .text_size(px(22.))
            .line_height(px(28.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgba_of(t.text))
            .child(SharedString::from(text))
            .into_any_element()
    };
    let title_row = |t: &Theme, with_tag: bool| {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .text_size(px(13.))
            .text_color(rgba_of(t.mist))
            .child(label_fill(tr!("Estimated size")).flex_none())
            .children(with_tag.then(|| {
                div()
                    .flex_none()
                    .px(px(8.))
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .rounded(px(scale::RADIUS_PILL))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .text_size(px(11.5))
                    .child(SharedString::from(tr_ctx!("size tag", "Estimate")))
            }))
            .into_any_element()
    };
    match &p.look {
        Look::Bound {
            max_bytes,
            typical_bytes,
            codec,
            width,
            height,
        } => {
            // EXP-5 AC10: the typical figure leads and the ceiling follows in the same sentence;
            // without one (an audio capture) the bound alone, as before.
            let (big_text, detail) = match typical_bytes {
                Some(typ) => (
                    daemon_text::typical_label(*typ),
                    daemon_text::typical_detail(
                        p.format.label(),
                        codec.label(),
                        *width,
                        *height,
                        *typ,
                        *max_bytes,
                    ),
                ),
                None => (
                    daemon_text::estimate_label(*max_bytes),
                    tr!(
                        "{format} \u{00B7} {codec} \u{00B7} {width} \u{00D7} {height}. A quiet screen usually comes out much smaller. The real size is shown after export.",
                        format = p.format.label(),
                        codec = codec.label(),
                        width = width,
                        height = height
                    ),
                ),
            };
            block(
                Tone::Neutral,
                "hard-drive",
                vec![title_row(t, true), big(t, big_text), small(t, detail)],
            )
            .into_any_element()
        }
        Look::Unknown => block(
            Tone::Neutral,
            "image",
            vec![
                div()
                    .text_size(px(13.))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(tr!("File size")))
                    .into_any_element(),
                div()
                    .text_size(px(18.))
                    .line_height(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgba_of(t.text_2))
                    .child(SharedString::from(tr!("Size known after export")))
                    .into_any_element(),
                small(
                    t,
                    match p.keep_under_mb() {
                        Some(mb) => tr!(
                            "GIF size can\u{2019}t be estimated. It will stay under {mb} MB: width, then frame rate, step down to fit.",
                            mb = mb
                        ),
                        None => tr!(
                            "GIF size can\u{2019}t be estimated. The real size is shown after export."
                        ),
                    },
                ),
            ],
        )
        .into_any_element(),
        Look::NotAllowed { code, message } => {
            let c = kit::tone_colors(t, Tone::Brass);
            block(
                Tone::Brass,
                "triangle-alert",
                vec![
                    div()
                        .text_size(px(13.))
                        .text_color(rgba_of(t.mist))
                        .child(SharedString::from(tr!("Can\u{2019}t export as GIF")))
                        .into_any_element(),
                    div()
                        .text_size(px(15.))
                        .line_height(px(21.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgba_of(c.fg))
                        .child(SharedString::from(daemon_text::error_text(*code, message)))
                        .into_any_element(),
                    small(t, daemon_text::gif_too_long_hint()),
                ],
            )
            .into_any_element()
        }
        Look::Failed { code, message } => block(
            Tone::Neutral,
            "hard-drive",
            vec![
                title_row(t, false),
                small(t, daemon_text::error_text(*code, message)),
            ],
        )
        .into_any_element(),
        // No reply yet: the same block with a dash, so nothing shifts when the number arrives.
        Look::Pending => block(
            Tone::Neutral,
            "hard-drive",
            vec![
                title_row(t, true),
                // i18n-ignore (a dash, not text)
                big(t, "\u{2014}".to_owned()),
                small(t, tr!("The real size is shown after export.")),
            ],
        )
        .into_any_element(),
    }
}

impl Render for ExportPopoverView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        let Some(p) = self.app.read(cx).export_popover.clone() else {
            self.focused = false;
            return div().into_any_element();
        };
        let opened = OPENED.load(Ordering::Relaxed);
        if self.seen_open != opened {
            self.seen_open = opened;
            self.mb
                .update(cx, |t, cx| t.set_text(&f::DEFAULT_MAX_MB.to_string(), cx));
        }
        if !self.focused {
            self.focused = true;
            window.focus(&self.focus, cx);
        }
        let narrow = f32::from(window.viewport_size().width) < NARROW_UNDER;
        let app = self.app.clone();

        // ---- header ----
        let sub = match p.header_line() {
            Some(HeaderLine::AfterCuts(s)) => {
                tr!("{time} after cuts", time = f::clock(s))
            }
            Some(HeaderLine::BetweenInOut(s)) => {
                tr!("{time} between In and Out", time = f::clock(s))
            }
            None => String::new(),
        };
        let a_close = app.clone();
        let header = div()
            .px(px(20.))
            .pt(px(18.))
            .pb(px(4.))
            .flex()
            .items_start()
            .gap(px(12.))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .id("ex-title")
                            .role(Role::Heading)
                            .aria_level(2)
                            .aria_label(SharedString::from(tr_ctx!("export popover", "Export")))
                            .text_size(px(18.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(tr_ctx!("export popover", "Export"))),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgba_of(t.text_3))
                            .child(SharedString::from(if sub.is_empty() {
                                p.title.clone()
                            } else {
                                format!("{} \u{00B7} {sub}", p.title)
                            })),
                    ),
            )
            .children(narrow.then(|| {
                div()
                    .id("ex-close")
                    .role(Role::Button)
                    .aria_label(tr!("Close"))
                    .focusable()
                    .tab_stop(true)
                    .size(px(32.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba_of(t.raised)))
                    .child(icon("x", 16., t.text_2))
                    .on_click(move |_, _, cx| {
                        let app = a_close.clone();
                        cx.defer(move |cx| close(&app, cx));
                    })
            }));

        // ---- format ----
        let formats = div()
            .id("ex-formats")
            .role(Role::RadioGroup)
            .aria_label(tr!("Format"))
            .flex()
            .flex_wrap()
            .gap(px(8.))
            .children(
                Format::ALL
                    .iter()
                    .map(|fmt| format_card(&t, &app, *fmt, p.format == *fmt)),
            );
        let vp9 = p.vp9_note().then(|| {
            let c = kit::tone_colors(&t, Tone::Agent);
            div()
                .id("ex-vp9-note")
                .px(px(12.))
                .py(px(10.))
                .flex()
                .items_start()
                .gap(px(10.))
                .rounded(px(scale::RADIUS_MD))
                .border_1()
                .border_color(rgba_of(c.border))
                .bg(rgba_of(c.bg))
                .child(icon("info", 16., c.icon))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(12.5))
                        .line_height(px(18.))
                        .text_color(rgba_of(t.text_2))
                        .child(SharedString::from(tr!(
                            "This computer has no H.264 video encoder, so MP4 files use VP9 video. Browsers play them; some older players and editors may not."
                        ))),
                )
        });

        // ---- size (MP4, WebM) or width, clip and MB (GIF) ----
        let middle: AnyElement = if p.format == Format::Gif {
            let widths = p.gif_widths();
            let mut seg = segmented(&t, "ex-widths", &tr!("Width"));
            for w in widths {
                let app = app.clone();
                seg = seg.child(
                    segment(&t, format!("ex-w-{w}"), &w.to_string(), p.gif_width == w)
                        .mono()
                        .on_click(move |_, _, cx| {
                            let app = app.clone();
                            cx.defer(move |cx| choose(&app, cx, |p| p.set_gif_width(w)));
                        }),
                );
            }
            let clip = p.clip_line().map(|c| {
                let text = match c {
                    ClipLine::InOut { from, to, secs } => tr!(
                        "In {from} \u{2192} Out {to} \u{00B7} {secs} s ({max} s at most)",
                        from = f::clock(from),
                        to = f::clock(to),
                        secs = secs.round() as u64,
                        max = f::GIF_MAX_SECS as u64
                    ),
                    ClipLine::Whole { secs } => tr!(
                        "Whole recording \u{00B7} {length} ({max} s at most)",
                        length = f::clock(secs),
                        max = f::GIF_MAX_SECS as u64
                    ),
                };
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .text_size(px(13.))
                    .text_color(rgba_of(t.text_2))
                    .child(icon("scissors", 16., t.mist))
                    .child(label_fill(text))
            });
            let a_keep = app.clone();
            let keep = p.keep_under;
            let checkbox = div()
                .id("ex-keep")
                .role(Role::CheckBox)
                .aria_label(SharedString::from(tr!(
                    "Keep the file under {mb} MB",
                    mb = p.max_mb.map(|m| m.to_string()).unwrap_or_default()
                )))
                .aria_toggled(if keep {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .focusable()
                .tab_stop(true)
                .flex()
                .flex_none()
                .items_center()
                .gap(px(10.))
                .cursor_pointer()
                .child(
                    div()
                        .size(px(20.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .border_1()
                        .border_color(rgba_of(t.line))
                        .when(keep, |d| d.bg(rgba_of(t.text)))
                        .children(keep.then(|| icon("check", 14., t.ground))),
                )
                .child(
                    div()
                        .text_size(px(13.5))
                        .child(SharedString::from(tr!("Keep the file under"))),
                )
                .on_click(move |_, _, cx| {
                    let app = a_keep.clone();
                    cx.defer(move |cx| {
                        app.update(cx, |m, cx| {
                            if let Some(p) = m.export_popover.as_mut() {
                                let on = !p.keep_under;
                                p.set_keep_under(on);
                            }
                            cx.notify();
                        })
                    });
                });
            let invalid = p.keep_under && p.max_mb.is_none();
            let mb_row = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(10.))
                .child(checkbox)
                .child(
                    div()
                        .w(px(76.))
                        .flex_none()
                        .rounded(px(8.))
                        .when(invalid, |d| d.border_1().border_color(rgba_of(t.warning)))
                        .child(self.mb.clone()),
                )
                .child(
                    div()
                        .text_size(px(13.5))
                        .child(SharedString::from(tr_ctx!("unit", "MB"))),
                );
            div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(label_text(&t, tr!("Width")))
                        .child(div().flex().child(seg)),
                )
                .children(clip)
                .child(mb_row)
                .into_any_element()
        } else {
            let rows = p.size_rows();
            let last = rows.len();
            let list = div()
                .id("ex-sizes")
                .role(Role::RadioGroup)
                .aria_label(tr!("Size"))
                .overflow_hidden()
                .rounded(px(scale::RADIUS_LG))
                .border_1()
                .border_color(rgba_of(t.border))
                .bg(rgba_of(t.surface))
                .children(
                    rows.iter()
                        .enumerate()
                        .map(|(i, o)| size_row(&t, &app, *o, p.size == o.size, i == 0)),
                )
                .children((last == 0).then(|| {
                    div()
                        .h(px(40.))
                        .px(px(14.))
                        .flex()
                        .items_center()
                        .text_size(px(13.))
                        .text_color(rgba_of(t.mist))
                        .child(SharedString::from(tr!("Recording size")))
                }));
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_baseline()
                        .gap_x(px(8.))
                        .child(label_text(&t, tr!("Size")))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(rgba_of(t.mist))
                                .child(SharedString::from(tr!("Never larger than the recording"))),
                        ),
                )
                .child(list)
                .into_any_element()
        };

        // ---- footer ----
        let (a_cancel, a_go) = (app.clone(), app.clone());
        let can = p.can_export();
        let go_label = tr!("Export {format}", format = p.format.label());
        let reason = match &p.look {
            Look::NotAllowed { code, message } => Some(format!(
                "{} {}",
                daemon_text::error_text(*code, message),
                daemon_text::gif_too_long_hint()
            )),
            _ => None,
        };
        let go = if can {
            button_with_icon(&t, "ex-go", go_label, ButtonKind::Primary, Some("download"))
                .when(p.busy, |d| d.opacity(0.6))
                .on_click(move |_, _, cx| {
                    let app = a_go.clone();
                    cx.defer(move |cx| start(&app, cx));
                })
        } else {
            // aria-disabled: it stays in the tab order, carries the reason as its description and
            // as its tooltip, and does nothing when pressed.
            let mut b =
                button_with_icon(&t, "ex-go", go_label, ButtonKind::Primary, Some("download"))
                    .opacity(0.45)
                    .cursor_default();
            if let Some(r) = reason {
                b = b
                    .aria_description(SharedString::from(r.clone()))
                    .tooltip(tip(r, Vec::new()));
            }
            b
        };
        let cancel = button_with_icon(
            &t,
            "ex-cancel",
            tr_ctx!("button", "Cancel"),
            ButtonKind::Secondary,
            None,
        )
        .on_click(move |_, _, cx| {
            let app = a_cancel.clone();
            cx.defer(move |cx| close(&app, cx));
        });
        let error_line = p.error.as_ref().map(|e| {
            div()
                .px(px(20.))
                .pb(px(8.))
                .text_size(px(12.5))
                .text_color(rgba_of(t.warning))
                .child(SharedString::from(daemon_text::error_text(
                    e.code, &e.message,
                )))
        });
        let footer = div()
            .px(px(20.))
            .py(px(14.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(12.))
            .border_t_1()
            .border_color(rgba_of(t.row_divider()))
            .child(
                label_fill(tr!("Runs in the background"))
                    .min_w(px(120.))
                    .flex_1()
                    .text_size(px(13.))
                    .text_color(rgba_of(t.mist)),
            )
            .child(cancel)
            .child(go);

        let body = div()
            .id("ex-body")
            .px(px(20.))
            .pt(px(12.))
            .pb(px(16.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .overflow_y_scroll()
            .min_h(px(0.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(label_text(&t, tr!("Format")))
                    .child(formats)
                    .children(vp9),
            )
            .child(middle)
            .child(size_block(&t, &p));

        let panel = div()
            .id("export-popover")
            .role(Role::Dialog)
            .aria_label(SharedString::from(tr_ctx!("export popover", "Export")))
            .w(px(WIDTH))
            .max_w_full()
            .max_h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(scale::RADIUS_XL))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface))
            .text_color(rgba_of(t.text))
            // A press inside is not a press on the backdrop.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(header)
            .child(body)
            .children(error_line)
            .child(footer);

        let a_esc = app.clone();
        let a_back = app.clone();
        let root = div()
            .id("export-popover-layer")
            .track_focus(&self.focus)
            .on_key_down(move |ev: &KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" {
                    let app = a_esc.clone();
                    cx.defer(move |cx| close(&app, cx));
                }
            })
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                let app = a_back.clone();
                cx.defer(move |cx| close(&app, cx));
            });
        if narrow {
            // A centred dialog over the scrim, the same content.
            root.p(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba_of(t.modal_scrim))
                .child(panel)
                .into_any_element()
        } else {
            // Under the page's toolbar, at the right edge where the button sits; the page behind
            // stays as it is (no scrim).
            root.child(div().absolute().top(px(TOP)).right(px(24.)).child(panel))
                .into_any_element()
        }
    }
}
