//! Transcription runs in flight, display-free: which recordings are being transcribed, their
//! progress and the last failure, and a counter the caption editor watches to reload when a new
//! transcript arrives (`transcript.ready`).

use std::collections::HashMap;

use cm_i18n::tr;

use super::summary::{RegenDraft, TranscriptCache};
use crate::daemon_text::ErrorNote;

#[derive(Debug, Clone, Default, PartialEq)]
struct Run {
    job_id: Option<String>,
    /// 0.0 to 1.0 when the job reports it.
    progress: Option<f32>,
    /// The job's own progress message (`job.progress` `message`, `list_jobs` `message`), for
    /// example "Transcribing 40%" or "Checking the transcript 80%".
    message: Option<String>,
}

/// What a running transcription looks like on a page or a card: the stage in words (no
/// percentage in it), the fraction when the job reports one, and the job to cancel.
#[derive(Debug, Clone, PartialEq)]
pub struct RunView {
    pub stage: String,
    pub fraction: Option<f32>,
    pub job_id: Option<String>,
}

/// The stage named by a transcription job's progress message, worded in the current language:
/// the daemon sends "Transcribing 40%" or "Checking the transcript 80%"; the percentage is shown
/// by its own part of the strip. No message (not started yet, or a job from an older daemon)
/// reads "Transcribing".
pub fn stage_label(message: Option<&str>) -> String {
    let text = message.map(str::trim).unwrap_or_default();
    let base = match text.rsplit_once(' ') {
        Some((head, tail))
            if tail.ends_with('%') && tail[..tail.len() - 1].parse::<f64>().is_ok() =>
        {
            head
        }
        _ => text,
    };
    match base {
        "" | "Transcribing" => tr!("Transcribing"),
        "Checking the transcript" => tr!("Checking the transcript"),
        other => other.to_owned(),
    }
}

/// The background jobs of the Speakers and Report tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceJobKind {
    /// `diarize`.
    Speakers,
    /// `generate_report`.
    Report,
}

/// The kinds of AI run the editor's tabs show, from the daemon's job list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveKind {
    Transcribe,
    Speakers,
    Report,
}

/// A speaker or report job this window started.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceJob {
    pub capture_id: String,
    pub kind: VoiceJobKind,
    /// 0.0 to 1.0 when the job reports it.
    pub progress: Option<f32>,
}

/// How a speaker or report job ended: `Err` holds the daemon's reason or message.
pub type VoiceEnd = (VoiceJobKind, Result<(), String>);

#[derive(Debug, Clone, Default)]
pub struct TranscribeVm {
    /// Speaker and report jobs in flight, by job id.
    voice_jobs: HashMap<String, VoiceJob>,
    /// Finished ones the editor has not read yet, by recording.
    voice_ended: Vec<(String, VoiceEnd)>,
    running: HashMap<String, Run>,
    errors: HashMap<String, ErrorNote>,
    /// Bumped whenever a transcript was written (or a run ended): readers reload.
    pub epoch: u64,
    /// What the details page knows about each recording's transcript.
    pub transcripts: TranscriptCache,
    /// Bumped when a transcript's summary may have changed (a transcript was written, any
    /// revision such as edited captions or an undo): the details page reads it again.
    pub info_epoch: u64,
    /// Open "Re-generate" panels (model and language for this run), by recording.
    pub regen: HashMap<String, RegenDraft>,
    /// The Audio choice a Re-generate panel opens on: the stored `ai.transcribe_raw_mic`, or
    /// `None` for the daemon's default (the original microphone). Kept in step by the AI page.
    pub regen_original_audio: Option<bool>,
    /// Put `[mm:ss]` before every line of a TXT export.
    pub txt_timestamps: bool,
    /// Bumped when a translation was written (`translation.ready`): the audio editor reads the
    /// languages of the recording again.
    pub voice_epoch: u64,
    /// "Export audio only" from another screen: the recording whose editor opens the Export audio
    /// sheet (VOC-2).
    pub export_audio_focus: Option<String>,
    /// "Open captions": the recording whose editor should scroll to the Captions card.
    pub captions_focus: Option<String>,
    /// "Set up report" from the toast after a transcript: the recording whose editor opens on the
    /// Report tab's setup (V1, AI-9).
    pub report_focus: Option<String>,
    /// The "Burn in captions" choice made for the next export of a recording, until it is
    /// exported or the recording's own setting is changed.
    export_burn: HashMap<String, bool>,
}

