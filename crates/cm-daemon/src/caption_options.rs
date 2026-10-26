//! `set_caption_options { capture_id, enabled?, burn_in? }` (AI-1): the two caption switches of a
//! recording.
//!
//! * `enabled`: show the captions on the video in the editor's preview.
//! * `burn_in`: burn the captions into exported videos by default (`export_video` can override it
//!   with `captions: "burn" | "none"`).
//!
//! Free tier, one revision and one undo step (`set_caption_options`), tagged `user` or
//! `agent:<name>`. A call that changes nothing records no revision. The caption lines and the edit
//! list are untouched. Screenshots have no captions.

use std::path::Path;
use std::sync::Arc;

use cm_ai::caption::{cues_for_timeline, CaptionLine};
use cm_ai::transcribe::Transcript;
use cm_core::{Actor, CmError};
use cm_media::{CaptionCue, CaptionSource};
use serde_json::{json, Value};

use crate::action::{Action, Fields};
use crate::daemon::{blocking, lock_library, Inner};
use crate::error::DaemonError;
use crate::events;
use crate::export::CaptionChoice;

/// The method name.
pub(crate) const METHOD: &str = "set_caption_options";

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CaptionOptionsParams {
    pub capture_id: String,
    pub enabled: Option<bool>,
    pub burn_in: Option<bool>,
    pub request_id: Option<String>,
}

fn opt_bool(f: &Fields<'_>, key: &str) -> Result<Option<bool>, CmError> {
    match f.obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(CmError::invalid_field(key, "must be a boolean")),
    }
}

pub(crate) fn parse(f: &Fields<'_>) -> Result<Action, CmError> {
    f.only(&["capture_id", "enabled", "burn_in", "request_id"])?;
    let p = CaptionOptionsParams {
        capture_id: f.str_req("capture_id")?,
        enabled: opt_bool(f, "enabled")?,
        burn_in: opt_bool(f, "burn_in")?,
        request_id: f.request_id()?,
    };
    if p.enabled.is_none() && p.burn_in.is_none() {
        return Err(CmError::invalid_input(
            "give `enabled` and/or `burn_in` (true or false)",
        ));
    }
    Ok(Action::CaptionOptions(p))
}

pub(crate) fn summary(p: &CaptionOptionsParams) -> String {
    let word = |b: bool| if b { "on" } else { "off" };
    let mut parts = Vec::new();
    if let Some(e) = p.enabled {
        parts.push(format!("show captions {}", word(e)));
    }
    if let Some(b) = p.burn_in {
        parts.push(format!("burn captions into exports {}", word(b)));
    }
    format!("Caption settings of {}: {}", p.capture_id, parts.join(", "))
}

/// Whether an export burns the captions in: an explicit choice wins, else the recording's
/// `burn_in` switch.
pub(crate) fn wants_burn_in(choice: Option<CaptionChoice>, manifest: &Value) -> bool {
    match choice {
        Some(c) => c == CaptionChoice::Burn,
        None => manifest["captions"]["burn_in"].as_bool().unwrap_or(false),
    }
}

/// The caption lines an export burns in, read from the bundle once: the user's edited lines, else
/// the transcript's words. `Ok(None)` when there is nothing to burn and `explicit` is false (the
/// recording's default asked for captions, but it has no transcript: the export goes ahead
/// without them). With `explicit` (the caller asked for `burn`) that is a `not_found` error
/// that says how to get captions, never a silent export without them.
pub fn burn_in_source(
    capture_id: &str,
    dir: &Path,
    manifest: &Value,
    explicit: bool,
) -> Result<Option<CaptionSource>, CmError> {
    let stored: Vec<CaptionLine> = manifest["captions"]["lines"]
        .as_array()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_value(Value::Array(l.clone())))
        .transpose()
        .map_err(|e| CmError::internal(format!("stored captions: {e}")))?
        .unwrap_or_default();
    let path = dir.join(crate::ai::TRANSCRIPT_FILE);
    let words = match std::fs::read_to_string(&path) {
        Ok(text) => {
            let t: Transcript = serde_json::from_str(&text).map_err(|e| {
                CmError::internal(format!("`{}` is not a transcript: {e}", path.display()))
            })?;
            t.words
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => {
            return Err(CmError::internal(format!(
                "cannot read `{}`: {e}",
                path.display()
            )))
        }
    };
    if stored.is_empty() && words.is_empty() {
        return if explicit {
            Err(CmError::not_found(format!(
                "`{capture_id}` has no captions yet; run `transcribe` first"
            )))
        } else {
            Ok(None)
        };
    }
    Ok(Some(Arc::new(move |tl| {
        let stored = (!stored.is_empty()).then_some(stored.as_slice());
        Ok(cues_for_timeline(stored, &words, tl)
            .into_iter()
            .map(|l| CaptionCue {
                start: l.start,
                end: l.end,
                text: l.text,
            })
            .collect())
    })))
}

