//! Windows: how the recording windows stack and which extended styles the full-monitor area
//! window needs, as pure arithmetic (the Win32 calls are in `platform_win_monitors`).
//!
//! GPUI 0.3.9 creates every pop-up with `WS_EX_TOOLWINDOW | WS_EX_TOPMOST`, so the HUD, the
//! bubble and the area window all sit in the topmost band, and inside the band the one shown
//! last is on top. The area window is shown after the HUD and covers the whole recorded
//! monitor; without click-through it swallowed every click aimed at the HUD (and at the apps
//! behind it). The area window therefore becomes a layered, transparent, non-activating
//! window, and the order is made explicit: area window lowest, HUD highest.

pub const WS_EX_TOPMOST: u32 = 0x0000_0008;
pub const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
pub const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
pub const WS_EX_LAYERED: u32 = 0x0008_0000;
pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
pub const WS_EX_NOREDIRECTIONBITMAP: u32 = 0x0020_0000;

/// The recording windows, lowest in the stack first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// The full-monitor outline window.
    Area,
    /// The webcam bubble.
    Bubble,
    /// The control bar.
    Hud,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Area => "area",
            Role::Bubble => "bubble",
            Role::Hud => "hud",
        }
    }
}

/// The order in which to raise the open recording windows so the stack ends as
/// area < bubble < HUD (each `SetWindowPos(HWND_TOPMOST)` puts a window on top of the band,
/// so the last one raised is the highest).
pub fn raise_order(open: &[Role]) -> Vec<Role> {
    let mut v = open.to_vec();
    v.sort();
    v.dedup();
    v
}

/// The extended style the area window gets: layered (so `WS_EX_TRANSPARENT` makes the mouse
/// fall through to the window below, even another process's), never activated, kept out of
/// Alt-Tab and the taskbar, topmost. `WS_EX_NOREDIRECTIONBITMAP` (GPUI's DirectComposition
/// surface) is left as it is.
pub fn area_ex_style(old: u32) -> u32 {
    old | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST
}

/// The style flags of a window as short names, for `desktop.log`.
pub fn describe_ex(ex: u32) -> String {
    let names = [
        (WS_EX_TOPMOST, "TOPMOST"),
        (WS_EX_TRANSPARENT, "TRANSPARENT"),
        (WS_EX_TOOLWINDOW, "TOOLWINDOW"),
        (WS_EX_LAYERED, "LAYERED"),
        (WS_EX_NOACTIVATE, "NOACTIVATE"),
        (WS_EX_NOREDIRECTIONBITMAP, "NOREDIRECTIONBITMAP"),
    ];
    let v: Vec<&str> = names
        .iter()
        .filter(|(bit, _)| ex & bit != 0)
        .map(|(_, n)| *n)
        .collect();
    format!("0x{ex:08x} [{}]", v.join("|"))
}

/// Position of each handle in a top-to-bottom list of top-level windows (0 = highest);
/// `None` when it is not in the list.
pub fn z_indices(top_to_bottom: &[isize], wanted: &[isize]) -> Vec<Option<usize>> {
    wanted
        .iter()
        .map(|w| top_to_bottom.iter().position(|h| h == w))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hud_is_raised_last_whatever_the_open_order() {
        assert_eq!(
            raise_order(&[Role::Hud, Role::Area, Role::Bubble]),
            vec![Role::Area, Role::Bubble, Role::Hud]
        );
        assert_eq!(raise_order(&[Role::Hud]), vec![Role::Hud]);
        assert_eq!(raise_order(&[Role::Hud, Role::Hud]), vec![Role::Hud]);
        assert!(raise_order(&[]).is_empty());
    }

    #[test]
    fn the_area_window_becomes_click_through_and_keeps_gpuis_flags() {
        let gpui = WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOREDIRECTIONBITMAP;
        let new = area_ex_style(gpui);
        for f in [
            WS_EX_LAYERED,
            WS_EX_TRANSPARENT,
            WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW,
            WS_EX_TOPMOST,
            WS_EX_NOREDIRECTIONBITMAP,
        ] {
            assert_ne!(new & f, 0, "{f:#x}");
        }
        assert_eq!(area_ex_style(new), new, "idempotent");
    }

    #[test]
    fn style_and_stack_descriptions() {
        let s = describe_ex(WS_EX_LAYERED | WS_EX_TOPMOST);
        assert!(s.contains("TOPMOST|LAYERED"), "{s}");
        assert_eq!(
            z_indices(&[7, 3, 9], &[9, 7, 1]),
            vec![Some(2), Some(0), None]
        );
    }
}
