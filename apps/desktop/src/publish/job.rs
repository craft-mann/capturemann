//! The publish job as the sheet sees it: progress, the result (link and warnings) and the
//! failure with what the user can do about it. Display-free.
//!
//! The daemon announces a job with `job.progress` (`{ job_id, type, progress, message }`),
//! `job.done` (`{ job_id, type, state, result }`) and `job.failed` (`{ job_id, type, state,
//! error }`); the publish job's `type` is `publish` and its result `{ url, video_id, status,
//! warnings[] }` (specs/features/11-publishing.md, PUB-2). The same parser reads a `get_job`
//! answer, which is how the sheet recovers a result it missed.

use std::collections::HashMap;

use cm_core::ErrorCode;
use cm_i18n::tr;
use serde_json::Value;

/// The job `type` of a publish.
pub const JOB_TYPE: &str = "publish";

/// A finished publish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub url: String,
    pub video_id: Option<String>,
    /// What to tell the user besides the link; worded when drawn ([`OutcomeWarning::text`]).
    pub warnings: Vec<OutcomeWarning>,
    /// YouTube kept the video private although another visibility was asked for.
    pub privacy_locked: bool,
}

/// A line under the link of a finished publish. Kept as data, so a result still on screen
/// follows a language switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutcomeWarning {
    /// A warning of the provider, as it sent it (words for the known ones, else as it came).
    Provider(String),
    /// YouTube kept the video private (unverified project).
    PrivacyLocked,
}

impl OutcomeWarning {
    pub fn text(&self) -> String {
        match self {
            OutcomeWarning::Provider(w) => warning_words(w),
            OutcomeWarning::PrivacyLocked => privacy_locked_words(),
        }
    }
}

/// A failure the app words itself (it has no daemon text). Worded when drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnFailure {
    NoLink,
    NoDaemon,
    NoJob,
    UploadFailed,
}

impl OwnFailure {
    pub fn text(self) -> String {
        match self {
            OwnFailure::NoLink => tr!("The upload finished but the daemon sent no link."),
            OwnFailure::NoDaemon => tr!("The app has no daemon to publish with."),
            OwnFailure::NoJob => tr!("The daemon accepted the publish but sent no job."),
            OwnFailure::UploadFailed => tr!("The upload failed."),
        }
    }
}

/// A publish that did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub code: Option<ErrorCode>,
    /// The daemon's (English) message, or a technical text; see [`Failure::own`] for the app's.
    pub message: String,
    /// The app's own sentence, when it made the failure (shown instead of `message`).
    pub own: Option<OwnFailure>,
    /// The job that failed, when there is one (the daemon refused or the user declined before a
    /// job existed otherwise).
    pub job_id: Option<String>,
}

impl Failure {
    /// A failure the app words itself.
    pub fn own(own: OwnFailure, code: Option<ErrorCode>, job_id: Option<String>) -> Failure {
        Failure {
            code,
            message: String::new(),
            own: Some(own),
            job_id,
        }
    }

    /// The daemon's message in the current language (its stable ids are translated).
    fn said(&self) -> String {
        crate::daemon_text::error_text(self.code, &self.message)
    }

    /// The sign-in is gone: the card in Settings offers Reconnect.
    pub fn needs_reconnect(&self) -> bool {
        self.code == Some(ErrorCode::AuthExpired)
    }

    /// Retrying the job (`retry_job`) can help: it exists and the failure is not a refusal of
    /// the request itself.
    pub fn can_retry_job(&self) -> bool {
        self.job_id.is_some()
            && !matches!(
                self.code,
                Some(ErrorCode::InvalidInput | ErrorCode::AuthExpired | ErrorCode::UserDeclined)
            )
    }

    /// What to tell the user, in plain words.
    pub fn text(&self) -> String {
        if let Some(own) = self.own {
            return own.text();
        }
        match self.code {
            Some(ErrorCode::NotImplemented) => tr!(
                "Publishing is not available in this build yet: {message}",
                message = self.said()
            ),
            Some(ErrorCode::UserDeclined) => {
                tr!("The publish was not confirmed, so nothing was sent to YouTube.")
            }
            Some(ErrorCode::ConfirmationTimeout) => {
                tr!("The confirmation dialog timed out, so nothing was sent to YouTube.")
            }
            Some(ErrorCode::AuthExpired) => tr!(
                "YouTube no longer accepts this sign-in. Reconnect the account in Settings \u{2192} Destinations. {message}",
                message = self.said()
            ),
            Some(ErrorCode::Network) => tr!(
                "The connection to YouTube failed. The upload continues from where it stopped when you retry. {message}",
                message = self.said()
            ),
            _ => self.said(),
        }
    }
}

/// Where the job is.
#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Running {
        /// 0..=1.
        progress: f64,
        message: Option<String>,
    },
    Done(Outcome),
    Failed(Failure),
}

