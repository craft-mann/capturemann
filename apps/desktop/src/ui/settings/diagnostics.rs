//! Settings -> About: the Diagnostics card (DIAG-1). "Show diagnostics" asks the daemon for
//! `get_diagnostics` and shows the text in a box the user can read before copying it; "Copy" puts it
//! on the clipboard and "Reveal log folder" opens the folder of the log files. Nothing is sent
//! anywhere: the text only goes where the user pastes it.

use cm_i18n::{tr, tr_ctx};
use gpui::{div, prelude::*, px, AnyElement, Context, SharedString};
use serde_json::json;

use super::widgets::{caption, small_button_with_icon};
use super::SettingsView;
use crate::backend::Backend;
use crate::theme::{type_scale, Theme};
use crate::ui::paint::{rgba_of, TextExt};

/// What the card shows.
#[derive(Default)]
pub enum DiagState {
    /// Nothing asked yet: the page reads nothing until the button is pressed.
    #[default]
    Idle,
    Loading,
    Ready(String),
    Failed(String),
}

impl SettingsView {
    /// Reads the report from the daemon, off the UI thread.
    pub(super) fn load_diagnostics(&mut self, cx: &mut Context<Self>) {
        let (rt, daemon) = {
            let b = &self.app.read(cx).backend;
            (b.rt.clone(), b.daemon.clone())
        };
        let Some(daemon) = daemon else {
            self.diag = DiagState::Failed(tr!("The CaptureMann engine is not running."));
            cx.notify();
            return;
        };
        self.diag = DiagState::Loading;
        cx.notify();
        let work =
            rt.spawn(async move { Backend::call(&daemon, "get_diagnostics", json!({})).await });
        self._diag_task = Some(cx.spawn(async move |this, cx| {
            let res = work.await;
            this.update(cx, |this, cx| {
                this.diag = match res {
                    Ok(Ok(v)) => {
                        DiagState::Ready(v["text"].as_str().unwrap_or_default().to_owned())
                    }
                    Ok(Err(e)) => DiagState::Failed(e),
                    Err(e) => DiagState::Failed(e.to_string()),
                };
                cx.notify();
            })
            .ok();
        }));
    }

    pub(super) fn diagnostics_card(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let show = small_button_with_icon(
            t,
            "diag-show",
            &match self.diag {
                DiagState::Ready(_) => tr_ctx!("button", "Refresh"),
                _ => tr_ctx!("button", "Show diagnostics"),
            },
            Some("refresh-cw"),
        )
        .on_click(cx.listener(|this, _, _, cx| this.load_diagnostics(cx)));
        let reveal = small_button_with_icon(
            t,
            "diag-reveal-logs",
            &tr_ctx!("button", "Reveal log folder"),
            Some("folder-open"),
        )
        .on_click(cx.listener(|_, _, _, cx| {
            let dir = cm_daemon::logs::log_dir();
            // The folder exists once the app has logged; before that revealing it fails quietly.
            let _ = std::fs::create_dir_all(&dir);
            cx.reveal_path(&dir);
        }));
        let mut buttons = div().flex().flex_wrap().gap(px(8.)).child(show);
        if let DiagState::Ready(text) = &self.diag {
            let text = text.clone();
            let copied = self.copied;
            buttons = buttons.child(
                small_button_with_icon(
                    t,
                    "diag-copy",
                    &if copied {
                        tr_ctx!("button", "Copied")
                    } else {
                        tr_ctx!("button", "Copy")
                    },
                    Some(if copied { "check" } else { "copy" }),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.copy_text(text.clone(), cx))),
            );
        }
        buttons = buttons.child(reveal);

        let body: Option<AnyElement> = match &self.diag {
            DiagState::Idle => None,
            DiagState::Loading => Some(caption(t, tr!("Reading\u{2026}")).into_any_element()),
            DiagState::Failed(e) => Some(
                caption(t, tr!("Couldn\u{2019}t read the diagnostics: {e}", e = e))
                    .into_any_element(),
            ),
            DiagState::Ready(text) => Some(
                div()
                    .id("diag-text")
                    .mono()
                    .max_h(px(260.))
                    .overflow_y_scroll()
                    .p(px(10.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.field))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.text_2))
                    .child(SharedString::from(text.clone()))
                    .into_any_element(),
            ),
        };

        div()
            .id("diagnostics-card")
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(16.))
            .rounded(px(14.))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .child(
                div()
                    .text_size(px(type_scale::BODY))
                    .text_color(rgba_of(t.text))
                    .child(tr!("Diagnostics")),
            )
            .child(caption(
                t,
                // TRANSLATORS: FACT: owner review required (privacy)
                tr!("When something goes wrong, copy this and paste it into a bug report. It has the version, your Mac, permissions, models, a few settings and the last warnings. It has no audio, no transcripts and no file names. Nothing is sent: check it, then paste it yourself."),
            ))
            .child(buttons)
            .children(body)
            .into_any_element()
    }
}