impl TranscribeVm {
    /// A fresh view model that knows the stored `ai.transcribe_raw_mic` (`None`: nothing stored).
    pub fn with_regen_original_audio(regen_original_audio: Option<bool>) -> Self {
        TranscribeVm {
            regen_original_audio,
            ..Default::default()
        }
    }

    /// Whether the next export of `capture_id` burns the captions in: what the user ticked for it,
    /// else the recording's own `burn_in` setting.
    pub fn export_burn(&self, capture_id: &str, recording_default: bool) -> bool {
        self.export_burn
            .get(capture_id)
            .copied()
            .unwrap_or(recording_default)
    }

    pub fn set_export_burn(&mut self, capture_id: &str, on: bool) {
        self.export_burn.insert(capture_id.to_owned(), on);
    }

    /// The recording's own setting changed: it is the default again.
    pub fn clear_export_burn(&mut self, capture_id: &str) {
        self.export_burn.remove(capture_id);
    }

    /// A `diarize` or `generate_report` call is on its way: the tab shows it running at once.
    /// `false` when one of that kind is already running for the recording.
    pub fn voice_job_begin(&mut self, capture_id: &str, kind: VoiceJobKind) -> bool {
        if self.voice_job(capture_id, kind).is_some() {
            return false;
        }
        self.voice_jobs.insert(
            format!("pending:{capture_id}:{kind:?}"),
            VoiceJob {
                capture_id: capture_id.to_owned(),
                kind,
                progress: None,
            },
        );
        true
    }

    /// The call was accepted: the job has an id now (when it already ended, nothing is left to
    /// rename).
    pub fn voice_job_adopt(&mut self, capture_id: &str, kind: VoiceJobKind, job_id: &str) {
        if let Some(j) = self
            .voice_jobs
            .remove(&format!("pending:{capture_id}:{kind:?}"))
        {
            self.voice_jobs.insert(job_id.to_owned(), j);
        }
    }

    /// The call failed: it is not running.
    pub fn voice_job_abort(&mut self, capture_id: &str, kind: VoiceJobKind) {
        self.voice_jobs
            .retain(|_, j| !(j.capture_id == capture_id && j.kind == kind));
    }

    /// `speakers.ready` or `report.ready`: that job is over, whoever started it.
    pub fn voice_ready(&mut self, capture_id: &str, kind: VoiceJobKind) {
        self.voice_job_abort(capture_id, kind);
        self.voice_epoch += 1;
    }

    /// The running job of `kind` for a recording, if any.
    pub fn voice_job(&self, capture_id: &str, kind: VoiceJobKind) -> Option<&VoiceJob> {
        self.voice_jobs
            .values()
            .find(|j| j.capture_id == capture_id && j.kind == kind)
    }

    /// The job id of the running job of `kind` for a recording.
    pub fn voice_job_id(&self, capture_id: &str, kind: VoiceJobKind) -> Option<String> {
        self.voice_jobs
            .iter()
            .find(|(_, j)| j.capture_id == capture_id && j.kind == kind)
            .map(|(id, _)| id.clone())
    }

    /// `job.progress` for a speaker or report job.
    pub fn voice_progress_by_job(&mut self, job_id: &str, progress: f64) -> bool {
        let p = if progress > 1.0 {
            progress / 100.0
        } else {
            progress
        };
        match self.voice_jobs.get_mut(job_id) {
            Some(j) => {
                j.progress = Some(p.clamp(0.0, 1.0) as f32);
                true
            }
            None => false,
        }
    }

