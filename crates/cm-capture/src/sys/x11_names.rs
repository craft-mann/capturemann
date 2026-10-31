//! Window titles that `xcap` leaves empty on X11. `xcap` reads `_NET_WM_NAME` and, when that is
//! empty, `WM_NAME` asked as `COMPOUND_TEXT`; but classic clients (`xclock`, `xterm`, `xeyes`,
//! most Tk and Motif programs) set `WM_NAME` as `STRING` (Latin-1), so their title came back
//! empty and `capture screenshot --window "xclock"` found nothing by title. This module reads
//! `WM_NAME` with whatever type it has and decodes it.

// Only Linux asks X11; the other systems compile the decoder for its tests.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use crate::WindowInfo;

/// `XA_STRING`: Latin-1 text.
const XA_STRING: u32 = 31;

/// Decodes a `WM_NAME` value by its property type. `utf8_string` is the atom of `UTF8_STRING`.
/// `STRING` is Latin-1; `UTF8_STRING` is UTF-8; anything else (`COMPOUND_TEXT`, which in
/// practice is ASCII or UTF-8 from modern toolkits) is read as UTF-8 and, if that is not valid,
/// as Latin-1. NUL bytes and surrounding white space are dropped.
pub fn decode_wm_name(type_atom: u32, utf8_string: u32, bytes: &[u8]) -> String {
    let latin1 = |b: &[u8]| b.iter().map(|&c| char::from(c)).collect::<String>();
    let text = if type_atom == XA_STRING {
        latin1(bytes)
    } else if type_atom == utf8_string {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .unwrap_or_else(|_| latin1(bytes))
    };
    text.replace('\0', "").trim().to_owned()
}

/// Gives every window whose title is empty the title from its `WM_NAME`, when there is one.
/// Does nothing without an X server (or off Linux).
pub fn fill_missing_titles(windows: &mut [WindowInfo]) {
    imp::fill_missing_titles(windows);
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{decode_wm_name, WindowInfo};
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, GetPropertyReply};
    use x11rb::rust_connection::RustConnection;

    pub fn fill_missing_titles(windows: &mut [WindowInfo]) {
        if windows.iter().all(|w| !w.title.is_empty()) {
            return;
        }
        let Ok((conn, _)) = RustConnection::connect(None) else {
            return;
        };
        let utf8 = conn
            .intern_atom(false, b"UTF8_STRING")
            .ok()
            .and_then(|c| c.reply().ok())
            .map_or(0, |r| r.atom);
        for w in windows.iter_mut().filter(|w| w.title.is_empty()) {
            if let Some(title) = wm_name(&conn, w.id, utf8) {
                w.title = title;
            }
        }
    }

    fn wm_name(conn: &RustConnection, window: u32, utf8: u32) -> Option<String> {
        let reply: GetPropertyReply = conn
            .get_property(false, window, AtomEnum::WM_NAME, AtomEnum::ANY, 0, 1024)
            .ok()?
            .reply()
            .ok()?;
        if reply.value.is_empty() {
            return None;
        }
        let title = decode_wm_name(reply.type_, utf8, &reply.value);
        (!title.is_empty()).then_some(title)
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::WindowInfo;

    pub fn fill_missing_titles(_windows: &mut [WindowInfo]) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    const UTF8: u32 = 300;

    #[test]
    fn a_string_property_is_latin_1() {
        // "Café" in Latin-1.
        assert_eq!(decode_wm_name(XA_STRING, UTF8, b"Caf\xe9"), "Café");
        assert_eq!(decode_wm_name(XA_STRING, UTF8, b"xclock\0"), "xclock");
    }

    #[test]
    fn a_utf8_string_property_is_utf_8() {
        assert_eq!(
            decode_wm_name(UTF8, UTF8, "Tiếng Việt".as_bytes()),
            "Tiếng Việt"
        );
    }

    #[test]
    fn other_types_try_utf_8_then_latin_1() {
        assert_eq!(decode_wm_name(7, UTF8, "naïve".as_bytes()), "naïve");
        assert_eq!(decode_wm_name(7, UTF8, b"na\xefve"), "naïve");
    }

    #[test]
    fn white_space_and_nuls_are_dropped() {
        assert_eq!(decode_wm_name(XA_STRING, UTF8, b"  a\0b \n"), "ab");
        assert_eq!(decode_wm_name(XA_STRING, UTF8, b"\0\0"), "");
    }
}
