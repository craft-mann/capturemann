//! The accessibility name of the clicked control (BUG-4, GDE-1): `Click "Save"` where the
//! system itself says the control is called Save, instead of the text nearest the click.
//!
//! The nearest on-screen text can be anything around the control (a name, an address in a
//! table), so a control that has an accessibility name is named by it and the screen text is
//! only the fallback. The lookup is platform code (macOS AX, Windows UI Automation) behind
//! [`AccessibleNames`]; tests use [`ScriptedNames`], so none of them needs a display or an
//! accessibility permission.

use std::sync::{Arc, Mutex, PoisonError};

use super::titles::MAX_LABEL_CHARS;

/// Asks the system what the control under a point is called.
pub trait AccessibleNames: Send + Sync {
    /// The accessibility name of the control at `(gx, gy)` in the OS's global pointer space
    /// (the space of [`LocalClick::gx`](super::LocalClick)), or `None` when the control has
    /// none, the point holds something that is not a control (a window, a page, a list row),
    /// the system gives no answer (no permission, no service) or the answer takes too long.
    /// Called on the click listener's thread at mouse-down.
    fn name_at(&self, gx: f64, gy: f64) -> Option<String>;
}

/// The default: no accessibility service, so titles come from the screen text.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoNames;

impl AccessibleNames for NoNames {
    fn name_at(&self, _: f64, _: f64) -> Option<String> {
        None
    }
}

/// Roles (macOS AX role names; Windows control types are filtered by their own list) whose name
/// is not a control's label but content or a container's title: the title of a window or a
/// page, the text of a list row, a cell or a paragraph. Those can hold personal data, so they
/// never name a step.
const CONTENT_ROLES: &[&str] = &[
    "AXApplication",
    "AXWindow",
    "AXSheet",
    "AXDrawer",
    "AXWebArea",
    "AXScrollArea",
    "AXSplitGroup",
    "AXGroup",
    "AXLayoutArea",
    "AXStaticText",
    "AXTextArea",
    "AXTable",
    "AXOutline",
    "AXList",
    "AXBrowser",
    "AXRow",
    "AXColumn",
    "AXCell",
    "AXUnknown",
];

/// Whether the name of an element of `role` may name a step.
pub fn role_may_name(role: &str) -> bool {
    !role.is_empty() && !CONTENT_ROLES.contains(&role)
}

/// `name` as the label of a title, or `None` when it is empty. White space is collapsed, double
/// quotes become single quotes (the title wraps the label in double quotes), control
/// characters go, and a name longer than [`MAX_LABEL_CHARS`] is cut with an ellipsis so the
/// title keeps its 40-character label limit. The shape rules of screen text (no digits, no
/// addresses) do not apply: the name belongs to the control, it was not picked up from around
/// it.
pub fn clean_name(name: &str) -> Option<String> {
    let t: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let t = t.replace('"', "'");
    if t.is_empty() {
        return None;
    }
    if t.chars().count() <= MAX_LABEL_CHARS {
        return Some(t);
    }
    let cut: String = t.chars().take(MAX_LABEL_CHARS - 1).collect();
    Some(format!("{}…", cut.trim_end()))
}

/// A scripted accessibility service for tests: names for rectangles of the global space, and a
/// count of the questions asked.
#[derive(Clone, Default)]
pub struct ScriptedNames {
    inner: Arc<Mutex<ScriptedState>>,
}

#[derive(Default)]
struct ScriptedState {
    /// `(x, y, w, h, name)`; the first rectangle holding the point answers.
    named: Vec<(f64, f64, f64, f64, String)>,
    asked: usize,
}

impl ScriptedNames {
    pub fn new() -> Self {
        Self::default()
    }

    /// The control in the rectangle is called `name`.
    #[must_use]
    pub fn with(self, x: f64, y: f64, w: f64, h: f64, name: &str) -> Self {
        self.lock().named.push((x, y, w, h, name.to_owned()));
        self
    }

    /// How many times a click asked.
    pub fn asked(&self) -> usize {
        self.lock().asked
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ScriptedState> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl AccessibleNames for ScriptedNames {
    fn name_at(&self, gx: f64, gy: f64) -> Option<String> {
        let mut g = self.lock();
        g.asked += 1;
        g.named
            .iter()
            .find(|(x, y, w, h, _)| gx >= *x && gx < x + w && gy >= *y && gy < y + h)
            .map(|n| n.4.clone())
    }
}

pub(crate) mod sys;

#[cfg(test)]
mod tests {
    use super::*;

    /// BUG-4 (GDE-1): a name is a label; the 40-character limit of titles holds.
    #[test]
    fn a_name_is_cleaned_into_a_label() {
        assert_eq!(clean_name("Save").as_deref(), Some("Save"));
        assert_eq!(clean_name("  Save \n as… ").as_deref(), Some("Save as…"));
        assert_eq!(clean_name("Say \"hi\"").as_deref(), Some("Say 'hi'"));
        assert_eq!(clean_name(""), None);
        assert_eq!(clean_name(" \t\n"), None);
        let long = clean_name(&"abcdefghij ".repeat(10)).unwrap();
        assert!(long.chars().count() <= MAX_LABEL_CHARS, "{long}");
        assert!(long.ends_with('…'), "{long}");
        assert_eq!(
            clean_name(&"x".repeat(MAX_LABEL_CHARS))
                .unwrap()
                .chars()
                .count(),
            MAX_LABEL_CHARS
        );
    }

    #[test]
    fn containers_and_content_never_name_a_step() {
        for role in [
            "AXWindow",
            "AXWebArea",
            "AXStaticText",
            "AXRow",
            "AXCell",
            "",
        ] {
            assert!(!role_may_name(role), "{role}");
        }
        for role in [
            "AXButton",
            "AXMenuItem",
            "AXCheckBox",
            "AXTextField",
            "AXLink",
        ] {
            assert!(role_may_name(role), "{role}");
        }
    }

    #[test]
    fn the_scripted_service_answers_by_rectangle_and_counts() {
        let n = ScriptedNames::new().with(10.0, 10.0, 50.0, 20.0, "Save");
        assert_eq!(n.name_at(20.0, 15.0).as_deref(), Some("Save"));
        assert_eq!(n.name_at(200.0, 15.0), None);
        assert_eq!(n.asked(), 2);
        assert_eq!(NoNames.name_at(1.0, 1.0), None);
    }
}
