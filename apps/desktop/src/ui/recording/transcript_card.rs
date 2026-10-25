//! The Transcript card of a recording's details page (redesign board R2): the title with the
//! transcript summary, Open captions, the joined Download TXT / SRT / VTT group, Re-generate and
//! its panel, and the three switch rows; or, when the recording has no transcript yet, the
//! Transcribe control and the gentle no-model hint (those states are not drawn on the board and
//! keep their behaviour, restyled with the same parts).
//!
//! `prefix` keeps the element ids apart (`rec-open-captions`, `rec-txt`, ...).

use gpui::{
    div, prelude::*, px, AnyElement, ClickEvent, Div, ElementId, Entity, FontWeight, SharedString,
    Stateful,
};

use cm_i18n::{tr, tr_ctx};

use crate::ai::client::CaptionFormat;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::ai;
use crate::ui::app::AppModel;
use crate::ui::components::{button, icon, icon_size, switch, ButtonKind};
use crate::ui::kit::{callout, Tone};
use crate::ui::paint::rgba_of;
use crate::ui::settings::widgets::{disabled_button, segment, segmented, small_button_with_icon};

/// The hint under "Timestamps in TXT": what the switch changes in the file (`[mm:ss]` before every
/// line, see `ai::client`).
pub fn txt_times_hint() -> String {
    tr!("Adds a [mm:ss] start time before every line of the TXT file.")
}

/// One segment of the Download group: the visible label is what the user reads; the aria label
/// keeps the name the button always had.
pub fn download_segments() -> [(&'static str, &'static str, String, CaptionFormat); 3] {
    [
        ("txt", "TXT", tr!("Download TXT"), CaptionFormat::Txt),
        ("srt", "SRT", "SRT".to_owned(), CaptionFormat::Srt),
        ("vtt", "VTT", "VTT".to_owned(), CaptionFormat::Vtt),
    ]
}

/// A switch row of the card: the label over a hint, the switch at the right, a rule above all but
/// the first.
fn switch_row(t: &Theme, label: &str, hint: &str, control: AnyElement, first: bool) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.))
        .py(px(10.))
        .when(!first, |d| d.border_t_1().border_color(rgba_of(t.border)))
        .child(
            div()
                .min_w(px(0.))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(type_scale::UI))
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(label.to_owned())),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(rgba_of(t.mist))
                        .child(SharedString::from(hint.to_owned())),
                ),
        )
        .child(control)
}

/// The card's title row: the icon, "Transcript", and the summary at the right.
fn header(t: &Theme, summary: Option<String>) -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(10.))
        .child(icon("audio-lines", 18., t.text))
        .child(
            div()
                .mr_auto()
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(tr_ctx!("card title", "Transcript")),
        )
        .children(summary.map(|s| {
            div()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.mist))
                .child(SharedString::from(s))
        }))
}

