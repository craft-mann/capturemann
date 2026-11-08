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

// The portal conversation is Linux code; its logic is also built for the tests of every system.
#![cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]

use crate::platform::Registration;
use crate::shortcuts::{Chord, ShortcutAction, ShortcutMap};
use std::collections::HashMap;

/// The status text recorded for a shortcut the session cannot register at all (the Shortcuts
/// page shows a translated note instead of "used by another app").
pub const UNSUPPORTED: &str = "unsupported-session";

/// What the desktop says each bound shortcut is set to (`Ctrl+Alt+S`), when the portal reports it.
pub type PortalTriggers = Vec<(ShortcutAction, String)>;

/// The triggers the portal reported for the actions it bound: `reported` is `(shortcut id,
/// trigger description)`; an empty description (the user has not assigned a key) is left out.
pub fn bound_triggers(reported: &[(String, String)]) -> PortalTriggers {
    reported
        .iter()
        .filter(|(_, d)| !d.trim().is_empty())
        .filter_map(|(id, d)| {
            let action = ShortcutAction::ALL.iter().find(|a| a.key() == id)?;
            Some((*action, d.trim().to_owned()))
        })
        .collect()
}

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
/// `on_bound` gets the outcome once (the Shortcuts page status, and the triggers the desktop
/// chose); `on_activated` gets the hotkey
/// id of each press. The thread lives as long as the session does.
pub fn start_portal(
    map: &ShortcutMap,
    on_bound: impl FnOnce(Registration, PortalTriggers) + Send + 'static,
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
        on_bound(unsupported(map), Vec::new());
    }
}

#[derive(Debug, Clone)]
struct Bind {
    action: ShortcutAction,
    id: u32,
    trigger: String,
}

/// What the app needs from the GlobalShortcuts portal, so the sequence (create a session,
/// listen, bind, forward presses) is tested with a scripted portal on any system. The real one
/// is `linux::AshpdPortal`.
#[allow(async_fn_in_trait)]
trait ShortcutPortal {
    /// The interface exists and a session is open. An `Err` means "this desktop has no such
    /// portal": every shortcut is then "unsupported".
    async fn open(&mut self) -> Result<(), String>;
    /// Starts listening for presses. Done before [`Self::bind`], so a press right after the
    /// desktop's dialog is not missed.
    async fn listen(&mut self) -> Result<(), String>;
    /// Asks for the shortcuts: `(id, description, preferred trigger)`. Answers `(id, trigger
    /// description)` of those the desktop bound (the description is empty when the user has not
    /// assigned a key yet).
    async fn bind(
        &mut self,
        wanted: &[(String, String, String)],
    ) -> Result<Vec<(String, String)>, String>;
    /// The id of the next shortcut pressed, `None` when the session ended.
    async fn next_press(&mut self) -> Option<String>;
}

