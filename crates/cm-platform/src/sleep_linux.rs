//! The idle-sleep guard on Linux (VOC-1, REC-1; parity matrix NEW-A).
//!
//! Two layers, because no single mechanism covers every desktop:
//!
//! 1. logind (`org.freedesktop.login1.Manager.Inhibit`, system bus): a `sleep:idle` inhibitor in
//!    `block` mode. The file descriptor it returns is the lock; closing it (or the process dying,
//!    even by `kill -9`) releases it. It stops suspend requests and logind's own idle action on
//!    every systemd system, GNOME, KDE or none.
//! 2. The desktop's own idle timer (screen blank and automatic suspend) is not logind's: GNOME and
//!    KDE keep their own. KDE honours `org.freedesktop.portal.Inhibit` (flags suspend + idle);
//!    GNOME 46's portal accepts the call but the session manager does not list it, so
//!    `org.gnome.SessionManager.Inhibit` is taken too where it exists. Both are tied to the bus
//!    connection, so a crash releases them too.
//!
//! The guard is taken when at least one layer succeeded; when none did (no D-Bus at all, as in a
//! container) the error says why and the caller decides. Everything goes through [`Bus`], so the
//! choice logic is tested with a fake and the real D-Bus calls are checked on a desktop.

use std::any::Any;

/// What is kept alive while the computer must stay awake.
pub type Hold = Box<dyn Any + Send + Sync>;

/// The D-Bus calls the guard needs. The real one is [`ZbusBus`]; tests script it.
pub trait Bus {
    /// logind `Inhibit("sleep:idle" or "idle", who, why, "block")`; the returned value owns the descriptor.
    fn logind(&self, who: &str, why: &str) -> Result<Hold, String>;
    /// `org.freedesktop.portal.Inhibit.Inhibit` with suspend and idle.
    fn portal(&self, why: &str) -> Result<Hold, String>;
    /// `org.gnome.SessionManager.Inhibit` with suspend and idle.
    fn gnome_session(&self, who: &str, why: &str) -> Result<Hold, String>;
}

/// The locks held; dropping releases them.
pub struct Held {
    /// Which mechanisms hold, for logs and tests (`logind`, `portal`, `gnome-session`).
    pub layers: Vec<&'static str>,
    _holds: Vec<Hold>,
}

const WHO: &str = "CaptureMann";

/// Takes the locks through `bus`.
pub fn acquire_with(bus: &dyn Bus, reason: &str) -> Result<Held, String> {
    let mut layers = Vec::new();
    let mut holds = Vec::new();
    let mut problems = Vec::new();
    match bus.logind(WHO, reason) {
        Ok(h) => {
            layers.push("logind");
            holds.push(h);
        }
        Err(e) => problems.push(format!("logind: {e}")),
    }
    match bus.portal(reason) {
        Ok(h) => {
            layers.push("portal");
            holds.push(h);
        }
        Err(e) => problems.push(format!("portal: {e}")),
    }
    // Tried as well, not only as a fallback: on GNOME 46 the portal answers but the session
    // manager does not list the inhibitor (checked on Ubuntu 24.04), while this call does work.
    match bus.gnome_session(WHO, reason) {
        Ok(h) => {
            layers.push("gnome-session");
            holds.push(h);
        }
        Err(e) => problems.push(format!("gnome-session: {e}")),
    }
    if holds.is_empty() {
        return Err(format!(
            "cannot prevent idle sleep ({})",
            problems.join("; ")
        ));
    }
    Ok(Held {
        layers,
        _holds: holds,
    })
}

/// The real system: D-Bus through `zbus` (blocking, no async runtime of our own).
pub struct ZbusBus;

const SUSPEND: u32 = 4;
const IDLE: u32 = 8;

/// Keeps the connection (and so the inhibitor) alive; closes the portal request on drop.
struct ConnHold {
    conn: zbus::blocking::Connection,
    request: Option<zbus::zvariant::OwnedObjectPath>,
}

impl Drop for ConnHold {
    fn drop(&mut self) {
        if let Some(path) = self.request.take() {
            let _ = self.conn.call_method(
                Some("org.freedesktop.portal.Desktop"),
                path.as_str(),
                Some("org.freedesktop.portal.Request"),
                "Close",
                &(),
            );
        }
    }
}

impl Bus for ZbusBus {
    fn logind(&self, who: &str, why: &str) -> Result<Hold, String> {
        let conn = zbus::blocking::Connection::system().map_err(|e| e.to_string())?;
        // polkit lets only a session on the seat take a blocking "sleep" lock (a session reached
        // over ssh gets "Access denied"), so `idle` alone is the fallback.
        let mut last = String::new();
        for what in ["sleep:idle", "idle"] {
            let reply = conn.call_method(
                Some("org.freedesktop.login1"),
                "/org/freedesktop/login1",
                Some("org.freedesktop.login1.Manager"),
                "Inhibit",
                &(what, who, why, "block"),
            );
            match reply {
                Ok(reply) => {
                    let fd: zbus::zvariant::OwnedFd =
                        reply.body().deserialize().map_err(|e| e.to_string())?;
                    return Ok(Box::new((conn, fd)));
                }
                Err(e) => last = e.to_string(),
            }
        }
        Err(last)
    }

