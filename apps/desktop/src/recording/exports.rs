//! The quick export in the UI's display-free logic (REC-1, EXP-1, EXP-4): which export jobs are
//! running and how far they are, and the toasts the user sees.
//!
//! After a recording stops the daemon enqueues the `export` job (setting `export.auto_on_stop`,
//! default on) and announces it with `export.queued`; `job.progress` carries the fraction;
//! `export.done` the file; `job.failed` the readable reason. The existing "Recording saved" toast
//! becomes "Recording saved - exporting..." with progress in steps, then "Exported <title>" with
//! Play and Reveal. When FFmpeg is missing the toast says so and how to install it; the recording
//! itself is unaffected.
//!
//! The window glue (`ui::export_ui`) feeds events into an [`ExportTracker`] and turns its
//! [`Effect`]s into toasts; the library details card reads [`ExportTracker::status_of`].

use std::collections::HashMap;

use cm_i18n::{tr, tr_ctx};
use serde_json::Value;

use crate::toast::{ToastAction, ToastKind, ToastSpec};

/// Progress toasts update in steps of this many percent. The toast window is updated in place now
/// (R4-23), so steps are small; they only bound how often the text is rebuilt.
pub const TOAST_STEP_PCT: u32 = 5;

/// Setting key of "Create a single MP4 after recording" (daemon side: `export.auto_on_stop`).
pub use cm_daemon::export::SETTING_AUTO_ON_STOP as KEY_AUTO_EXPORT;

/// Whether the setting is on (default true when unset or not a boolean).
pub fn auto_export_on(settings: &serde_json::Map<String, Value>) -> bool {
    settings
        .get(KEY_AUTO_EXPORT)
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Running,
    Done,
    Failed,
}

/// One export job as the UI sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportStatus {
    pub job_id: String,
    pub capture_id: String,
    pub title: Option<String>,
    /// 0..=1.
    pub progress: f64,
    pub phase: Phase,
    /// Started by the automatic export after a recording (not by the Export button).
    pub auto: bool,
    /// Last percent shown in a toast, `-1` before the first.
    pub toast_pct: i64,
    /// Why it failed.
    pub reason: Option<String>,
    /// Order of arrival (the newest job of a capture wins in [`ExportTracker::status_of`]).
    pub seq: u64,
    /// An audio export (VOC-2): M4A or WAV, not the combined MP4.
    pub audio: bool,
}

/// What a finished audio export tells (`export.done` with `kind: "audio"`).
#[derive(Debug, Clone, PartialEq)]
pub struct AudioDone {
    /// The file in the recording's `exports/` folder.
    pub path: String,
    pub size_bytes: u64,
    /// The loudness target that was applied, if any.
    pub loudness_lufs: Option<f64>,
    /// Where the extra copy went, when one was asked for.
    pub copy_path: Option<String>,
}

/// What the window glue should do after an event.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Show (first = true) or update the progress toast.
    Progress {
        capture_id: String,
        title: Option<String>,
        pct: u32,
        auto: bool,
        first: bool,
    },
    Done {
        capture_id: String,
        title: String,
        /// What the export did with the captions (`burned`, `off`, `skipped_no_transcript`).
        captions: Option<String>,
    },
    Failed {
        capture_id: String,
        title: Option<String>,
        reason: String,
        ffmpeg_missing: bool,
    },
    /// The automatic export did not run (`export.skipped`): a hint, not an error.
    Skipped { capture_id: String, ffmpeg: bool },
    /// An audio export is running (VOC-2).
    AudioProgress {
        capture_id: String,
        title: Option<String>,
        pct: u32,
        first: bool,
    },
    /// An audio export finished.
    AudioDone {
        capture_id: String,
        title: String,
        done: AudioDone,
    },
    /// An audio export failed.
    AudioFailed {
        capture_id: String,
        title: Option<String>,
        reason: String,
        ffmpeg_missing: bool,
    },
}

/// The running and recently finished export jobs.
#[derive(Debug, Default)]
pub struct ExportTracker {
    jobs: HashMap<String, ExportStatus>,
    next_seq: u64,
}

