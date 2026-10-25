//! Cancelling a running publish from the Publish page: what the page remembers between the press
//! of Cancel and the moment the job has really stopped. Display-free.
//!
//! The daemon's `cancel_job` answers at once, but a running upload only reaches `cancelled` when
//! its handler stops (`cancelling: true` until then), and a cancelled job emits no bus event, so
//! the page learns about it from the answer or from `get_job`. The page never stays stuck: an
//! answer that says the job already ended, or any error, ends the "Cancelling" state.

use cm_core::ErrorCode;
use serde_json::Value;
use std::time::Duration;

/// How often the page asks `get_job` while it is not cancelling (events are the main source).
pub const POLL_IDLE: Duration = Duration::from_secs(3);
/// How often it asks while a cancel is pending.
pub const POLL_CANCELLING: Duration = Duration::from_millis(500);

/// What to do after an answer or a job state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Nothing to change (still stopping, or it is not about the job being cancelled).
    Wait,
    /// The job is `cancelled`: back to the form with the "Publish cancelled" toast.
    Cancelled,
    /// The job ended another way (done or failed) before the cancel took: ask `get_job` and show
    /// its result as usual, no cancel toast.
    Ended,
    /// `cancel_job` failed: the typed error toast.
    Error {
        code: Option<ErrorCode>,
        message: String,
    },
}

/// The page's cancel state; at most one publish is being cancelled (the page shows one).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CancelState {
    #[default]
    Idle,
    /// Cancel was pressed for this job and the job has not stopped yet.
    Cancelling(String),
}

fn ended(state: &str) -> bool {
    matches!(state, "done" | "succeeded" | "completed" | "failed")
}

impl CancelState {
    pub fn is_cancelling(&self, job_id: &str) -> bool {
        matches!(self, CancelState::Cancelling(j) if j == job_id)
    }

    /// Cancel was pressed. `running` is whether the page is showing the progress of `job_id`;
    /// `false` (nothing to cancel) or a second press changes nothing and sends nothing.
    pub fn press(&mut self, job_id: &str, running: bool) -> bool {
        if !running || *self != CancelState::Idle {
            return false;
        }
        *self = CancelState::Cancelling(job_id.to_owned());
        true
    }

    /// The answer of `cancel_job` for `job_id`: the job object, or the typed error.
    pub fn on_reply(
        &mut self,
        job_id: &str,
        reply: Result<&Value, (Option<ErrorCode>, &str)>,
    ) -> Step {
        if !self.is_cancelling(job_id) {
            return Step::Wait;
        }
        match reply {
            Ok(job) => self.on_job(job_id, job["state"].as_str().unwrap_or_default()),
            Err((Some(ErrorCode::InvalidInput), _)) => {
                // "A done or failed job is invalid_input": it finished while the cancel was sent.
                *self = CancelState::Idle;
                Step::Ended
            }
            Err((code, message)) => {
                *self = CancelState::Idle;
                Step::Error {
                    code,
                    message: message.to_owned(),
                }
            }
        }
    }

    /// The state of a job from `get_job` (or a cancel answer).
    pub fn on_job(&mut self, job_id: &str, state: &str) -> Step {
        if !self.is_cancelling(job_id) {
            return Step::Wait;
        }
        if state == "cancelled" {
            *self = CancelState::Idle;
            Step::Cancelled
        } else if ended(state) {
            *self = CancelState::Idle;
            Step::Ended
        } else {
            Step::Wait
        }
    }

    /// The page is no longer showing a running job (a result, a failure or the form): nothing is
    /// pending any more.
    pub fn sync(&mut self, running: bool) {
        if !running {
            *self = CancelState::Idle;
        }
    }

