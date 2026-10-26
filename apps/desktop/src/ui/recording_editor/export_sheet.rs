//! The Export audio sheet (VOC-2, redesign V7): format cards (M4A, WAV), tracks (Mix, Mic only,
//! System only), the loudness switch, the tags line, "Also save a copy to" a folder, and the
//! footer with the length after the cuts and the size estimate. The choices and the call they make
//! are in `crate::recedit::audio_export` (unit tested); the job is the daemon's `export_audio`,
//! and its progress and result come back as `export.*` events (toasts with Show in Finder and
//! Upload). Not seen by eye yet (`tests/manual/VOC-2.md`).

use std::sync::Mutex;

use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, prelude::*, px, AnyElement, Context, FontWeight, MouseButton, Role, SharedString, Stateful,
};

use super::tabs::icon_only;
use super::{Load, RecordingEditor};
use crate::recedit::audio_export::{self, size_text, Available, Form, Format, Tracks};
use crate::recedit::voice::clock;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::components::{button_with_icon, icon, switch, ButtonKind};
use crate::ui::paint::rgba_of;

/// The folder of the last "Also save a copy to" in this run (the sheet starts with it).
static LAST_COPY: Mutex<Option<String>> = Mutex::new(None);

pub(super) fn last_copy_dir() -> Option<String> {
    LAST_COPY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

fn remember_copy_dir(dir: Option<String>) {
    *LAST_COPY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = dir;
}

/// The open sheet.
pub(super) struct Sheet {
    pub form: Form,
    pub avail: Available,
    /// The call is on its way.
    pub busy: bool,
    /// Why the call was refused.
    pub error: Option<String>,
}

impl Sheet {
    pub(super) fn new(form: Form, avail: Available) -> Sheet {
        let mut sheet = Sheet {
            form,
            avail,
            busy: false,
            error: None,
        };
        // A recording with only one track exports that one by default.
        if !avail.allows(sheet.form.tracks) {
            sheet.form.tracks = Tracks::Mix;
        }
        sheet
    }
}

/// A choice card: glyph and name over a muted line; the chosen one has the strong border.
fn card(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    glyph: &'static str,
    name: String,
    text: String,
    active: bool,
    enabled: bool,
) -> Stateful<gpui::Div> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .id(id)
        .role(Role::RadioButton)
        .aria_label(SharedString::from(name.clone()))
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
        .gap(px(3.))
        .px(px(12.))
        .py(px(11.))
        .rounded(px(12.))
        .border_1()
        .border_color(rgba_of(if active { t.line } else { t.border }))
        .bg(rgba_of(if active { t.raised } else { t.surface_2 }))
        .when(enabled, |d| {
            d.cursor_pointer()
                .when(!active, |d| d.hover(move |s| s.bg(hover)))
        })
        .when(!enabled, |d| d.opacity(0.45))
        .focus_visible(move |s| s.border_color(focus))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(7.))
                .text_size(px(13.5))
                .font_weight(FontWeight::SEMIBOLD)
                .child(icon(glyph, 14., if active { t.text } else { t.mist }))
                .child(crate::ui::components::shrink_text(SharedString::from(name))),
        )
        .child(
            div()
                .text_size(px(11.5))
                .line_height(px(16.))
                .text_color(rgba_of(t.mist))
                .child(SharedString::from(text)),
        )
}

/// A row of the settings group: a glyph, a label over a muted line, and the control.
fn row(
    t: &Theme,
    first: bool,
    glyph: &'static str,
    label: String,
    sub: String,
    control: Option<AnyElement>,
) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(14.))
        .py(px(11.))
        .when(!first, |d| {
            d.border_t_1().border_color(rgba_of(t.row_divider()))
        })
        .child(icon(glyph, 15., t.text_2))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(label)),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgba_of(t.mist))
                        .child(SharedString::from(sub)),
                ),
        )
        .children(control)
}

/// `Oct 26, 2026` for the tags line.
fn date_text(created: Option<time::OffsetDateTime>) -> String {
    let Some(c) = created else {
        return String::new();
    };
    let local = c.to_offset(crate::save_folder::local_now().offset());
    tr!(
        "{month} {day}, {year}",
        month = cm_i18n::fmt::month_short(cm_i18n::language(), u8::from(local.month())),
        day = local.day(),
        year = local.year()
    )
}

