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
}

/// The background jobs of the Speakers and Report tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceJobKind {
    /// `diarize`.
    Speakers,
    /// `generate_report`.
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
    /// The "Burn in captions" choice made for the next export of a recording, until it is
    /// exported or the recording's own setting is changed.
    export_burn: HashMap<String, bool>,
}

impl TranscribeVm {
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
        let p = if progress > 1.0 {
            progress / 100.0
        } else {
            progress
        };
        for r in self.running.values_mut() {
            if r.job_id.as_deref() == Some(job_id) {
                r.progress = Some(p.clamp(0.0, 1.0) as f32);
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

    /// Opens the Re-generate panel (the defaults: the active model, auto-detect).
    pub fn open_regen(&mut self, capture_id: &str) {
        self.regen.entry(capture_id.to_owned()).or_default();
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

    pub fn fraction(&self, capture_id: &str) -> Option<f32> {
        self.running.get(capture_id).and_then(|r| r.progress)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
