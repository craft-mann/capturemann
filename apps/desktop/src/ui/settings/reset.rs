//! Settings -> Shortcuts "Reset to defaults" (board R16): which keys the page owns, which of them
//! differ from the defaults, what is written, and the button, the confirmation and the apply step.
//! The decisions are pure functions (unit tested below); only [`reset_shortcuts`] touches the app.
//!
//! Only the Shortcuts page draws this control (R12 and R13 have no such button). Nothing outside
//! the page's own keys is written: never the library path, the privacy lists, destinations or
//! accounts.

use gpui::{div, prelude::*, px, Role, SharedString};
use serde_json::{json, Map, Value};

use super::widgets::small_button_with_icon;
use crate::confirm::DialogModel;
use crate::shortcuts::{Platform, ShortcutAction, ShortcutMap};
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::{self, AppModel};
use crate::ui::components::{icon, icon_size};
use crate::ui::paint::rgba_of;
use cm_i18n::tr;

/// The settings keys of the Shortcuts page, in page order. Kept as a literal list on purpose: a
/// test checks it against the keys the page reads ([`ShortcutAction::key`]).
#[cfg(test)]
pub const SHORTCUT_RESET_KEYS: [&str; 10] = [
    "shortcuts.capture_region",
    "shortcuts.capture_window",
    "shortcuts.capture_display",
    "shortcuts.toggle_recording",
    "shortcuts.pause_recording",
    "shortcuts.open_library",
    "shortcuts.open_launcher",
    "shortcuts.discard_recording",
    "shortcuts.restart_recording",
    "shortcuts.annotate_last",
];

/// Id of the confirmation window (one at a time).
pub const RESET_CONFIRM_ID: &str = "local_reset_shortcuts";

/// The actions whose binding is not the default: another chord, or a stored value that could not
/// be read (the page then shows the default, the file still holds the bad text).
pub fn shortcut_reset_targets(map: &ShortcutMap, platform: Platform) -> Vec<ShortcutAction> {
    ShortcutAction::ALL
        .into_iter()
        .filter(|a| {
            map.get(*a) != a.default_chord(platform) || map.invalid.iter().any(|(x, _)| x == a)
        })
        .collect()
}

/// The `set_settings` values of one reset: each target's key set to its default chord text.
pub fn shortcut_reset_values(targets: &[ShortcutAction], platform: Platform) -> Map<String, Value> {
    targets
        .iter()
        .map(|a| {
            (
                a.key().to_owned(),
                json!(a.default_chord(platform).to_setting()),
            )
        })
        .collect()
}

/// The effective bindings once the reset is applied.
pub fn map_after_reset(
    map: &ShortcutMap,
    targets: &[ShortcutAction],
    platform: Platform,
) -> ShortcutMap {
    targets
        .iter()
        .fold(map.clone(), |m, a| m.with(*a, a.default_chord(platform)))
}

/// The "Reset the keyboard shortcuts to their defaults?" question (the normal primary button,
/// OD-12). `changed` are the shortcuts that will change.
pub fn reset_dialog(changed: &[ShortcutAction]) -> DialogModel {
    let list = changed
        .iter()
        .map(|a| a.label())
        .collect::<Vec<_>>()
        .join(", ");
    DialogModel {
        id: RESET_CONFIRM_ID.to_owned(),
        chip: String::new(),
        agent: String::new(),
        tool: String::new(),
        title: tr!("Reset the keyboard shortcuts to their defaults?"),
        summary: tr!("Every shortcut on this page goes back to its default keys. Other settings are not changed."),
        details: vec![(tr!("Will change"), list)],
        primary_label: tr!("Reset shortcuts"),
        primary_is_danger: false,
        primary_is_record: false,
        allow_session: false,
        option: None,
    }
}

