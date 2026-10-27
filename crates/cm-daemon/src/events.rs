//! The event stream: a broadcast bus plus the extension point for `cm-jobs`.
//!
//! Event names follow `specs/02-agent-interface.md` (`capture.created`, `capture.revised`,
//! `confirmation.requested`, `confirmation.resolved`, `job.progress`, `job.done`,
//! `job.failed`, and the `recording.*` family of the recording engine, see
//! [`crate::recording::engine`]). On the wire an event is a JSON-RPC notification:
//!
//! ```json
//! {"jsonrpc":"2.0","method":"event","params":{"seq":12,"event":"capture.revised",
//!   "at":"2026-10-19T09:00:00Z","data":{"capture_id":"shot_...","revision":3,"by":"agent:claude"}}}
//! ```

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use time::OffsetDateTime;
use tokio::sync::broadcast;

use crate::clock::Clock;

pub const CAPTURE_CREATED: &str = "capture.created";
pub const CAPTURE_REVISED: &str = "capture.revised";
/// A capture was moved to the Trash: `{ capture_id, title, by }`.
pub const CAPTURE_DELETED: &str = "capture.deleted";
/// The library folder cannot be read (an unplugged drive, a missing folder), seen by the library
/// watcher's poll: `{ path }`. Published once per outage.
pub const LIBRARY_OFFLINE: &str = "library.offline";
/// The library folder can be read again after `library.offline`: `{ path }`.
pub const LIBRARY_ONLINE: &str = "library.online";
/// A library move (`move_library`, mode `move`) was queued: `{ job_id, from, to, bundles, bytes }`.
pub const LIBRARY_MOVE_STARTED: &str = "library.move_started";
/// The library folder was changed (after a verified copy, or at once for `fresh` and
/// `use_existing`): `{ from, to, mode, bundles, bytes, skipped, restart_required }`. The new
/// folder is used after a restart.
pub const LIBRARY_MOVED: &str = "library.moved";
pub const CONFIRMATION_REQUESTED: &str = "confirmation.requested";
pub const CONFIRMATION_RESOLVED: &str = "confirmation.resolved";
pub const RECORDING_COUNTDOWN: &str = "recording.countdown";
/// Published as soon as a start is accepted (after the countdown, before the tracks are up):
/// `{ recording_id, indicator, tracks_planned }`, so the HUD and indicator can show at once
/// (REC-5.AC1, NFR-11).
pub const RECORDING_STARTING: &str = "recording.starting";
pub const RECORDING_STARTED: &str = "recording.started";
pub const RECORDING_PAUSED: &str = "recording.paused";
pub const RECORDING_RESUMED: &str = "recording.resumed";
/// The microphone was muted (AUD-6): `{ recording_id, at_ms, recorded_ms }`. The track keeps
/// running; the file holds digital silence from `at_ms` (session time) on.
pub const RECORDING_MIC_MUTED: &str = "recording.mic_muted";
/// The microphone was unmuted: `{ recording_id, at_ms, recorded_ms }`.
pub const RECORDING_MIC_UNMUTED: &str = "recording.mic_unmuted";
/// A marker was set on the running recording (VOC-1): `{ recording_id, at_ms, label, by, count }`.
pub const RECORDING_MARKER: &str = "recording.marker";
pub const RECORDING_STATS: &str = "recording.stats";
pub const RECORDING_HEALTH: &str = "recording.health";
pub const RECORDING_WARNING: &str = "recording.warning";
pub const RECORDING_STOPPED: &str = "recording.stopped";
pub const RECORDING_DISCARDED: &str = "recording.discarded";
pub const RECORDING_SELFTEST: &str = "recording.selftest";
pub const RECORDING_RECOVERED: &str = "recording.recovered";
/// A camera appeared (also while idle): `{ device_id, name, kind, is_default }`. Published by the
/// device watcher the daemon owns ([`crate::DaemonBuilder::camera_watcher`]).
pub const CAMERA_CONNECTED: &str = "camera.connected";
/// A camera went away (also while idle; during a recording it comes with the
/// `recording.warning` `camera_lost`): `{ device_id, name, kind, is_default }`.
pub const CAMERA_DISCONNECTED: &str = "camera.disconnected";
/// An `export` job was enqueued: `{ job_id, capture_id, title, preset, auto }`; `recovered: true`
/// when it is republished after a restart for a job that survived in the queue (EXP-4.AC3).
pub const EXPORT_QUEUED: &str = "export.queued";
/// An automatic upload (STO-3) was queued: `{ job_id, capture_id, title, destination_id,
/// destination, what, trigger, export_job_id }`.
pub const UPLOAD_QUEUED: &str = "upload.queued";
/// An automatic upload was not queued: `{ capture_id, title, destination_id, destination,
/// reason }` (for example the export it needs cannot run).
pub const UPLOAD_SKIPPED: &str = "upload.skipped";
/// A `publish` job was queued (`publish_video` was approved): `{ job_id, capture_id, title,
/// video_title, platform, account_id, account, privacy, export_job_id }`.
pub const PUBLISH_QUEUED: &str = "publish.queued";
/// A video was published: `{ job_id, capture_id, title, platform, account_id, account, video_id,
/// url, privacy, status, warnings }`. `privacy` is what the platform reports, `status` is
/// `processing`, `processed` or `privacy_locked` (the platform kept the video private).
pub const PUBLISH_DONE: &str = "publish.done";
/// An export finished and is listed in the manifest: `{ job_id, capture_id, title, preset, file,
/// path, size_bytes, duration_s, encoder }`.
pub const EXPORT_DONE: &str = "export.done";
/// The automatic export after a recording was not queued: `{ capture_id, reason }` with
/// `reason: "ffmpeg_not_found"`. Published once per daemon run (R5-3).
pub const EXPORT_SKIPPED: &str = "export.skipped";
/// A user-started model download advanced: `{ model, bytes_done, bytes_total, fraction }`.
pub const MODEL_DOWNLOAD_PROGRESS: &str = "model.download.progress";
/// The set of speech models changed: `{ model, change }`, `change` one of `downloaded`,
/// `imported`, `selected`, `removed`.
pub const MODEL_CHANGED: &str = "model.changed";
/// A transcript was written: `{ capture_id, model, language, words }`.
pub const TRANSCRIPT_READY: &str = "transcript.ready";
/// A caption translation was written (AI-5): `{ capture_id, language, engine, model, lines,
/// untranslated, path, kept_edits, dropped_edits }`.
pub const TRANSLATION_READY: &str = "translation.ready";
/// The automatic speaker step queued a run after a transcript (AI-8, `ai.auto_speakers`):
/// `{ capture_id, job_id, num_speakers, automatic: true }`. The app shows the run on the
/// Speakers tab like one it started itself.
pub const SPEAKERS_STARTED: &str = "speakers.started";
/// A diarization finished (AI-8): `{ capture_id, model, speakers, segments, note, headset }`.
pub const SPEAKERS_READY: &str = "speakers.ready";
/// A report was written (AI-9): `{ capture_id, report_id, template, dropped_uncited, chunks }`.
pub const REPORT_READY: &str = "report.ready";
/// A report run failed (AI-9): `{ capture_id, template, reason, message }`.
pub const REPORT_FAILED: &str = "report.failed";
pub const JOB_PROGRESS: &str = "job.progress";
pub const JOB_DONE: &str = "job.done";
pub const JOB_FAILED: &str = "job.failed";
/// A job is waiting for the user to sign in again: `{ job_id, type, state: "paused", pause_reason:
/// "auth_expired" }`. Resuming it (Reconnect in Settings → Destinations) publishes nothing
/// itself; the job then goes on and ends with `job.done` or `job.failed`. Other pauses (a
/// recording holds the queue, the user paused it) publish nothing.
pub const JOB_PAUSED: &str = "job.paused";
/// Sent to a subscriber that fell behind; `data.dropped` counts the lost events.
pub const EVENTS_DROPPED: &str = "events.dropped";

