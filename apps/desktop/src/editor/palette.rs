//! The grouped tool palette (0.3.3 design spec section 2, board N6): the tools sit in eight
//! buttons, each button shows the tool used last in its group, and every tool key still picks
//! its tool directly. Pure data and rules, so it is tested without a window.

use super::tools::Tool;
use cm_i18n::tr_ctx;
use serde_json::{Map, Value};

/// The blue "new" dot on groups that hold a tool new in this release. It carries no per-user
/// state: it is shown for the whole release. **Remove in 0.3.4** (delete this constant, the
/// `Tool::is_new` arms and the dot in `ui/editor_view/palette.rs`).
pub const RELEASE_NEW_DOT_UNTIL: &str = "0.3.4";

/// Whether this build still shows the "new" dot (see [`RELEASE_NEW_DOT_UNTIL`]).
pub const SHOW_NEW_DOT: bool = true;

/// The `editor.palette_faces` setting: the tool each group's button shows.
pub const FACES_KEY: &str = "editor.palette_faces";

/// One palette button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    Select,
    Shapes,
    Text,
    Highlighter,
    Markers,
    Emphasis,
    Redact,
    Crop,
}

impl Group {
    /// Top to bottom, as drawn.
    pub const ALL: [Group; 8] = [
        Group::Select,
        Group::Shapes,
        Group::Text,
        Group::Highlighter,
        Group::Markers,
        Group::Emphasis,
        Group::Redact,
        Group::Crop,
    ];

    pub fn index(self) -> usize {
        Group::ALL.iter().position(|g| *g == self).unwrap_or(0)
    }

    /// Stable id for the setting file and element ids.
    pub fn id(self) -> &'static str {
        match self {
            Group::Select => "select",
            Group::Shapes => "shapes",
            Group::Text => "text",
            Group::Highlighter => "highlighter",
            Group::Markers => "markers",
            Group::Emphasis => "emphasis",
            Group::Redact => "redact",
            Group::Crop => "crop",
        }
    }

    pub fn label(self) -> String {
        match self {
            Group::Select => tr_ctx!("tool group", "Select"),
            Group::Shapes => tr_ctx!("tool group", "Shapes"),
            Group::Text => tr_ctx!("tool group", "Text"),
            Group::Highlighter => tr_ctx!("tool group", "Highlighter"),
            Group::Markers => tr_ctx!("tool group", "Markers"),
            Group::Emphasis => tr_ctx!("tool group", "Emphasis"),
            Group::Redact => tr_ctx!("tool group", "Redact"),
            Group::Crop => tr_ctx!("tool group", "Crop"),
        }
    }

    /// The tools of the group in menu order; the first is the default face. A group with no
    /// tool in this build is not drawn.
    pub fn tools(self) -> &'static [Tool] {
        match self {
            Group::Select => &[Tool::Select, Tool::SelectText],
            Group::Shapes => &[Tool::Arrow, Tool::Line, Tool::Rect, Tool::Ellipse],
            Group::Text => &[Tool::Text, Tool::Callout],
            Group::Highlighter => &[Tool::Highlight],
            Group::Markers => &[Tool::Step, Tool::Stamp],
            Group::Emphasis => &[Tool::Spotlight],
            Group::Redact => &[Tool::Blur, Tool::Pixelate],
            Group::Crop => &[Tool::Crop],
        }
    }

    /// Whether the palette draws this button.
    pub fn visible(self) -> bool {
        !self.tools().is_empty()
    }

    /// More than one tool: the button carries the corner mark and a menu.
    pub fn has_menu(self) -> bool {
        self.tools().len() > 1
    }

    /// A divider is drawn above this button (before Shapes, Emphasis, Redact and Crop).
    pub fn starts_section(self) -> bool {
        matches!(
            self,
            Group::Shapes | Group::Emphasis | Group::Redact | Group::Crop
        )
    }

    /// Whether the blue "new" dot is drawn on the button.
    pub fn has_new_tool(self) -> bool {
        SHOW_NEW_DOT && self.tools().iter().any(|t| t.is_new())
    }

    pub fn default_face(self) -> Tool {
        self.tools().first().copied().unwrap_or(Tool::Select)
    }

    /// The groups the palette draws.
    pub fn visible_groups() -> impl Iterator<Item = Group> {
        Group::ALL.into_iter().filter(|g| g.visible())
    }
}

impl Tool {
    /// The palette button this tool belongs to.
    pub fn group(self) -> Group {
        Group::ALL
            .into_iter()
            .find(|g| g.tools().contains(&self))
            .unwrap_or(Group::Select)
    }

    /// Whether some palette group offers this tool.
    pub fn in_palette(self) -> bool {
        Group::ALL.iter().any(|g| g.tools().contains(&self))
    }

    /// New in 0.3.3 (drives the blue dot; remove with it in 0.3.4).
    pub fn is_new(self) -> bool {
        matches!(self, Tool::Callout | Tool::Spotlight | Tool::Stamp)
    }
}

/// The tool each group's button shows, remembered per user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Faces([Tool; 8]);

impl Default for Faces {
    fn default() -> Self {
        Faces(Group::ALL.map(Group::default_face))
    }
}

