//! The editor's keyboard map (EDT-1: I and O set the in and out points at the playhead, S splits,
//! Delete cuts the selection, ←/→ move one frame and Shift+←/→ one second, N toggles snapping).
//! Display-free: GPUI's key names go in, a [`Cmd`] comes out.

use cm_i18n::tr_ctx;

/// What a key asks the editor to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    SetIn,
    SetOut,
    Split,
    /// Cut the selected range or segment, or restore the selected cut.
    Delete,
    /// Restore the selected cut (or the cuts the selected range touches).
    Restore,
    /// Add a zoom at the playhead, or over the selected range (EDT-4).
    AddZoom,
    ToggleSnap,
    PlayPause,
    /// Loop playback (L).
    ToggleLoop,
    Step {
        frames: i64,
    },
    StepSecs {
        secs: i64,
    },
    Home,
    End,
    Undo,
    Redo,
    ClearSelection,
}

/// Maps one key press. `key` is GPUI's lower-case key name (`"left"`, `"space"`, `"i"`, ...).
/// `cmd` is the platform shortcut modifier (Command on macOS, Control on Windows).
pub fn map_key(key: &str, shift: bool, cmd: bool, alt: bool) -> Option<Cmd> {
    if alt {
        return None;
    }
    if cmd {
        return match (key, shift) {
            ("z", false) => Some(Cmd::Undo),
            ("z", true) | ("y", false) => Some(Cmd::Redo),
            _ => None,
        };
    }
    Some(match key {
        "i" => Cmd::SetIn,
        "o" => Cmd::SetOut,
        "s" => Cmd::Split,
        "r" => Cmd::Restore,
        "z" => Cmd::AddZoom,
        "n" => Cmd::ToggleSnap,
        "l" => Cmd::ToggleLoop,
        "space" => Cmd::PlayPause,
        "delete" | "backspace" => Cmd::Delete,
        "left" if shift => Cmd::StepSecs { secs: -1 },
        "right" if shift => Cmd::StepSecs { secs: 1 },
        "left" => Cmd::Step { frames: -1 },
        "right" => Cmd::Step { frames: 1 },
        "home" => Cmd::Home,
        "end" => Cmd::End,
        "escape" => Cmd::ClearSelection,
        _ => return None,
    })
}

/// The keys shown in the toolbar tooltips and the help line (the key glyph stays, the
/// description is translated, so this is built when asked and follows the language).
pub fn help() -> Vec<(&'static str, String)> {
    vec![
        ("I", tr_ctx!("editor shortcut", "In point")),
        ("O", tr_ctx!("editor shortcut", "Out point")),
        ("S", tr_ctx!("editor shortcut", "Split")),
        ("\u{232B}", tr_ctx!("editor shortcut", "Cut")),
        ("R", tr_ctx!("editor shortcut", "Restore")),
        ("Z", tr_ctx!("editor shortcut", "Zoom")),
        ("N", tr_ctx!("editor shortcut", "Snapping")),
        ("L", tr_ctx!("editor shortcut", "Loop")),
        ("\u{2190}\u{2192}", tr_ctx!("editor shortcut", "Frame")),
        (
            "\u{21E7}\u{2190}\u{2192}",
            tr_ctx!("editor shortcut", "Second"),
        ),
    ]
}

/// The help strip under the timeline (redesign R3): one keycap and one word each. Restore (R)
/// and Snapping (N) are on the toolbar buttons and their tooltips instead.
pub fn help_shown() -> Vec<(&'static str, String)> {
    vec![
        // i18n-ignore (key name)
        ("Space", tr_ctx!("editor shortcut", "Play")),
        ("I", tr_ctx!("editor shortcut", "In")),
        ("O", tr_ctx!("editor shortcut", "Out")),
        ("S", tr_ctx!("editor shortcut", "Split")),
        ("\u{232B}", tr_ctx!("editor shortcut", "Cut")),
        ("Z", tr_ctx!("editor shortcut", "Zoom")),
        ("L", tr_ctx!("editor shortcut", "Loop")),
        ("\u{2190}\u{2192}", tr_ctx!("editor shortcut", "Frame")),
        (
            "\u{21E7}\u{2190}\u{2192}",
            tr_ctx!("editor shortcut", "Second"),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R3: every key of the help strip does what it says (and R and N, left off the strip, still
    /// work).
    #[test]
    fn help_strip_keys_are_real() {
        assert_eq!(map_key("space", false, false, false), Some(Cmd::PlayPause));
        assert_eq!(map_key("l", false, false, false), Some(Cmd::ToggleLoop));
        assert_eq!(map_key("r", false, false, false), Some(Cmd::Restore));
        assert_eq!(map_key("n", false, false, false), Some(Cmd::ToggleSnap));
        assert_eq!(help_shown().len(), 9);
        assert!(help_shown()
            .iter()
            .all(|(k, w)| !k.is_empty() && !w.is_empty()));
    }

    /// I18N P2: the key glyph stays, the description is the English word in English.
    #[test]
    fn help_texts_in_english() {
        let shown = help_shown();
        assert_eq!(shown[0], ("Space", "Play".to_owned()));
        assert_eq!(shown[3], ("S", "Split".to_owned()));
        let all = help();
        assert_eq!(all[0], ("I", "In point".to_owned()));
        assert_eq!(all[9].0, "\u{21E7}\u{2190}\u{2192}");
        assert_eq!(all[9].1, "Second");
    }

    /// EDT-1: the documented keys.
    #[test]
    fn edt1_documented_keys() {
        assert_eq!(map_key("i", false, false, false), Some(Cmd::SetIn));
        assert_eq!(map_key("o", false, false, false), Some(Cmd::SetOut));
        assert_eq!(map_key("s", false, false, false), Some(Cmd::Split));
        assert_eq!(map_key("delete", false, false, false), Some(Cmd::Delete));
        assert_eq!(map_key("backspace", false, false, false), Some(Cmd::Delete));
        assert_eq!(map_key("n", false, false, false), Some(Cmd::ToggleSnap));
        assert_eq!(
            map_key("left", false, false, false),
            Some(Cmd::Step { frames: -1 })
        );
        assert_eq!(
            map_key("right", true, false, false),
            Some(Cmd::StepSecs { secs: 1 })
        );
    }

    /// EDT-4: Z adds a zoom; with the platform modifier it is still Undo.
    #[test]
    fn edt4_z_adds_a_zoom() {
        assert_eq!(map_key("z", false, false, false), Some(Cmd::AddZoom));
        assert_eq!(map_key("z", false, true, false), Some(Cmd::Undo));
    }

    #[test]
    fn undo_and_redo_need_the_platform_modifier() {
        assert_eq!(map_key("z", false, true, false), Some(Cmd::Undo));
        assert_eq!(map_key("z", true, true, false), Some(Cmd::Redo));
        // Plain Z is the zoom command (EDT-4), not Undo.
        assert_eq!(map_key("z", false, false, false), Some(Cmd::AddZoom));
        // A modified letter is never the plain command: Cmd+S is not Split.
        assert_eq!(map_key("s", false, true, false), None);
        assert_eq!(map_key("i", false, false, true), None);
    }

    #[test]
    fn unknown_keys_do_nothing() {
        assert_eq!(map_key("q", false, false, false), None);
        assert_eq!(map_key("enter", false, false, false), None);
    }
}
