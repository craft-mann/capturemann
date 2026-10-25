//! Per-tool permissions (AGT-2.AC9 to AC14, owner decision OD-24, Oct 25, 2026).
//!
//! The Settings -> Agents & CLI list shows **Allow** or **Ask first** for each tool. They are
//! labels over the two tiers that already exist: *Ask first* makes an agent's call wait for the
//! on-screen confirmation, *Allow* runs it. The user can change two things, both through the
//! app-only setting [`KEY`] (`agents.tool_policy`, a JSON object `{ "<tool>": "allow" | "ask" }`
//! that lists only the tools whose value differs from the default):
//!
//! * **Tighten** a Free tool that changes something (`mutating`) to `"ask"`.
//! * **Loosen** exactly one tool, `start_recording`, to `"allow"`.
//!
//! Everything else is locked: reading tools stay Allow (an ask-first on a polling read would be
//! unusable), and `publish_video`, `delete_capture`, `run_fps_selftest`, `download_model` and
//! `remove_model` stay Ask first (NFR-11). Internal plumbing actions are not tools and have no
//! entry. The locked rules are enforced **twice**: a write of a bad shape, an unknown or
//! internal tool, a value for a locked tool or a loosening of anything but `start_recording` is
//! refused ([`parse_strict`]), and a file that was edited by hand is read with [`parse_lenient`],
//! which ignores every entry the rules do not allow, so such an edit can never loosen publish or
//! delete. The user (`Actor::User`) is never asked; only agent calls are affected.

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

use cm_core::CmError;

use crate::action::{is_internal, spec_for, ActionSpec, Tier};

/// The setting key: app-only (`agents.*`), so an agent can neither read-modify nor write it.
pub const KEY: &str = "agents.tool_policy";

/// The kind of the generic confirmation of a tightened tool.
pub const KIND_ASK_FIRST: &str = "tool_ask_first";
/// `data.reason` of a Cancel on that dialog.
pub const DECLINED_REASON: &str = "ask_first";

/// Most key/value lines of the parameter summary.
pub const MAX_SUMMARY_LINES: usize = 8;
/// Longest value in the parameter summary, in characters.
pub const MAX_SUMMARY_VALUE_CHARS: usize = 120;

/// A tool's setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolSetting {
    /// Runs immediately.
    Allow,
    /// Pauses on the on-screen confirmation.
    Ask,
}

impl ToolSetting {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolSetting::Allow => "allow",
            ToolSetting::Ask => "ask",
        }
    }

    fn parse(s: &str) -> Option<ToolSetting> {
        match s {
            "allow" => Some(ToolSetting::Allow),
            "ask" => Some(ToolSetting::Ask),
            _ => None,
        }
    }
}

/// Whether a tool's setting can be changed, and if not, why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolControl {
    /// Can be set to either value; `default` is what the tier says.
    Editable { default: ToolSetting },
    /// A tool that only reads: Allow, locked ("Reading never needs asking.").
    LockedRead,
    /// Confirm tier other than `start_recording`: Ask first, locked ("Publishing and deleting
    /// always ask first.").
    LockedAsk,
    /// Plumbing for first-party clients, not a tool: no setting.
    Internal,
}

/// The control of an action.
pub fn control_of(spec: &ActionSpec) -> ToolControl {
    if is_internal(spec.name) {
        return ToolControl::Internal;
    }
    match (spec.tier, spec.mutating) {
        (Tier::Free, false) => ToolControl::LockedRead,
        (Tier::Free, true) => ToolControl::Editable {
            default: ToolSetting::Allow,
        },
        (Tier::Confirm, _) if spec.name == "start_recording" => ToolControl::Editable {
            default: ToolSetting::Ask,
        },
        (Tier::Confirm, _) => ToolControl::LockedAsk,
        (Tier::Never, _) => ToolControl::Internal,
    }
}

/// Validates the value of [`KEY`] for a write. Returns the entries on success.
pub fn parse_strict(value: &Value) -> Result<BTreeMap<String, ToolSetting>, CmError> {
    let bad = |m: String| CmError::invalid_field("values", format!("`{KEY}`: {m}"));
    let Value::Object(map) = value else {
        return Err(bad(
            "must be an object like {\"capture_screenshot\": \"ask\"}".to_owned(),
        ));
    };
    let mut out = BTreeMap::new();
    for (tool, v) in map {
        let Some(spec) = spec_for(tool) else {
            return Err(bad(format!("`{tool}` is not a tool")));
        };
        let Some(setting) = v.as_str().and_then(ToolSetting::parse) else {
            return Err(bad(format!(
                "the value of `{tool}` must be \"allow\" or \"ask\""
            )));
        };
        match control_of(spec) {
            ToolControl::Internal => return Err(bad(format!("`{tool}` is not a tool"))),
            ToolControl::LockedRead => {
                return Err(bad(format!(
                    "`{tool}` only reads and is always allowed; it has no setting"
                )))
            }
            ToolControl::LockedAsk => {
                return Err(bad(format!(
                    "`{tool}` always asks first; it cannot be changed"
                )))
            }
            ToolControl::Editable { .. } => {}
        }
        out.insert(tool.clone(), setting);
    }
    Ok(out)
}