impl Inner {
    pub(crate) async fn set_caption_options(
        &self,
        actor: &Actor,
        p: CaptionOptionsParams,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&p.capture_id).await;
        let (library, this, now, by) = (
            self.library.clone(),
            self.library_view(),
            self.clock.now(),
            actor.clone(),
        );
        let id = p.capture_id.clone();
        let (enabled, burn_in) = (p.enabled, p.burn_in);
        let (outcome, options) = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&id)?;
            if !rec.kind.is_recording() {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot; captions belong to recordings"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "caption")?;
            let out = cm_library::rec_edit::apply_caption_options(&dir, enabled, burn_in, by, now)?;
            let options = cm_library::rec_edit::caption_options(&cm_library::read_manifest(&dir)?);
            if out.is_some() {
                lib.index_dir(&dir)?;
            }
            Ok((out, options))
        })
        .await?;
        let mut v = json!({
            "capture_id": p.capture_id,
            "enabled": options.enabled,
            "burn_in": options.burn_in,
            "changed": outcome.is_some(),
        });
        if let Some(out) = outcome {
            self.events.publish(
                events::CAPTURE_REVISED,
                json!({
                    "capture_id": p.capture_id,
                    "revision": out.revision.rev,
                    "action": out.revision.action,
                    "by": out.revision.by,
                }),
            );
            v["revision"] = json!(out.revision.rev);
            v["action"] = json!(out.revision.action);
            v["by"] = json!(out.revision.by);
            v["can_undo"] = json!(out.can_undo);
            v["can_redo"] = json!(out.can_redo);
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_params(params: Value) -> Result<Action, CmError> {
        Action::parse(METHOD, &params)
    }

    #[test]
    fn parses_either_or_both_switches() {
        let Action::CaptionOptions(p) =
            parse_params(json!({"capture_id": "rec_1", "enabled": true})).unwrap()
        else {
            panic!("not a caption options action");
        };
        assert_eq!(p.enabled, Some(true));
        assert_eq!(p.burn_in, None);
        let Action::CaptionOptions(p) = parse_params(
            json!({"capture_id": "rec_1", "enabled": false, "burn_in": true, "request_id": "r1"}),
        )
        .unwrap() else {
            panic!("not a caption options action");
        };
        assert_eq!((p.enabled, p.burn_in), (Some(false), Some(true)));
        assert_eq!(p.request_id.as_deref(), Some("r1"));
    }

    #[test]
    fn rejects_empty_unknown_and_wrongly_typed_input() {
        assert!(parse_params(json!({"capture_id": "rec_1"})).is_err());
        assert!(parse_params(json!({"enabled": true})).is_err());
        assert!(parse_params(json!({"capture_id": "rec_1", "enabled": "yes"})).is_err());
        assert!(parse_params(json!({"capture_id": "rec_1", "burn": true})).is_err());
    }

    #[test]
    fn summary_names_what_changes() {
        let p = CaptionOptionsParams {
            capture_id: "rec_1".into(),
            enabled: Some(true),
            burn_in: Some(false),
            request_id: None,
        };
        let s = summary(&p);
        assert!(
            s.contains("show captions on") && s.contains("exports off"),
            "{s}"
        );
    }
}
