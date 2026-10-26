//! `add_marker { recording_id?, label? }` and `add_marker { capture_id, at, label? }` (VOC-1):
//! a marker on the running recording or on a finished one.
//!
//! A marker is `{ at, label? }` in `manifest.markers`, with `at` the recorded time (the HUD
//! timer: pauses removed). On the running recording it is set at the current recorded time by the
//! engine ([`crate::RecordingEngine::add_marker`]); on a finished one it is one revision
//! (`add_marker`, tagged `user` or `agent:<name>`) and one undo step, like any other edit. Free
//! tier: it changes a label, never the media.

use cm_core::{parse_time, Actor, CmError, MediaTime};
use serde_json::{json, Value};

use crate::action::{Action, Fields};
use crate::daemon::{blocking, lock_library, Inner};
use crate::error::DaemonError;
use crate::events;

/// The method name.
pub(crate) const METHOD: &str = "add_marker";

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AddMarkerParams {
    /// The running recording (default) when no `capture_id` is given.
    pub recording_id: Option<String>,
    /// A finished recording; needs `at`.
    pub capture_id: Option<String>,
    /// Recorded time, for a finished recording.
    pub at: Option<MediaTime>,
    pub label: Option<String>,
    pub request_id: Option<String>,
}

pub(crate) fn parse(f: &Fields<'_>) -> Result<Action, CmError> {
    f.only(&["recording_id", "capture_id", "at", "label", "request_id"])?;
    let capture_id = f.str_opt("capture_id")?;
    let recording_id = f.str_opt("recording_id")?;
    let at = match f.obj.get("at") {
        None | Some(Value::Null) => None,
        Some(v) => Some(parse_time("at", v)?),
    };
    match (&capture_id, at, &recording_id) {
        (Some(_), _, Some(_)) => {
            return Err(CmError::invalid_field(
                "recording_id",
                "cannot be combined with `capture_id`",
            ))
        }
        (Some(_), None, _) => {
            return Err(CmError::invalid_field(
                "at",
                "is required with `capture_id` (seconds of recorded time)",
            ))
        }
        // The running form with `at` and a label gives the marker set at that time its label
        // (the HUD's "Add a label" after a marker); without a label it makes no sense.
        (None, Some(_), _) if f.str_opt("label")?.is_none() => {
            return Err(CmError::invalid_field(
                "at",
                "needs `label` for a running recording (it names the marker set at that time); give `capture_id` too for a finished one",
            ))
        }
        _ => {}
    }
    let label = f.str_opt("label")?;
    // Validate the label now, so a bad one never reaches the engine.
    cm_library::rec_edit::normalize_marker_label(label.as_deref())?;
    Ok(Action::AddMarker(AddMarkerParams {
        recording_id,
        capture_id,
        at,
        label,
        request_id: f.request_id()?,
    }))
}

pub(crate) fn summary(p: &AddMarkerParams) -> String {
    match (&p.capture_id, p.at) {
        (Some(id), Some(at)) => format!("Add a marker at {:.1} s to {id}", at.as_secs_f64()),
        (None, Some(at)) => format!("Label the marker at {:.1} s", at.as_secs_f64()),
        _ => "Add a marker to the running recording".to_owned(),
    }
}

impl Inner {
    /// A marker on a finished recording: one revision and one undo step.
    pub(crate) async fn add_marker_stored(
        &self,
        actor: &Actor,
        p: AddMarkerParams,
    ) -> Result<Value, DaemonError> {
        let (Some(capture_id), Some(at)) = (p.capture_id.clone(), p.at) else {
            return Err(CmError::invalid_input("`capture_id` and `at` are required").into());
        };
        let _serial = self.capture_locks.lock(&capture_id).await;
        let (library, this, now, by) = (
            self.library.clone(),
            self.library_view(),
            self.clock.now(),
            actor.clone(),
        );
        let (id, label) = (capture_id.clone(), p.label.clone());
        let (marker, out) = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&id)?;
            if !rec.kind.is_recording() {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot; markers belong to recordings"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "marker")?;
            let r = cm_library::rec_edit::apply_marker(&dir, at, label.as_deref(), by, now)?;
            lib.index_dir(&dir)?;
            Ok(r)
        })
        .await?;
        self.events.publish(
            events::CAPTURE_REVISED,
            json!({
                "capture_id": capture_id,
                "revision": out.revision.rev,
                "action": out.revision.action,
                "by": out.revision.by,
            }),
        );
        Ok(json!({
            "capture_id": capture_id,
            "marker": {
                "at": marker.at.as_secs_f64(),
                "at_ms": marker.at.as_millis_floor(),
                "label": marker.label,
                "by": marker.by,
            },
            "revision": out.revision.rev,
            "action": out.revision.action,
            "by": out.revision.by,
            "can_undo": out.can_undo,
            "can_redo": out.can_redo,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(params: Value) -> Result<Action, CmError> {
        Action::parse(METHOD, &params)
    }

    #[test]
    fn running_recording_form_and_stored_form_parse() {
        let Action::AddMarker(m) = p(json!({"label": "decision"})).unwrap() else {
            panic!("not add_marker");
        };
        assert_eq!(
            (m.capture_id, m.at, m.label.as_deref()),
            (None, None, Some("decision"))
        );
        let Action::AddMarker(m) = p(json!({"capture_id": "rec_1", "at": 12.5})).unwrap() else {
            panic!("not add_marker");
        };
        assert_eq!(m.at, Some(MediaTime::from_secs_f64(12.5).unwrap()));
        assert!(p(json!({})).is_ok());
    }

    #[test]
    fn rejects_mixed_missing_and_unknown_input() {
        for bad in [
            json!({"capture_id": "rec_1"}),
            json!({"at": 3}),
            json!({"capture_id": "rec_1", "recording_id": "rec_x", "at": 1}),
            json!({"capture_id": "rec_1", "at": "soon"}),
            json!({"label": 5}),
            json!({"label": "x".repeat(201)}),
            json!({"mark": true}),
        ] {
            assert!(p(bad.clone()).is_err(), "{bad}");
        }
    }
}
