//! MCP resources (AGT-1, "MCP resources, prompts and events"): read-only views of daemon
//! state. A read is one Free daemon action over IPC, the same as the tool it mirrors, so the
//! per-client policy applies the same way and the engine crates are never called from here.
//!
//! | URI | Action behind it |
//! | --- | --- |
//! | `capturemann://library` | `list_library { limit: 50 }` |
//! | `capturemann://jobs` | `list_jobs { limit: 50 }` |
//! | `capturemann://capture/{id}` | `get_capture { include_image: false }` |
//! | `capturemann://capture/{id}/transcript` | `get_transcript` |
//! | `capturemann://capture/{id}/reports` | `get_report` (without a report id) |
//! | `capturemann://platforms/{name}/rules` | `list_publish_accounts` (only its `platforms` part) |
//!
//! Not built: `capturemann://capture/{id}/cursor` (no daemon action serves the cursor log
//! yet). Nothing here can return a token or a keychain key: the actions behind it do not
//! carry one, the platform rules are cut out of the accounts answer before it is returned,
//! and the content is bounded by [`MAX_RESOURCE_BYTES`].

use rmcp::model::{Resource, ResourceTemplate};
use serde_json::{json, Value};

pub const SCHEME: &str = "capturemann://";
/// Most bytes of one resource's text. A bigger answer is refused (`too_large`) rather than
/// cut, because cut JSON is not JSON; the tool of the same name pages or filters.
pub const MAX_RESOURCE_BYTES: usize = 1_000_000;
const MAX_ID_LEN: usize = 64;
const JSON_MIME: &str = "application/json";

/// What a URI points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Library,
    Jobs,
    Capture(String),
    Transcript(String),
    Reports(String),
    PlatformRules(String),
}

/// Why a URI is not a resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UriError {
    /// Not a `capturemann://` URI or not one of the known shapes.
    Unknown,
    /// A known shape with an id or name that is not valid.
    BadSegment(&'static str),
}

fn valid_segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_ID_LEN
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn segment(s: &str, what: &'static str) -> Result<String, UriError> {
    if valid_segment(s) {
        Ok(s.to_owned())
    } else {
        Err(UriError::BadSegment(what))
    }
}

/// Parses a resource URI. Query strings, fragments, empty and extra segments are refused.
pub fn parse(uri: &str) -> Result<Target, UriError> {
    let rest = uri.strip_prefix(SCHEME).ok_or(UriError::Unknown)?;
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        ["library"] => Ok(Target::Library),
        ["jobs"] => Ok(Target::Jobs),
        ["capture", id] => Ok(Target::Capture(segment(id, "capture id")?)),
        ["capture", id, "transcript"] => Ok(Target::Transcript(segment(id, "capture id")?)),
        ["capture", id, "reports"] => Ok(Target::Reports(segment(id, "capture id")?)),
        ["platforms", name, "rules"] => Ok(Target::PlatformRules(segment(name, "platform name")?)),
        _ => Err(UriError::Unknown),
    }
}

impl Target {
    /// The daemon action and params that serve it.
    pub fn action(&self) -> (&'static str, Value) {
        match self {
            Target::Library => ("list_library", json!({ "limit": 50 })),
            Target::Jobs => ("list_jobs", json!({ "limit": 50 })),
            Target::Capture(id) => (
                "get_capture",
                json!({ "capture_id": id, "include_image": false }),
            ),
            Target::Transcript(id) => ("get_transcript", json!({ "capture_id": id })),
            Target::Reports(id) => ("get_report", json!({ "capture_id": id })),
            Target::PlatformRules(_) => ("list_publish_accounts", json!({})),
        }
    }

    /// Turns the action's answer into the resource's JSON; `None` means "no such resource".
    pub fn shape(&self, answer: Value) -> Option<Value> {
        match self {
            Target::PlatformRules(name) => answer.get("platforms")?.get(name).cloned(),
            _ => Some(answer),
        }
    }
}

/// Serializes a resource body, refusing one over [`MAX_RESOURCE_BYTES`].
pub fn render(v: &Value) -> Result<String, usize> {
    let text = serde_json::to_string(v).unwrap_or_else(|_| "null".into());
    if text.len() > MAX_RESOURCE_BYTES {
        Err(text.len())
    } else {
        Ok(text)
    }
}

