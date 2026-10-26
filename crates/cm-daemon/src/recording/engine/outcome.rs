use super::*;

// ---- stop result and JSON helpers ----------------------------------------------------------

pub(super) struct StopDone {
    pub(super) record: CaptureRecord,
    path: PathBuf,
    pub(super) tracks: Vec<TrackInfo>,
    pub(super) warnings: Vec<SessionNote>,
    paused: MediaTime,
    live_at: MediaTime,
    wall_end: MediaTime,
    stop_ms: f64,
}

impl StopDone {
    pub(super) fn duration(&self) -> MediaTime {
        self.record.duration.unwrap_or_else(|| {
            MediaTime::from_ticks(
                (self.wall_end.ticks() - self.live_at.ticks() - self.paused.ticks()).max(0),
            )
        })
    }
}

/// Stops every track, closes the bundle (`stop_reason` and `started_by` go into the manifest)
/// and indexes it. Blocking.
pub(super) fn finalize_blocking(
    session: RecordingSession,
    library: &Arc<Mutex<Library>>,
    reason: &str,
    by: &str,
    layout: Option<Value>,
) -> Result<StopDone, CmError> {
    let mut session = session;
    let mut stopped = session.stop()?;
    stopped.set_extra("stop_reason", json!(reason))?;
    stopped.set_extra("started_by", json!(by))?;
    // The camera layout at stop goes into `edits[0].layout` (the export reads it). Only when a
    // camera track really exists.
    if let Some(l) = layout {
        if stopped.tracks.iter().any(|t| t.kind == TrackKind::Camera) {
            stopped.set_extra(cm_library::rec::LAYOUT_KEY, l)?;
        }
    }
    let tracks = stopped.tracks.clone();
    let warnings = stopped.warnings.clone();
    let (paused, live_at, wall_end, stop_ms) = (
        stopped.paused,
        stopped.live_at,
        stopped.wall_end,
        stopped.stop_ms,
    );
    let mut lib = lock_library(library);
    let record = stopped.finish_into(&mut lib)?;
    let path = lib.bundle_path(&record);
    Ok(StopDone {
        record,
        path,
        tracks,
        warnings,
        paused,
        live_at,
        wall_end,
        stop_ms,
    })
}

/// For a stop with `reason: "permission_revoked"`: which permission and the message to show.
pub(super) fn revoked_permission(
    reason: &str,
    warnings: &[SessionNote],
) -> Option<(Permission, String)> {
    if reason != "permission_revoked" {
        return None;
    }
    warnings
        .iter()
        .find(|w| w.code == "permission_revoked" && w.level == NoteLevel::StopRequested)
        .and_then(|w| w.permission.map(|p| (p, w.message.clone())))
}

pub(super) fn outcome_json(sh: &Shared, d: &StopDone, reason: &str) -> Value {
    let dur = d.duration();
    let mut v = json!({
        "recording_id": sh.id,
        "capture_id": d.record.id,
        "title": d.record.title,
        "path": d.path,
        "state": "stopped",
        "reason": reason,
        "duration_s": dur.as_secs_f64(),
        "duration_ms": dur.as_millis_floor(),
        "paused_ms": d.paused.as_millis_floor(),
        "live_at_ms": d.live_at.as_millis_floor(),
        "stop_ms": d.stop_ms,
        "size_bytes": d.record.size_bytes,
        "tracks": d.tracks.iter().map(track_json).collect::<Vec<_>>(),
        "warnings": d.warnings.iter().map(note_json).collect::<Vec<_>>(),
        "restart_of": sh.restart_of,
    });
    if let Some((p, message)) = revoked_permission(reason, &d.warnings) {
        v["permission"] = json!(p.as_str());
        v["message"] = json!(message);
    }
    v
}

fn track_json(t: &TrackInfo) -> Value {
    json!({
        "kind": t.kind,
        "file": t.file,
        "start_ms": t.start.as_millis_floor(),
        "end_ms": t.end.as_millis_floor(),
        "stats": t.stats,
    })
}

pub(super) fn note_json(n: &SessionNote) -> Value {
    json!({
        "track": n.track.map(TrackKind::as_str),
        "code": n.code,
        "message": n.message,
        "level": match n.level {
            NoteLevel::Info => "info",
            NoteLevel::Warning => "warning",
            NoteLevel::StopRequested => "stop_requested",
        },
        "at_ms": n.at.map(MediaTime::as_millis_floor),
        "permission": n.permission.map(Permission::as_str),
    })
}

pub(super) fn error_json(e: &DaemonError) -> Value {
    json!({ "code": e.code().map_or("internal", cm_core::ErrorCode::as_str), "message": e.to_string() })
}

