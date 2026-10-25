//! The windows the user picked most recently. Window ids change between runs, so a window is
//! remembered by `{ app, title }`; when only the title changed (a document renamed, a tab
//! switched) the app alone still matches, with a worse rank than an exact match.

use serde_json::{json, Value};

/// How many identities are kept.
pub const MAX_LAST_USED: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowIdentity {
    pub app: String,
    pub title: String,
}

/// Most recent first, at most [`MAX_LAST_USED`], no exact duplicates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LastUsed {
    entries: Vec<WindowIdentity>,
}

impl LastUsed {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[WindowIdentity] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Record a pick: moves an identical `{ app, title }` to the front, else inserts at the
    /// front and drops the oldest beyond the cap.
    pub fn touch(&mut self, app: &str, title: &str) {
        self.entries.retain(|e| !(e.app == app && e.title == title));
        self.entries.insert(
            0,
            WindowIdentity {
                app: app.to_string(),
                title: title.to_string(),
            },
        );
        self.entries.truncate(MAX_LAST_USED);
    }

    /// Position of a window in the memory (0 = most recent), or `None`. An exact `{ app, title }`
    /// match always wins; otherwise the most recent entry of the same app (title changed).
    pub fn rank_of(&self, app: &str, title: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.app == app && e.title == title)
            .or_else(|| self.entries.iter().position(|e| e.app == app))
    }

    /// A JSON array of `{ "app": .., "title": .. }`, most recent first.
    pub fn to_value(&self) -> Value {
        Value::Array(
            self.entries
                .iter()
                .map(|e| json!({ "app": e.app, "title": e.title }))
                .collect(),
        )
    }

    /// Reads what [`to_value`](Self::to_value) wrote. Anything else yields an empty memory;
    /// entries that are not objects with string `app` and `title` are skipped; duplicates are
    /// dropped (first wins) and the cap applies.
    pub fn from_value(value: &Value) -> Self {
        let mut out = Self::new();
        let Some(items) = value.as_array() else {
            return out;
        };
        for item in items {
            let (Some(app), Some(title)) = (
                item.get("app").and_then(Value::as_str),
                item.get("title").and_then(Value::as_str),
            ) else {
                continue;
            };
            if app.is_empty() && title.is_empty() {
                continue;
            }
            if out.entries.iter().any(|e| e.app == app && e.title == title) {
                continue;
            }
            if out.entries.len() == MAX_LAST_USED {
                break;
            }
            out.entries.push(WindowIdentity {
                app: app.to_string(),
                title: title.to_string(),
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_puts_newest_first_and_dedupes() {
        let mut l = LastUsed::new();
        l.touch("Safari", "A");
        l.touch("Finder", "B");
        l.touch("Safari", "A");
        let apps: Vec<_> = l.entries().iter().map(|e| e.app.as_str()).collect();
        assert_eq!(apps, ["Safari", "Finder"]);
    }

    #[test]
    fn cap_is_eight() {
        let mut l = LastUsed::new();
        for i in 0..20 {
            l.touch("App", &format!("t{i}"));
        }
        assert_eq!(l.entries().len(), MAX_LAST_USED);
        assert_eq!(l.entries()[0].title, "t19");
        assert_eq!(l.entries()[7].title, "t12");
    }

    #[test]
    fn rank_exact_then_app_fallback_then_none() {
        let mut l = LastUsed::new();
        l.touch("Finder", "Docs");
        l.touch("Safari", "Home");
        assert_eq!(l.rank_of("Safari", "Home"), Some(0));
        assert_eq!(l.rank_of("Finder", "Docs"), Some(1));
        assert_eq!(l.rank_of("Finder", "Renamed"), Some(1));
        assert_eq!(l.rank_of("Mail", "Inbox"), None);
        assert_eq!(LastUsed::new().rank_of("a", "b"), None);
    }

    #[test]
    fn exact_beats_earlier_app_only() {
        let mut l = LastUsed::new();
        l.touch("Safari", "Old");
        l.touch("Safari", "New");
        assert_eq!(l.rank_of("Safari", "Old"), Some(1));
        assert_eq!(l.rank_of("Safari", "Other"), Some(0));
    }

    #[test]
    fn value_round_trip() {
        let mut l = LastUsed::new();
        l.touch("Safari", "Tài liệu");
        l.touch("Finder", "");
        let v = l.to_value();
        assert_eq!(v[0]["app"], "Finder");
        assert_eq!(LastUsed::from_value(&v), l);
    }

    #[test]
    fn junk_does_not_panic() {
        for v in [
            json!(null),
            json!(5),
            json!("x"),
            json!({"app": "a"}),
            json!([1, "x", null, [], {"app": 1, "title": 2}, {"app": "a"}]),
        ] {
            assert!(LastUsed::from_value(&v).is_empty(), "{v}");
        }
        let mixed =
            json!([1, {"app": "A", "title": "t"}, {"app": "A", "title": "t"}, {"title": "x"}]);
        assert_eq!(LastUsed::from_value(&mixed).entries().len(), 1);
    }

    #[test]
    fn from_value_applies_cap() {
        let arr: Vec<_> = (0..30)
            .map(|i| json!({"app": "A", "title": i.to_string()}))
            .collect();
        let l = LastUsed::from_value(&Value::Array(arr));
        assert_eq!(l.entries().len(), MAX_LAST_USED);
        assert_eq!(l.entries()[0].title, "0");
    }
}
