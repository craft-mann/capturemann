//! Settings -> Shortcuts (redesign Phase 5a, board R16): keycap rows by group, one cap per key and
//! a "Change" button with the `pen-line` icon. The rows are the real actions (CAP-2, R4-12) with
//! the real chords; the rebind, conflict and register-failure states are kept.

use gpui::{div, prelude::*, px, AnyElement, Context, KeyDownEvent, Role, SharedString};

use super::model::{self, shortcut_groups};
use super::rows::shortcut_tile;
use super::widgets::{
    caption, group_with_card, list_card, settings_row, small_button_with_icon, warning_caption,
};
use super::SettingsView;
use crate::shortcuts::ShortcutAction;
use crate::theme::{type_scale, Theme};
use crate::ui::kit::keycap_lg;
use crate::ui::paint::rgba_of;
use cm_i18n::{tr, tr_ctx};

impl SettingsView {
    /// The board's "Reset to defaults" (R16, page header, right): enabled only when a shortcut
    /// differs from its default; asks first.
    pub(super) fn shortcuts_reset_control(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let (platform, map) = {
            let m = self.app.read(cx);
            (m.platform, m.shortcuts.clone())
        };
        let enabled = !super::reset::shortcut_reset_targets(&map, platform).is_empty();
        let app = self.app.clone();
        let b = super::reset::reset_button(t, enabled);
        if enabled {
            b.on_click(move |_, _, cx| super::reset::ask_reset_shortcuts(&app, cx))
                .into_any_element()
        } else {
            b.into_any_element()
        }
    }

    pub(super) fn shortcuts(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let (platform, map, reg) = {
            let m = self.app.read(cx);
            (m.platform, m.shortcuts.clone(), m.registration.clone())
        };
        let rows = model::shortcut_rows(&map, platform);
        // TODO(design): the board's rows "Skip the countdown", "Draw
        // on screen", "Show or hide the webcam bubble" and "Mute or unmute the microphone" are
        // not built: there is no such action, and D, T and A are Discard, Restart and Annotate.
        let groups =
            shortcut_groups()
                .into_iter()
                .enumerate()
                .map(|(gi, (title, note, actions))| {
                    let last = actions.len().saturating_sub(1);
                    let items = actions.into_iter().enumerate().filter_map(|(i, a)| {
                        let r = rows.iter().find(|r| r.action == a)?;
                        // CAP-2.AC3: "Couldn't register - used by another app" next to that shortcut.
                        let err = ShortcutAction::ALL
                            .iter()
                            .find(|x| x.key() == r.key)
                            .and_then(|a| {
                                reg.iter()
                                    .find(|(x, _)| x == a)
                                    .and_then(|(_, res)| res.as_ref().err().cloned())
                            });
                        let status = match err {
                            Some(e) if e == "Not available yet" => {
                                Some((tr!("Not available yet"), t.text_3))
                            }
                            Some(_) => Some((
                                tr!("Couldn\u{2019}t register \u{2014} used by another app"),
                                t.warning,
                            )),
                            None => None,
                        };
                        let waiting = self.rebinding == Some(r.action);
                        let change_id = SharedString::from(format!("rebind-{}", r.key));
                        let action_for_click = r.action;
                        let label = r.label.clone();
                        let control = div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_end()
                            .gap(px(12.))
                            .children(status.map(|(text, c)| {
                                div()
                                    .text_size(px(type_scale::CAPTION))
                                    .text_color(rgba_of(c))
                                    .child(SharedString::from(text))
                            }))
                            .child(if waiting {
                                div()
                                    .text_size(px(type_scale::CAPTION))
                                    .text_color(rgba_of(t.text))
                                    .child(tr!("Press the new shortcut"))
                                    .into_any_element()
                            } else {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .children(r.keycaps.iter().map(|k| keycap_lg(t, k.clone())))
                                    .into_any_element()
                            })
                            .child(if waiting {
                                small_button_with_icon(
                                    t,
                                    change_id,
                                    &tr_ctx!("shortcut", "Cancel"),
                                    None,
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.rebinding = None;
                                    this.rebind_error = None;
                                    cx.notify();
                                }))
                                .into_any_element()
                            } else {
                                small_button_with_icon(
                                    t,
                                    change_id,
                                    &tr_ctx!("shortcut", "Change"),
                                    Some("pen-line"),
                                )
                                .aria_label(SharedString::from(tr!(
                                    "Change shortcut for {action}",
                                    action = label
                                )))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.rebinding = Some(action_for_click);
                                    this.rebind_error = None;
                                    window.focus(&this.rebind_focus, cx);
                                    cx.notify();
                                }))
                                .into_any_element()
                            });
                        Some(
                            settings_row(
                                t,
                                Some(shortcut_tile(r.key)),
                                label.clone(),
                                None,
                                Some(control.into_any_element()),
                                i == last,
                            )
                            .id(SharedString::from(format!("shortcut-{}", r.key)))
                            .role(Role::ListItem)
                            .aria_label(SharedString::from(format!(
                                "{}, {}",
                                label,
                                r.keycaps.join(" ")
                            ))),
                        )
                    });
                    group_with_card(
                        t,
                        &title,
                        Some(SharedString::from(note)),
                        list_card(t)
                            .id(SharedString::from(format!(
                                "shortcut-list-{}",
                                ["capture", "recording"][gi]
                            )))
                            .role(Role::List)
                            .aria_label(SharedString::from(tr!("{group} shortcuts", group = title)))
                            .children(items),
                    )
                });
        let conflicts = map.conflicts();
        let rebind_error = self
            .rebind_error
            .as_ref()
            .map(crate::ui::app::RebindError::text);
        div()
            .id("shortcuts-page")
            .track_focus(&self.rebind_focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| this.rebind_key(ev, cx)))
            .flex()
            .flex_col()
            .gap(px(24.))
            .children(groups)
            .when(!conflicts.is_empty(), |d| {
                d.child(warning_caption(
                    t,
                    tr!("Two actions share a shortcut; only the first one is registered."),
                ))
            })
            .children(rebind_error.map(|e| {
                warning_caption(t, SharedString::from(e.clone()))
                    .id("rebind-error")
                    .role(Role::Alert)
                    .aria_label(SharedString::from(e))
            }))
            .child(caption(t, tr!("Choose Change, then press the new combination. A shortcut another action or another app already uses is refused. Esc cancels.")))
            .into_any_element()
    }
}