    /// `job.done` or `job.failed` for a speaker or report job: it is no longer running, and the
    /// editor reads the end (and the bundle again) on its next frame.
    pub fn voice_ended_by_job(&mut self, job_id: &str, result: Result<(), String>) -> bool {
        let Some(j) = self.voice_jobs.remove(job_id) else {
            return false;
        };
        self.voice_ended.push((j.capture_id, (j.kind, result)));
        self.voice_epoch += 1;
        true
    }

    /// A job the daemon has in its queue that this window did not start (picked up again after a
    /// restart, or started by the CLI, an agent or the automatic steps): it shows like a click.
    /// `progress` is 0.0 to 1.0.
    pub fn adopt_active(
        &mut self,
        kind: ActiveKind,
        capture_id: &str,
        job_id: &str,
        progress: f32,
        message: Option<&str>,
    ) {
        match kind {
            ActiveKind::Transcribe => {
                self.start(capture_id);
                self.started(capture_id, job_id.to_owned());
                if progress > 0.0 || message.is_some() {
                    self.progress_by_job_message(job_id, f64::from(progress), message);
                }
            }
            ActiveKind::Speakers | ActiveKind::Report => {
                let kind = if kind == ActiveKind::Speakers {
                    VoiceJobKind::Speakers
                } else {
                    VoiceJobKind::Report
                };
                self.voice_job_begin(capture_id, kind);
                self.voice_job_adopt(capture_id, kind, job_id);
                if progress > 0.0 {
                    self.voice_progress_by_job(job_id, f64::from(progress));
                }
            }
        }
    }