/// The publish jobs seen in this run.
#[derive(Debug, Default)]
pub struct Tracker {
    jobs: HashMap<String, Phase>,
}

impl Tracker {
    /// Applies one daemon event; `true` when it was a publish job's.
    pub fn on_event(&mut self, name: &str, data: &Value) -> bool {
        if data["type"] != JOB_TYPE {
            return false;
        }
        let Some(id) = data["job_id"].as_str() else {
            return false;
        };
        match name {
            "job.progress" => {
                let progress = data["progress"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0);
                // A late progress event never undoes a result.
                if matches!(self.jobs.get(id), Some(Phase::Done(_) | Phase::Failed(_))) {
                    return true;
                }
                self.jobs.insert(
                    id.to_owned(),
                    Phase::Running {
                        progress,
                        message: data["message"].as_str().map(str::to_owned),
                    },
                );
            }
            "job.done" => {
                let phase = match parse_outcome(&data["result"]) {
                    Some(o) => Phase::Done(o),
                    None => Phase::Failed(Failure::own(
                        OwnFailure::NoLink,
                        Some(ErrorCode::Internal),
                        Some(id.to_owned()),
                    )),
                };
                self.jobs.insert(id.to_owned(), phase);
            }
            "job.failed" => {
                self.jobs.insert(
                    id.to_owned(),
                    Phase::Failed(parse_failure(&data["error"], id)),
                );
            }
            _ => return false,
        }
        true
    }

    pub fn phase(&self, job_id: &str) -> Option<&Phase> {
        self.jobs.get(job_id)
    }

    /// Forgets a job (a retry starts it afresh).
    pub fn reset(&mut self, job_id: &str) {
        self.jobs.remove(job_id);
    }

    /// Takes the state of a job from a `get_job` answer (the sheet asks when it may have missed
    /// the events).
    pub fn on_job(&mut self, job: &Value) {
        let state = job["state"].as_str().unwrap_or_default();
        let name = match state {
            "done" | "succeeded" | "completed" => "job.done",
            "failed" => "job.failed",
            "running" | "queued" | "paused" => "job.progress",
            _ => return,
        };
        let mut data = job.clone();
        data["type"] = Value::String(JOB_TYPE.to_owned());
        if job["type"] == JOB_TYPE {
            self.on_event(name, &data);
        }
    }
}