/// The header button of the Shortcuts page: secondary, `rotate-ccw` icon; muted and not
/// clickable (`aria-disabled`) when nothing differs.
pub fn reset_button(t: &Theme, enabled: bool) -> gpui::Stateful<gpui::Div> {
    let label = tr!("Reset to defaults");
    if enabled {
        return small_button_with_icon(t, "shortcuts-reset", &label, Some("rotate-ccw"))
            .h(px(scale::CONTROL_H));
    }
    let focus = t.text;
    div()
        .id("shortcuts-reset")
        .role(Role::Button)
        .aria_label(SharedString::from(tr!(
            "{label}, unavailable",
            label = label
        )))
        .aria_description(SharedString::from(tr!(
            "Every shortcut already has its default keys."
        )))
        .focusable()
        .tab_stop(true)
        .h(px(scale::CONTROL_H))
        .px(px(12.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_SM))
        .border_1()
        .border_color(rgba_of(t.border))
        .text_color(rgba_of(t.mist))
        .gap(px(icon_size::SMALL_GAP))
        .text_size(px(type_scale::CAPTION))
        .cursor_default()
        .focus_visible(move |s| s.border_color(rgba_of(focus)))
        .child(icon("rotate-ccw", icon_size::SMALL, t.mist))
        .child(SharedString::from(label))
}

/// Asks, then resets. The question lists what changes; Cancel leaves everything as it is.
pub fn ask_reset_shortcuts(app: &gpui::Entity<AppModel>, cx: &mut gpui::App) {
    let (map, platform) = {
        let m = app.read(cx);
        (m.shortcuts.clone(), m.platform)
    };
    let targets = shortcut_reset_targets(&map, platform);
    if targets.is_empty() {
        return;
    }
    let model = reset_dialog(&targets);
    app::ask_confirm(
        app,
        model,
        |app, decision, cx| {
            if matches!(
                decision,
                cm_daemon::Decision::Approve | cm_daemon::Decision::ApproveForSession
            ) {
                reset_shortcuts(app, cx);
            }
        },
        cx,
    );
}

