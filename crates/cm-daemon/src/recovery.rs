//! Crash recovery at startup (REC-8, NFR-6): [`crate::Daemon::recover_recordings`].
//!
//! Wraps `Library::recover_incomplete`: every `.cmrec` bundle that still has its `recording.lock`
//! and whose owner process is gone is truncated to its last complete fragment, given a manifest
//! and indexed. The daemon publishes `recording.recovered { capture_id, lost_ms }` for each (and
//! `capture.created`), and returns what the app needs for "Recovered recording from `<time>`
//! (lost up to 2 s)".

use std::sync::{Arc, Mutex};

use cm_library::rec::TrackRecovery;
use cm_library::Library;
use serde::Serialize;
use serde_json::json;
use time::format_description::well_known::Rfc3339;

use crate::daemon::lock_library;
use crate::events::{self, EventBus};

/// One recovered recording.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecoveredRecording {
    pub capture_id: String,
    pub title: String,
    /// When the recording was started (RFC 3339).
    pub created: String,
    pub duration_s: f64,
    /// Upper bound of the media lost, per track: one fragment (REC-8: at most 2 s).
    pub lost_ms: i64,
    /// Tracks that could not be recovered and are marked missing in the manifest.
    pub missing_tracks: Vec<String>,
}

/// What [`crate::Daemon::recover_recordings`] found.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RecoveredRecordings {
    pub recovered: Vec<RecoveredRecording>,
    /// Bundles whose lock was stale after a finished manifest (lock removed, nothing lost).
    pub already_complete: usize,
    /// Bundles owned by a live process (this one included): left alone.
    pub in_progress: usize,
    /// Bundles with nothing usable on disk: `(folder, reason)`. Their lock stays, marked, so the next launch skips them (see `quarantined`).
    pub failed: Vec<(String, String)>,
    /// Bundles a previous launch already gave up on (marked `recovery_failed`): skipped, listed
    /// here so the UI can offer to open or delete them (R3-5).
    pub quarantined: Vec<String>,
}

pub(crate) fn recover_recordings(
    library: &Arc<Mutex<Library>>,
    events: &EventBus,
) -> Result<RecoveredRecordings, cm_core::CmError> {
    let summary = lock_library(library).recover_incomplete()?;
    let mut out = RecoveredRecordings {
        already_complete: summary.already_complete.len(),
        in_progress: summary.in_progress.len(),
        failed: summary
            .failed
            .iter()
            .map(|(f, e)| (f.clone(), e.to_string()))
            .collect(),
        quarantined: summary.quarantined.clone(),
        recovered: Vec::new(),
    };
    for r in &summary.recovered {
        let rec = RecoveredRecording {
            capture_id: r.id.clone(),
            title: r.title.clone(),
            created: r.created.format(&Rfc3339).unwrap_or_default(),
            duration_s: r.duration.as_secs_f64(),
            lost_ms: r.lost_ms_max,
            missing_tracks: r
                .tracks
                .iter()
                .filter(|(_, t)| matches!(t, TrackRecovery::Missing { .. }))
                .map(|(name, _)| name.clone())
                .collect(),
        };
        events.publish(
            events::RECORDING_RECOVERED,
            json!({
                "capture_id": rec.capture_id,
                "title": rec.title,
                "lost_ms": rec.lost_ms,
                "duration_s": rec.duration_s,
                "created": rec.created,
                "missing_tracks": rec.missing_tracks,
            }),
        );
        events.publish(
            events::CAPTURE_CREATED,
            json!({ "capture_id": rec.capture_id, "by": "recovery" }),
        );
        out.recovered.push(rec);
    }
    Ok(out)
}