    /// The ends of this recording's jobs, once.
    pub fn take_voice_ended(&mut self, capture_id: &str) -> Vec<VoiceEnd> {
        let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.voice_ended)
            .into_iter()
            .partition(|(id, _)| id == capture_id);
        self.voice_ended = rest;
        mine.into_iter().map(|(_, e)| e).collect()
    }

    pub fn is_running(&self, capture_id: &str) -> bool {
        self.running.contains_key(capture_id)
    }

    /// Transcribe pressed. `false` when it is already running.
    pub fn start(&mut self, capture_id: &str) -> bool {
        if self.is_running(capture_id) {
            return false;
        }
        self.errors.remove(capture_id);
        self.running.insert(capture_id.to_owned(), Run::default());
        true
    }

    pub fn started(&mut self, capture_id: &str, job_id: String) {
        if let Some(r) = self.running.get_mut(capture_id) {
            r.job_id = Some(job_id);
        }
    }

    /// `job.progress`: `progress` is 0 to 1 (a value above 1 is read as a percentage).
    pub fn progress_by_job(&mut self, job_id: &str, progress: f64) -> bool {
        self.progress_by_job_message(job_id, progress, None)
    }

    /// `job.progress` with the job's message (the stage). A `None` message keeps the last one.
    pub fn progress_by_job_message(
        &mut self,
        job_id: &str,
        progress: f64,
        message: Option<&str>,
    ) -> bool {
        let p = if progress > 1.0 {
            progress / 100.0
        } else {
            progress
        };
        for r in self.running.values_mut() {
            if r.job_id.as_deref() == Some(job_id) {
                r.progress = Some(p.clamp(0.0, 1.0) as f32);
                if let Some(m) = message {
                    r.message = Some(m.to_owned());
                }
                return true;
            }
        }
        false
    }

    /// `transcript.ready`.
    pub fn ready(&mut self, capture_id: &str) {
        self.running.remove(capture_id);
        self.errors.remove(capture_id);
        self.regen.remove(capture_id);
        self.epoch += 1;
        self.info_epoch += 1;
    }

    /// The summary of some transcript may have changed (`capture.revised`).
    pub fn bump_info(&mut self) {
        self.info_epoch += 1;
    }

    /// Opens the Re-generate panel (the defaults: the active model, auto-detect, the audio the
    /// setting `ai.transcribe_raw_mic` names, the original microphone when nothing is stored).
    pub fn open_regen(&mut self, capture_id: &str) {
        let original_audio = self
            .regen_original_audio
            .unwrap_or(RegenDraft::default().original_audio);
        self.regen
            .entry(capture_id.to_owned())
            .or_insert_with(|| RegenDraft {
                original_audio,
                ..Default::default()
            });
    }

    pub fn close_regen(&mut self, capture_id: &str) {
        self.regen.remove(capture_id);
    }

    pub fn regen_of(&self, capture_id: &str) -> Option<&RegenDraft> {
        self.regen.get(capture_id)
    }

    /// The draft as the user changes it; `None` when the panel is closed.
    pub fn regen_mut(&mut self, capture_id: &str) -> Option<&mut RegenDraft> {
        self.regen.get_mut(capture_id)
    }

    /// `job.done` for a transcription job: the capture it belonged to.
    pub fn done_by_job(&mut self, job_id: &str) -> Option<String> {
        let id = self.capture_of(job_id)?;
        self.ready(&id);
        Some(id)
    }

    pub fn failed(&mut self, capture_id: &str, message: impl Into<ErrorNote>) {
        self.running.remove(capture_id);
        self.errors.insert(capture_id.to_owned(), message.into());
        self.epoch += 1;
    }

    /// `job.failed` for a transcription job: the capture it belonged to.
    pub fn failed_by_job(&mut self, job_id: &str, message: &str) -> Option<String> {
        let id = self.capture_of(job_id)?;
        self.failed(&id, message);
        Some(id)
    }

    fn capture_of(&self, job_id: &str) -> Option<String> {
        self.running
            .iter()
            .find(|(_, r)| r.job_id.as_deref() == Some(job_id))
            .map(|(id, _)| id.clone())
    }

    /// The failure of the last run, worded in the current language.
    pub fn error_of(&self, capture_id: &str) -> Option<String> {
        self.errors.get(capture_id).map(ErrorNote::text)
    }

    pub fn clear_error(&mut self, capture_id: &str) {
        self.errors.remove(capture_id);
    }

    /// `Transcribing\u{2026}` or `Transcribing\u{2026} 40%`; `None` when not running.
    pub fn status_text(&self, capture_id: &str) -> Option<String> {
        let r = self.running.get(capture_id)?;
        Some(match r.progress {
            Some(p) => tr!(
                "Transcribing\u{2026} {percent}%",
                percent = format!("{:.0}", p * 100.0)
            ),
            None => tr!("Transcribing\u{2026}"),
        })
    }

    /// What to draw for a running transcription (the same strip as an import); `None` when it is
    /// not running.
    pub fn view(&self, capture_id: &str) -> Option<RunView> {
        let r = self.running.get(capture_id)?;
        Some(RunView {
            stage: stage_label(r.message.as_deref()),
            fraction: r.progress,
            job_id: r.job_id.clone(),
        })
    }

    /// The job ids of the transcriptions shown as running (those that have one).
    pub fn running_job_ids(&self) -> Vec<String> {
        self.running
            .values()
            .filter_map(|r| r.job_id.clone())
            .collect()
    }

    /// The daemon's job list no longer has these jobs as active (`known` is what was shown when
    /// the list was asked for): they were cancelled, or ended while the window was not told (a
    /// cancel sends no bus event; the CLI or an agent may have cancelled it). They stop showing as
    /// running, without an error. Returns whether anything changed.
    pub fn drop_ended(&mut self, known: &[String], active: &[String]) -> bool {
        let gone: Vec<String> = self
            .running
            .iter()
            .filter(|(_, r)| {
                r.job_id
                    .as_ref()
                    .is_some_and(|j| known.contains(j) && !active.contains(j))
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &gone {
            self.running.remove(id);
        }
        if !gone.is_empty() {
            self.epoch += 1;
        }
        !gone.is_empty()
    }

    /// The user cancelled the run: it is not running and it is not an error. (A cancelled job
    /// sends no bus event, so the window must say so itself.)
    pub fn cancelled(&mut self, capture_id: &str) {
        self.running.remove(capture_id);
        self.errors.remove(capture_id);
        self.epoch += 1;
    }

    pub fn fraction(&self, capture_id: &str) -> Option<f32> {
        self.running.get(capture_id).and_then(|r| r.progress)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Jobs the daemon already had (picked up after a restart) are shown like a click, with their
    /// progress, and asking again changes nothing.
    #[test]
    fn jobs_the_daemon_already_has_are_shown_as_running() {
        let mut v = TranscribeVm::default();
        v.adopt_active(ActiveKind::Transcribe, "rec_1", "job_1", 0.4, None);
        assert_eq!(
            v.status_text("rec_1").as_deref(),
            Some("Transcribing\u{2026} 40%")
        );
        v.adopt_active(ActiveKind::Speakers, "rec_1", "job_2", 0.7, None);
        v.adopt_active(ActiveKind::Report, "rec_2", "job_3", 0.0, None);
        assert_eq!(
            v.voice_job("rec_1", VoiceJobKind::Speakers)
                .unwrap()
                .progress,
            Some(0.7)
        );
        assert!(v.voice_job("rec_2", VoiceJobKind::Report).is_some());
        assert!(v.voice_job("rec_2", VoiceJobKind::Speakers).is_none());
        // Again (every editor that opens asks): nothing is duplicated and the progress carries on.
        v.adopt_active(ActiveKind::Transcribe, "rec_1", "job_1", 0.5, None);
        v.adopt_active(ActiveKind::Speakers, "rec_1", "job_2", 0.8, None);
        assert_eq!(v.fraction("rec_1"), Some(0.5));
        assert_eq!(
            v.voice_job_id("rec_1", VoiceJobKind::Speakers).as_deref(),
            Some("job_2")
        );
        // Its end clears it like any other.
        assert_eq!(v.done_by_job("job_1").as_deref(), Some("rec_1"));
        assert_eq!(v.status_text("rec_1"), None);
        assert!(v.voice_ended_by_job("job_2", Ok(())));
        assert!(v.voice_job("rec_1", VoiceJobKind::Speakers).is_none());
    }

    /// The automatic transcription after a recording (`transcript.started`) shows as running on
    /// the tabs, with its progress, and ends with the transcript like a run started by a click.
    /// Seen on Oct 28, 2026: an 88 minute recording looked idle for minutes.
    #[test]
    fn an_automatic_transcription_shows_as_running_until_the_transcript_is_ready() {
        let mut v = TranscribeVm::default();
        assert_eq!(v.status_text("rec_1"), None, "idle before the event");
        // What the `transcript.started` event does.
        v.start("rec_1");
        v.started("rec_1", "job_7".into());
        assert!(v.is_running("rec_1"));
        assert_eq!(
            v.status_text("rec_1").as_deref(),
            Some("Transcribing\u{2026}")
        );
        assert!(v.progress_by_job("job_7", 0.4));
        assert_eq!(
            v.status_text("rec_1").as_deref(),
            Some("Transcribing\u{2026} 40%")
        );
        // A second event for the same run (or a click on top of it) changes nothing.
        assert!(!v.start("rec_1"));
        v.started("rec_1", "job_7".into());
        assert_eq!(v.fraction("rec_1"), Some(0.4));
        // The transcript arrives: not running any more, readers reload.
        let epoch = v.epoch;
        assert_eq!(v.done_by_job("job_7").as_deref(), Some("rec_1"));
        assert_eq!(v.status_text("rec_1"), None);
        assert!(v.epoch > epoch);
    }

    /// AI-8 and AI-9: a speaker or report job is "running" from the click, takes its id when the
    /// daemon accepts it, and its end (or failure reason) is read once by the editor.
    #[test]
    fn a_voice_job_runs_from_the_click_and_ends_once() {
        let mut v = TranscribeVm::default();
        assert!(v.voice_job_begin("rec_1", VoiceJobKind::Report));
        assert!(
            !v.voice_job_begin("rec_1", VoiceJobKind::Report),
            "one at a time"
        );
        assert!(
            v.voice_job_begin("rec_1", VoiceJobKind::Speakers),
            "another kind runs"
        );
        assert!(v.voice_job("rec_2", VoiceJobKind::Report).is_none());
        v.voice_job_adopt("rec_1", VoiceJobKind::Report, "job_9");
        assert!(v.voice_progress_by_job("job_9", 40.0));
        assert_eq!(
            v.voice_job("rec_1", VoiceJobKind::Report).unwrap().progress,
            Some(0.4)
        );
        assert!(!v.voice_progress_by_job("job_x", 0.5));
        let before = v.voice_epoch;
        assert!(v.voice_ended_by_job("job_9", Err("no_text_model".into())));
        assert!(v.voice_epoch > before, "the editor reads the bundle again");
        assert!(v.voice_job("rec_1", VoiceJobKind::Report).is_none());
        assert!(v.take_voice_ended("rec_2").is_empty());
        assert_eq!(
            v.take_voice_ended("rec_1"),
            vec![(VoiceJobKind::Report, Err("no_text_model".to_owned()))]
        );
        assert!(v.take_voice_ended("rec_1").is_empty(), "read once");
        // `speakers.ready` ends a job whoever started it, and a failed call clears the click.
        v.voice_ready("rec_1", VoiceJobKind::Speakers);
        assert!(v.voice_job("rec_1", VoiceJobKind::Speakers).is_none());
        assert!(v.voice_job_begin("rec_1", VoiceJobKind::Speakers));
        v.voice_job_abort("rec_1", VoiceJobKind::Speakers);
        assert!(v.voice_job("rec_1", VoiceJobKind::Speakers).is_none());
    }

    #[test]
    fn a_run_starts_once_and_reports_progress_by_job() {
        let mut v = TranscribeVm::default();
        assert!(v.start("rec_1"));
        assert!(!v.start("rec_1"));
        assert_eq!(
            v.status_text("rec_1").as_deref(),
            Some("Transcribing\u{2026}")
        );
        v.started("rec_1", "job_3".into());
        assert!(v.progress_by_job("job_3", 0.4));
        assert!(!v.progress_by_job("other", 0.9));
        assert_eq!(
            v.status_text("rec_1").as_deref(),
            Some("Transcribing\u{2026} 40%")
        );
        // A percentage is read as one.
        v.progress_by_job("job_3", 55.0);
        assert!((v.fraction("rec_1").unwrap() - 0.55).abs() < 1e-6);
    }

    #[test]
    fn ready_ends_the_run_and_bumps_the_epoch() {
        let mut v = TranscribeVm::default();
        v.start("rec_1");
        let e = v.epoch;
        v.ready("rec_1");
        assert!(!v.is_running("rec_1"));
        assert_eq!(v.epoch, e + 1);
        assert_eq!(v.status_text("rec_1"), None);
    }

    #[test]
    fn job_events_find_the_capture() {
        let mut v = TranscribeVm::default();
        v.start("rec_1");
        v.started("rec_1", "job_3".into());
        assert_eq!(
            v.failed_by_job("job_3", "model file is damaged").as_deref(),
            Some("rec_1")
        );
        assert_eq!(
            v.error_of("rec_1").as_deref(),
            Some("model file is damaged")
        );
        assert!(!v.is_running("rec_1"));
        // A new run clears the old failure.
        assert!(v.start("rec_1"));
        assert_eq!(v.error_of("rec_1"), None);
        v.started("rec_1", "job_4".into());
        assert_eq!(v.done_by_job("job_4").as_deref(), Some("rec_1"));
        assert_eq!(v.done_by_job("job_4"), None);
    }

    /// Re-generate: the panel opens with the defaults, the choices are kept until it closes, and a
    /// finished transcription closes it and bumps what the details page watches.
    #[test]
    fn regenerate_draft_lives_until_the_transcript_is_ready() {
        let mut v = TranscribeVm::default();
        assert!(v.regen_of("rec_1").is_none());
        v.open_regen("rec_1");
        assert_eq!(v.regen_of("rec_1"), Some(&RegenDraft::default()));
        assert!(
            v.regen_of("rec_1").unwrap().original_audio,
            "nothing stored: the original microphone"
        );
        v.regen_mut("rec_1").unwrap().model = Some("medium".into());
        v.regen_mut("rec_1").unwrap().language = "vi".into();
        // Opening again keeps the choices.
        v.open_regen("rec_1");
        assert_eq!(v.regen_of("rec_1").unwrap().language, "vi");
        v.close_regen("rec_1");
        assert!(v.regen_of("rec_1").is_none());
        v.open_regen("rec_1");
        let info = v.info_epoch;
        v.ready("rec_1");
        assert!(
            v.regen_of("rec_1").is_none(),
            "a finished run closes the panel"
        );
        assert_eq!(v.info_epoch, info + 1);
        v.bump_info();
        assert_eq!(v.info_epoch, info + 2);
    }

    /// The Audio choice of a new Re-generate panel follows the stored `ai.transcribe_raw_mic`:
    /// a stored `false` opens on the noise-reduced copy, nothing stored on the original microphone.
    #[test]
    fn a_regenerate_panel_opens_on_the_stored_audio_choice() {
        let mut v = TranscribeVm::with_regen_original_audio(Some(false));
        v.open_regen("rec_1");
        assert!(!v.regen_of("rec_1").unwrap().original_audio);
        v.regen_original_audio = None;
        v.open_regen("rec_2");
        assert!(v.regen_of("rec_2").unwrap().original_audio);
        // An open panel keeps what the user picked in it.
        v.regen_mut("rec_1").unwrap().original_audio = true;
        v.open_regen("rec_1");
        assert!(v.regen_of("rec_1").unwrap().original_audio);
    }

    /// The export checkbox starts at the recording's own setting, remembers a tick, and goes back
    /// to the setting when that is changed.
    #[test]
    fn the_export_captions_choice_follows_the_recording_until_ticked() {
        let mut v = TranscribeVm::default();
        assert!(!v.export_burn("rec_1", false));
        assert!(v.export_burn("rec_1", true));
        v.set_export_burn("rec_1", true);
        assert!(v.export_burn("rec_1", false), "the tick wins");
        assert!(!v.export_burn("rec_2", false), "per recording");
        v.clear_export_burn("rec_1");
        assert!(!v.export_burn("rec_1", false));
    }

    #[test]
    fn clearing_an_error_keeps_the_rest() {
        let mut v = TranscribeVm::default();
        v.failed("rec_1", "x");
        v.clear_error("rec_1");
        assert_eq!(v.error_of("rec_1"), None);
    }

    /// TRN-PROGRESS: the stage comes from the job's message without its percentage, the page
    /// shows it with the fraction and the job to cancel.
    #[test]
    fn a_run_view_has_the_stage_the_fraction_and_the_job() {
        assert_eq!(stage_label(None), "Transcribing");
        assert_eq!(stage_label(Some("Transcribing 40%")), "Transcribing");
        assert_eq!(
            stage_label(Some("Checking the transcript 80%")),
            "Checking the transcript"
        );
        assert_eq!(stage_label(Some("Something else 5%")), "Something else");
        assert_eq!(
            stage_label(Some("Waiting for 3 files")),
            "Waiting for 3 files"
        );
        let mut v = TranscribeVm::default();
        assert_eq!(v.view("rec_1"), None);
        v.start("rec_1");
        assert_eq!(
            v.view("rec_1").unwrap().job_id,
            None,
            "no Cancel before an id"
        );
        v.started("rec_1", "job_1".into());
        assert!(v.progress_by_job_message("job_1", 0.8, Some("Checking the transcript 80%")));
        let view = v.view("rec_1").unwrap();
        assert_eq!(view.stage, "Checking the transcript");
        assert_eq!(view.fraction, Some(0.8));
        assert_eq!(view.job_id.as_deref(), Some("job_1"));
        // A progress event without a message keeps the stage.
        assert!(v.progress_by_job("job_1", 0.9));
        assert_eq!(v.view("rec_1").unwrap().stage, "Checking the transcript");
    }

    /// TRN-PROGRESS: a page opened while the daemon is already transcribing (the automatic run
    /// after the stop) reads the job's state, with its stage, from the job list.
    #[test]
    fn a_reopened_page_adopts_the_running_job_with_its_stage() {
        let mut v = TranscribeVm::default();
        v.adopt_active(
            ActiveKind::Transcribe,
            "rec_1",
            "job_1",
            0.4,
            Some("Transcribing 40%"),
        );
        let view = v.view("rec_1").unwrap();
        assert_eq!(
            (view.stage.as_str(), view.fraction),
            ("Transcribing", Some(0.4))
        );
        // Asking again (every page that opens asks) keeps the run and updates it.
        v.adopt_active(
            ActiveKind::Transcribe,
            "rec_1",
            "job_1",
            0.6,
            Some("Transcribing 60%"),
        );
        assert_eq!(v.view("rec_1").unwrap().fraction, Some(0.6));
    }

    /// TRN-PROGRESS: a failed run shows its error (no progress), Retry runs again and clears it,
    /// and a cancel ends the run without an error.
    #[test]
    fn an_error_is_kept_until_retry_and_cancel_is_not_an_error() {
        let mut v = TranscribeVm::default();
        v.start("rec_1");
        v.started("rec_1", "job_1".into());
        assert_eq!(
            v.failed_by_job("job_1", "no model").as_deref(),
            Some("rec_1")
        );
        assert_eq!(v.view("rec_1"), None);
        assert_eq!(v.error_of("rec_1").as_deref(), Some("no model"));
        // Retry.
        assert!(v.start("rec_1"));
        assert_eq!(v.error_of("rec_1"), None);
        v.started("rec_1", "job_2".into());
        // Cancel: a cancelled job sends no bus event, so the window ends the run itself.
        let epoch = v.epoch;
        v.cancelled("rec_1");
        assert_eq!(v.view("rec_1"), None);
        assert_eq!(v.error_of("rec_1"), None);
        assert!(v.epoch > epoch);
        assert!(v.start("rec_1"), "can start again after a cancel");
    }

    /// TRN-PROGRESS: a run the daemon no longer lists (cancelled from the CLI, no bus event) stops
    /// showing as running; one that started after the list was asked for is left alone.
    #[test]
    fn a_run_the_daemon_no_longer_lists_stops_showing() {
        let mut v = TranscribeVm::default();
        v.adopt_active(ActiveKind::Transcribe, "rec_1", "job_1", 0.2, None);
        v.adopt_active(ActiveKind::Transcribe, "rec_2", "job_2", 0.2, None);
        v.start("rec_3"); // the call is still on its way: no id yet
        let known = v.running_job_ids();
        v.adopt_active(ActiveKind::Transcribe, "rec_4", "job_4", 0.0, None); // after the ask
        assert!(v.drop_ended(&known, &["job_2".to_owned()]));
        assert!(v.view("rec_1").is_none(), "cancelled elsewhere");
        assert!(v.view("rec_2").is_some(), "still listed");
        assert!(v.view("rec_3").is_some(), "no id yet");
        assert!(
            v.view("rec_4").is_some(),
            "started after the list was asked for"
        );
        assert_eq!(v.error_of("rec_1"), None);
        assert!(!v.drop_ended(&known, &["job_2".to_owned()]));
    }
}