/// Applies the reset: moves the OS hotkeys (all old ones off first, so a swap cannot collide),
/// writes every changed key in one `set_settings` call and updates the app's map. A key the OS
/// refuses is shown as "Couldn't register" on its row, like at start-up.
pub fn reset_shortcuts(app: &gpui::Entity<AppModel>, cx: &mut gpui::App) {
    let (map, platform) = {
        let m = app.read(cx);
        (m.shortcuts.clone(), m.platform)
    };
    let targets = shortcut_reset_targets(&map, platform);
    if targets.is_empty() {
        return;
    }
    let mut moved: Vec<(ShortcutAction, u32, Result<u32, String>)> = Vec::new();
    if let Some(h) = cx.try_global::<crate::platform::HotkeyHolder>() {
        let mgr = &h.0;
        let olds: Vec<_> = targets
            .iter()
            .map(|a| crate::platform::chord_to_hotkey(&map.get(*a)))
            .collect();
        for hk in olds.iter().flatten() {
            let _ = mgr.unregister(*hk);
        }
        for (a, old) in targets.iter().zip(&olds) {
            let new = crate::platform::chord_to_hotkey(&a.default_chord(platform)).and_then(|hk| {
                mgr.register(hk)
                    .map(|()| hk.id())
                    .map_err(|e| e.to_string())
            });
            let old_id = old.as_ref().map(|h| h.id()).unwrap_or(0);
            moved.push((*a, old_id, new));
        }
    }
    let values = shortcut_reset_values(&targets, platform);
    let next = map_after_reset(&map, &targets, platform);
    app.update(cx, |m, cx| {
        m.shortcuts = next;
        for (a, old_id, res) in &moved {
            m.hotkey_ids.remove(old_id);
            if let Ok(id) = res {
                m.hotkey_ids.insert(*id, *a);
            }
            for (x, r) in &mut m.registration {
                if x == a {
                    *r = res.as_ref().map(|_| ()).map_err(|e| e.clone());
                }
            }
        }
        cx.notify();
    });
    let backend = app.read(cx).backend.clone();
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let Some(daemon) = backend.daemon.clone() else {
            return;
        };
        let res = backend
            .rt
            .spawn(async move {
                crate::backend::Backend::call(&daemon, "set_settings", json!({ "values": values }))
                    .await
            })
            .await;
        cx.update(|cx| match res {
            Ok(Ok(_)) => app::push_toast(
                &app2,
                crate::toast::ToastSpec::new(
                    crate::toast::ToastKind::Success,
                    tr!("Keyboard shortcuts reset to their defaults"),
                ),
                cx,
            ),
            Ok(Err(e)) => app::push_toast(
                &app2,
                crate::flows::error_toast_live(|| tr!("Couldn\u{2019}t save the setting"), e),
                cx,
            ),
            Err(e) => app::push_toast(
                &app2,
                crate::flows::error_toast_live(
                    || tr!("Couldn\u{2019}t save the setting"),
                    e.to_string(),
                ),
                cx,
            ),
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Platform = Platform::Mac;

    fn settings(pairs: &[(&str, &str)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), json!(v)))
            .collect()
    }

    #[test]
    fn keys_are_the_keys_the_page_reads() {
        let page: Vec<&str> = ShortcutAction::ALL.iter().map(|a| a.key()).collect();
        assert_eq!(SHORTCUT_RESET_KEYS.to_vec(), page);
        let written = shortcut_reset_values(&ShortcutAction::ALL, P);
        assert_eq!(written.len(), SHORTCUT_RESET_KEYS.len());
        assert!(SHORTCUT_RESET_KEYS.iter().all(|k| written.contains_key(*k)));
        // Read back through the page's own parser, the written values are the defaults.
        assert_eq!(
            ShortcutMap::from_settings(&written, P),
            ShortcutMap::defaults(P)
        );
    }

    #[test]
    fn nothing_differs_at_defaults() {
        let map = ShortcutMap::defaults(P);
        assert!(shortcut_reset_targets(&map, P).is_empty());
        // A stored value equal to the default (any spelling) is no difference either.
        let map = ShortcutMap::from_settings(
            &settings(&[("shortcuts.capture_region", "Shift+Option+1")]),
            P,
        );
        assert!(shortcut_reset_targets(&map, P).is_empty());
    }

    #[test]
    fn only_changed_actions_are_targets() {
        let map = ShortcutMap::from_settings(
            &settings(&[
                ("shortcuts.open_library", "alt+shift+k"),
                ("shortcuts.capture_window", "alt+shift+2"),
            ]),
            P,
        );
        let t = shortcut_reset_targets(&map, P);
        assert_eq!(t, vec![ShortcutAction::OpenLibrary]);
        let v = shortcut_reset_values(&t, P);
        assert_eq!(v.len(), 1);
        assert_eq!(v["shortcuts.open_library"], json!("alt+shift+l"));
    }

    #[test]
    fn an_invalid_stored_value_is_replaced() {
        let map =
            ShortcutMap::from_settings(&settings(&[("shortcuts.pause_recording", "banana")]), P);
        assert_eq!(map.invalid.len(), 1);
        let t = shortcut_reset_targets(&map, P);
        assert_eq!(t, vec![ShortcutAction::PauseRecording]);
        let after = map_after_reset(&map, &t, P);
        assert!(after.invalid.is_empty());
        assert!(shortcut_reset_targets(&after, P).is_empty());
    }

    #[test]
    fn a_swap_resets_cleanly_and_leaves_no_conflict() {
        let map = ShortcutMap::from_settings(
            &settings(&[
                ("shortcuts.capture_region", "alt+shift+2"),
                ("shortcuts.capture_window", "alt+shift+1"),
                ("shortcuts.open_library", "alt+shift+3"),
            ]),
            P,
        );
        assert!(!map.conflicts().is_empty());
        let t = shortcut_reset_targets(&map, P);
        assert_eq!(t.len(), 3);
        let after = map_after_reset(&map, &t, P);
        assert!(after.conflicts().is_empty());
        assert_eq!(after, ShortcutMap::defaults(P));
        assert!(ShortcutMap::defaults(Platform::Windows)
            .conflicts()
            .is_empty());
        assert!(ShortcutMap::defaults(P).conflicts().is_empty());
    }

    #[test]
    fn the_dialog_names_the_action_and_is_not_danger() {
        let d = reset_dialog(&[ShortcutAction::OpenLibrary, ShortcutAction::AnnotateLast]);
        assert!(!d.primary_is_danger && !d.primary_is_record && d.agent.is_empty());
        assert!(d.title.contains("keyboard shortcuts"));
        assert_eq!(
            d.details[0].1,
            format!(
                "{}, {}",
                ShortcutAction::OpenLibrary.label(),
                ShortcutAction::AnnotateLast.label()
            )
        );
        assert_ne!(d.primary_label, "OK");
    }
}
