//! The Auto colour of a new Text annotation (ANN-1.AC8, design request D10) in the display-free
//! state: when a text is placed with Auto on, the state asks for the picture under the box to be
//! sampled once (the document answers, see `ui/editor_doc/touch.rs`), stores the answer in the
//! text being typed, and remembers where the colour came from for the panel. Typing never asks
//! again; the colour is a stored value of the op, never a render-time guess.

use super::{EditorState, TextTarget};
use crate::editor::tools::Tool;
use cm_core::{Color, Pt};
use cm_i18n::tr;
use cm_render::auto_text::{AUTO_DARK, AUTO_LIGHT};
use cm_render::AutoText;

/// A sample the editor wants taken (once per placed text, or when Auto is chosen again).
#[derive(Debug, Clone, PartialEq)]
pub struct AutoRequest {
    /// Answers for an older request are dropped.
    pub seq: u64,
    pub at: Pt,
    pub size: f64,
}

/// Where the colour of the text being typed came from (editor state only: never stored).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoPick {
    pub color: Color,
    /// The picture under the box is light (so the text is dark).
    pub light_picture: bool,
    pub busy: bool,
}

/// Everything the editor keeps for Auto between events.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextAuto {
    pub pick: Option<AutoPick>,
    request: Option<AutoRequest>,
    seq: u64,
}

/// Which of the Auto / Light / Dark buttons is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoMode {
    Auto,
    Light,
    Dark,
    /// The user's own colour is in use.
    None,
}

/// What the Text panel shows for Auto.
#[derive(Debug, Clone, PartialEq)]
pub struct TextAutoInfo {
    pub mode: AutoMode,
    /// The selected swatch carries the AUTO tag (the colour came from the picture).
    pub tag: bool,
    /// The line under the palette.
    pub line: String,
    /// The outline of the text being typed (Auto turned it on), for the Outline line.
    pub outline: Option<Color>,
}

impl EditorState {
    /// A new text is placed: with Auto on, ask for the sample.
    pub(super) fn auto_on_new_text(&mut self, at: Pt, size: f64) {
        self.text_auto.pick = None;
        if self.styles.get(Tool::Text).auto_color {
            self.ask_auto(at, size);
        }
    }

    fn ask_auto(&mut self, at: Pt, size: f64) {
        self.text_auto.seq += 1;
        self.text_auto.request = Some(AutoRequest {
            seq: self.text_auto.seq,
            at,
            size,
        });
    }

    /// The sample the editor wants taken (once); the document answers with
    /// [`EditorState::auto_text_measured`].
    pub fn take_auto_request(&mut self) -> Option<AutoRequest> {
        self.text_auto.request.take()
    }

    /// The sample was taken: the text being typed gets the colour (and outline) unless the user
    /// chose a colour meanwhile, or the text is gone.
    pub fn auto_text_measured(&mut self, seq: u64, a: AutoText) {
        if seq != self.text_auto.seq || !self.styles.get(Tool::Text).auto_color {
            return;
        }
        let Some(t) = self.text.as_mut() else {
            return;
        };
        if !matches!(t.target, TextTarget::Text) || t.existing.is_some() {
            return;
        }
        t.color = a.color;
        t.outline = a.outline;
        self.text_auto.pick = Some(AutoPick {
            color: a.color,
            light_picture: a.color == AUTO_DARK,
            busy: a.busy,
        });
    }

    /// The Auto button: the automatic choice is on again for this text (if one is open) and the
    /// next ones.
    pub fn choose_auto(&mut self) {
        self.styles.set_text_auto(true);
        self.styles_dirty = true;
        self.text_auto.pick = None;
        let open = self.text.as_ref().and_then(|t| {
            (matches!(t.target, TextTarget::Text) && t.existing.is_none()).then_some((t.at, t.size))
        });
        if let Some((at, size)) = open {
            self.ask_auto(at, size);
        }
    }

    /// The user picked a colour by hand while a new text is open: Auto's outline goes with it.
    pub(super) fn auto_dropped_by_user(&mut self) {
        if let Some(p) = self.text_auto.pick.take() {
            if p.busy {
                if let Some(t) = self.text.as_mut() {
                    t.outline = None;
                }
            }
        }
    }

