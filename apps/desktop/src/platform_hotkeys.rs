//! Which global-hotkey route a session has (CAP-2) and the Wayland one.
//!
//! * X11 (and Windows, macOS): `global-hotkey` (XGrabKey on X11), registered in `platform`.
//! * Wayland: no app may grab keys. The `org.freedesktop.portal.GlobalShortcuts` portal (KDE,
//!   newer GNOME) lets the app *ask* for shortcuts; the desktop shows its own confirmation and
//!   the user may pick other keys. Where the portal is missing (GNOME 46 and older) nothing is
//!   registered and the app says so: the fallback is the desktop's own custom keyboard shortcut
//!   running `cm-desktop --launcher` or `cm-desktop --capture region` (see `instance`).
//!
//! `XGrabKey` through XWayland would "succeed" in a Wayland session but only fire while an
//! XWayland window has focus, so it is never used there.

use crate::platform::Registration;
use crate::shortcuts::{Chord, ShortcutAction, ShortcutMap};
use std::collections::HashMap;

/// The status text recorded for a shortcut the session cannot register at all (the Shortcuts
/// page shows a translated note instead of "used by another app").
pub const UNSUPPORTED: &str = "unsupported-session";

/// How this session registers global shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// `global-hotkey`: the OS lets an app grab keys.
    Native,
    /// A Wayland session: only the GlobalShortcuts portal, when the desktop has it.
    WaylandPortal,
}

/// A Wayland session, from `XDG_SESSION_TYPE` and `WAYLAND_DISPLAY`.
pub fn is_wayland(xdg_session_type: Option<&str>, wayland_display: bool) -> bool {
    match xdg_session_type {
        Some(t) if t.eq_ignore_ascii_case("wayland") => true,
        Some(t) if t.eq_ignore_ascii_case("x11") => false,
        _ => wayland_display,
    }
}

/// The backend of the running session (always `Native` off Linux).
pub fn backend() -> Backend {
    if cfg!(target_os = "linux")
        && is_wayland(
            std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty()),
        )
    {
        Backend::WaylandPortal
    } else {
        Backend::Native
    }
}

/// The ids `Command::Hotkey` carries, for every action (same ids the `global-hotkey` route
/// uses, so one handler serves both).
pub fn hotkey_ids(map: &ShortcutMap) -> HashMap<u32, ShortcutAction> {
    map.iter()
        .filter_map(|(a, c)| {
            crate::platform::chord_to_hotkey(c)
                .ok()
                .map(|hk| (hk.id(), a))
        })
        .collect()
}

/// A chord in the XDG shortcuts form the portal takes: `CTRL+ALT+SHIFT+LOGO+key`, the key as an
/// xkb keysym name (`1`, `r`, `space`, `F5`).
pub fn portal_trigger(c: &Chord) -> String {
    let mut parts: Vec<String> = Vec::new();
    if c.ctrl {
        parts.push("CTRL".into());
    }
    if c.alt {
        parts.push("ALT".into());
    }
    if c.shift {
        parts.push("SHIFT".into());
    }
    if c.meta {
        parts.push("LOGO".into());
    }
    let key = if c.key.len() > 1 && c.key.starts_with('f') {
        c.key.to_ascii_uppercase()
    } else {
        c.key.clone()
    };
    parts.push(key);
    parts.join("+")
}

/// Every action marked as not registrable in this session.
pub fn unsupported(map: &ShortcutMap) -> Registration {
    map.iter()
        .map(|(a, _)| (a, Err(UNSUPPORTED.to_owned())))
        .collect()
}