/// The whole conversation with the portal.
async fn drive<P: ShortcutPortal>(
    portal: &mut P,
    binds: Vec<Bind>,
    on_bound: impl FnOnce(Registration, PortalTriggers),
    on_activated: impl Fn(u32),
) {
    let all_failed = |why: &str| -> Registration {
        binds
            .iter()
            .map(|b| (b.action, Err(why.to_owned())))
            .collect()
    };
    if let Err(e) = portal.open().await {
        log::info!("no GlobalShortcuts portal: {e}");
        on_bound(all_failed(UNSUPPORTED), Vec::new());
        return;
    }
    if let Err(e) = portal.listen().await {
        log::warn!("cannot listen for GlobalShortcuts presses: {e}");
        on_bound(all_failed(&e), Vec::new());
        return;
    }
    let wanted: Vec<(String, String, String)> = binds
        .iter()
        .map(|b| {
            (
                b.action.key().to_owned(),
                b.action.label(),
                b.trigger.clone(),
            )
        })
        .collect();
    match portal.bind(&wanted).await {
        Ok(reported) => {
            log::info!(
                "GlobalShortcuts bound: {}",
                reported
                    .iter()
                    .map(|(i, d)| format!("{i}={d}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            on_bound(
                binds
                    .iter()
                    .map(|b| {
                        let res = if reported.iter().any(|(g, _)| g == b.action.key()) {
                            Ok(())
                        } else {
                            Err("not bound by the desktop".to_owned())
                        };
                        (b.action, res)
                    })
                    .collect(),
                bound_triggers(&reported),
            );
        }
        Err(e) => {
            log::warn!("GlobalShortcuts bind failed: {e}");
            on_bound(all_failed(&e), Vec::new());
            return;
        }
    }
    while let Some(id) = portal.next_press().await {
        if let Some(b) = binds.iter().find(|b| b.action.key() == id) {
            on_activated(b.id);
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{drive, Bind, PortalTriggers, Registration, ShortcutPortal};
    use ashpd::desktop::global_shortcuts::{
        Activated, BindShortcutsOptions, GlobalShortcuts, NewShortcut,
    };
    use ashpd::desktop::{CreateSessionOptions, Session};
    use futures_lite::{Stream, StreamExt};
    use std::pin::Pin;

    type Presses = Pin<Box<dyn Stream<Item = Activated> + Send>>;

    struct AshpdPortal {
        portal: Option<GlobalShortcuts>,
        session: Option<Session<GlobalShortcuts>>,
        presses: Option<Presses>,
    }

    impl ShortcutPortal for AshpdPortal {
        async fn open(&mut self) -> Result<(), String> {
            // A desktop without the interface fails here (the proxy reads the interface version).
            let portal = GlobalShortcuts::new().await.map_err(|e| e.to_string())?;
            let session = portal
                .create_session(CreateSessionOptions::default())
                .await
                .map_err(|e| e.to_string())?;
            self.portal = Some(portal);
            self.session = Some(session);
            Ok(())
        }

        async fn listen(&mut self) -> Result<(), String> {
            let portal = self.portal.as_ref().ok_or("no portal")?;
            let s = portal
                .receive_activated()
                .await
                .map_err(|e| e.to_string())?;
            self.presses = Some(Box::pin(s));
            Ok(())
        }

        async fn bind(
            &mut self,
            wanted: &[(String, String, String)],
        ) -> Result<Vec<(String, String)>, String> {
            let (portal, session) = self
                .portal
                .as_ref()
                .zip(self.session.as_ref())
                .ok_or("no session")?;
            let shortcuts: Vec<NewShortcut> = wanted
                .iter()
                .map(|(id, label, trigger)| {
                    NewShortcut::new(id.as_str(), label.as_str())
                        .preferred_trigger(Some(trigger.as_str()))
                })
                .collect();
            let done = portal
                .bind_shortcuts(session, &shortcuts, None, BindShortcutsOptions::default())
                .await
                .map_err(|e| e.to_string())?
                .response()
                .map_err(|e| e.to_string())?;
            Ok(done
                .shortcuts()
                .iter()
                .map(|s| (s.id().to_owned(), s.trigger_description().to_owned()))
                .collect())
        }

        async fn next_press(&mut self) -> Option<String> {
            let a = self.presses.as_mut()?.next().await?;
            Some(a.shortcut_id().to_owned())
        }
    }

    pub(super) async fn run(
        binds: Vec<Bind>,
        on_bound: impl FnOnce(Registration, PortalTriggers),
        on_activated: impl Fn(u32),
    ) {
        let mut portal = AshpdPortal {
            portal: None,
            session: None,
            presses: None,
        };
        drive(&mut portal, binds, on_bound, on_activated).await;
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
    fn portal_triggers_are_kept_per_action_and_blanks_dropped() {
        let t = bound_triggers(&[
            ("shortcuts.capture_region".into(), " Ctrl+Alt+S ".into()),
            ("shortcuts.open_library".into(), "".into()),
            ("not.ours".into(), "F1".into()),
        ]);
        assert_eq!(
            t,
            vec![(ShortcutAction::CaptureRegion, "Ctrl+Alt+S".to_owned())]
        );
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

    /// A scripted portal: the replies are fixed, the presses are a list.
    struct Scripted {
        open: Result<(), String>,
        bind: Result<Vec<(String, String)>, String>,
        presses: std::collections::VecDeque<String>,
        calls: Vec<&'static str>,
    }

    impl ShortcutPortal for Scripted {
        async fn open(&mut self) -> Result<(), String> {
            self.calls.push("open");
            self.open.clone()
        }
        async fn listen(&mut self) -> Result<(), String> {
            self.calls.push("listen");
            Ok(())
        }
        async fn bind(
            &mut self,
            wanted: &[(String, String, String)],
        ) -> Result<Vec<(String, String)>, String> {
            self.calls.push("bind");
            assert!(wanted.iter().all(|(_, _, t)| !t.is_empty()));
            self.bind.clone()
        }
        async fn next_press(&mut self) -> Option<String> {
            self.presses.pop_front()
        }
    }

    fn run_scripted(p: &mut Scripted) -> (Registration, PortalTriggers, Vec<u32>) {
        let map = ShortcutMap::defaults(Platform::Windows);
        let binds: Vec<Bind> = map
            .iter()
            .filter_map(|(a, c)| {
                Some(Bind {
                    action: a,
                    id: crate::platform::chord_to_hotkey(c).ok()?.id(),
                    trigger: portal_trigger(c),
                })
            })
            .collect();
        let got = std::cell::RefCell::new(None);
        let pressed = std::cell::RefCell::new(Vec::new());
        futures_lite::future::block_on(drive(
            p,
            binds,
            |r, t| *got.borrow_mut() = Some((r, t)),
            |id| pressed.borrow_mut().push(id),
        ));
        let (r, t) = got.into_inner().expect("on_bound is called once");
        (r, t, pressed.into_inner())
    }

    /// CAP-2 (Wayland, KDE or a newer GNOME): the portal binds what it can, the keys it reports
    /// are kept per action, a press reaches the app as the hotkey id, and the listener is
    /// started before the bind. Run with a scripted portal; a real KDE desktop was not available.
    #[test]
    fn cap2_portal_bind_reports_triggers_and_forwards_presses() {
        let region = ShortcutAction::CaptureRegion.key().to_owned();
        let mut p = Scripted {
            open: Ok(()),
            bind: Ok(vec![
                (region.clone(), "Ctrl+Alt+S".into()),
                (ShortcutAction::OpenLibrary.key().to_owned(), "".into()),
            ]),
            presses: [region.clone(), "not.ours".to_owned(), region].into(),
            calls: Vec::new(),
        };
        let (reg, triggers, pressed) = run_scripted(&mut p);
        assert_eq!(p.calls, ["open", "listen", "bind"]);
        let ok = |a: ShortcutAction| reg.iter().find(|(x, _)| *x == a).unwrap().1.is_ok();
        assert!(ok(ShortcutAction::CaptureRegion) && ok(ShortcutAction::OpenLibrary));
        // Not reported by the desktop: refused with a reason, not "unsupported".
        let r = &reg
            .iter()
            .find(|(x, _)| *x == ShortcutAction::CaptureWindow)
            .unwrap()
            .1;
        assert_eq!(r.as_ref().unwrap_err(), "not bound by the desktop");
        assert_eq!(
            triggers,
            vec![(ShortcutAction::CaptureRegion, "Ctrl+Alt+S".to_owned())]
        );
        assert_eq!(pressed.len(), 2, "unknown ids are ignored");
        assert_eq!(pressed[0], pressed[1]);
    }

    /// CAP-2 (GNOME 46 and older): no interface means every shortcut is "unsupported" and nothing is
    /// listened for.
    #[test]
    fn cap2_no_portal_marks_every_shortcut_unsupported() {
        let mut p = Scripted {
            open: Err("no such interface".into()),
            bind: Ok(vec![]),
            presses: Default::default(),
            calls: Vec::new(),
        };
        let (reg, triggers, pressed) = run_scripted(&mut p);
        assert_eq!(p.calls, ["open"]);
        assert!(triggers.is_empty() && pressed.is_empty());
        assert!(reg
            .iter()
            .all(|(_, r)| r.as_ref().err().map(String::as_str) == Some(UNSUPPORTED)));
    }

    /// A refused bind (the user closed the desktop's dialog) says why on every row.
    #[test]
    fn cap2_a_refused_bind_is_reported_on_every_row() {
        let mut p = Scripted {
            open: Ok(()),
            bind: Err("cancelled".into()),
            presses: Default::default(),
            calls: Vec::new(),
        };
        let (reg, _, _) = run_scripted(&mut p);
        assert!(reg
            .iter()
            .all(|(_, r)| r.as_ref().err().map(String::as_str) == Some("cancelled")));
    }
}