const BUS_CAPACITY: usize = 1024;

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    /// Monotonic per daemon run.
    pub seq: u64,
    #[serde(rename = "event")]
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    pub data: Value,
}

/// Does `topic` select `event`? A topic is an exact name, `*`, or a prefix ending in `.*`
/// (`job.*`).
pub fn topic_matches(topic: &str, event: &str) -> bool {
    if topic == "*" {
        return true;
    }
    match topic.strip_suffix(".*") {
        Some(prefix) => event
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('.')),
        None => topic == event,
    }
}

/// Cheap to clone; every clone publishes into the same stream.
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
    /// Next sequence number. The lock is held across assigning the number and sending, so two
    /// publishers can never deliver `seq` 6 before 5 (R2-9).
    seq: Arc<Mutex<u64>>,
    /// Last assigned sequence number, readable without the lock (for `events.dropped`).
    last: Arc<AtomicU64>,
    clock: Arc<dyn Clock>,
}

impl EventBus {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        let (tx, _) = broadcast::channel(BUS_CAPACITY);
        EventBus {
            tx,
            seq: Arc::new(Mutex::new(1)),
            last: Arc::new(AtomicU64::new(0)),
            clock,
        }
    }

    pub fn publish(&self, name: &str, data: Value) {
        let mut next = self.seq.lock().unwrap_or_else(PoisonError::into_inner);
        let event = Event {
            seq: *next,
            name: name.to_owned(),
            at: self.clock.now(),
            data,
        };
        *next += 1;
        self.last.store(event.seq, Ordering::SeqCst);
        // No subscribers is fine. Sent while the lock is held: delivery order == seq order.
        let _ = self.tx.send(event);
    }

    /// The sequence number of the most recently published event (0 before the first).
    pub fn current_seq(&self) -> u64 {
        self.last.load(Ordering::SeqCst)
    }

    pub fn now(&self) -> OffsetDateTime {
        self.clock.now()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobEventKind {
    Progress,
    Done,
    Failed,
    Paused,
}

/// A job-queue event as the bridge sees it (`cm-jobs` adapts its own event type to this).
#[derive(Debug, Clone, PartialEq)]
pub struct JobEvent {
    pub kind: JobEventKind,
    pub job_id: String,
    /// Extra fields merged next to `job_id` (progress fraction, result, failure reason...).
    pub data: Value,
}

/// Extension point for `cm-jobs`: implement this on a receiver of job events and hand it to
/// [`crate::Daemon::bridge_job_events`]. Each event is republished as `job.progress`,
/// `job.done` or `job.failed` with `data = { "job_id": .., ..data }`. The stream ends when
/// `next_event` returns `None`.
#[async_trait]
pub trait JobEvents: Send {
    async fn next_event(&mut self) -> Option<JobEvent>;
}

pub(crate) fn publish_job_event(bus: &EventBus, ev: JobEvent) {
    let name = match ev.kind {
        JobEventKind::Progress => JOB_PROGRESS,
        JobEventKind::Done => JOB_DONE,
        JobEventKind::Failed => JOB_FAILED,
        JobEventKind::Paused => JOB_PAUSED,
    };
    let mut data = match ev.data {
        Value::Object(m) => m,
        Value::Null => serde_json::Map::new(),
        other => {
            let mut m = serde_json::Map::new();
            m.insert("value".into(), other);
            m
        }
    };
    // A failure whose reason is a message with a stable id (a missing FFmpeg) also carries the
    // id as `error.data.reason_code`; the shape of the error is otherwise unchanged.
    if ev.kind == JobEventKind::Failed {
        if let Some(d) = data
            .get_mut("error")
            .and_then(|e| e.get_mut("data"))
            .and_then(Value::as_object_mut)
        {
            let id = d
                .get("reason")
                .and_then(Value::as_str)
                .and_then(cm_core::message::ffmpeg_missing_id);
            if let Some(id) = id {
                d.insert("reason_code".into(), Value::String(id.to_owned()));
            }
        }
    }
    data.insert("job_id".into(), Value::String(ev.job_id));
    bus.publish(name, Value::Object(data));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A failed job whose reason is a missing FFmpeg carries the stable id as
    /// `error.data.reason_code`; other failures are untouched.
    #[test]
    fn a_missing_ffmpeg_failure_carries_a_reason_code() {
        let bus = EventBus::new(Arc::new(crate::clock::SystemClock));
        let mut rx = bus.subscribe();
        let failed = |reason: &str| JobEvent {
            kind: JobEventKind::Failed,
            job_id: "j1".into(),
            data: serde_json::json!({"type": "export", "error": {"code": "job_failed",
                "message": reason, "data": {"job_id": "j1", "reason": reason}}}),
        };
        publish_job_event(
            &bus,
            failed("FFmpeg was not found: install it with Homebrew (brew install ffmpeg) or set CM_FFMPEG to the ffmpeg executable. CaptureMann releases include their own copy."),
        );
        publish_job_event(&bus, failed("FFmpeg failed (exit status: 1): boom"));
        let a = rx.try_recv().unwrap();
        assert_eq!(a.data["error"]["data"]["reason_code"], "ffmpeg_not_found");
        assert_eq!(a.data["error"]["code"], "job_failed");
        let b = rx.try_recv().unwrap();
        assert!(b.data["error"]["data"].get("reason_code").is_none());
    }

    #[test]
    fn r2_9_concurrent_publishers_deliver_in_seq_order() {
        let bus = EventBus::new(Arc::new(crate::clock::SystemClock));
        let mut rx = bus.subscribe();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let b = bus.clone();
                std::thread::spawn(move || {
                    for _ in 0..100 {
                        b.publish("x.y", Value::Null);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let mut last = 0;
        let mut n = 0;
        while let Ok(ev) = rx.try_recv() {
            assert!(ev.seq > last, "seq {} after {last}", ev.seq);
            last = ev.seq;
            n += 1;
        }
        assert_eq!(n, 800);
        assert_eq!(bus.current_seq(), 800);
    }

    #[test]
    fn topics() {
        assert!(topic_matches("*", "job.done"));
        assert!(topic_matches("job.*", "job.done"));
        assert!(!topic_matches("job.*", "jobs.done"));
        assert!(!topic_matches("job.*", "capture.created"));
        assert!(topic_matches("capture.created", "capture.created"));
        assert!(!topic_matches("capture.created", "capture.revised"));
    }
}