impl Faces {
    pub fn get(&self, g: Group) -> Tool {
        self.0[g.index()]
    }

    /// `tool` became the active tool: it is now the face of its group. Returns whether the
    /// remembered face changed (the caller then persists).
    pub fn note(&mut self, tool: Tool) -> bool {
        let g = tool.group();
        let slot = &mut self.0[g.index()];
        if *slot == tool {
            return false;
        }
        *slot = tool;
        true
    }

    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        for g in Group::ALL {
            m.insert(g.id().to_owned(), Value::from(self.get(g).id()));
        }
        Value::Object(m)
    }

    /// Reads a stored value. An unknown tool, a tool of another group or garbage falls back to
    /// the group's default face.
    pub fn from_json(v: &Value) -> Faces {
        let mut out = Faces::default();
        for g in Group::ALL {
            let stored = v.get(g.id()).and_then(Value::as_str);
            if let Some(t) = stored.and_then(|id| g.tools().iter().find(|t| t.id() == id)) {
                out.0[g.index()] = *t;
            }
        }
        out
    }
}

/// The position of `tool` in its group's menu, and the next / previous one (arrow keys wrap).
pub fn menu_step(g: Group, cur: usize, forward: bool) -> usize {
    let n = g.tools().len().max(1);
    if forward {
        (cur + 1) % n
    } else {
        (cur + n - 1) % n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Design spec section 2: every tool is in exactly one group, in the order of the table.
    #[test]
    fn every_tool_is_in_exactly_one_group() {
        for t in Tool::ALL.into_iter().filter(|t| t.in_palette()) {
            let n = Group::ALL.iter().filter(|g| g.tools().contains(&t)).count();
            assert_eq!(n, 1, "{t:?}");
            assert!(t.group().tools().contains(&t));
        }
        let in_groups: usize = Group::ALL.iter().map(|g| g.tools().len()).sum();
        assert_eq!(
            in_groups,
            Tool::ALL.iter().filter(|t| t.in_palette()).count()
        );
    }

    /// Board N6: the groups, their tools and the dividers.
    #[test]
    fn groups_match_the_board() {
        let ids = |g: Group| g.tools().iter().map(|t| t.id()).collect::<Vec<_>>();
        assert_eq!(ids(Group::Select), ["select", "select_text"]);
        assert_eq!(ids(Group::Shapes), ["arrow", "line", "rect", "ellipse"]);
        assert_eq!(ids(Group::Text), ["text", "callout"]);
        assert_eq!(ids(Group::Markers), ["step", "stamp"]);
        assert_eq!(ids(Group::Emphasis), ["spotlight"]);
        assert_eq!(ids(Group::Highlighter), ["highlight"]);
        assert_eq!(ids(Group::Redact), ["blur", "pixelate"]);
        assert_eq!(ids(Group::Crop), ["crop"]);
        let dividers: Vec<_> = Group::ALL
            .iter()
            .filter(|g| g.starts_section())
            .map(|g| g.id())
            .collect();
        assert_eq!(dividers, ["shapes", "emphasis", "redact", "crop"]);
        assert!(Group::Shapes.has_menu() && !Group::Crop.has_menu());
    }

    #[test]
    fn default_faces_are_the_first_tool() {
        let f = Faces::default();
        assert_eq!(f.get(Group::Select), Tool::Select);
        assert_eq!(f.get(Group::Shapes), Tool::Arrow);
        assert_eq!(f.get(Group::Redact), Tool::Blur);
    }

    /// Every key picks its tool directly and updates the face of its group.
    #[test]
    fn picking_a_tool_updates_only_its_group_face() {
        let mut f = Faces::default();
        assert!(f.note(Tool::Rect));
        assert_eq!(f.get(Group::Shapes), Tool::Rect);
        assert_eq!(f.get(Group::Redact), Tool::Blur);
        assert!(!f.note(Tool::Rect), "unchanged: nothing to persist");
        assert!(f.note(Tool::Pixelate));
        assert_eq!(f.get(Group::Redact), Tool::Pixelate);
    }

    /// The face is remembered per user: it survives the settings round trip, and garbage in the
    /// file never breaks the palette.
    #[test]
    fn faces_persist_and_garbage_falls_back() {
        let mut f = Faces::default();
        f.note(Tool::Ellipse);
        f.note(Tool::SelectText);
        assert_eq!(Faces::from_json(&f.to_json()), f);
        // A tool of another group, an unknown id and a wrong type are ignored.
        let junk = json!({"shapes": "blur", "redact": 7, "select": "nope", "crop": "crop"});
        assert_eq!(Faces::from_json(&junk), Faces::default());
        assert_eq!(Faces::from_json(&json!("x")), Faces::default());
    }

    #[test]
    fn menu_arrow_keys_wrap() {
        assert_eq!(menu_step(Group::Shapes, 3, true), 0);
        assert_eq!(menu_step(Group::Shapes, 0, false), 3);
        assert_eq!(menu_step(Group::Select, 0, true), 1);
    }

    #[test]
    fn group_labels_in_english() {
        assert_eq!(Group::Shapes.label(), "Shapes");
        assert_eq!(Tool::Arrow.group().label(), "Shapes");
    }
}
