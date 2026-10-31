//! Which display server a Linux session uses; it decides the capture path (X11 through `xcap`,
//! Wayland through the desktop portal). Pure, so the rule is tested on every system.

// Only Linux asks; the other systems compile the rule for its tests.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

/// The display server of the current session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    X11,
    Wayland,
    /// Neither `DISPLAY` nor `WAYLAND_DISPLAY` is set (a text console, an SSH session).
    None,
}

/// The rule: `XDG_SESSION_TYPE` decides when it says `wayland` or `x11`; otherwise the display
/// variables do, Wayland first. A Wayland desktop also runs an X server for old programs
/// (`DISPLAY` is set there), but capturing it shows only those programs, so a Wayland session is
/// never treated as X11. An `x11` session without `DISPLAY` has no server to talk to.
pub fn detect(
    xdg_session_type: Option<&str>,
    wayland_display: Option<&str>,
    display: Option<&str>,
) -> DisplayServer {
    let set = |v: Option<&str>| v.is_some_and(|s| !s.is_empty());
    match xdg_session_type {
        Some("wayland") => DisplayServer::Wayland,
        Some("x11") => {
            if set(display) {
                DisplayServer::X11
            } else {
                DisplayServer::None
            }
        }
        _ => {
            if set(wayland_display) {
                DisplayServer::Wayland
            } else if set(display) {
                DisplayServer::X11
            } else {
                DisplayServer::None
            }
        }
    }
}

/// [`detect`] on this process's environment.
#[cfg(target_os = "linux")]
pub fn from_env() -> DisplayServer {
    let get = |k: &str| std::env::var(k).ok();
    detect(
        get("XDG_SESSION_TYPE").as_deref(),
        get("WAYLAND_DISPLAY").as_deref(),
        get("DISPLAY").as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use DisplayServer::{None as Nothing, Wayland, X11};

    #[test]
    fn the_session_type_wins_when_it_names_a_server() {
        // A GNOME Wayland desktop: Xwayland sets DISPLAY too, but this is Wayland.
        assert_eq!(
            detect(Some("wayland"), Some("wayland-0"), Some(":0")),
            Wayland
        );
        assert_eq!(detect(Some("wayland"), None, Some(":0")), Wayland);
        assert_eq!(detect(Some("x11"), Some("wayland-0"), Some(":0")), X11);
        assert_eq!(detect(Some("x11"), None, None), Nothing);
        assert_eq!(detect(Some("x11"), None, Some("")), Nothing);
    }

    #[test]
    fn without_a_usable_session_type_the_display_variables_decide() {
        assert_eq!(detect(None, Some("wayland-0"), None), Wayland);
        assert_eq!(detect(Some("tty"), Some("wayland-0"), Some(":0")), Wayland);
        assert_eq!(detect(None, None, Some(":99")), X11);
        assert_eq!(detect(Some("tty"), Some(""), Some(":1")), X11);
        assert_eq!(detect(None, None, None), Nothing);
        assert_eq!(detect(Some(""), Some(""), Some("")), Nothing);
    }
}
