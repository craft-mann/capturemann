//! "After you stop" report suggestion (V1, AI-9): when the transcript of an audio recording is
//! ready, the app offers the report the recording's preset suggests, in a small toast. The rules
//! here are display-free and tested with fakes; the toast and the call to `get_text_model` are in
//! `ui::report_offer`.
//!
//! The offer is only made when all of these hold: the recording is audio, it was made with a
//! preset that suggests a template (the Record sheet's "Suggest: ..." chip), it has no report
//! yet, and a local text model is set up. It never starts anything: the toast's button opens the
//! Report tab's setup with the template marked suggested, and the user presses Write.

use cm_library::manifest::{CaptureKind, RecManifest};
use serde_json::Value;

use crate::ai::voice::TextVm;
use crate::recedit::voice_panels::suggested_template;

/// Whether `get_text_model`'s answer names a local model that can write (the same test the
/// Report tab uses to enable Write).
pub fn text_model_ready(answer: &Value) -> bool {
    let mut t = TextVm::default();
    t.set_from_daemon(answer);
    t.ready()
}

/// The template to offer for a recording, or `None` to stay silent.
pub fn template_to_offer(m: &RecManifest) -> Option<&'static str> {
    if m.capture_kind() != CaptureKind::Audio {
        return None;
    }
    if !cm_ai::report::store::index_of(m).is_empty() {
        return None;
    }
    let preset = m
        .extra
        .get("preset")
        .or_else(|| m.settings.get("preset"))
        .and_then(Value::as_str);
    suggested_template(preset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest(kind: &str, preset: Option<&str>, reports: bool) -> RecManifest {
        let mut v = json!({ "kind": kind });
        if let Some(p) = preset {
            v["preset"] = json!(p);
        }
        if reports {
            v["reports"] = json!([{ "id": "rpt_00000001", "v": 1, "template": "minutes",
                                    "created": "2026-10-26T10:00:00Z", "edited": false }]);
        }
        let mut merged = json!({
            "schema": 1, "id": "rec_1", "title": "Sync",
            "created": "2026-10-26T09:00:00Z", "tracks": {}
        });
        for (k, x) in v.as_object().unwrap() {
            merged[k] = x.clone();
        }
        RecManifest::from_json(&merged.to_string()).unwrap_or_else(|e| panic!("{e:?}"))
    }

    #[test]
    fn only_a_local_model_with_a_name_counts() {
        assert!(text_model_ready(
            &json!({ "provider": "local", "model": "qwen3.5:9b", "base_url": "http://127.0.0.1:11434/v1" })
        ));
        assert!(!text_model_ready(
            &json!({ "provider": "none", "model": "" })
        ));
        assert!(!text_model_ready(
            &json!({ "provider": "local", "model": " " })
        ));
        assert!(!text_model_ready(&json!({})));
    }

    #[test]
    fn the_offer_follows_the_preset_and_stays_silent_otherwise() {
        assert_eq!(
            template_to_offer(&manifest("audio", Some("room"), false)),
            Some("minutes")
        );
        assert_eq!(
            template_to_offer(&manifest("audio", Some("lecture"), false)),
            Some("lecture")
        );
        // No preset, a recording that is not audio, or one that already has a report.
        assert_eq!(template_to_offer(&manifest("audio", None, false)), None);
        assert_eq!(
            template_to_offer(&manifest("recording", Some("room"), false)),
            None
        );
        assert_eq!(
            template_to_offer(&manifest("audio", Some("room"), true)),
            None
        );
    }
}
