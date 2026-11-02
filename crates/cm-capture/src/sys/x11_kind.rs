//! Which X11 windows are not windows a person would pick. The window list of a desktop holds
//! windows that draw the background or the panels: GNOME's desktop-icons extension has a
//! full-screen "Desktop Icons" window, and docks and panels are windows too. The window picker
//! offered them and a capture of one came out black. The standard says what they are:
//! `_NET_WM_WINDOW_TYPE` names the kind (`_DESKTOP`, `_DOCK`), so the rule is by kind and does
//! not depend on any program's name.

// Only Linux asks X11; the other systems compile the rule for its tests.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use crate::WindowInfo;

/// True when one of the window's types (`_NET_WM_WINDOW_TYPE` atoms) is the desktop or a dock.
fn is_background_or_panel(types: &[u32], desktop: u32, dock: u32) -> bool {
    types.iter().any(|&t| t == desktop || t == dock)
}

/// Removes the desktop and dock/panel windows from the list. Does nothing without an X server
/// (or off Linux), and keeps a window whose type cannot be read.
pub fn drop_background_windows(windows: &mut Vec<WindowInfo>) {
    imp::drop_background_windows(windows);
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{is_background_or_panel, WindowInfo};
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};
    use x11rb::rust_connection::RustConnection;

    pub fn drop_background_windows(windows: &mut Vec<WindowInfo>) {
        let Ok((conn, _)) = RustConnection::connect(None) else {
            return;
        };
        let atom = |name: &[u8]| {
            conn.intern_atom(false, name)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| r.atom)
        };
        let (Some(kind), Some(desktop), Some(dock)) = (
            atom(b"_NET_WM_WINDOW_TYPE"),
            atom(b"_NET_WM_WINDOW_TYPE_DESKTOP"),
            atom(b"_NET_WM_WINDOW_TYPE_DOCK"),
        ) else {
            return;
        };
        windows.retain(|w| {
            let types: Vec<u32> = conn
                .get_property(false, w.id, kind, AtomEnum::ATOM, 0, 32)
                .ok()
                .and_then(|c| c.reply().ok())
                .and_then(|r| r.value32().map(Iterator::collect))
                .unwrap_or_default();
            !is_background_or_panel(&types, desktop, dock)
        });
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::WindowInfo;

    pub fn drop_background_windows(_windows: &mut Vec<WindowInfo>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESKTOP: u32 = 10;
    const DOCK: u32 = 11;
    const NORMAL: u32 = 12;

    #[test]
    fn the_desktop_and_docks_are_not_pickable_windows() {
        assert!(is_background_or_panel(&[DESKTOP], DESKTOP, DOCK));
        assert!(is_background_or_panel(&[DOCK], DESKTOP, DOCK));
        // A window with several types is dropped when any of them is one of these.
        assert!(is_background_or_panel(&[NORMAL, DOCK], DESKTOP, DOCK));
    }

    #[test]
    fn ordinary_windows_and_windows_without_a_type_stay() {
        assert!(!is_background_or_panel(&[NORMAL], DESKTOP, DOCK));
        assert!(!is_background_or_panel(&[], DESKTOP, DOCK));
    }
}
