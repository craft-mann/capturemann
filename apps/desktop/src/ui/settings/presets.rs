//! Settings -> Capture & recording -> Presets (QC-4): the presets in order, each with its summary
//! and shortcut, Move up, Move down, Edit and Delete, and a New preset button. Every change is a
//! preset action (`reorder_presets`, `delete_preset`, `save_preset`); the file is `presets.json`.

use cm_i18n::{tr, tr_ctx};
use gpui::{div, prelude::*, px, AnyElement, Context, Role, SharedString};
use serde_json::json;

use super::widgets::{caption, card_row, group_with_card, list_card, small_button_with_icon};
use super::SettingsView;
use crate::quick::presets::{chord_of, summary_text};
use crate::theme::{type_scale, Theme};
use crate::ui::components::{icon_button, keycaps};
use crate::ui::paint::rgba_of;

impl SettingsView {
    /// The Presets group of the Capture page.
    pub(super) fn presets_group(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let (presets, notice, platform, armed) = {
            let m = self.app.read(cx);
            (
                m.quick.presets.clone(),
                m.quick.notice.clone(),
                m.platform,
                m.quick.delete_armed.clone(),
            )
        };
        let last = presets.len().saturating_sub(1);
        let ids: Vec<String> = presets.iter().map(|p| p.id.clone()).collect();
        let rows = presets.iter().cloned().enumerate().map(|(i, p)| {
            let name = p.name.clone();
            let summary = summary_text(&p);
            let (app_up, app_down, app_edit, app_del) = (
                self.app.clone(),
                self.app.clone(),
                self.app.clone(),
                self.app.clone(),
            );
            let (ids_up, ids_down) = (ids.clone(), ids.clone());
            let p_edit = p.clone();
            let del_id = p.id.clone();
            let is_armed = armed.as_deref() == Some(p.id.as_str());
            let mover = |id: String, glyph: &'static str, aria: String, enabled: bool| {
                icon_button(t, SharedString::from(id), glyph, aria, false, None)
                    .size(px(32.))
                    .when(!enabled, |d| d.opacity(0.35))
            };
            card_row(t, i == last)
                .id(SharedString::from(format!("preset-row-{i}")))
                .role(Role::ListItem)
                .aria_label(SharedString::from(tr_ctx!(
                    "preset row",
                    "{name}, {summary}",
                    name = name.clone(),
                    summary = summary.clone()
                )))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(160.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(type_scale::UI))
                                .text_color(rgba_of(t.text))
                                .child(SharedString::from(name.clone())),
                        )
                        .child(caption(t, summary)),
                )
                .children(chord_of(&p).map(|c| keycaps(t, &c.keycaps(platform)).flex_none()))
                .child(
                    mover(
                        format!("preset-up-{i}"),
                        "arrow-up",
                        tr_ctx!("preset row", "Move {name} up", name = name.clone()),
                        i > 0,
                    )
                    .on_click(move |_, _, cx| {
                        if i == 0 {
                            return;
                        }
                        let mut order = ids_up.clone();
                        order.swap(i, i - 1);
                        crate::ui::quick_flow::preset_call(
                            &app_up,
                            "reorder_presets",
                            json!({ "ids": order }),
                            |_, _, _| {},
                            cx,
                        );
                    }),
                )
                .child(
                    mover(
                        format!("preset-down-{i}"),
                        "arrow-down",
                        tr_ctx!("preset row", "Move {name} down", name = name.clone()),
                        i < last,
                    )
                    .on_click(move |_, _, cx| {
                        if i >= last {
                            return;
                        }
                        let mut order = ids_down.clone();
                        order.swap(i, i + 1);
                        crate::ui::quick_flow::preset_call(
                            &app_down,
                            "reorder_presets",
                            json!({ "ids": order }),
                            |_, _, _| {},
                            cx,
                        );
                    }),
                )
                .child(
                    small_button_with_icon(
                        t,
                        SharedString::from(format!("preset-edit-{i}")),
                        &tr_ctx!("button", "Edit"),
                        Some("pencil"),
                    )
                    .aria_label(SharedString::from(tr_ctx!(
                        "preset row",
                        "Edit {name}",
                        name = name.clone()
                    )))
                    .on_click(move |_, _, cx| {
                        crate::ui::quick_sheet::open(&app_edit, Some(p_edit.clone()), cx)
                    }),
                )
                .child(
                    small_button_with_icon(
                        t,
                        SharedString::from(format!("preset-delete-{i}")),
                        &if is_armed {
                            tr_ctx!("button", "Delete for good")
                        } else {
                            tr_ctx!("button", "Delete")
                        },
                        Some("trash-2"),
                    )
                    .aria_label(SharedString::from(tr_ctx!(
                        "preset row",
                        "Delete {name}",
                        name = name
                    )))
                    .on_click(move |_, _, cx| {
                        if !is_armed {
                            // A second click confirms: nothing is asked in a dialog.
                            app_del.update(cx, |m, cx| {
                                m.quick.delete_armed = Some(del_id.clone());
                                cx.notify();
                            });
                            return;
                        }
                        app_del.update(cx, |m, _| m.quick.delete_armed = None);
                        crate::ui::quick_flow::preset_call(
                            &app_del,
                            "delete_preset",
                            json!({ "preset": del_id }),
                            |_, _, _| {},
                            cx,
                        );
                    }),
                )
        });
        let app_new = self.app.clone();
        let card = list_card(t)
            .id("presets-list")
            .role(Role::List)
            .aria_label(SharedString::from(tr!("Presets")))
            .children(rows)
            .children(presets.is_empty().then(|| {
                card_row(t, true).child(caption(t, tr!("No presets yet. New preset makes one.")))
            }));
        let note = caption(
            t,
            tr!("Presets are kept in presets.json in the settings folder; copy the file to share them."),
        );
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(group_with_card(
                t,
                &tr_ctx!("settings group", "Presets"),
                Some(SharedString::from(tr!(
                    "One pick, or one shortcut, runs the setup"
                ))),
                card,
            ))
            .children(notice.map(|n| {
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.warning))
                    .child(SharedString::from(n))
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        small_button_with_icon(
                            t,
                            "preset-new",
                            &tr_ctx!("button", "New preset"),
                            Some("plus"),
                        )
                        .on_click(move |_, _, cx| crate::ui::quick_sheet::open(&app_new, None, cx)),
                    )
                    .child(div().flex_1().min_w(px(0.)).child(note)),
            )
            .into_any_element()
    }
}
