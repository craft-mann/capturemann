//! Display-free logic of the window picker (boards R19 "Window picker (screenshot)" and R20
//! "Record sheet > Window"): fuzzy search, filters, the app rail, keyboard navigation and the
//! last-used memory. The GPUI views build on this module; nothing here touches the platform.

pub mod last_used;
pub mod list;
pub mod session;
pub mod state;
pub mod thumbs;

pub use last_used::{LastUsed, WindowIdentity, MAX_LAST_USED};
pub use list::{
    app_for_shortcut, app_groups, fold, keep_selection, navigate, rank_window, visible, AppFilter,
    AppGroup, AppGroups, DisplayFilter, Layout, Nav,
};

/// One window as the picker sees it. The order of a slice of these is front to back, as
/// `list_targets` returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerWindow {
    pub id: u32,
    pub title: String,
    pub app: String,
    pub width: u32,
    pub height: u32,
    pub display_id: Option<u32>,
}

/// One display, for the display filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerDisplay {
    pub id: u32,
    pub name: String,
    pub builtin: bool,
}

/// "1680 × 1050": the multiplication sign U+00D7 with a space on both sides. Not translated.
pub fn format_size(width: u32, height: u32) -> String {
    format!("{width} \u{00D7} {height}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_uses_multiplication_sign() {
        assert_eq!(format_size(1680, 1050), "1680 \u{00D7} 1050");
        assert_eq!(format_size(0, 0), "0 \u{00D7} 0");
        assert!(format_size(3840, 2160).contains('\u{00D7}'));
    }
}