impl RecordingEditor {
    /// The sheet over the editor, when open (the scrim closes it).
    pub(super) fn export_sheet_overlay(
        &self,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sheet = self.voice_ui.sheet.as_ref()?;
        let Load::Ready(s) = &self.load else {
            return None;
        };
        let form = &sheet.form;
        let avail = sheet.avail;
        let secs = s.model.output_duration().as_secs_f64();
        let estimate = form.estimate_bytes(secs, avail);
        let home = std::env::var("HOME").ok();

        let format_cards = div()
            .id("re-xs-format")
            .role(Role::RadioGroup)
            .aria_label(tr!("Format"))
            .flex()
            .gap(px(8.))
            .child(
                card(
                    t,
                    "re-xs-m4a",
                    "file-headphone",
                    Format::M4a.label().to_owned(),
                    tr!("AAC, 48 kHz \u{00B7} 128 kbps mono or 192 kbps stereo. Plays everywhere."),
                    form.format == Format::M4a,
                    true,
                )
                .on_click(cx.listener(|this, _, _, cx| this.sheet_set_format(Format::M4a, cx))),
            )
            .child(
                card(
                    t,
                    "re-xs-wav",
                    "audio-lines",
                    Format::Wav.label().to_owned(),
                    tr!("48 kHz \u{00B7} 24-bit. Lossless, about 10\u{00D7} larger."),
                    form.format == Format::Wav,
                    true,
                )
                .on_click(cx.listener(|this, _, _, cx| this.sheet_set_format(Format::Wav, cx))),
            );

        let track_card = |tr_: Tracks,
                          id: &'static str,
                          glyph: &'static str,
                          name: String,
                          text: String,
                          cx: &mut Context<Self>| {
            let enabled = avail.allows(tr_);
            let c = card(t, id, glyph, name, text, form.tracks == tr_, enabled);
            if enabled {
                c.on_click(cx.listener(move |this, _, _, cx| this.sheet_set_tracks(tr_, cx)))
            } else {
                c
            }
        };
        let tracks = div()
            .id("re-xs-tracks")
            .role(Role::RadioGroup)
            .aria_label(tr!("Tracks"))
            .flex()
            .gap(px(8.))
            .child(track_card(
                Tracks::Mix,
                "re-xs-mix",
                "venn",
                tr_ctx!("export tracks", "Mix"),
                tr!("Microphone and system audio together"),
                cx,
            ))
            .child(track_card(
                Tracks::Mic,
                "re-xs-mic",
                "mic",
                tr_ctx!("export tracks", "Mic only"),
                tr!("Just your microphone"),
                cx,
            ))
            .child(track_card(
                Tracks::System,
                "re-xs-system",
                "volume-2",
                tr_ctx!("export tracks", "System only"),
                tr!("Just the call or app sound"),
                cx,
            ));

        let loudness = switch(
            t,
            "re-xs-loudness",
            tr!("Even out the loudness"),
            form.loudness,
            true,
        )
        .on_click(cx.listener(|this, _, _, cx| {
            if let Some(sh) = this.voice_ui.sheet.as_mut() {
                sh.form.toggle_loudness();
            }
            cx.notify();
        }));
        let folder_text = form
            .copy_dir
            .as_deref()
            .map(|d| audio_export::tilde(d, home.as_deref()))
            .unwrap_or_else(|| tr!("Choose a folder\u{2026}"));
        let folder = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .id("re-xs-folder")
                    .role(Role::Button)
                    .aria_label(tr!("Choose the folder for the copy"))
                    .focusable()
                    .tab_stop(true)
                    .h(px(28.))
                    .max_w(px(220.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.surface_2))
                    .text_size(px(12.5))
                    .text_color(rgba_of(t.text_2))
                    .cursor_pointer()
                    .child(
                        div()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(SharedString::from(folder_text)),
                    )
                    .child(icon("arrow-down", 12., t.mist))
                    .on_click(cx.listener(|this, _, _, cx| this.sheet_choose_folder(cx))),
            )
            .children(form.copy_dir.is_some().then(|| {
                icon_only(
                    t,
                    "re-xs-folder-clear",
                    "x",
                    tr!("Don\u{2019}t save a copy"),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(sh) = this.voice_ui.sheet.as_mut() {
                        sh.form.copy_dir = None;
                    }
                    cx.notify();
                }))
            }));
        let tags = format!("{} \u{00B7} {}", s.title, date_text(s.voice.created));
        let settings = div()
            .flex()
            .flex_col()
            .rounded(px(12.))
            .border_1()
            .border_color(rgba_of(t.border))
            .overflow_hidden()
            .child(row(
                t,
                true,
                "gauge",
                tr!("Even out the loudness"),
                tr!(
                    "Normalise to {lufs} LUFS, the level podcasts and players expect",
                    lufs = "\u{2212}16"
                ),
                Some(loudness.into_any_element()),
            ))
            .child(row(
                t,
                false,
                "type",
                tr!("Title and date are written into the file"),
                tags,
                None,
            ))
            .child(row(
                t,
                false,
                "folder",
                tr!("Also save a copy to"),
                tr!("Always kept in the recording\u{2019}s exports folder too"),
                Some(folder.into_any_element()),
            ));

        let label = |text: String| {
            div()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.mist))
                .child(SharedString::from(text))
        };
        let header = div()
            .flex()
            .items_center()
            .gap(px(14.))
            .px(px(22.))
            .pt(px(18.))
            .pb(px(8.))
            .child(
                div()
                    .size(px(40.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12.))
                    .bg(rgba_of(t.raised))
                    .child(icon("file-headphone", 20., t.text)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .id("re-xs-title")
                            .role(Role::Heading)
                            .aria_level(2)
                            .aria_label(tr!("Export audio"))
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr!("Export audio")),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgba_of(t.mist))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(SharedString::from(tr!(
                                "{title} \u{00B7} cuts and speed applied",
                                title = s.title
                            ))),
                    ),
            )
            .child(
                icon_only(t, "re-xs-close", "x", tr!("Close")).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.voice_ui.sheet = None;
                        cx.notify();
                    },
                )),
            );
        let body = div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .px(px(22.))
            .pt(px(12.))
            .pb(px(18.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(label(tr!("Format")))
                    .child(format_cards),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(label(tr!("Tracks")))
                    .child(tracks),
            )
            .child(settings);

        let busy = sheet.busy;
        let go = button_with_icon(
            t,
            "re-xs-go",
            tr!("Export {format}", format = form.format.label()),
            ButtonKind::Primary,
            Some("download"),
        )
        .when(busy, |d| d.opacity(0.6))
        .on_click(cx.listener(|this, _, _, cx| this.sheet_start(cx)));
        let cancel = button_with_icon(
            t,
            "re-xs-cancel",
            tr_ctx!("button", "Cancel"),
            ButtonKind::Secondary,
            None,
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.voice_ui.sheet = None;
            cx.notify();
        }));
        let footer = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(12.))
            .px(px(22.))
            .py(px(16.))
            .border_t_1()
            .border_color(rgba_of(t.row_divider()))
            .child(
                div()
                    .flex_1()
                    .min_w(px(180.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().text_size(px(13.)).text_color(rgba_of(t.mist)).child(
                        SharedString::from(tr!(
                            "{time} after cuts \u{00B7} about {size}",
                            time = clock(s.model.output_duration()),
                            size = size_text(estimate)
                        )),
                    ))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgba_of(if sheet.error.is_some() {
                                t.warning
                            } else {
                                t.mist
                            }))
                            .child(SharedString::from(sheet.error.clone().unwrap_or_else(
                                || tr!("Runs in the background; you can keep working"),
                            ))),
                    ),
            )
            .child(cancel)
            .child(go);

        let dialog = div()
            .id("re-export-sheet")
            .role(Role::Dialog)
            .aria_label(tr!("Export audio"))
            .w(px(580.))
            .max_w_full()
            .max_h_full()
            .overflow_hidden()
            .flex()
            .flex_col()
            .rounded(px(scale::RADIUS_DIALOG_LG))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface))
            // A press inside the sheet is not a press on the scrim.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(header)
            .child(body)
            .child(footer);
        Some(
            div()
                .id("re-export-scrim")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .p(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba_of(t.modal_scrim))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.voice_ui.sheet = None;
                        cx.notify();
                    }),
                )
                .child(dialog)
                .into_any_element(),
        )
    }

    fn sheet_set_format(&mut self, f: Format, cx: &mut Context<Self>) {
        if let Some(sh) = self.voice_ui.sheet.as_mut() {
            sh.form.set_format(f);
        }
        cx.notify();
    }

    fn sheet_set_tracks(&mut self, tr_: Tracks, cx: &mut Context<Self>) {
        if let Some(sh) = self.voice_ui.sheet.as_mut() {
            let avail = sh.avail;
            sh.form.set_tracks(tr_, avail);
        }
        cx.notify();
    }

    /// "Also save a copy to": the system folder picker.
    fn sheet_choose_folder(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr!("Choose a folder").into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update(cx, |this, cx| {
                if let Some(sh) = this.voice_ui.sheet.as_mut() {
                    sh.form.copy_dir = Some(path.display().to_string());
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Export M4A or WAV: `export_audio` as the user. The sheet closes when the daemon accepted
    /// the job; a refusal (no audio, nothing left after the cuts, a folder that is gone) stays in
    /// the sheet.
    fn sheet_start(&mut self, cx: &mut Context<Self>) {
        let (Some(id), Some(client)) = (self.id.clone(), crate::ui::ai::client_of(&self.app, cx))
        else {
            return;
        };
        let Some(sh) = self.voice_ui.sheet.as_mut() else {
            return;
        };
        if sh.busy {
            return;
        }
        sh.busy = true;
        sh.error = None;
        let params = sh.form.params(&id);
        let copy = sh.form.copy_dir.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = client.export_audio(params).await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(_) => {
                        remember_copy_dir(copy);
                        this.voice_ui.sheet = None;
                    }
                    Err(e) => {
                        if let Some(sh) = this.voice_ui.sheet.as_mut() {
                            sh.busy = false;
                            sh.error = Some(crate::daemon_text::any_error_text(&e.message));
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
