//! Pure conversions for the X11 cursor backend (REC-7 on Linux): X11 button numbers, modifier
//! masks and XFixes cursor pixels. No X11 library is used here, so these are tested on every OS.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use crate::schema::{Button, Modifier};

/// What an X11 pointer button number means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XButton {
    Button(Button),
    /// Wheel buttons 4 to 7 as direction signs `(dx, dy)` in the track's convention (`dy > 0` is
    /// wheel up, `dx > 0` is right, as the Windows wheel reports them).
    Scroll(i8, i8),
}

/// X11 core button numbers: 1 left, 2 middle, 3 right, 4/5 wheel up/down, 6/7 wheel left/right,
/// 8/9 back/forward, others unknown.
pub fn classify_button(detail: u32) -> XButton {
    match detail {
        1 => XButton::Button(Button::Left),
        2 => XButton::Button(Button::Middle),
        3 => XButton::Button(Button::Right),
        4 => XButton::Scroll(0, 1),
        5 => XButton::Scroll(0, -1),
        6 => XButton::Scroll(-1, 0),
        7 => XButton::Scroll(1, 0),
        8 => XButton::Button(Button::Back),
        9 => XButton::Button(Button::Forward),
        _ => XButton::Button(Button::Other),
    }
}

/// Modifier flags from the core key/button state mask: Shift 0x1, Control 0x4, Mod1 (Alt) 0x8,
/// Mod4 (Super) 0x40. Only these four bits are looked at.
pub fn modifiers_from_state(state: u16) -> Vec<Modifier> {
    let mut m = Vec::new();
    if state & 0x1 != 0 {
        m.push(Modifier::Shift);
    }
    if state & 0x4 != 0 {
        m.push(Modifier::Ctrl);
    }
    if state & 0x8 != 0 {
        m.push(Modifier::Alt);
    }
    if state & 0x40 != 0 {
        m.push(Modifier::Cmd);
    }
    m
}

/// Is this a Wayland session? `XDG_SESSION_TYPE=wayland`, or (when the type is unset) a
/// `WAYLAND_DISPLAY`. XWayland offers a `DISPLAY` too, but it sees the pointer only over X11
/// windows, so a Wayland session never counts as having a global pointer.
pub fn is_wayland_session(xdg_session_type: Option<&str>, has_wayland_display: bool) -> bool {
    match xdg_session_type {
        Some(t) if t.eq_ignore_ascii_case("wayland") => true,
        Some(t) if t.eq_ignore_ascii_case("x11") => false,
        _ => has_wayland_display,
    }
}

/// XFixes cursor pixels (premultiplied ARGB, one `u32` each) to straight RGBA bytes.
pub fn argb_premultiplied_to_rgba(argb: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(argb.len() * 4);
    for &p in argb {
        let a = (p >> 24) & 0xff;
        let un = |c: u32| -> u8 { ((c * 255 + a / 2).checked_div(a).unwrap_or(0)).min(255) as u8 };
        out.extend_from_slice(&[
            un((p >> 16) & 0xff),
            un((p >> 8) & 0xff),
            un(p & 0xff),
            a as u8,
        ]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_and_side_buttons_are_told_apart_from_clicks() {
        assert_eq!(classify_button(1), XButton::Button(Button::Left));
        assert_eq!(classify_button(3), XButton::Button(Button::Right));
        assert_eq!(classify_button(4), XButton::Scroll(0, 1));
        assert_eq!(classify_button(5), XButton::Scroll(0, -1));
        assert_eq!(classify_button(7), XButton::Scroll(1, 0));
        assert_eq!(classify_button(8), XButton::Button(Button::Back));
        assert_eq!(classify_button(12), XButton::Button(Button::Other));
    }

    #[test]
    fn wayland_is_told_from_x11() {
        assert!(is_wayland_session(Some("wayland"), true));
        assert!(!is_wayland_session(Some("x11"), true));
        assert!(is_wayland_session(None, true));
        assert!(!is_wayland_session(Some("tty"), false));
    }

    #[test]
    fn only_four_modifier_bits_count() {
        assert_eq!(modifiers_from_state(0), vec![]);
        // Shift + Lock + Control + Mod2 (num lock) + Button1: only Shift and Ctrl.
        assert_eq!(
            modifiers_from_state(0x1 | 0x2 | 0x4 | 0x10 | 0x100),
            vec![Modifier::Shift, Modifier::Ctrl]
        );
        assert_eq!(
            modifiers_from_state(0x8 | 0x40),
            vec![Modifier::Alt, Modifier::Cmd]
        );
    }

    #[test]
    fn cursor_pixels_are_unpremultiplied() {
        // Opaque red, half-transparent premultiplied white (0x80 each), fully transparent.
        let px = [0xffff_0000u32, 0x8080_8080, 0x0000_0000];
        let rgba = argb_premultiplied_to_rgba(&px);
        assert_eq!(&rgba[0..4], &[255, 0, 0, 255]);
        assert_eq!(&rgba[4..8], &[255, 255, 255, 128]);
        assert_eq!(&rgba[8..12], &[0, 0, 0, 0]);
    }
}
