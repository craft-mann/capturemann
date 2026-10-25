//! Where the sheet is: editing the form, waiting for the user to answer the daemon's
//! confirmation, following the job, or showing its result. Display-free.

use cm_i18n::tr;

use super::job::{Failure, Outcome, Phase, Tracker};

/// What the sheet itself remembers; the job's progress and result come from the [`Tracker`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Stage {
    #[default]
    Editing,
    /// `publish_video` was sent; the daemon is asking the user to confirm.
    Starting,
    /// The daemon started this job.
    Job(String),
    /// The call itself failed (declined, not implemented, refused input) so there is no job.
    Failed(Failure),
}

/// What to draw.
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Editing,
    /// Waiting for the confirmation dialog.
    Confirming,
    Running {
        progress: f64,
        message: Option<String>,
    },
    Done(Outcome),
    Failed(Failure),
}

impl Stage {
    pub fn view(&self, tracker: &Tracker) -> View {
        match self {
            Stage::Editing => View::Editing,
            Stage::Starting => View::Confirming,
            Stage::Failed(f) => View::Failed(f.clone()),
            Stage::Job(id) => match tracker.phase(id) {
                None => View::Running {
                    progress: 0.0,
                    message: None,
                },
                Some(Phase::Running { progress, message }) => View::Running {
                    progress: *progress,
                    message: message.clone(),
                },
                Some(Phase::Done(o)) => View::Done(o.clone()),
                Some(Phase::Failed(f)) => View::Failed(f.clone()),
            },
        }
    }

    /// The form cannot be edited or sent again while this is true.
    pub fn locked(&self, tracker: &Tracker) -> bool {
        !matches!(self.view(tracker), View::Editing | View::Failed(_))
    }
}

/// The text of the progress line.
pub fn progress_text(progress: f64, message: Option<&str>) -> String {
    let pct = (progress * 100.0).floor() as u32;
    match message {
        Some(m) if !m.is_empty() => format!("{m} {pct}%"),
        _ => tr!("Uploading to YouTube {pct}%", pct = pct),
    }
}

/// A new key for every send, so the daemon's idempotency replay never answers a fresh publish
/// with an old one (a retry of a failed job uses [`retry_key`] instead).
pub fn request_key(counter: u64, nanos: u128) -> String {
    format!("ui-publish-{nanos:x}-{counter}")
}

pub fn retry_key(job_id: &str, counter: u64) -> String {
    format!("ui-publish-retry-{job_id}-{counter}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_stage_follows_the_tracker() {
        let mut t = Tracker::default();
        let s = Stage::Job("j1".into());
        assert_eq!(
            s.view(&t),
            View::Running {
                progress: 0.0,
                message: None
            }
        );
        t.on_event(
            "job.progress",
            &json!({"job_id":"j1","type":"publish","progress":0.25}),
        );
        assert!(matches!(s.view(&t), View::Running { progress, .. } if progress == 0.25));
        assert!(s.locked(&t));
        t.on_event(
            "job.done",
            &json!({"job_id":"j1","type":"publish","result":{"url":"https://youtu.be/z"}}),
        );
        assert!(matches!(s.view(&t), View::Done(o) if o.url.ends_with("/z")));
        assert!(s.locked(&t), "a published sheet stays read-only");
    }

    #[test]
    fn a_failed_job_or_call_unlocks_the_form_for_a_retry() {
        let mut t = Tracker::default();
        t.on_event(
            "job.failed",
            &json!({"job_id":"j1","type":"publish","error":{"code":"network","message":"x"}}),
        );
        let s = Stage::Job("j1".into());
        assert!(matches!(s.view(&t), View::Failed(f) if f.can_retry_job()));
        assert!(!s.locked(&t));
        let call = Stage::Failed(Failure {
            code: None,
            message: "no".into(),
            own: None,
            job_id: None,
        });
        assert!(!call.locked(&t));
        assert!(Stage::Starting.locked(&t));
        assert_eq!(Stage::Starting.view(&t), View::Confirming);
    }

    #[test]
    fn progress_reads_in_percent_and_keys_do_not_repeat() {
        assert_eq!(progress_text(0.456, None), "Uploading to YouTube 45%");
        assert_eq!(
            progress_text(0.5, Some("Setting the thumbnail")),
            "Setting the thumbnail 50%"
        );
        assert_ne!(request_key(1, 5), request_key(2, 5));
        assert_ne!(retry_key("j", 1), retry_key("j", 2));
    }
}
