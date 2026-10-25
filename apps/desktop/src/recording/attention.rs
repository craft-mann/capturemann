//! The tray's Attention rule beyond uploads, display-free: a publish job that failed or waits for
//! its account to sign in again, and a library folder that is offline. `ui::upload_ui` feeds
//! daemon events in and ORs [`AttentionTracker::needs_attention`] with the upload rule
//! (`UploadTracker::needs_attention`).
//!
//! Events used: `job.failed` and `job.paused` (`pause_reason: "auth_expired"`) with
//! `type: "publish"`; they clear on `job.progress` or `job.done` of the same job (a retry ran
//! again, or it finished), on `publish.done`, and on [`AttentionTracker::acknowledge_publishes`]
//! (the user opened the Published page or the publish sheet and saw them). `library.offline` and
//! `library.online` come from the daemon's library watcher.

use std::collections::HashSet;

use serde_json::Value;

#[derive(Debug, Default)]
pub struct AttentionTracker {
    /// Publish jobs that are failed or paused as `auth_expired` and not yet seen or retried.
    publishes: HashSet<String>,
    library_offline: bool,
}

impl AttentionTracker {
    /// Applies one daemon event; true when the answer of [`Self::needs_attention`] changed (the
    /// caller refreshes the tray).
    pub fn on_event(&mut self, name: &str, data: &Value) -> bool {
        let before = self.needs_attention();
        let job = data["job_id"].as_str().filter(|s| !s.is_empty());
        let is_publish = data["type"] == "publish";
        match (name, job) {
            ("library.offline", _) => self.library_offline = true,
            ("library.online", _) => self.library_offline = false,
            ("job.failed", Some(id)) if is_publish => {
                self.publishes.insert(id.to_owned());
            }
            ("job.paused", Some(id)) if is_publish && data["pause_reason"] == "auth_expired" => {
                self.publishes.insert(id.to_owned());
            }
            ("job.progress" | "job.done" | "publish.done", Some(id)) => {
                self.publishes.remove(id);
            }
            _ => {}
        }
        before != self.needs_attention()
    }

    /// The user looked at the publishes (Published page, publish sheet): they were seen. True
    /// when that changed the answer.
    pub fn acknowledge_publishes(&mut self) -> bool {
        let before = self.needs_attention();
        self.publishes.clear();
        before != self.needs_attention()
    }

    /// A publish job is failed or paused for a sign-in, or the library folder is offline.
    pub fn needs_attention(&self) -> bool {
        !self.publishes.is_empty() || self.library_offline
    }

    pub fn library_offline(&self) -> bool {
        self.library_offline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn failed(job: &str, ty: &str) -> Value {
        json!({ "job_id": job, "type": ty, "state": "failed", "error": { "code": "network" } })
    }
    fn paused(job: &str, ty: &str, why: &str) -> Value {
        json!({ "job_id": job, "type": ty, "state": "paused", "pause_reason": why })
    }

    #[test]
    fn a_failed_publish_turns_attention_on_and_a_retry_or_finish_clears_it() {
        let mut t = AttentionTracker::default();
        assert!(!t.needs_attention());
        assert!(t.on_event("job.failed", &failed("p1", "publish")));
        assert!(t.needs_attention());
        // Retry: the job runs again.
        assert!(t.on_event(
            "job.progress",
            &json!({ "job_id": "p1", "type": "publish" })
        ));
        assert!(!t.needs_attention());
        t.on_event("job.failed", &failed("p1", "publish"));
        assert!(t.on_event("publish.done", &json!({ "job_id": "p1" })));
        assert!(!t.needs_attention());
        t.on_event("job.failed", &failed("p2", "publish"));
        assert!(t.on_event("job.done", &json!({ "job_id": "p2", "type": "publish" })));
        assert!(!t.needs_attention());
    }

    #[test]
    fn a_publish_waiting_for_sign_in_needs_attention_other_pauses_do_not() {
        let mut t = AttentionTracker::default();
        assert!(!t.on_event("job.paused", &paused("p1", "publish", "user")));
        assert!(!t.needs_attention());
        assert!(t.on_event("job.paused", &paused("p1", "publish", "auth_expired")));
        assert!(t.needs_attention());
    }

    #[test]
    fn other_job_types_and_unrelated_events_are_ignored() {
        let mut t = AttentionTracker::default();
        assert!(!t.on_event("job.failed", &failed("e1", "export")));
        assert!(!t.on_event("job.failed", &failed("u1", "upload")));
        assert!(!t.on_event("job.paused", &paused("u1", "upload", "auth_expired")));
        assert!(!t.on_event("capture.created", &json!({})));
        assert!(!t.needs_attention());
        // A failed job without an id cannot be tracked, so it cannot be cleared either.
        assert!(!t.on_event("job.failed", &json!({ "type": "publish" })));
    }

    #[test]
    fn two_failed_publishes_stay_on_until_both_are_gone() {
        let mut t = AttentionTracker::default();
        t.on_event("job.failed", &failed("p1", "publish"));
        t.on_event("job.failed", &failed("p2", "publish"));
        assert!(!t.on_event("job.progress", &json!({ "job_id": "p1" })));
        assert!(t.needs_attention());
        assert!(t.on_event("job.progress", &json!({ "job_id": "p2" })));
        assert!(!t.needs_attention());
    }

    #[test]
    fn seeing_the_publishes_clears_them() {
        let mut t = AttentionTracker::default();
        assert!(!t.acknowledge_publishes());
        t.on_event("job.failed", &failed("p1", "publish"));
        assert!(t.acknowledge_publishes());
        assert!(!t.needs_attention());
    }

    #[test]
    fn an_offline_library_needs_attention_until_it_is_back() {
        let mut t = AttentionTracker::default();
        assert!(t.on_event("library.offline", &json!({ "path": "/Volumes/X" })));
        assert!(t.needs_attention() && t.library_offline());
        // Again: nothing changes.
        assert!(!t.on_event("library.offline", &json!({ "path": "/Volumes/X" })));
        assert!(t.on_event("library.online", &json!({ "path": "/Volumes/X" })));
        assert!(!t.needs_attention());
    }

    #[test]
    fn the_two_causes_are_independent() {
        let mut t = AttentionTracker::default();
        t.on_event("library.offline", &json!({}));
        t.on_event("job.failed", &failed("p1", "publish"));
        t.on_event("library.online", &json!({}));
        assert!(t.needs_attention(), "the publish still needs the user");
        t.on_event("job.progress", &json!({ "job_id": "p1" }));
        assert!(!t.needs_attention());
        t.on_event("library.offline", &json!({}));
        t.acknowledge_publishes();
        assert!(
            t.needs_attention(),
            "seeing publishes does not bring the drive back"
        );
    }
}