/// Asks the GlobalShortcuts portal for every shortcut and keeps listening for presses.
/// Runs on its own thread: the first bind may wait for the user to answer the desktop's dialog.
/// `on_bound` gets the outcome once (the Shortcuts page status); `on_activated` gets the hotkey
/// id of each press. The thread lives as long as the session does.
pub fn start_portal(
    map: &ShortcutMap,
    on_bound: impl FnOnce(Registration) + Send + 'static,
    on_activated: impl Fn(u32) + Send + 'static,
) {
    #[cfg(target_os = "linux")]
    {
        let binds: Vec<Bind> = map
            .iter()
            .filter_map(|(a, c)| {
                let id = crate::platform::chord_to_hotkey(c).ok()?.id();
                Some(Bind {
                    action: a,
                    id,
                    trigger: portal_trigger(c),
                })
            })
            .collect();
        let spawned = std::thread::Builder::new()
            .name("shortcuts-portal".into())
            .spawn(move || {
                futures_lite::future::block_on(linux::run(binds, on_bound, on_activated));
            });
        if let Err(e) = spawned {
            log::warn!("cannot start the shortcuts portal thread: {e}");
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = on_activated;
        on_bound(unsupported(map));
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
struct Bind {
    action: ShortcutAction,
    id: u32,
    trigger: String,
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{Bind, Registration, UNSUPPORTED};
    use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
    use ashpd::desktop::CreateSessionOptions;
    use futures_lite::StreamExt;

    pub(super) async fn run(
        binds: Vec<Bind>,
        on_bound: impl FnOnce(Registration),
        on_activated: impl Fn(u32),
    ) {
        let all_failed = |why: &str| -> Registration {
            binds
                .iter()
                .map(|b| (b.action, Err(why.to_owned())))
                .collect()
        };
        // A desktop without the interface fails here (the proxy reads the interface version).
        let portal = match GlobalShortcuts::new().await {
            Ok(p) => p,
            Err(e) => {
                log::info!("no GlobalShortcuts portal: {e}");
                on_bound(all_failed(UNSUPPORTED));
                return;
            }
        };
        let session = match portal.create_session(CreateSessionOptions::default()).await {
            Ok(s) => s,
            Err(e) => {
                log::warn!("GlobalShortcuts session refused: {e}");
                on_bound(all_failed(UNSUPPORTED));
                return;
            }
        };
        // Subscribe before binding so a press right after the dialog is not missed.
        let mut activated = match portal.receive_activated().await {
            Ok(s) => s,
            Err(e) => {
                log::warn!("cannot listen for GlobalShortcuts presses: {e}");
                on_bound(all_failed(&e.to_string()));
                return;
            }
        };
        let wanted: Vec<NewShortcut> = binds
            .iter()
            .map(|b| {
                NewShortcut::new(b.action.key(), b.action.label())
                    .preferred_trigger(Some(b.trigger.as_str()))
            })
            .collect();
        let bound = match portal
            .bind_shortcuts(&session, &wanted, None, BindShortcutsOptions::default())
            .await
        {
            Ok(request) => request.response(),
            Err(e) => Err(e),
        };
        match bound {
            Ok(done) => {
                let got: Vec<String> = done.shortcuts().iter().map(|s| s.id().to_owned()).collect();
                log::info!(
                    "GlobalShortcuts bound: {}",
                    done.shortcuts()
                        .iter()
                        .map(|s| format!("{}={}", s.id(), s.trigger_description()))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                on_bound(
                    binds
                        .iter()
                        .map(|b| {
                            let res = if got.iter().any(|g| g == b.action.key()) {
                                Ok(())
                            } else {
                                Err("not bound by the desktop".to_owned())
                            };
                            (b.action, res)
                        })
                        .collect(),
                );
            }
            Err(e) => {
                log::warn!("GlobalShortcuts bind failed: {e}");
                on_bound(all_failed(&e.to_string()));
                return;
            }
        }
        while let Some(a) = activated.next().await {
            if let Some(b) = binds.iter().find(|b| b.action.key() == a.shortcut_id()) {
                on_activated(b.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcuts::Platform;

    #[test]
    fn wayland_is_detected_from_the_session_type_first() {
        assert!(is_wayland(Some("wayland"), false));
        assert!(!is_wayland(Some("x11"), true));
        assert!(is_wayland(None, true));
        assert!(!is_wayland(None, false));
        assert!(!is_wayland(Some("tty"), false));
    }

    /// CAP-2 (Wayland): the preferred triggers follow the XDG shortcuts spelling.
    #[test]
    fn triggers_use_the_xdg_spelling() {
        let c = |s: &str| Chord::parse(s).unwrap();
        assert_eq!(portal_trigger(&c("ctrl+alt+shift+1")), "CTRL+ALT+SHIFT+1");
        assert_eq!(portal_trigger(&c("alt+shift+r")), "ALT+SHIFT+r");
        assert_eq!(portal_trigger(&c("super+space")), "LOGO+space");
        assert_eq!(portal_trigger(&c("ctrl+f5")), "CTRL+F5");
    }

    #[test]
    fn every_default_has_an_id_and_a_trigger() {
        let map = ShortcutMap::defaults(Platform::Windows);
        assert_eq!(hotkey_ids(&map).len(), ShortcutAction::ALL.len());
        assert_eq!(unsupported(&map).len(), ShortcutAction::ALL.len());
        assert!(unsupported(&map)
            .iter()
            .all(|(_, r)| r.as_ref().err().map(String::as_str) == Some(UNSUPPORTED)));
    }
}
