//! General page, Library group (board R11): the "Library folder" row with the path, the size used
//! and the free space, the Available pill and "Change...". While a change is on, the same row
//! shows the copy's progress, or what to do next (restart, the optional Trash step).
//!
//! The size of the library is measured off the UI thread and kept for 60 seconds
//! ([`crate::library_move::SizeCache`]); the row says "Calculating..." meanwhile. The words come
//! from `crate::library_move`; the actions are in `crate::ui::library_move_flow`.

use gpui::{div, prelude::*, px, AnyElement, Context, SharedString};
use std::path::PathBuf;
use std::time::Instant;

use super::rows::tile;
use super::widgets::{caption, group_with_card, list_card, settings_row};
use super::widgets::{small_button_with_icon, warning_caption};
use super::SettingsView;
use crate::library_move::{self as lm, Lookup, Phase};
use crate::theme::Theme;
use crate::ui::library_move_flow as flow;
use cm_i18n::{tr, tr_ctx};

impl SettingsView {
    /// The Library group of the General page. `pill` is the Available / Unavailable status;
    /// `note` the warning under the card while the folder is unavailable.
    pub(super) fn library_group(
        &self,
        t: &Theme,
        (title, row_label): (&str, &str),
        root: Option<PathBuf>,
        pill: AnyElement,
        note: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let phase = self.app.read(cx).library_move.clone();
        let busy = matches!(phase, Phase::Checking | Phase::Moving { .. });

        let row = match lm::phase_texts(&phase) {
            Some((label, hint)) => self.library_phase_row(t, &phase, label, hint),
            None => {
                let shown = root
                    .as_deref()
                    .map_or_else(|| tr!("Not set"), lm::shown_path);
                let lookup = match &root {
                    Some(r) => self.library_size(r, cx),
                    None => Lookup::Fresh(lm::Measured::Unknown),
                };
                let subtitle = if root.is_some() {
                    lm::row_subtitle(&shown, &lookup)
                } else {
                    shown
                };
                let app = self.app.clone();
                let change = small_button_with_icon(
                    t,
                    "library-change",
                    &tr_ctx!("button", "Change\u{2026}"),
                    Some("folder-open"),
                )
                .when(busy, |b| b.opacity(0.5))
                .on_click(move |_, _, cx| flow::change_folder(&app, cx));
                settings_row(
                    t,
                    Some(tile::LIBRARY),
                    row_label.to_owned(),
                    Some(SharedString::from(subtitle)),
                    Some(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(pill)
                            .child(change)
                            .into_any_element(),
                    ),
                    true,
                )
            }
        };
        group_with_card(t, title, None, list_card(t).child(row))
            .children(note.map(|n| warning_caption(t, n)))
            .child(caption(
                t,
                tr!("Changing the folder copies your captures and checks each one. Nothing is deleted from the old folder. CaptureMann uses the new folder after it restarts."),
            ))
            .into_any_element()
    }

    /// What the cache knows about the library's size; starts the walk when it knows nothing.
    fn library_size(&self, root: &std::path::Path, cx: &mut Context<Self>) -> Lookup {
        let lookup = self.lib_size.borrow().lookup(root, Instant::now());
        if lookup == Lookup::Start {
            self.lib_size.borrow_mut().begin(root);
            let root = root.to_path_buf();
            cx.spawn(async move |this, cx| {
                let r2 = root.clone();
                let m = cx
                    .background_executor()
                    .spawn(async move { lm::measure(&r2) })
                    .await;
                this.update(cx, |this, cx| {
                    this.lib_size.borrow_mut().finish(&root, m, Instant::now());
                    cx.notify();
                })
                .ok();
            })
            .detach();
            return Lookup::Calculating;
        }
        lookup
    }

    fn library_phase_row(
        &self,
        t: &Theme,
        phase: &Phase,
        label: String,
        hint: String,
    ) -> gpui::Div {
        let app = self.app.clone();
        let mut controls = div()
            .flex()
            .flex_wrap()
            .max_w(px(480.))
            .justify_end()
            .items_center()
            .gap(px(8.));
        match phase {
            Phase::Moving { fraction, .. } => {
                let a = app.clone();
                controls = controls
                    .child(div().w(px(140.)).child(crate::ui::ai_view::progress_bar(
                        t,
                        &label,
                        *fraction as f32,
                    )))
                    .child(
                        small_button_with_icon(
                            t,
                            "library-move-cancel",
                            &tr_ctx!("button", "Cancel"),
                            Some("x"),
                        )
                        .on_click(move |_, _, cx| flow::cancel(&a, cx)),
                    );
            }
            Phase::Failed { .. } => {
                let (a, b) = (app.clone(), app.clone());
                controls = controls
                    .child(
                        small_button_with_icon(
                            t,
                            "library-move-retry",
                            &tr_ctx!("button", "Retry"),
                            Some("rotate-cw"),
                        )
                        .on_click(move |_, _, cx| flow::retry(&a, cx)),
                    )
                    .child(
                        small_button_with_icon(
                            t,
                            "library-move-dismiss",
                            &tr_ctx!("button", "Dismiss"),
                            None,
                        )
                        .on_click(move |_, _, cx| flow::dismiss(&b, cx)),
                    );
            }
            Phase::Cancelled { .. } => {
                let a = app.clone();
                controls = controls.child(
                    small_button_with_icon(
                        t,
                        "library-move-dismiss",
                        &tr_ctx!("button", "Dismiss"),
                        None,
                    )
                    .on_click(move |_, _, cx| flow::dismiss(&a, cx)),
                );
            }
            Phase::Moved(info) => {
                let a = app.clone();
                controls = controls.child(
                    small_button_with_icon(
                        t,
                        "library-restart",
                        &tr!("Restart CaptureMann"),
                        Some("rotate-cw"),
                    )
                    .on_click(move |_, _, cx| {
                        crate::ui::app::restart(&a, cx);
                    }),
                );
                if !info.trashed && !info.copied.is_empty() {
                    let b = app.clone();
                    controls = controls.child(
                        small_button_with_icon(
                            t,
                            "library-trash-old",
                            &tr!("Move the old copies to the Trash\u{2026}"),
                            Some("trash-2"),
                        )
                        .on_click(move |_, _, cx| flow::ask_trash(&b, cx)),
                    );
                }
            }
            Phase::Switched { .. } => {
                let a = app.clone();
                controls = controls.child(
                    small_button_with_icon(
                        t,
                        "library-restart",
                        &tr!("Restart CaptureMann"),
                        Some("rotate-cw"),
                    )
                    .on_click(move |_, _, cx| {
                        crate::ui::app::restart(&a, cx);
                    }),
                );
            }
            Phase::Idle | Phase::Checking => {}
        }
        settings_row(
            t,
            Some(tile::LIBRARY),
            label,
            Some(SharedString::from(hint)),
            Some(controls.into_any_element()),
            true,
        )
    }
}