    fn portal(&self, why: &str) -> Result<Hold, String> {
        use std::collections::HashMap;
        use zbus::zvariant::Value;
        let conn = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
        let mut options: HashMap<&str, Value<'_>> = HashMap::new();
        options.insert("reason", Value::from(why));
        let reply = conn
            .call_method(
                Some("org.freedesktop.portal.Desktop"),
                "/org/freedesktop/portal/desktop",
                Some("org.freedesktop.portal.Inhibit"),
                "Inhibit",
                &("", SUSPEND | IDLE, options),
            )
            .map_err(|e| e.to_string())?;
        let request: zbus::zvariant::OwnedObjectPath =
            reply.body().deserialize().map_err(|e| e.to_string())?;
        Ok(Box::new(ConnHold {
            conn,
            request: Some(request),
        }))
    }

    fn gnome_session(&self, who: &str, why: &str) -> Result<Hold, String> {
        let conn = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
        let reply = conn
            .call_method(
                Some("org.gnome.SessionManager"),
                "/org/gnome/SessionManager",
                Some("org.gnome.SessionManager"),
                "Inhibit",
                &(who, 0u32, why, SUSPEND | IDLE),
            )
            .map_err(|e| e.to_string())?;
        let _cookie: u32 = reply.body().deserialize().map_err(|e| e.to_string())?;
        // The inhibitor lives as long as this connection (the session manager drops it when the
        // client leaves the bus), so the connection is the hold.
        Ok(Box::new(ConnHold {
            conn,
            request: None,
        }))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct Token(Arc<AtomicUsize>);
    impl Drop for Token {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    struct FakeBus {
        logind: bool,
        portal: bool,
        gnome: bool,
        live: Arc<AtomicUsize>,
        calls: std::sync::Mutex<Vec<&'static str>>,
    }

    impl FakeBus {
        fn new(logind: bool, portal: bool, gnome: bool) -> Self {
            Self {
                logind,
                portal,
                gnome,
                live: Arc::new(AtomicUsize::new(0)),
                calls: Default::default(),
            }
        }
        fn hold(&self, ok: bool, call: &'static str) -> Result<Hold, String> {
            self.calls.lock().unwrap().push(call);
            if ok {
                self.live.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(Token(self.live.clone())))
            } else {
                Err(format!("{call} missing"))
            }
        }
    }

    impl Bus for FakeBus {
        fn logind(&self, _: &str, _: &str) -> Result<Hold, String> {
            self.hold(self.logind, "logind")
        }
        fn portal(&self, _: &str) -> Result<Hold, String> {
            self.hold(self.portal, "portal")
        }
        fn gnome_session(&self, _: &str, _: &str) -> Result<Hold, String> {
            self.hold(self.gnome, "gnome")
        }
    }

    /// Prints what each real layer says on this desktop (run with `--ignored --nocapture`).
    #[test]
    #[ignore = "needs a Linux desktop session bus and logind"]
    fn the_real_layers_on_this_desktop() {
        let bus = ZbusBus;
        for (name, r) in [
            ("logind", bus.logind("CaptureMann", "layer probe")),
            ("portal", bus.portal("layer probe")),
            (
                "gnome-session",
                bus.gnome_session("CaptureMann", "layer probe"),
            ),
        ] {
            println!(
                "{name}: {}",
                r.map(|_| "held".to_string()).unwrap_or_else(|e| e)
            );
        }
    }

    /// VOC-1: logind and the portal both hold, and dropping the guard releases both.
    #[test]
    fn holds_every_layer_and_releases_on_drop() {
        let bus = FakeBus::new(true, true, true);
        let held = acquire_with(&bus, "Recording").unwrap();
        assert_eq!(held.layers, ["logind", "portal", "gnome-session"]);
        assert_eq!(bus.live.load(Ordering::SeqCst), 3);
        drop(held);
        assert_eq!(bus.live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn the_gnome_session_still_holds_when_there_is_no_portal() {
        let bus = FakeBus::new(true, false, true);
        let held = acquire_with(&bus, "Recording").unwrap();
        assert_eq!(held.layers, ["logind", "gnome-session"]);
    }

    #[test]
    fn one_layer_is_enough() {
        let bus = FakeBus::new(false, true, false);
        let held = acquire_with(&bus, "Recording").unwrap();
        assert_eq!(held.layers, ["portal"]);
    }

    #[test]
    fn no_layer_is_an_error_that_names_each_reason() {
        let bus = FakeBus::new(false, false, false);
        let err = acquire_with(&bus, "Recording").map(|_| ()).unwrap_err();
        assert!(err.contains("logind: logind missing"), "{err}");
        assert!(err.contains("portal: portal missing"), "{err}");
        assert!(err.contains("gnome-session: gnome missing"), "{err}");
    }
}