pub fn mime() -> &'static str {
    JSON_MIME
}

/// The `resources/list` entries: the fixed resources. Captures are reached through the
/// templates (and `capturemann://library` lists their ids).
pub fn list() -> Vec<Resource> {
    vec![
        Resource::new("capturemann://library", "library")
            .with_title("Recent captures")
            .with_description("The 50 newest captures: id, title, kind, duration, created. Search and paging: the list_library tool.")
            .with_mime_type(JSON_MIME),
        Resource::new("capturemann://jobs", "jobs")
            .with_title("Jobs")
            .with_description("Active and recent background jobs (export, transcribe, upload, publish) with state and progress.")
            .with_mime_type(JSON_MIME),
        Resource::new("capturemann://platforms/youtube/rules", "youtube-rules")
            .with_title("YouTube limits")
            .with_description("Current limits and required fields for publishing to YouTube.")
            .with_mime_type(JSON_MIME),
    ]
}

/// The `resources/templates/list` entries.
pub fn templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new("capturemann://capture/{id}", "capture")
            .with_title("Capture")
            .with_description("Metadata, tracks, uploads, annotation ops and revisions of one capture (no image: use get_capture for that).")
            .with_mime_type(JSON_MIME),
        ResourceTemplate::new("capturemann://capture/{id}/transcript", "transcript")
            .with_title("Transcript")
            .with_description("Word-level transcript with times and the caption lines of a recording; not found until it is transcribed.")
            .with_mime_type(JSON_MIME),
        ResourceTemplate::new("capturemann://capture/{id}/reports", "reports")
            .with_title("Reports")
            .with_description("The AI-9 reports of a recording: the list (`reports`) and the newest one in full (`latest`, JSON and Markdown).")
            .with_mime_type(JSON_MIME),
        ResourceTemplate::new("capturemann://platforms/{name}/rules", "platform-rules")
            .with_title("Platform rules")
            .with_description("Current limits and required fields of a publishing platform (youtube).")
            .with_mime_type(JSON_MIME),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_known_shapes() {
        assert_eq!(parse("capturemann://library"), Ok(Target::Library));
        assert_eq!(parse("capturemann://jobs"), Ok(Target::Jobs));
        assert_eq!(
            parse("capturemann://capture/rec_1"),
            Ok(Target::Capture("rec_1".into()))
        );
        assert_eq!(
            parse("capturemann://capture/rec_1/transcript"),
            Ok(Target::Transcript("rec_1".into()))
        );
        assert_eq!(
            parse("capturemann://platforms/youtube/rules"),
            Ok(Target::PlatformRules("youtube".into()))
        );
    }

    #[test]
    fn refuses_everything_else() {
        for bad in [
            "file:///etc/passwd",
            "capturemann://",
            "capturemann://library/",
            "capturemann://library?x=1",
            "capturemann://capture",
            "capturemann://capture/rec_1/cursor",
            "capturemann://capture/a/b/c",
            "capturemann://secrets",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        for bad in [
            "capturemann://capture/..",
            "capturemann://capture/a%2Fb",
            "capturemann://capture/",
            &format!("capturemann://capture/{}", "a".repeat(65)),
        ] {
            assert!(
                matches!(
                    parse(bad),
                    Err(UriError::BadSegment(_)) | Err(UriError::Unknown)
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn rules_are_cut_out_of_the_accounts_answer() {
        let answer = json!({ "accounts": [{ "id": "a1", "name": "me" }],
                             "platforms": { "youtube": { "title_max_chars": 100 } } });
        let t = Target::PlatformRules("youtube".into());
        let shaped = t.shape(answer.clone()).unwrap();
        assert_eq!(shaped, json!({ "title_max_chars": 100 }));
        assert!(Target::PlatformRules("tiktok".into())
            .shape(answer)
            .is_none());
    }

    #[test]
    fn oversize_bodies_are_refused() {
        let big = json!({ "x": "a".repeat(MAX_RESOURCE_BYTES) });
        assert!(render(&big).is_err());
        assert!(render(&json!({ "x": 1 })).is_ok());
    }
}