    pub fn poll_interval(&self) -> Duration {
        match self {
            CancelState::Idle => POLL_IDLE,
            CancelState::Cancelling(_) => POLL_CANCELLING,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn idle_ignores_everything() {
        let mut c = CancelState::default();
        assert!(!c.is_cancelling("j1"));
        assert_eq!(c.on_job("j1", "cancelled"), Step::Wait);
        assert_eq!(
            c.on_reply("j1", Ok(&json!({"state":"cancelled"}))),
            Step::Wait
        );
        assert_eq!(c.poll_interval(), POLL_IDLE);
    }

    #[test]
    fn a_press_while_running_waits_for_the_job_to_stop() {
        let mut c = CancelState::default();
        assert!(c.press("j1", true));
        assert!(c.is_cancelling("j1"));
        assert_eq!(c.poll_interval(), POLL_CANCELLING);
        // `cancel_job` says the stop was requested.
        let reply = json!({"state":"running","cancelling":true});
        assert_eq!(c.on_reply("j1", Ok(&reply)), Step::Wait);
        assert!(c.is_cancelling("j1"));
        assert_eq!(c.on_job("j1", "running"), Step::Wait);
        assert_eq!(c.on_job("j1", "cancelled"), Step::Cancelled);
        assert_eq!(c, CancelState::Idle);
    }

    #[test]
    fn a_queued_job_is_cancelled_by_the_answer_itself() {
        let mut c = CancelState::default();
        assert!(c.press("j1", true));
        let reply = json!({"state":"cancelled","cancelling":false});
        assert_eq!(c.on_reply("j1", Ok(&reply)), Step::Cancelled);
        assert_eq!(c, CancelState::Idle);
    }

    #[test]
    fn a_second_press_or_a_press_with_no_progress_sends_nothing() {
        let mut c = CancelState::default();
        assert!(!c.press("j1", false));
        assert!(c.press("j1", true));
        assert!(!c.press("j1", true));
    }

    #[test]
    fn finished_while_cancelling_shows_the_result_not_a_cancel() {
        let mut c = CancelState::default();
        assert!(c.press("j1", true));
        assert_eq!(c.on_job("j1", "done"), Step::Ended);
        assert_eq!(c, CancelState::Idle);
        // The daemon refused the cancel because the job had just ended.
        let mut c = CancelState::default();
        assert!(c.press("j1", true));
        assert_eq!(
            c.on_reply("j1", Err((Some(ErrorCode::InvalidInput), "job is done"))),
            Step::Ended
        );
        assert_eq!(c, CancelState::Idle);
        // Failed on its own while the stop was pending.
        assert!(c.press("j1", true));
        assert_eq!(c.on_job("j1", "failed"), Step::Ended);
    }

    #[test]
    fn a_cancel_error_ends_the_cancelling_state_and_is_reported() {
        let mut c = CancelState::default();
        assert!(c.press("j1", true));
        let step = c.on_reply("j1", Err((Some(ErrorCode::Internal), "boom")));
        assert_eq!(
            step,
            Step::Error {
                code: Some(ErrorCode::Internal),
                message: "boom".into()
            }
        );
        assert_eq!(c, CancelState::Idle);
        // The button works again.
        assert!(c.press("j1", true));
    }

    #[test]
    fn two_publishes_only_the_cancelled_one_is_followed() {
        let mut c = CancelState::default();
        assert!(c.press("j1", true));
        assert_eq!(c.on_job("j2", "cancelled"), Step::Wait);
        assert_eq!(c.on_job("j2", "done"), Step::Wait);
        assert_eq!(c.on_reply("j2", Err((None, "x"))), Step::Wait);
        assert!(c.is_cancelling("j1") && !c.is_cancelling("j2"));
        assert_eq!(c.on_job("j1", "cancelled"), Step::Cancelled);
    }

    #[test]
    fn sync_clears_a_pending_cancel_when_the_page_leaves_the_running_view() {
        let mut c = CancelState::default();
        assert!(c.press("j1", true));
        c.sync(true);
        assert!(c.is_cancelling("j1"));
        c.sync(false);
        assert_eq!(c, CancelState::Idle);
    }
}
