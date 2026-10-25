//! Keyboard map of the editor, as a pure function so it is testable without a display
//! (NFR-13: everything is reachable by keyboard).
//!
//! `secondary` is Cmd on macOS and Ctrl on Windows.

use super::tools::Tool;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub secondary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCommand {
    SetTool(Tool),
    Delete,
    Undo,
    Redo,
    /// Cmd/Ctrl+C: copies the selected op, or the image when nothing is selected.
    Copy,
    /// Shift+Cmd/Ctrl+C: always the rendered image.
    CopyImage,
    Paste,
    Duplicate,
    SelectNext,
    SelectPrev,
    /// Arrow keys: move the selection; with Alt resize it. `big` is Shift (10 px).
    Nudge {
        dx: i32,
        dy: i32,
        resize: bool,
        big: bool,
    },
    /// Enter on the canvas: place a default-sized op of the active tool at the view center.
    PlaceDefault,
    Escape,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    ZoomActual,
    Back,
    SaveAs,
}

/// Maps a key press to a command.
///
/// - `key` is GPUI's key name (`"a"`, `"delete"`, `"left"`, `"["`...).
/// - `canvas_focused`: the canvas (not a button) has focus, so Enter, Delete and arrows act on
///   the canvas instead of the focused control.
/// - Single letters only switch tools without modifiers, so they never fight with text
///   entry (the caller routes keys to the text editor first while one is open).
pub fn interpret(key: &str, mods: Mods, canvas_focused: bool) -> Option<KeyCommand> {
    use KeyCommand as K;
    if mods.secondary {
        return match (key, mods.shift) {
            ("z", false) => Some(K::Undo),
            ("z", true) | ("y", false) => Some(K::Redo),
            ("c", false) => Some(K::Copy),
            ("c", true) => Some(K::CopyImage),
            ("v", false) => Some(K::Paste),
            ("d", false) => Some(K::Duplicate),
            ("s", true) => Some(K::SaveAs),
            ("=" | "+", _) => Some(K::ZoomIn),
            ("-", _) => Some(K::ZoomOut),
            ("0", false) => Some(K::ZoomFit),
            ("1", false) => Some(K::ZoomActual),
            ("[", false) => Some(K::Back),
            _ => None,
        };
    }
    if mods.alt && !canvas_focused {
        return None;
    }
    match key {
        "escape" => Some(K::Escape),
        "delete" | "backspace" if canvas_focused => Some(K::Delete),
        "enter" if canvas_focused => Some(K::PlaceDefault),
        "[" => Some(K::SelectPrev),
        "]" => Some(K::SelectNext),
        "left" | "right" | "up" | "down" if canvas_focused => {
            let (dx, dy) = match key {
                "left" => (-1, 0),
                "right" => (1, 0),
                "up" => (0, -1),
                _ => (0, 1),
            };
            Some(K::Nudge {
                dx,
                dy,
                resize: mods.alt,
                big: mods.shift,
            })
        }
        k if !mods.alt => {
            let mut chars = k.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Tool::from_key(c).map(K::SetTool),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use KeyCommand as K;

    const NONE: Mods = Mods {
        shift: false,
        alt: false,
        secondary: false,
    };
    const CMD: Mods = Mods {
        shift: false,
        alt: false,
        secondary: true,
    };

    /// ANN-1 / ANN-2: every documented tool key switches tools.
    #[test]
    fn tool_letters_switch_tools() {
        for (k, t) in [
            ("v", Tool::Select),
            ("a", Tool::Arrow),
            ("l", Tool::Line),
            ("r", Tool::Rect),
            ("e", Tool::Ellipse),
            ("t", Tool::Text),
            ("h", Tool::Highlight),
            ("n", Tool::Step),
            ("c", Tool::Crop),
            ("b", Tool::Blur),
            ("x", Tool::Pixelate),
        ] {
            assert_eq!(interpret(k, NONE, false), Some(K::SetTool(t)));
            assert_eq!(interpret(k, NONE, true), Some(K::SetTool(t)));
        }
        assert_eq!(interpret("q", NONE, true), None);
    }

    /// ANN-3: Cmd/Ctrl-Z undoes, Shift-Cmd-Z (and Ctrl-Y) redoes.
    #[test]
    fn undo_redo_chords() {
        assert_eq!(interpret("z", CMD, false), Some(K::Undo));
        assert_eq!(
            interpret("z", Mods { shift: true, ..CMD }, false),
            Some(K::Redo)
        );
        assert_eq!(interpret("y", CMD, false), Some(K::Redo));
    }

    #[test]
    fn modified_letters_never_switch_tools() {
        assert_eq!(interpret("a", CMD, false), None);
        assert_eq!(interpret("r", CMD, false), None);
        assert_eq!(interpret("r", Mods { alt: true, ..NONE }, true), None);
    }

    /// ANN-1: Delete/Backspace delete the selection, but only from the canvas so a focused
    /// button keeps its own key handling.
    #[test]
    fn delete_and_arrows_need_canvas_focus() {
        assert_eq!(interpret("delete", NONE, true), Some(K::Delete));
        assert_eq!(interpret("backspace", NONE, true), Some(K::Delete));
        assert_eq!(interpret("delete", NONE, false), None);
        assert_eq!(interpret("enter", NONE, false), None);
        assert_eq!(interpret("enter", NONE, true), Some(K::PlaceDefault));
        assert_eq!(
            interpret(
                "left",
                Mods {
                    shift: true,
                    ..NONE
                },
                true
            ),
            Some(K::Nudge {
                dx: -1,
                dy: 0,
                resize: false,
                big: true
            })
        );
        assert_eq!(
            interpret("down", Mods { alt: true, ..NONE }, true),
            Some(K::Nudge {
                dx: 0,
                dy: 1,
                resize: true,
                big: false
            })
        );
        assert_eq!(interpret("left", NONE, false), None);
    }

    #[test]
    fn zoom_copy_and_navigation_chords() {
        assert_eq!(interpret("=", CMD, false), Some(K::ZoomIn));
        assert_eq!(interpret("-", CMD, false), Some(K::ZoomOut));
        assert_eq!(interpret("0", CMD, false), Some(K::ZoomFit));
        assert_eq!(interpret("1", CMD, false), Some(K::ZoomActual));
        assert_eq!(interpret("c", CMD, false), Some(K::Copy));
        assert_eq!(
            interpret("c", Mods { shift: true, ..CMD }, false),
            Some(K::CopyImage)
        );
        assert_eq!(interpret("v", CMD, false), Some(K::Paste));
        assert_eq!(interpret("]", NONE, false), Some(K::SelectNext));
        assert_eq!(interpret("[", NONE, false), Some(K::SelectPrev));
        assert_eq!(interpret("escape", NONE, false), Some(K::Escape));
        assert_eq!(interpret("[", CMD, false), Some(K::Back));
    }
}