/// The entries of the stored value that the rules allow; everything else is ignored (treated as
/// the default). A value that is not an object means no entries. This is what the gate reads, so
/// a hand-edited file can tighten a tool but never loosen anything but `start_recording`.
pub fn parse_lenient(value: Option<&Value>) -> BTreeMap<String, ToolSetting> {
    let Some(Value::Object(map)) = value else {
        return BTreeMap::new();
    };
    map.iter()
        .filter_map(|(tool, v)| {
            let spec = spec_for(tool)?;
            let setting = ToolSetting::parse(v.as_str()?)?;
            matches!(control_of(spec), ToolControl::Editable { .. })
                .then(|| (tool.clone(), setting))
        })
        .collect()
}

/// The setting of `spec` given the (already lenient) entries.
pub fn setting_of(spec: &ActionSpec, entries: &BTreeMap<String, ToolSetting>) -> ToolSetting {
    match control_of(spec) {
        ToolControl::Editable { default } => entries.get(spec.name).copied().unwrap_or(default),
        ToolControl::LockedRead => ToolSetting::Allow,
        ToolControl::LockedAsk => ToolSetting::Ask,
        ToolControl::Internal => match spec.tier {
            Tier::Free => ToolSetting::Allow,
            _ => ToolSetting::Ask,
        },
    }
}

/// The tier an **agent's** call to `spec` is gated by: Free -> Confirm for a tightened tool,
/// Confirm -> Free for `start_recording` set to Allow, the table's tier otherwise.
pub fn effective_tier(spec: &ActionSpec, entries: &BTreeMap<String, ToolSetting>) -> Tier {
    match (control_of(spec), setting_of(spec, entries)) {
        (ToolControl::Editable { .. }, ToolSetting::Ask) => Tier::Confirm,
        (ToolControl::Editable { .. }, ToolSetting::Allow) => Tier::Free,
        _ => spec.tier,
    }
}

/// `capture_screenshot` -> `Capture screenshot`: the English title when the client has no better.
pub fn english_title(tool: &str) -> String {
    let s = tool.replace('_', " ");
    let mut cs = s.chars();
    match cs.next() {
        Some(c) => c.to_uppercase().collect::<String>() + cs.as_str(),
        None => String::new(),
    }
}

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    crate::engines::SECRET_SUBSTRINGS
        .iter()
        .chain(["authorization", "cookie", "session_key"].iter())
        .any(|w| lower.contains(w))
}

fn truncate_value(s: &str) -> String {
    match s.char_indices().nth(MAX_SUMMARY_VALUE_CHARS) {
        Some((i, _)) => format!("{}...", &s[..i]),
        None => s.to_owned(),
    }
}

/// One value as a short line: strings truncated, numbers and booleans as they are, nested
/// values summarized (`{...}`, `[3 items]`).
fn summarize_value(v: &Value) -> Option<String> {
    Some(match v {
        Value::Null => return None,
        Value::String(s) => truncate_value(&s.replace(['\n', '\r'], " ")),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(a) if a.len() == 1 => "[1 item]".to_owned(),
        Value::Array(a) => format!("[{} items]", a.len()),
        Value::Object(_) => "{...}".to_owned(),
    })
}

/// The compact parameter summary of the generic confirmation: at most
/// [`MAX_SUMMARY_LINES`] `(key, value)` lines in key order, never a key that looks like a secret
/// (`token`, `secret`, `password`, `api_key`, ...), never `request_id`. The second number is how
/// many lines did not fit.
pub fn summarize_params(details: &Value) -> (Vec<(String, String)>, usize) {
    let Value::Object(map) = details else {
        return (Vec::new(), 0);
    };
    let mut lines: Vec<(String, String)> = map
        .iter()
        .filter(|(k, _)| k.as_str() != "request_id" && !is_secret_key(k))
        .filter_map(|(k, v)| summarize_value(v).map(|s| (truncate_value(k), s)))
        .collect();
    let omitted = lines.len().saturating_sub(MAX_SUMMARY_LINES);
    lines.truncate(MAX_SUMMARY_LINES);
    (lines, omitted)
}