/// The `error.data.reason` of a `job.failed` event (the readable reason), else its message.
fn failure_reason(data: &Value) -> String {
    let e = &data["error"];
    e["data"]["reason"]
        .as_str()
        .or_else(|| e["message"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| tr!("The export failed."))
}

/// A missing FFmpeg is recognised by the one prefix every such message keeps (the queue wraps
/// every failure in `job_failed`, so the reason is the only carrier). The prefix is owned by
/// `cm_media::tools`, which pins it with a test; the check does not look at the translated text.
pub fn is_ffmpeg_missing(reason: &str) -> bool {
    cm_core::message::ffmpeg_missing_id(reason).is_some()
        || cm_media::tools::is_not_found_message(reason)
}

/// Whether the missing FFmpeg is the copy inside the app (damaged or blocked), not a developer
/// build without one: the toast then says to reinstall instead of pointing at Homebrew.
fn ffmpeg_bundle_problem(reason: &str) -> bool {
    matches!(
        cm_core::message::ffmpeg_missing_id(reason),
        Some("ffmpeg_bundle_missing" | "ffmpeg_bundle_cannot_run")
    )
}

impl ExportTracker {
    /// Applies one daemon event; returns what to show.
    pub fn on_event(&mut self, name: &str, data: &Value) -> Vec<Effect> {
        let job_id = data["job_id"].as_str().unwrap_or_default().to_owned();
        match name {
            "export.queued" => {
                let capture_id = data["capture_id"].as_str().unwrap_or_default().to_owned();
                let title = data["title"].as_str().map(str::to_owned);
                let auto = data["auto"].as_bool().unwrap_or(false);
                let audio = data["kind"].as_str() == Some("audio");
                self.next_seq += 1;
                let seq = self.next_seq;
                self.jobs.insert(
                    job_id.clone(),
                    ExportStatus {
                        job_id,
                        capture_id: capture_id.clone(),
                        title: title.clone(),
                        progress: 0.0,
                        phase: Phase::Running,
                        auto,
                        toast_pct: 0,
                        reason: None,
                        seq,
                        audio,
                    },
                );
                if audio {
                    return vec![Effect::AudioProgress {
                        capture_id,
                        title,
                        pct: 0,
                        first: true,
                    }];
                }
                vec![Effect::Progress {
                    capture_id,
                    title,
                    pct: 0,
                    auto,
                    first: true,
                }]
            }
            "job.progress" if data["type"] == "export" => {
                let Some(j) = self.jobs.get_mut(&job_id) else {
                    return Vec::new();
                };
                let p = data["progress"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0);
                j.progress = p;
                let pct = (p * 100.0).floor() as i64;
                let step = i64::from(TOAST_STEP_PCT);
                if pct / step > j.toast_pct / step && pct < 100 {
                    j.toast_pct = pct / step * step;
                    if j.audio {
                        return vec![Effect::AudioProgress {
                            capture_id: j.capture_id.clone(),
                            title: j.title.clone(),
                            pct: j.toast_pct as u32,
                            first: false,
                        }];
                    }
                    return vec![Effect::Progress {
                        capture_id: j.capture_id.clone(),
                        title: j.title.clone(),
                        pct: j.toast_pct as u32,
                        auto: j.auto,
                        first: false,
                    }];
                }
                Vec::new()
            }
            "export.done" => {
                let capture_id = data["capture_id"].as_str().unwrap_or_default().to_owned();
                let title = data["title"].as_str().map(str::to_owned);
                if let Some(j) = self.jobs.get_mut(&job_id) {
                    j.phase = Phase::Done;
                    j.progress = 1.0;
                    if title.is_some() {
                        j.title.clone_from(&title);
                    }
                }
                let title = title.unwrap_or_else(|| tr_ctx!("fallback title", "recording"));
                if data["kind"].as_str() == Some("audio") {
                    return vec![Effect::AudioDone {
                        capture_id,
                        title,
                        done: AudioDone {
                            path: data["path"].as_str().unwrap_or_default().to_owned(),
                            size_bytes: data["size_bytes"].as_u64().unwrap_or(0),
                            loudness_lufs: data["loudness_lufs"].as_f64(),
                            copy_path: data["copy_path"].as_str().map(str::to_owned),
                        },
                    }];
                }
                vec![Effect::Done {
                    title,
                    capture_id,
                    captions: data["captions"].as_str().map(str::to_owned),
                }]
            }
            "export.skipped" => {
                let capture_id = data["capture_id"].as_str().unwrap_or_default().to_owned();
                vec![Effect::Skipped {
                    capture_id,
                    ffmpeg: data["reason"].as_str() == Some("ffmpeg_not_found"),
                }]
            }
            "job.failed" if data["type"] == "export" => {
                let reason = failure_reason(data);
                let Some(j) = self.jobs.get_mut(&job_id) else {
                    return Vec::new();
                };
                j.phase = Phase::Failed;
                j.reason = Some(reason.clone());
                if j.audio {
                    return vec![Effect::AudioFailed {
                        capture_id: j.capture_id.clone(),
                        title: j.title.clone(),
                        ffmpeg_missing: is_ffmpeg_missing(&reason)
                            || data["error"]["data"]["reason_code"]
                                .as_str()
                                .is_some_and(|c| c.starts_with("ffmpeg_")),
                        reason,
                    }];
                }
                vec![Effect::Failed {
                    capture_id: j.capture_id.clone(),
                    title: j.title.clone(),
                    ffmpeg_missing: is_ffmpeg_missing(&reason)
                        || data["error"]["data"]["reason_code"]
                            .as_str()
                            .is_some_and(|c| c.starts_with("ffmpeg_")),
                    reason,
                }]
            }
            _ => Vec::new(),
        }
    }

    /// The newest running job for a capture, else the newest finished one.
    pub fn status_of(&self, capture_id: &str) -> Option<&ExportStatus> {
        let of_capture = || self.jobs.values().filter(|j| j.capture_id == capture_id);
        of_capture()
            .filter(|j| j.phase == Phase::Running)
            .max_by_key(|j| j.seq)
            .or_else(|| of_capture().max_by_key(|j| j.seq))
    }

    /// Whether a job for this capture is running (the Export button is hidden then).
    pub fn is_running(&self, capture_id: &str) -> bool {
        self.jobs
            .values()
            .any(|j| j.capture_id == capture_id && j.phase == Phase::Running)
    }

    /// Keeps memory bounded over a long session: at most `keep` finished jobs stay (running ones
    /// never go).
    pub fn prune(&mut self, keep: usize) {
        let mut finished: Vec<(u64, String)> = self
            .jobs
            .values()
            .filter(|j| j.phase != Phase::Running)
            .map(|j| (j.seq, j.job_id.clone()))
            .collect();
        finished.sort_unstable();
        let drop = finished.len().saturating_sub(keep);
        for (_, id) in finished.into_iter().take(drop) {
            self.jobs.remove(&id);
        }
    }
}

// ---- toasts ----------------------------------------------------------------------------------------

/// "Recording saved - exporting..." with progress (the saved toast, turned into a progress toast).
pub fn exporting_toast(
    capture_id: &str,
    title: Option<&str>,
    pct: u32,
    after_recording: bool,
) -> ToastSpec {
    let head = if after_recording {
        tr!("Recording saved \u{2014} exporting\u{2026}")
    } else {
        tr!("Exporting\u{2026}")
    };
    let spec = ToastSpec::new(ToastKind::Info, head).button(
        tr_ctx!("button", "Open"),
        ToastAction::OpenCapture(capture_id.to_owned()),
    );
    let body = match title {
        Some(t) => tr!(
            "{title} \u{00B7} Making one MP4 with the camera bubble and audio: {pct}%",
            title = t,
            pct = pct
        ),
        None => tr!(
            "Making one MP4 with the camera bubble and audio: {pct}%",
            pct = pct
        ),
    };
    spec.body(body)
}

/// "Exporting audio..." with progress (VOC-2).
pub fn audio_exporting_toast(capture_id: &str, title: Option<&str>, pct: u32) -> ToastSpec {
    let body = match title {
        Some(t) => tr!(
            "{title} \u{00B7} Exporting the audio: {pct}%",
            title = t,
            pct = pct
        ),
        None => tr!("Exporting the audio: {pct}%", pct = pct),
    };
    ToastSpec::new(ToastKind::Info, tr!("Exporting audio\u{2026}"))
        .body(body)
        .button(
            tr_ctx!("button", "Open"),
            ToastAction::OpenCapture(capture_id.to_owned()),
        )
}

/// "Audio exported": the file name, its size and the loudness, with Show in Finder and Upload
/// (redesign V7). A live toast: it is worded again when the language changes.
pub fn audio_exported_toast(capture_id: &str, done: &AudioDone) -> ToastSpec {
    let (capture_id, done) = (capture_id.to_owned(), done.clone());
    ToastSpec::live(move || audio_exported_toast_now(&capture_id, &done))
}

fn audio_exported_toast_now(capture_id: &str, done: &AudioDone) -> ToastSpec {
    let name = crate::recedit::audio_export::file_name(&done.path);
    let size = crate::recedit::audio_export::size_text(done.size_bytes);
    let body = match done.loudness_lufs {
        Some(l) => tr!(
            "{name} \u{00B7} {size} \u{00B7} {lufs} LUFS",
            name = name,
            size = size,
            lufs = format!("\u{2212}{}", cm_i18n::fmt::decimal(l.abs(), 0))
        ),
        None => tr!("{name} \u{00B7} {size}", name = name, size = size),
    };
    ToastSpec::new(ToastKind::Success, tr!("Audio exported"))
        .body(body)
        .button(
            tr_ctx!("button", "Show in Finder"),
            ToastAction::RevealPath(done.path.clone()),
        )
        .button(
            tr_ctx!("button", "Upload"),
            ToastAction::UploadFile(capture_id.to_owned(), done.path.clone()),
        )
}

/// "Audio export failed" (VOC-2), with the reason.
pub fn audio_export_failed_toast(reason: &str, ffmpeg_missing: bool) -> ToastSpec {
    let reason = reason.to_owned();
    ToastSpec::live(move || {
        let why = if ffmpeg_missing {
            tr!("FFmpeg is needed to export audio and was not found.")
        } else {
            crate::daemon_text::any_error_text(&reason)
        };
        ToastSpec::new(
            ToastKind::Error,
            tr!("The audio couldn\u{2019}t be exported"),
        )
        .body(why)
    })
}

/// [`exported_toast`] as a live toast: it is worded again when the language changes.
pub fn exported_toast(capture_id: &str, title: &str, captions: Option<&str>) -> ToastSpec {
    let (capture_id, title, captions) = (
        capture_id.to_owned(),
        title.to_owned(),
        captions.map(str::to_owned),
    );
    ToastSpec::live(move || exported_toast_now(&capture_id, &title, captions.as_deref()))
}

/// "Exported <title>" with Play and Reveal.
fn exported_toast_now(capture_id: &str, title: &str, captions: Option<&str>) -> ToastSpec {
    let body = match captions_note(captions) {
        Some(note) => tr!(
            "One MP4 with the screen, camera bubble and audio. The original tracks are untouched. {captions}",
            captions = note
        ),
        None => tr!(
            "One MP4 with the screen, camera bubble and audio. The original tracks are untouched."
        ),
    };
    ToastSpec::new(ToastKind::Success, tr!("Exported {title}", title = title))
        .body(body)
        .button(
            tr_ctx!("button", "Play"),
            ToastAction::PlayExport(capture_id.to_owned()),
        )
        .button(
            tr_ctx!("button", "Reveal"),
            ToastAction::RevealExport(capture_id.to_owned()),
        )
}

/// What the export did with the captions, for the result toast (`None`: nothing to say).
pub fn captions_note(captions: Option<&str>) -> Option<String> {
    match captions? {
        "burned" => Some(tr!("Captions are burned into the picture.")),
        "skipped_no_transcript" => Some(tr!(
            "Captions were left out: this recording has no transcript yet."
        )),
        _ => None,
    }
}

/// [`export_skipped_toast`] as a live toast: it is worded again when the language changes.
pub fn export_skipped_toast(capture_id: &str) -> ToastSpec {
    let capture_id = capture_id.to_owned();
    ToastSpec::live(move || export_skipped_toast_now(&capture_id))
}

/// The one-time hint when the automatic export is skipped because FFmpeg is not installed. The
/// recording is saved; this is information, not an error.
fn export_skipped_toast_now(capture_id: &str) -> ToastSpec {
    ToastSpec::new(ToastKind::Info, tr!("Saved without a single MP4"))
        .body(tr!(
            "FFmpeg isn\u{2019}t installed, so the combined video was not made. Install it with Homebrew (brew install ffmpeg), then press Export on the recording."
        ))
        .button(
            tr_ctx!("button", "Open"),
            ToastAction::OpenCapture(capture_id.to_owned()),
        )
}

/// [`export_failed_toast`] as a live toast: it is worded again when the language changes.
pub fn export_failed_toast(capture_id: &str, reason: &str, ffmpeg_missing: bool) -> ToastSpec {
    let (capture_id, reason) = (capture_id.to_owned(), reason.to_owned());
    ToastSpec::live(move || export_failed_toast_now(&capture_id, &reason, ffmpeg_missing))
}

/// A failed export. A missing FFmpeg gets its own wording and the install hint; the recording is
/// saved either way.
fn export_failed_toast_now(capture_id: &str, reason: &str, ffmpeg_missing: bool) -> ToastSpec {
    if ffmpeg_missing && ffmpeg_bundle_problem(reason) {
        return ToastSpec::new(ToastKind::Error, tr!("FFmpeg not found"))
            .body(tr!(
                "The recording is saved, but the single MP4 needs FFmpeg, and the copy inside CaptureMann is missing or cannot run. Reinstall CaptureMann, or set CM_FFMPEG to an ffmpeg executable, then press Export on the recording."
            ))
            .button(
                tr_ctx!("button", "Open"),
                ToastAction::OpenCapture(capture_id.to_owned()),
            );
    }
    if ffmpeg_missing {
        return ToastSpec::new(ToastKind::Error, tr!("FFmpeg not found"))
            .body(tr!(
                "The recording is saved, but the single MP4 needs FFmpeg. Install it with Homebrew (brew install ffmpeg), then press Export on the recording."
            ))
            .button(
                tr_ctx!("button", "Open"),
                ToastAction::OpenCapture(capture_id.to_owned()),
            );
    }
    ToastSpec::new(
        ToastKind::Error,
        tr!("The video couldn\u{2019}t be exported"),
    )
    .body(tr!(
        "{reason} The recording itself is saved.",
        reason = crate::daemon_text::error_text(Some(cm_core::ErrorCode::NotFound), reason)
    ))
    .button(
        tr_ctx!("button", "Open"),
        ToastAction::OpenCapture(capture_id.to_owned()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn queued(job: &str, auto: bool) -> Value {
        json!({"job_id": job, "capture_id": "rec_1", "title": "Lesson", "preset": "quick", "auto": auto})
    }

    /// `export.skipped` (no FFmpeg) is a hint for the user, never a failure, and an unknown reason
    /// is ignored by the UI.
    #[test]
    fn a_skipped_export_is_a_hint() {
        let mut t = ExportTracker::default();
        let e = t.on_event(
            "export.skipped",
            &json!({"capture_id": "rec_1", "reason": "ffmpeg_not_found"}),
        );
        assert_eq!(
            e,
            vec![Effect::Skipped {
                capture_id: "rec_1".into(),
                ffmpeg: true
            }]
        );
        let other = t.on_event(
            "export.skipped",
            &json!({"capture_id": "rec_1", "reason": "off"}),
        );
        assert_eq!(
            other,
            vec![Effect::Skipped {
                capture_id: "rec_1".into(),
                ffmpeg: false
            }]
        );
        let toast = export_skipped_toast("rec_1");
        assert_eq!(toast.kind, ToastKind::Info);
        assert!(toast.body.unwrap().contains("brew install ffmpeg"));
    }

    /// REC-1: queued -> progress in 5% steps -> done; the toasts follow.
    #[test]
    fn progress_is_reported_in_steps_then_done() {
        let mut t = ExportTracker::default();
        let e = t.on_event("export.queued", &queued("j1", true));
        assert_eq!(
            e,
            vec![Effect::Progress {
                capture_id: "rec_1".into(),
                title: Some("Lesson".into()),
                pct: 0,
                auto: true,
                first: true
            }]
        );
        assert!(t.is_running("rec_1"));
        let mut shown = Vec::new();
        for p in [0.05, 0.19, 0.2, 0.31, 0.45, 0.61, 0.99] {
            for eff in t.on_event(
                "job.progress",
                &json!({"job_id": "j1", "type": "export", "progress": p}),
            ) {
                if let Effect::Progress { pct, first, .. } = eff {
                    assert!(!first);
                    shown.push(pct);
                }
            }
        }
        assert_eq!(shown, [5, 15, 20, 30, 45, 60, 95]);
        assert_eq!(t.status_of("rec_1").unwrap().progress, 0.99);
        // Other job types and unknown jobs are ignored.
        assert!(t
            .on_event(
                "job.progress",
                &json!({"job_id": "j1", "type": "upload", "progress": 0.9})
            )
            .is_empty());
        assert!(t
            .on_event(
                "job.progress",
                &json!({"job_id": "zz", "type": "export", "progress": 0.9})
            )
            .is_empty());
        let d = t.on_event(
            "export.done",
            &json!({"job_id": "j1", "capture_id": "rec_1", "title": "Lesson"}),
        );
        assert_eq!(
            d,
            vec![Effect::Done {
                capture_id: "rec_1".into(),
                title: "Lesson".into(),
                captions: None,
            }]
        );
        assert!(!t.is_running("rec_1"));
        assert_eq!(t.status_of("rec_1").unwrap().phase, Phase::Done);
    }

    /// A missing FFmpeg is recognised from the failure reason and gets the install hint.
    #[test]
    fn failures_carry_the_reason_and_detect_a_missing_ffmpeg() {
        let mut t = ExportTracker::default();
        t.on_event("export.queued", &queued("j1", true));
        let e = t.on_event(
            "job.failed",
            &json!({"job_id": "j1", "type": "export", "state": "failed",
                    "error": {"code": "job_failed", "message": "Job j1 failed",
                              "data": {"job_id": "j1", "reason": "FFmpeg was not found: install it with Homebrew (brew install ffmpeg) or set CM_FFMPEG"}}}),
        );
        let Effect::Failed {
            ffmpeg_missing,
            reason,
            ..
        } = &e[0]
        else {
            panic!()
        };
        assert!(ffmpeg_missing);
        assert!(reason.contains("brew install ffmpeg"));
        let toast = export_failed_toast("rec_1", reason, *ffmpeg_missing);
        assert_eq!(toast.title, "FFmpeg not found");
        assert!(toast.body.unwrap().contains("brew install ffmpeg"));
        assert_eq!(toast.kind, ToastKind::Error);

        t.on_event("export.queued", &queued("j2", false));
        let e = t.on_event(
            "job.failed",
            &json!({"job_id": "j2", "type": "export",
                    "error": {"data": {"reason": "FFmpeg failed (exit status: 1): boom"}}}),
        );
        let Effect::Failed {
            ffmpeg_missing,
            reason,
            ..
        } = &e[0]
        else {
            panic!()
        };
        assert!(!ffmpeg_missing);
        let toast = export_failed_toast("rec_1", reason, false);
        assert_eq!(toast.title, "The video couldn\u{2019}t be exported");
        assert!(toast.body.unwrap().contains("boom"));
        assert_eq!(
            t.status_of("rec_1").unwrap().reason.as_deref(),
            Some("FFmpeg failed (exit status: 1): boom")
        );
    }

    /// The typed code: a `job.failed` with `error.data.reason_code` marks a missing FFmpeg even
    /// when the reason text is not recognised, and a damaged app's reason gets the reinstall
    /// wording (English and Vietnamese); the details row follows the language too.
    #[test]
    fn a_missing_ffmpeg_is_known_by_its_code_and_worded_by_its_id() {
        let mut t = ExportTracker::default();
        t.on_event("export.queued", &queued("j1", true));
        let e = t.on_event(
            "job.failed",
            &json!({"job_id": "j1", "type": "export", "state": "failed",
                    "error": {"code": "job_failed", "message": "Job j1 failed",
                              "data": {"job_id": "j1", "reason_code": "ffmpeg_not_found",
                                       "reason": "Something a newer daemon words differently"}}}),
        );
        let Effect::Failed { ffmpeg_missing, .. } = &e[0] else {
            panic!()
        };
        assert!(ffmpeg_missing);

        let reason = cm_media::tools::BUNDLE_MISSING_MESSAGE;
        assert!(is_ffmpeg_missing(reason));
        let toast = export_failed_toast("rec_1", reason, true);
        assert!(toast
            .body
            .as_ref()
            .unwrap()
            .contains("Reinstall CaptureMann"));
        assert!(!toast.body.unwrap().contains("brew"));
        cm_i18n::with_language(cm_i18n::Lang::Vi, || {
            let toast = export_failed_toast("rec_1", reason, true);
            let body = toast.body.unwrap();
            assert!(body.contains("cài lại CaptureMann"), "{body}");
        });
        // Not the damaged-app case: the developer hint stays.
        let toast = export_failed_toast("rec_1", cm_media::tools::NOT_FOUND_MESSAGE, true);
        assert!(toast.body.unwrap().contains("brew install ffmpeg"));
        // A generic failure shows the reason, worded by id when it has one.
        let toast = export_failed_toast("rec_1", "FFmpeg failed (exit status: 1): boom", false);
        assert!(toast.body.unwrap().contains("boom"));
    }

    #[test]
    fn toasts_say_what_is_happening() {
        let saving = exporting_toast("rec_1", Some("Lesson"), 40, true);
        assert_eq!(saving.title, "Recording saved \u{2014} exporting\u{2026}");
        assert!(saving.body.as_deref().unwrap().contains("40%"));
        assert_eq!(
            saving.buttons[0].action,
            ToastAction::OpenCapture("rec_1".into())
        );
        assert_eq!(
            exporting_toast("rec_1", None, 0, false).title,
            "Exporting\u{2026}"
        );
        let done = exported_toast("rec_1", "Lesson", None);
        assert_eq!(done.title, "Exported Lesson");
        assert_eq!(done.kind, ToastKind::Success);
        assert_eq!(done.buttons[0].label, "Play");
        assert_eq!(
            done.buttons[0].action,
            ToastAction::PlayExport("rec_1".into())
        );
        assert_eq!(
            done.buttons[1].action,
            ToastAction::RevealExport("rec_1".into())
        );
    }

    /// The export result says whether the captions were burned in or left out; "off" says nothing.
    #[test]
    fn the_result_toast_says_what_happened_to_the_captions() {
        let body = |c: Option<&str>| {
            exported_toast("rec_1", "Lesson", c)
                .body
                .unwrap_or_default()
        };
        assert!(body(Some("burned")).contains("burned into the picture"));
        assert!(body(Some("skipped_no_transcript")).contains("no transcript"));
        assert!(!body(Some("off")).contains("aption"));
        assert!(!body(None).contains("aption"));
        let mut t = ExportTracker::default();
        t.on_event("export.queued", &queued("j1", false));
        let d = t.on_event(
            "export.done",
            &json!({"job_id": "j1", "capture_id": "rec_1", "title": "Lesson", "captions": "burned"}),
        );
        assert_eq!(
            d,
            vec![Effect::Done {
                capture_id: "rec_1".into(),
                title: "Lesson".into(),
                captions: Some("burned".into()),
            }]
        );
    }

    #[test]
    fn the_setting_defaults_to_on() {
        let mut s = serde_json::Map::new();
        assert!(auto_export_on(&s));
        s.insert(KEY_AUTO_EXPORT.into(), json!(false));
        assert!(!auto_export_on(&s));
        s.insert(KEY_AUTO_EXPORT.into(), json!("yes"));
        assert!(auto_export_on(&s), "not a bool: default");
    }

    #[test]
    fn running_job_wins_over_a_finished_one() {
        let mut t = ExportTracker::default();
        t.on_event("export.queued", &queued("j1", true));
        t.on_event(
            "export.done",
            &json!({"job_id": "j1", "capture_id": "rec_1", "title": "L"}),
        );
        t.on_event("export.queued", &queued("j2", false));
        assert_eq!(t.status_of("rec_1").unwrap().job_id, "j2");
        t.prune(0);
        assert_eq!(
            t.status_of("rec_1").unwrap().job_id,
            "j2",
            "running jobs are never pruned"
        );
        assert_eq!(t.jobs.len(), 1);
    }

    fn audio_queued(job: &str) -> Value {
        json!({"job_id": job, "capture_id": "rec_1", "title": "Weekly sync", "kind": "audio",
               "preset": "audio-mix", "auto": false})
    }

    /// VOC-2: an audio export (`kind: "audio"`) has its own progress, result and failure
    /// effects, so it never shows the MP4 wording or the take's "Recording saved" toast.
    #[test]
    fn an_audio_export_has_its_own_effects() {
        let mut t = ExportTracker::default();
        let e = t.on_event("export.queued", &audio_queued("a1"));
        assert_eq!(
            e,
            vec![Effect::AudioProgress {
                capture_id: "rec_1".into(),
                title: Some("Weekly sync".into()),
                pct: 0,
                first: true,
            }]
        );
        assert!(t.status_of("rec_1").unwrap().audio);
        let e = t.on_event(
            "job.progress",
            &json!({"job_id": "a1", "type": "export", "progress": 0.5}),
        );
        assert!(matches!(
            e.as_slice(),
            [Effect::AudioProgress {
                pct: 50,
                first: false,
                ..
            }]
        ));
        let e = t.on_event(
            "export.done",
            &json!({"job_id": "a1", "capture_id": "rec_1", "title": "Weekly sync",
                    "kind": "audio", "path": "/lib/rec_1/exports/Weekly sync.m4a",
                    "size_bytes": 44_000_000, "loudness_lufs": -16.0,
                    "copy_path": "/Users/a/Meetings/Weekly sync.m4a"}),
        );
        let [Effect::AudioDone { done, .. }] = e.as_slice() else {
            panic!("{e:?}")
        };
        assert_eq!(done.size_bytes, 44_000_000);
        assert_eq!(done.loudness_lufs, Some(-16.0));
        assert_eq!(
            done.copy_path.as_deref(),
            Some("/Users/a/Meetings/Weekly sync.m4a")
        );
        assert_eq!(t.status_of("rec_1").unwrap().phase, Phase::Done);
    }

    /// V7: the done toast names the file, its size and the loudness, and offers Show in Finder
    /// (the file) and Upload (the file, not the capture).
    #[test]
    fn the_audio_done_toast_offers_finder_and_upload() {
        let done = AudioDone {
            path: "/lib/rec_1/exports/Weekly sync.m4a".into(),
            size_bytes: 44_000_000,
            loudness_lufs: Some(-16.0),
            copy_path: None,
        };
        let toast = audio_exported_toast_now("rec_1", &done);
        assert_eq!(toast.kind, ToastKind::Success);
        assert_eq!(toast.title, "Audio exported");
        assert_eq!(
            toast.body.as_deref(),
            Some("Weekly sync.m4a \u{00B7} 44 MB \u{00B7} \u{2212}16 LUFS")
        );
        let actions: Vec<_> = toast.buttons.iter().map(|b| b.action.clone()).collect();
        assert_eq!(
            actions,
            vec![
                ToastAction::RevealPath(done.path.clone()),
                ToastAction::UploadFile("rec_1".into(), done.path.clone()),
            ]
        );
        // Without a loudness target the line has no LUFS.
        let plain = audio_exported_toast_now(
            "rec_1",
            &AudioDone {
                loudness_lufs: None,
                ..done
            },
        );
        assert_eq!(
            plain.body.as_deref(),
            Some("Weekly sync.m4a \u{00B7} 44 MB")
        );
    }

    #[test]
    fn a_failed_audio_export_says_what_is_missing() {
        let mut t = ExportTracker::default();
        t.on_event("export.queued", &audio_queued("a2"));
        let e = t.on_event(
            "job.failed",
            &json!({"job_id": "a2", "type": "export",
                    "error": {"message": "x", "data": {"reason_code": "ffmpeg_not_found"}}}),
        );
        assert!(matches!(
            e.as_slice(),
            [Effect::AudioFailed {
                ffmpeg_missing: true,
                ..
            }]
        ));
    }
}