/// One button of the joined Download group.
fn download_button(
    t: &Theme,
    id: impl Into<ElementId>,
    label: &'static str,
    aria: String,
) -> Stateful<Div> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(SharedString::from(aria))
        .focusable()
        .tab_stop(true)
        .h(px(32.))
        .px(px(12.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .border_l_1()
        .border_color(rgba_of(t.line))
        .text_size(px(type_scale::CAPTION))
        .text_color(rgba_of(t.text_2))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .focus_visible(move |s| s.bg(focus).text_color(rgba_of(Theme::DARK.ink)))
        .child(label)
}

/// The Transcript card's content (the caller wraps it in a card): the stored transcript with its
/// actions and switches; or Transcribe and the no-model hint when there is none.
pub fn transcript_section(
    t: &Theme,
    app: &Entity<AppModel>,
    capture_id: &str,
    prefix: &'static str,
    cx: &gpui::App,
) -> AnyElement {
    use crate::ai::summary::{regen_warning, summary_line, TranscriptState};
    let id_of = |what: &str| SharedString::from(format!("{prefix}-{what}"));
    let (state, running, error, draft, installed, txt_stamps) = {
        let m = app.read(cx);
        (
            m.transcribe.transcripts.state(capture_id).cloned(),
            m.transcribe.status_text(capture_id),
            m.transcribe.error_of(capture_id),
            m.transcribe.regen_of(capture_id).cloned(),
            m.ai.installed_models()
                .into_iter()
                .map(|i| (i.id.clone(), i.label.clone(), i.active))
                .collect::<Vec<_>>(),
            m.transcribe.txt_timestamps,
        )
    };
    let info = match &state {
        Some(TranscriptState::Present(i)) => Some(i.clone()),
        _ => None,
    };
    let Some(info) = info else {
        let note = match state {
            None | Some(TranscriptState::Loading) => {
                Some((false, tr!("Checking for a transcript\u{2026}")))
            }
            Some(TranscriptState::Failed(m)) => Some((
                true,
                tr!("Couldn\u{2019}t read the transcript: {m}", m = m.text()),
            )),
            _ => None,
        };
        return div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(header(t, None))
            .children(note.map(|(warn, text)| {
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(if warn { t.warning } else { t.mist }))
                    .child(SharedString::from(text))
            }))
            .child(crate::ui::ai_view::transcribe_row(
                t, app, capture_id, prefix, cx,
            ))
            .into_any_element();
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let cap = capture_id.to_owned();
    let defer_call =
        |app: &Entity<AppModel>, cap: &str, f: fn(&Entity<AppModel>, &str, &mut gpui::App)| {
            let (app, cap) = (app.clone(), cap.to_owned());
            move |_: &ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
                let (app, cap) = (app.clone(), cap.clone());
                cx.defer(move |cx| f(&app, &cap, cx));
            }
        };
    let download = |fmt: CaptionFormat| {
        let (app, cap) = (app.clone(), cap.clone());
        move |_: &ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
            let (app, cap) = (app.clone(), cap.clone());
            cx.defer(move |cx| ai::export_captions(&app, &cap, fmt, cx));
        }
    };
    let actions: AnyElement = if let Some(text) = &running {
        disabled_button(
            t,
            id_of("working"),
            text,
            &tr_ctx!("button reason", "Working"),
            false,
        )
        .into_any_element()
    } else {
        let mut group = div()
            .flex()
            .flex_none()
            .items_center()
            .overflow_hidden()
            .rounded(px(scale::RADIUS_SM))
            .border_1()
            .border_color(rgba_of(t.line))
            .child(
                div()
                    .h(px(32.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .child(icon("download", icon_size::SMALL, t.text_2)),
            );
        for (key, label, aria, fmt) in download_segments() {
            group =
                group.child(download_button(t, id_of(key), label, aria).on_click(download(fmt)));
        }
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(
                small_button_with_icon(
                    t,
                    id_of("open-captions"),
                    &tr!("Open captions"),
                    Some("captions"),
                )
                .on_click(defer_call(app, &cap, ai::open_captions)),
            )
            .child(group)
            .child(
                small_button_with_icon(t, id_of("regen"), &tr!("Re-generate"), Some("refresh-cw"))
                    .border_color(gpui::transparent_black())
                    .on_click(defer_call(app, &cap, ai::open_regen)),
            )
            .into_any_element()
    };

    // The Re-generate panel: which model and language, and what it replaces.
    let panel = draft.filter(|_| running.is_none()).map(|d| {
        let mut models = segmented(t, id_of("regen-models"), &tr!("Model for this run"));
        if installed.is_empty() {
            models = models.child(
                div()
                    .px(px(8.))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(tr!("No model installed")),
            );
        }
        for (mid, label, active) in &installed {
            let picked = d.model.as_deref().map_or(*active, |m| m == mid);
            let (app, cap, mid2) = (app.clone(), cap.clone(), mid.clone());
            models = models.child(
                segment(t, id_of(&format!("regen-model-{mid}")), label, picked)
                    .px(px(10.))
                    .on_click(move |_, _, cx| {
                        let (app, cap, m) = (app.clone(), cap.clone(), mid2.clone());
                        cx.defer(move |cx| ai::set_regen_model(&app, &cap, Some(m), cx));
                    }),
            );
        }
        let mut langs = segmented(t, id_of("regen-langs"), &tr_ctx!("field", "Language"));
        for (code, english) in crate::ai::summary::LANGUAGES {
            let label =
                crate::ai::summary::language_name(code).unwrap_or_else(|| english.to_string());
            let (app, cap) = (app.clone(), cap.clone());
            langs = langs.child(
                segment(
                    t,
                    id_of(&format!("regen-lang-{code}")),
                    &label,
                    d.language == *code,
                )
                .px(px(10.))
                .on_click(move |_, _, cx| {
                    let (app, cap) = (app.clone(), cap.clone());
                    cx.defer(move |cx| ai::set_regen_language(&app, &cap, code, cx));
                }),
            );
        }
        let (start_app, start_cap) = (app.clone(), cap.clone());
        let (cancel_app, cancel_cap) = (app.clone(), cap.clone());
        let warning: AnyElement = if info.edited {
            // Discarding the user's caption edits: a brass callout.
            callout(
                t,
                Tone::Brass,
                "triangle-alert",
                regen_warning(true),
                None,
                None,
            )
            .into_any_element()
        } else {
            div()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.mist))
                .child(regen_warning(false))
                .into_any_element()
        };
        let label = |text: String| {
            div()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.mist))
                .child(SharedString::from(text))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(12.))
            .rounded(px(12.))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.field))
            .child(warning)
            .child(label(tr_ctx!("field", "Model")))
            .child(models)
            .child(label(tr_ctx!("field", "Language")))
            .child(langs)
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        button(
                            t,
                            id_of("regen-start"),
                            tr!("Re-generate"),
                            ButtonKind::Primary,
                        )
                        .on_click(move |_, _, cx| {
                            let (app, cap) = (start_app.clone(), start_cap.clone());
                            cx.defer(move |cx| ai::start_regen(&app, &cap, cx));
                        }),
                    )
                    .child(
                        small_button_with_icon(
                            t,
                            id_of("regen-cancel"),
                            &tr_ctx!("button", "Cancel"),
                            None,
                        )
                        .on_click(move |_, _, cx| {
                            let (app, cap) = (cancel_app.clone(), cancel_cap.clone());
                            cx.defer(move |cx| ai::close_regen(&app, &cap, cx));
                        }),
                    ),
            )
    });

    let stamps_app = app.clone();
    let (show_app, show_cap) = (app.clone(), cap.clone());
    let (burn_app, burn_cap) = (app.clone(), cap.clone());
    let options = info.options;
    div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(header(t, Some(summary_line(&info, now))))
        .child(actions)
        .children(panel)
        .child(
            div()
                .flex()
                .flex_col()
                .child(switch_row(
                    t,
                    &tr!("Timestamps in TXT"),
                    &txt_times_hint(),
                    switch(
                        t,
                        id_of("txt-times"),
                        tr!("Timestamps in the TXT file"),
                        txt_stamps,
                        true,
                    )
                    .on_click(move |_, _, cx| {
                        let app = stamps_app.clone();
                        cx.defer(move |cx| ai::set_txt_timestamps(&app, !txt_stamps, cx));
                    })
                    .into_any_element(),
                    true,
                ))
                .child(switch_row(
                    t,
                    &tr!("Show captions in the preview"),
                    &tr!("Drawn on the video in the editor."),
                    switch(
                        t,
                        id_of("show-captions"),
                        tr!("Show captions in the preview"),
                        options.enabled,
                        true,
                    )
                    .on_click(move |_, _, cx| {
                        let (app, cap) = (show_app.clone(), show_cap.clone());
                        cx.defer(move |cx| {
                            ai::set_caption_options(&app, &cap, Some(!options.enabled), None, cx)
                        });
                    })
                    .into_any_element(),
                    false,
                ))
                .child(switch_row(
                    t,
                    &tr!("Include in exported video"),
                    &tr!("Burned into new exports unless you turn it off when exporting."),
                    switch(
                        t,
                        id_of("burn-captions"),
                        tr!("Include captions in the exported video"),
                        options.burn_in,
                        true,
                    )
                    .on_click(move |_, _, cx| {
                        let (app, cap) = (burn_app.clone(), burn_cap.clone());
                        cx.defer(move |cx| {
                            ai::set_caption_options(&app, &cap, None, Some(!options.burn_in), cx)
                        });
                    })
                    .into_any_element(),
                    false,
                )),
        )
        .children(error.map(|e| {
            div()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.warning))
                .child(SharedString::from(e))
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Download group keeps the aria names the buttons had: "Download TXT", "SRT", "VTT".
    #[test]
    fn download_group_keeps_its_aria_labels() {
        let s = download_segments();
        assert_eq!(s.clone().map(|x| x.0), ["txt", "srt", "vtt"]);
        assert_eq!(s[1].3, CaptionFormat::Srt);
        assert_eq!(s.clone().map(|x| x.1), ["TXT", "SRT", "VTT"]);
        assert_eq!(s.map(|x| x.2), ["Download TXT", "SRT", "VTT"]);
    }

    /// The timestamps hint states what the export does (a `[mm:ss]` stamp per line).
    #[test]
    fn timestamps_hint_states_the_format() {
        assert!(txt_times_hint().contains("[mm:ss]"));
    }
}