/// `(summary, details, params)` of the generic confirmation of a tightened tool. `details` is
/// the English label/value rows (the parameter summary), `params` the typed data the app words
/// the dialog from: `{ tool, title, client, lines: [{key, value}], omitted }`.
pub fn ask_first_dialog(client: &str, spec: &ActionSpec, input: &Value) -> (String, Value, Value) {
    let (lines, omitted) = summarize_params(input);
    let title = english_title(spec.name);
    let summary = format!(
        "{client} wants to run `{}` ({title}). You set this tool to Ask first.",
        spec.name
    );
    let details: Map<String, Value> = lines
        .iter()
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect();
    let params = json!({
        "tool": spec.name,
        "title": title,
        "client": client,
        "lines": lines.iter().map(|(k, v)| json!({ "key": k, "value": v })).collect::<Vec<_>>(),
        "omitted": omitted,
    });
    (summary, Value::Object(details), params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ACTIONS;

    fn entries(v: Value) -> BTreeMap<String, ToolSetting> {
        parse_lenient(Some(&v))
    }

    /// AGT-2.AC9: every action, every value: the tier an agent's call gets.
    #[test]
    fn agt_2_ac9_effective_tier_matrix() {
        for s in ACTIONS {
            let control = control_of(s);
            for value in ["allow", "ask"] {
                let e = entries(json!({ s.name: value }));
                let t = effective_tier(s, &e);
                let expected = match (control, value) {
                    (ToolControl::Editable { .. }, "ask") => Tier::Confirm,
                    (ToolControl::Editable { .. }, _) => Tier::Free,
                    _ => s.tier,
                };
                assert_eq!(t, expected, "{} = {value}", s.name);
            }
            // No entry: the table's tier.
            assert_eq!(effective_tier(s, &BTreeMap::new()), s.tier, "{}", s.name);
        }
        // The shape of the rule, spelled out.
        let ss = |n: &str| *spec_for(n).unwrap();
        let ask = |n: &str| entries(json!({ n: "ask" }));
        let allow = |n: &str| entries(json!({ n: "allow" }));
        assert_eq!(
            effective_tier(&ss("capture_screenshot"), &ask("capture_screenshot")),
            Tier::Confirm
        );
        assert_eq!(
            effective_tier(&ss("start_recording"), &allow("start_recording")),
            Tier::Free
        );
        for locked in [
            "publish_video",
            "delete_capture",
            "run_fps_selftest",
            "download_model",
            "remove_model",
        ] {
            assert_eq!(
                effective_tier(&ss(locked), &allow(locked)),
                Tier::Confirm,
                "{locked} cannot be loosened"
            );
        }
        for read in ["list_library", "get_settings", "get_recording_status"] {
            assert_eq!(
                effective_tier(&ss(read), &ask(read)),
                Tier::Free,
                "{read} is never asked"
            );
        }
    }

    /// AGT-2.AC10: the locked rules at write time.
    #[test]
    fn agt_2_ac10_strict_validation() {
        assert!(parse_strict(&json!({})).is_ok());
        assert!(
            parse_strict(&json!({"capture_screenshot": "ask", "start_recording": "allow"})).is_ok()
        );
        for bad in [
            json!("ask"),
            json!([]),
            json!(null),
            json!({"capture_screenshot": "maybe"}),
            json!({"capture_screenshot": true}),
            json!({"capture_screenshot": null}),
            json!({"no_such_tool": "ask"}),
            // internal actions are not tools
            json!({"freeze_display": "ask"}),
            json!({"discard_recording": "allow"}),
            json!({"skip_countdown": "ask"}),
            // reading tools and locked Confirm tools have no setting, in either direction
            json!({"list_library": "ask"}),
            json!({"get_settings": "allow"}),
            json!({"publish_video": "allow"}),
            json!({"publish_video": "ask"}),
            json!({"delete_capture": "allow"}),
            json!({"run_fps_selftest": "allow"}),
            json!({"download_model": "allow"}),
            json!({"remove_model": "allow"}),
            // one bad entry refuses the write
            json!({"capture_screenshot": "ask", "publish_video": "allow"}),
        ] {
            assert!(parse_strict(&bad).is_err(), "{bad}");
        }
    }

    /// AGT-2.AC10: a hand-edited file is read with the same rules; what they refuse is ignored.
    #[test]
    fn agt_2_ac10_lenient_read_ignores_what_a_write_would_refuse() {
        let e = entries(json!({
            "publish_video": "allow",
            "delete_capture": "allow",
            "run_fps_selftest": "allow",
            "list_library": "ask",
            "freeze_display": "ask",
            "nope": "ask",
            "capture_screenshot": "ask",
            "annotate": "sideways",
            "start_recording": "allow",
        }));
        assert_eq!(
            e.keys().map(String::as_str).collect::<Vec<_>>(),
            ["capture_screenshot", "start_recording"]
        );
        assert!(parse_lenient(Some(&json!("allow"))).is_empty());
        assert!(parse_lenient(Some(&json!([1, 2]))).is_empty());
        assert!(parse_lenient(None).is_empty());
    }

    #[test]
    fn controls() {
        let c = |n: &str| control_of(spec_for(n).unwrap());
        assert_eq!(c("list_library"), ToolControl::LockedRead);
        assert_eq!(c("publish_video"), ToolControl::LockedAsk);
        assert_eq!(c("delete_capture"), ToolControl::LockedAsk);
        assert_eq!(c("discard_recording"), ToolControl::Internal);
        assert_eq!(
            c("start_recording"),
            ToolControl::Editable {
                default: ToolSetting::Ask
            }
        );
        assert_eq!(
            c("capture_screenshot"),
            ToolControl::Editable {
                default: ToolSetting::Allow
            }
        );
        // Every editable Free tool is mutating, and start_recording is the only editable
        // Confirm tool.
        for s in ACTIONS {
            if let ToolControl::Editable { .. } = control_of(s) {
                assert!(s.mutating, "{}", s.name);
                assert!(s.tier == Tier::Free || s.name == "start_recording");
            }
        }
    }

    /// AGT-2.AC11: the parameter summary.
    #[test]
    fn agt_2_ac11_summary_truncates_redacts_and_summarizes() {
        let long = "x".repeat(300);
        let input = json!({
            "request_id": "r1",
            "capture_id": "shot_1",
            "path": "/Users/a/Desktop/my file.png",
            "note": long,
            "access_token": "SECRET-1",
            "client_secret": "SECRET-2",
            "Password": "SECRET-3",
            "api_key_id": "SECRET-4",
            "values": {"a": 1},
            "ops": [1, 2, 3],
            "one": [1],
            "empty": null,
            "flag": true,
            "n": 7,
        });
        let (lines, omitted) = summarize_params(&input);
        let m: BTreeMap<_, _> = lines.iter().cloned().collect();
        assert_eq!(omitted, 0);
        assert!(lines.len() <= MAX_SUMMARY_LINES);
        assert_eq!(m["capture_id"], "shot_1");
        assert_eq!(m["path"], "/Users/a/Desktop/my file.png");
        assert_eq!(m["values"], "{...}");
        assert_eq!(m["ops"], "[3 items]");
        assert_eq!(m["one"], "[1 item]");
        assert_eq!(m["flag"], "true");
        assert_eq!(m["n"], "7");
        assert!(!m.contains_key("empty"));
        assert!(!m.contains_key("request_id"));
        assert!(m["note"].ends_with("..."));
        assert_eq!(m["note"].chars().count(), MAX_SUMMARY_VALUE_CHARS + 3);
        let all = serde_json::to_string(&lines).unwrap();
        assert!(!all.contains("SECRET"), "{all}");
        for k in ["access_token", "client_secret", "Password", "api_key_id"] {
            assert!(!m.contains_key(k), "{k}");
        }
    }

    #[test]
    fn agt_2_ac11_summary_is_capped_at_eight_lines() {
        let mut o = Map::new();
        for i in 0..12 {
            o.insert(format!("k{i:02}"), json!(i));
        }
        let (lines, omitted) = summarize_params(&Value::Object(o));
        assert_eq!(lines.len(), 8);
        assert_eq!(omitted, 4);
        assert_eq!(lines[0].0, "k00");
        // Multi-byte text is cut on a character boundary.
        let (lines, _) = summarize_params(&json!({ "t": "é".repeat(200) }));
        assert_eq!(lines[0].1.chars().count(), MAX_SUMMARY_VALUE_CHARS + 3);
    }

    #[test]
    fn dialog_params_shape() {
        let s = *spec_for("capture_screenshot").unwrap();
        let (summary, details, params) = ask_first_dialog(
            "claude-desktop",
            &s,
            &json!({"target": {"display_id": 1}, "x": 2}),
        );
        assert!(summary.contains("claude-desktop") && summary.contains("capture_screenshot"));
        assert_eq!(details["target"], "{...}");
        assert_eq!(params["tool"], "capture_screenshot");
        assert_eq!(params["client"], "claude-desktop");
        assert_eq!(params["title"], "Capture screenshot");
        assert_eq!(params["lines"][0]["key"], "target");
        assert_eq!(params["omitted"], 0);
    }
}