/// The `result` of a publish job.
pub fn parse_outcome(result: &Value) -> Option<Outcome> {
    let url = result["url"].as_str().filter(|u| !u.is_empty())?.to_owned();
    let privacy_locked = result["status"].as_str() == Some("privacy_locked");
    let raw: Vec<&str> = result["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let mut warnings: Vec<OutcomeWarning> = raw
        .iter()
        .map(|w| OutcomeWarning::Provider((*w).to_owned()))
        .collect();
    // Said once: the provider's own warning about the privacy already covers the lock.
    if privacy_locked && !raw.iter().any(|w| w.to_lowercase().contains("private")) {
        warnings.insert(0, OutcomeWarning::PrivacyLocked);
    }
    Some(Outcome {
        url,
        video_id: result["video_id"].as_str().map(str::to_owned),
        warnings,
        privacy_locked,
    })
}

/// Why a video stays private although another visibility was asked for.
pub fn privacy_locked_words() -> String {
    tr!(
        "YouTube kept this video private because your Google Cloud project is not verified. Nothing was changed afterwards. Open the video in YouTube Studio to make it unlisted or public, or verify your project."
    )
}

/// A provider warning in words for the sheet: the thumbnail refusals and the privacy lock are
/// the ones people ask about.
fn warning_words(w: &str) -> String {
    let l = w.to_lowercase();
    if l.contains("thumbnail") && l.contains("verif") {
        tr!(
            "The video is on YouTube, but it refused the thumbnail: a custom thumbnail needs a verified YouTube account (verify your phone number in YouTube Studio)."
        )
    } else if l.contains("thumbnail") {
        tr!(
            "The video is on YouTube, but the thumbnail was not set: {warning}",
            warning = w
        )
    } else if l.contains("privacy") && l.contains("private") {
        privacy_locked_words()
    } else {
        w.to_owned()
    }
}

/// An `error` payload (`{ code, message, data? }`).
pub fn parse_failure(error: &Value, job_id: &str) -> Failure {
    let code: Option<ErrorCode> = serde_json::from_value(error["code"].clone()).ok();
    let message = error["data"]["reason"]
        .as_str()
        .or_else(|| error["message"].as_str())
        .map(str::to_owned);
    match message {
        Some(message) => Failure {
            code,
            message,
            own: None,
            job_id: Some(job_id.to_owned()),
        },
        None => Failure::own(OwnFailure::UploadFailed, code, Some(job_id.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(t: &mut Tracker, name: &str, data: Value) -> bool {
        t.on_event(name, &data)
    }

    #[test]
    fn progress_then_the_link_and_the_warnings() {
        let mut t = Tracker::default();
        assert!(ev(
            &mut t,
            "job.progress",
            json!({"job_id":"j1","type":"publish","progress":0.4,"message":"Uploading"})
        ));
        assert_eq!(
            t.phase("j1"),
            Some(&Phase::Running {
                progress: 0.4,
                message: Some("Uploading".into())
            })
        );
        ev(
            &mut t,
            "job.done",
            json!({"job_id":"j1","type":"publish","state":"done","result":{
            "url":"https://youtu.be/abc","video_id":"abc","status":"processing",
            "warnings":["Thumbnail needs a verified YouTube account"]}}),
        );
        let Some(Phase::Done(o)) = t.phase("j1") else {
            panic!("done")
        };
        assert_eq!(o.url, "https://youtu.be/abc");
        assert_eq!(o.video_id.as_deref(), Some("abc"));
        assert!(!o.privacy_locked);
        // PUB-2.AC2: a warning in plain words, not a failure.
        assert!(o.warnings[0].text().contains("refused the thumbnail"));
        assert!(o.warnings[0].text().contains("verified YouTube account"));
        // A late progress event does not undo the result.
        ev(
            &mut t,
            "job.progress",
            json!({"job_id":"j1","type":"publish","progress":0.9}),
        );
        assert!(matches!(t.phase("j1"), Some(Phase::Done(_))));
    }

    #[test]
    fn a_locked_privacy_is_said_in_plain_words() {
        let o = parse_outcome(
            &json!({"url":"https://youtu.be/x","status":"privacy_locked","warnings":[]}),
        )
        .unwrap();
        assert!(o.privacy_locked);
        assert!(o.warnings[0].text().contains("kept this video private"));
        assert!(o.warnings[0].text().contains("not verified"));
        // The provider's own warning is not shown twice.
        let o = parse_outcome(
            &json!({"url":"https://youtu.be/x","status":"privacy_locked",
            "warnings":["YouTube kept the privacy private for an unverified project"]}),
        )
        .unwrap();
        assert_eq!(o.warnings.len(), 1);
    }

    #[test]
    fn other_jobs_are_ignored() {
        let mut t = Tracker::default();
        assert!(!ev(
            &mut t,
            "job.progress",
            json!({"job_id":"e1","type":"export","progress":0.4})
        ));
        assert!(t.phase("e1").is_none());
    }

    #[test]
    fn failures_carry_the_code_and_what_to_do() {
        let mut t = Tracker::default();
        ev(
            &mut t,
            "job.failed",
            json!({"job_id":"j2","type":"publish","state":"failed",
            "error":{"code":"auth_expired","message":"YouTube no longer accepts this account."}}),
        );
        let Some(Phase::Failed(f)) = t.phase("j2") else {
            panic!("failed")
        };
        assert!(f.needs_reconnect());
        assert!(!f.can_retry_job());
        assert!(f.text().contains("Reconnect"));

        ev(
            &mut t,
            "job.failed",
            json!({"job_id":"j3","type":"publish","state":"failed",
            "error":{"code":"network","message":"Cannot reach YouTube"}}),
        );
        let Some(Phase::Failed(f)) = t.phase("j3") else {
            panic!("failed")
        };
        assert!(f.can_retry_job());
        assert!(f.text().contains("continues from where it stopped"));

        ev(
            &mut t,
            "job.failed",
            json!({"job_id":"j4","type":"publish","state":"failed",
            "error":{"code":"platform_rejected","message":"Upload limit reached for today; try after 09:00","data":{"reason":"Upload limit reached for today."}}}),
        );
        let Some(Phase::Failed(f)) = t.phase("j4") else {
            panic!("failed")
        };
        assert_eq!(f.message, "Upload limit reached for today.");
        assert!(f.can_retry_job());
    }

    #[test]
    fn the_honest_daemon_error_is_shown_as_it_is() {
        let f = Failure {
            code: Some(ErrorCode::NotImplemented),
            message: "publish_video is not implemented".into(),
            own: None,
            job_id: None,
        };
        assert!(f.text().starts_with("Publishing is not available"));
        assert!(!f.can_retry_job());
        let declined = Failure {
            code: Some(ErrorCode::UserDeclined),
            message: "no".into(),
            own: None,
            job_id: None,
        };
        assert!(declined.text().contains("nothing was sent"));
    }

    #[test]
    fn get_job_recovers_a_result_that_was_missed() {
        let mut t = Tracker::default();
        t.on_job(&json!({"job_id":"j9","type":"publish","state":"done","result":{"url":"https://youtu.be/q"}}));
        assert!(matches!(t.phase("j9"), Some(Phase::Done(o)) if o.url.ends_with("/q")));
        t.on_job(&json!({"job_id":"j8","type":"publish","state":"running","progress":0.5}));
        assert!(matches!(t.phase("j8"), Some(Phase::Running { .. })));
        t.on_job(&json!({"job_id":"j7","type":"export","state":"done"}));
        assert!(t.phase("j7").is_none());
    }
}
