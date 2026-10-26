//! Daemon events as MCP notifications (AGT-1).
//!
//! MCP has no event channel of its own, so each daemon event is sent to the client as a
//! `notifications/message` (the MCP logging notification) with `logger: "capturemann.events"`
//! and `data` = the daemon's event object `{ seq, event, at, data }`. The server declares the
//! `logging` capability and honours `logging/setLevel` (default `info`, which forwards all).
//!
//! The stream is the daemon's `events.subscribe`, so the per-client policy applies: a blocked
//! client gets no events. Only the topics in [`TOPICS`] are forwarded: the 1 Hz
//! `recording.stats`, the 5 s `recording.health` and the per-second `recording.countdown` are
//! left out because they would flood a model's context; agents read the same numbers with
//! `get_recording_status`. A client that reads slowly loses events and gets one
//! `events.dropped { dropped }` (see [`crate::conn`]).

// The logging notification types are deprecated upstream (SEP-2577) but are the only standard
// server-to-client notification a generic MCP client displays.
#![allow(deprecated)]

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use rmcp::model::{LoggingLevel, LoggingMessageNotificationParam, SetLevelRequestParams};
use rmcp::{Peer, RoleServer};
use serde_json::Value;
use tokio::sync::mpsc;

/// The `logger` field of every event notification.
pub const LOGGER: &str = "capturemann.events";

/// Daemon topics forwarded to MCP clients.
pub const TOPICS: &[&str] = &[
    "capture.*",
    "job.*",
    "export.*",
    "confirmation.*",
    "recording.starting",
    "recording.started",
    "recording.paused",
    "recording.resumed",
    "recording.stopped",
    "recording.discarded",
    "recording.warning",
    "recording.recovered",
    "recording.selftest",
    "camera.*",
    // Local AI: a model was installed, selected or removed; a transcript is ready. The download
    // progress of `model.download.progress` is left out (`job.progress` carries it).
    "model.changed",
    "transcript.ready",
    "translation.ready",
];

/// The minimum level a client asked for (`logging/setLevel`), shared with the forwarder.
#[derive(Clone)]
pub struct LevelFilter(Arc<AtomicU8>);

impl Default for LevelFilter {
    fn default() -> Self {
        LevelFilter(Arc::new(AtomicU8::new(rank(LoggingLevel::Info))))
    }
}

impl LevelFilter {
    pub fn set(&self, params: &SetLevelRequestParams) {
        self.0.store(rank(params.level), Ordering::Relaxed);
    }

    fn allows(&self, level: LoggingLevel) -> bool {
        rank(level) >= self.0.load(Ordering::Relaxed)
    }
}

fn rank(level: LoggingLevel) -> u8 {
    match level {
        LoggingLevel::Debug => 0,
        LoggingLevel::Info => 1,
        LoggingLevel::Notice => 2,
        LoggingLevel::Warning => 3,
        LoggingLevel::Error => 4,
        LoggingLevel::Critical => 5,
        LoggingLevel::Alert => 6,
        LoggingLevel::Emergency => 7,
    }
}

/// The severity an event is sent with.
pub fn level_of(event: &str) -> LoggingLevel {
    match event {
        "job.failed" => LoggingLevel::Error,
        "recording.warning" | "events.dropped" => LoggingLevel::Warning,
        _ => LoggingLevel::Info,
    }
}

/// Forwards `rx` to `peer` until either side ends.
pub fn spawn_forwarder(peer: Peer<RoleServer>, mut rx: mpsc::Receiver<Value>, filter: LevelFilter) {
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let level = level_of(ev.get("event").and_then(Value::as_str).unwrap_or_default());
            if !filter.allows(level) {
                continue;
            }
            let note = LoggingMessageNotificationParam::new(level, ev).with_logger(LOGGER);
            if peer.notify_logging_message(note).await.is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels() {
        assert!(matches!(level_of("job.failed"), LoggingLevel::Error));
        assert!(matches!(
            level_of("recording.warning"),
            LoggingLevel::Warning
        ));
        assert!(matches!(level_of("job.done"), LoggingLevel::Info));
    }

    #[test]
    fn filter_defaults_to_everything_and_follows_set_level() {
        let f = LevelFilter::default();
        assert!(f.allows(LoggingLevel::Info));
        f.set(&SetLevelRequestParams::new(LoggingLevel::Warning));
        assert!(!f.allows(LoggingLevel::Info));
        assert!(f.allows(LoggingLevel::Error));
    }

    #[test]
    fn noisy_topics_are_not_forwarded() {
        for noisy in ["recording.stats", "recording.health", "recording.countdown"] {
            assert!(!TOPICS.iter().any(|t| cm_daemon_topic_matches(t, noisy)));
        }
        assert!(TOPICS
            .iter()
            .any(|t| cm_daemon_topic_matches(t, "job.done")));
        assert!(TOPICS
            .iter()
            .any(|t| cm_daemon_topic_matches(t, "recording.started")));
        // Camera hot-plug events reach agents too.
        for ev in ["camera.connected", "camera.disconnected"] {
            assert!(TOPICS.iter().any(|t| cm_daemon_topic_matches(t, ev)));
        }
    }

    /// Same rule as the daemon's `topic_matches` (exact name or `prefix.*`).
    fn cm_daemon_topic_matches(topic: &str, event: &str) -> bool {
        match topic.strip_suffix(".*") {
            Some(p) => event.strip_prefix(p).is_some_and(|r| r.starts_with('.')),
            None => topic == event,
        }
    }
}