/// `mm:ss` (or `h:mm:ss`) of a millisecond count.
pub fn clock_text(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

pub(super) fn duration_words(secs: f64) -> String {
    let s = secs.round() as u64;
    match (s / 3600, (s / 60) % 60, s % 60) {
        (0, 0, s) => format!("{s} s"),
        (0, m, 0) => format!("{m} min"),
        (0, m, s) => format!("{m} min {s} s"),
        (h, 0, _) => format!("{h} h"),
        (h, m, _) => format!("{h} h {m} min"),
    }
}

/// `disk_space` dialog row: free space and what it is in minutes at the planned bitrate.
pub(super) fn disk_text(free_bytes: u64, minutes_left: f64) -> String {
    let left = if minutes_left >= 600.0 {
        format!("{:.0} h", minutes_left / 60.0)
    } else if minutes_left >= 120.0 {
        format!("{:.1} h", minutes_left / 60.0)
    } else {
        format!("{minutes_left:.0} min")
    };
    format!(
        "{:.1} GB free, room for about {left} at this setting (an estimate)",
        free_bytes as f64 / 1e9
    )
}

/// The `disk_space` row as typed parameters (P3): free space in GB and the minutes it holds.
pub(super) fn disk_params(free_bytes: u64, minutes_left: f64) -> Value {
    json!({ "free_gb": free_bytes as f64 / 1e9, "minutes_left": minutes_left })
}

/// The tracks a start will try to record, for `recording.starting`.
pub(super) fn tracks_planned(r: &ResolvedStart) -> Vec<&'static str> {
    let mut v = vec!["screen"];
    if r.camera.is_some() {
        v.push("camera");
    }
    if r.mic.is_some() {
        v.push("mic");
    }
    if r.system_audio {
        v.push("system");
    }
    if r.cursor {
        v.push("cursor");
    }
    v
}

/// The live numbers of a [`SessionStatus`] for the HUD.
pub(super) fn session_status_json(st: &SessionStatus) -> Value {
    let mic = st
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Mic)
        .and_then(|t| t.level);
    let screen = st.tracks.iter().find(|t| t.kind == TrackKind::Screen);
    let live = |key: &str| screen.map_or(Value::Null, |t| t.live[key].clone());
    json!({
        "recorded_ms": st.recorded.as_millis_floor(),
        "elapsed_ms": st.elapsed.as_millis_floor(),
        "paused_ms": st.paused_total.as_millis_floor(),
        "live_at_ms": st.live_at.as_millis_floor(),
        "session_state": st.state,
        "tracks": st.tracks,
        "mic_level_dbfs": mic.map(|l| l.peak_dbfs),
        "mic_rms_dbfs": mic.map(|l| l.rms_dbfs),
        "mic_clip_events": mic.map(|l| l.clip_events),
        "late_frame_pct": live("late_pct"),
        "recent_late_frame_pct": live("recent_late_pct"),
        "disk_left_minutes": st.disk_minutes_left.map_or_else(|| live("disk_left_minutes"), |m| json!(m)),
        "stop_requested": st.stop_requested,
        "mic_muted": st.mic_muted,
    })
}

pub(super) fn build_status(sh: &Shared, meta: &Meta, fresh: Option<&SessionStatus>) -> Value {
    let mut v = json!({
        "recording_id": sh.id,
        "state": meta.phase.as_str(),
        "capture_id": meta.capture_id,
        "title": meta.title,
        "countdown_remaining": meta.countdown_remaining,
        "started_by": sh.by,
        "started_at": meta.started_at,
        "restart_of": sh.restart_of,
        "max_duration_s": sh.resolved.max_duration_s,
        "pauses": meta.pauses,
        "stop_reason": meta.stop_reason,
        "warnings": meta.warnings,
        "indicator": sh.indicator,
        "recorded_ms": 0,
        "elapsed_ms": 0,
        "paused_ms": 0,
    });
    let live = match fresh {
        Some(st) => Some(session_status_json(st)),
        None => meta.status.clone(),
    };
    if let (Some(Value::Object(src)), Some(dst)) = (live, v.as_object_mut()) {
        for (k, x) in src {
            dst.insert(k, x);
        }
    }
    v
}

pub(super) fn status_value(sh: &Shared) -> Value {
    let fresh = sh
        .session
        .try_lock()
        .ok()
        .and_then(|g| g.as_ref().map(RecordingSession::status));
    let meta = lock(&sh.meta);
    build_status(sh, &meta, fresh.as_ref())
}