    /// What the Text panel shows for Auto, when it applies: a new text being typed, or the Text
    /// tool's settings with nothing selected.
    pub fn text_auto_info(&self) -> Option<TextAutoInfo> {
        if self.inspector_tool() != Tool::Text {
            return None;
        }
        let (color, outline) = match &self.text {
            Some(t) if matches!(t.target, TextTarget::Text) && t.existing.is_none() => {
                (t.color, t.outline)
            }
            Some(_) => return None,
            None if self.selected.is_none() && self.tool == Tool::Text => {
                (self.styles.get(Tool::Text).color, None)
            }
            None => return None,
        };
        let auto = self.styles.get(Tool::Text).auto_color;
        let mode = if auto {
            AutoMode::Auto
        } else if color == AUTO_LIGHT {
            AutoMode::Light
        } else if color == AUTO_DARK {
            AutoMode::Dark
        } else {
            AutoMode::None
        };
        let pick = self.text_auto.pick.filter(|p| auto && p.color == color);
        let line = match (auto, pick) {
            (false, _) => tr!("Your last colour (Text tool)"),
            (true, Some(p)) if p.busy => tr!("Auto: busy picture, outline added"),
            (true, Some(p)) if p.light_picture => tr!("Auto: picked from the picture (light)"),
            (true, Some(_)) => tr!("Auto: picked from the picture (dark)"),
            (true, None) => tr!("Auto: picked from the picture when text is placed"),
        };
        Some(TextAutoInfo {
            mode,
            tag: pick.is_some(),
            line,
            outline,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::keys::Mods;
    use crate::editor::state::Edit;
    use crate::editor::tools::{ParamValue, Styles};
    use cm_core::OpKind;
    use image::{Rgba, RgbaImage};

    const NONE: Mods = Mods {
        shift: false,
        alt: false,
        secondary: false,
    };

    fn st() -> EditorState {
        EditorState::new((1200, 800), Vec::new(), Styles::default())
    }

    fn solid(c: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(1200, 800, Rgba(c))
    }

    fn busy() -> RgbaImage {
        let mut img = solid([255, 255, 255, 255]);
        for (x, _, p) in img.enumerate_pixels_mut() {
            if x % 2 == 0 {
                *p = Rgba([0, 0, 0, 255]);
            }
        }
        img
    }

    /// Places a text at (100, 100) and answers the sample the way the document does.
    fn place(s: &mut EditorState, img: &RgbaImage) {
        s.set_tool(Tool::Text);
        s.pointer_down(Pt::new(100.0, 100.0), NONE, 1, 4.0);
        if let Some(req) = s.take_auto_request() {
            let a = cm_render::auto_text_style(img, (req.at.x, req.at.y), req.size);
            s.auto_text_measured(req.seq, a);
        }
    }

    fn open_text(s: &EditorState) -> (Color, Option<Color>) {
        let t = s.text.as_ref().unwrap();
        (t.color, t.outline)
    }

    fn type_and_finish(s: &mut EditorState, word: &str) -> OpKind {
        for c in word.chars() {
            s.text_ime(None, &c.to_string(), false);
        }
        let Some(Edit::Append(ops)) = s.finish_text() else {
            panic!("no append")
        };
        ops[0].kind.clone()
    }

    #[test]
    fn auto_on_a_light_picture_is_dark_and_tagged() {
        let mut s = st();
        place(&mut s, &solid([245, 245, 245, 255]));
        assert_eq!(open_text(&s), (AUTO_DARK, None));
        let info = s.text_auto_info().unwrap();
        assert_eq!(info.mode, AutoMode::Auto);
        assert!(info.tag);
        assert_eq!(info.line, "Auto: picked from the picture (light)");
        let OpKind::Text { color, outline, .. } = type_and_finish(&mut s, "Hi") else {
            panic!()
        };
        assert_eq!((color, outline), (AUTO_DARK, None));
        // The AUTO tag is editor state only: gone once the text is finished.
        assert!(s.text_auto.pick.is_none());
    }

    #[test]
    fn auto_on_a_dark_picture_is_light() {
        let mut s = st();
        place(&mut s, &solid([0x1E, 0x1E, 0x1E, 255]));
        assert_eq!(open_text(&s), (AUTO_LIGHT, None));
        assert_eq!(
            s.text_auto_info().unwrap().line,
            "Auto: picked from the picture (dark)"
        );
    }

    #[test]
    fn auto_on_a_busy_picture_adds_the_outline() {
        let mut s = st();
        place(&mut s, &busy());
        let (c, o) = open_text(&s);
        assert!(c == AUTO_DARK || c == AUTO_LIGHT);
        assert!(o.is_some() && o != Some(c));
        let info = s.text_auto_info().unwrap();
        assert_eq!(info.line, "Auto: busy picture, outline added");
        assert_eq!(info.outline, o);
        let OpKind::Text { outline, .. } = type_and_finish(&mut s, "Hi") else {
            panic!()
        };
        assert_eq!(outline, o);
    }

    #[test]
    fn a_colour_picked_by_hand_wins_and_is_remembered() {
        let mut s = st();
        let orange = Color::rgb(0xFF, 0x8A, 0x00);
        place(&mut s, &busy());
        s.set_param(ParamValue::Color(orange));
        // Auto's outline goes with Auto; the tag and the Auto button are off.
        assert_eq!(open_text(&s), (orange, None));
        let info = s.text_auto_info().unwrap();
        assert_eq!(info.mode, AutoMode::None);
        assert!(!info.tag);
        assert_eq!(info.line, "Your last colour (Text tool)");
        type_and_finish(&mut s, "a");
        // The next texts start with the user's colour, whatever the picture; nothing is sampled.
        place(&mut s, &solid([245, 245, 245, 255]));
        assert_eq!(open_text(&s), (orange, None));
        assert!(s.take_auto_request().is_none());
        // Light and Dark are colours too: Light is selected, Auto is off.
        s.set_param(ParamValue::Color(AUTO_LIGHT));
        assert_eq!(s.text_auto_info().unwrap().mode, AutoMode::Light);
        s.set_param(ParamValue::Color(AUTO_DARK));
        assert_eq!(s.text_auto_info().unwrap().mode, AutoMode::Dark);
        // The choice persists with the styles.
        let again = Styles::from_json(&s.styles.to_json());
        assert!(!again.get(Tool::Text).auto_color);
        assert_eq!(again.get(Tool::Text).color, AUTO_DARK);
    }

    #[test]
    fn clicking_auto_restores_the_automatic_choice() {
        let mut s = st();
        place(&mut s, &solid([245, 245, 245, 255]));
        s.set_param(ParamValue::Color(Color::rgb(0xFF, 0x3B, 0x3B)));
        assert_eq!(s.text_auto_info().unwrap().mode, AutoMode::None);
        // Auto while the text is open: sampled again for this text.
        s.choose_auto();
        let req = s.take_auto_request().expect("a new sample");
        let a = cm_render::auto_text_style(&solid([0, 0, 0, 255]), (req.at.x, req.at.y), req.size);
        s.auto_text_measured(req.seq, a);
        assert_eq!(open_text(&s), (AUTO_LIGHT, None));
        assert_eq!(s.text_auto_info().unwrap().mode, AutoMode::Auto);
        type_and_finish(&mut s, "a");
        // And for the next ones, each sampled afresh.
        place(&mut s, &solid([250, 250, 250, 255]));
        assert_eq!(open_text(&s).0, AUTO_DARK);
        s.finish_text();
        place(&mut s, &solid([10, 10, 10, 255]));
        assert_eq!(open_text(&s).0, AUTO_LIGHT);
    }

    #[test]
    fn typing_does_not_sample_again() {
        let mut s = st();
        place(&mut s, &solid([245, 245, 245, 255]));
        for c in "Some longer text".chars() {
            s.text_ime(None, &c.to_string(), false);
            assert!(s.take_auto_request().is_none());
        }
        // A late answer of an older request is ignored.
        let old = s.text_auto.seq;
        let a = cm_render::auto_text_style(&solid([0, 0, 0, 255]), (0.0, 0.0), 24.0);
        s.auto_text_measured(old + 1, a);
        assert_eq!(open_text(&s).0, AUTO_DARK);
    }

    #[test]
    fn existing_text_and_other_tools_are_not_touched() {
        let mut s = st();
        place(&mut s, &solid([245, 245, 245, 255]));
        type_and_finish(&mut s, "keep");
        let before = s.ops.clone();
        // Re-editing the text asks for nothing and shows no Auto control.
        s.pointer_down(Pt::new(102.0, 110.0), NONE, 2, 4.0);
        assert!(s.take_auto_request().is_none());
        assert!(s.text_auto_info().is_none());
        s.finish_text();
        assert_eq!(s.ops, before);
        // The Callout keeps its own default.
        assert_eq!(
            s.styles.get(Tool::Callout).color,
            crate::editor::tools::ToolStyle::defaults(Tool::Callout).color
        );
    }
}
