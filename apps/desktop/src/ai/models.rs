//! The AI page's state, display-free: the model list, the user-started downloads, the remove
//! confirmation, the `ai.auto_transcribe` switch and the "no model yet" notice rules.
//!
//! A download only ever starts through [`ModelsVm::confirm_download`], which the page calls when
//! the user presses the confirm button of a row; there is no other path (nothing is downloaded
//! silently, `specs/features/07-ai.md`).

use std::collections::HashMap;

use cm_ai::model::{ModelInfo, ModelList};
use cm_i18n::{fmt, tr, tr_ctx};

use crate::daemon_text::ErrorNote;

/// Where the "no model" notice may appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeSite {
    /// The quiet banner at the top of the AI page.
    AiPage,
    /// The one-line hint under the recordings library header.
    Library,
    /// Beside the Transcribe button (the recording editor and the details card). It explains a
    /// disabled button, so dismissing the banners does not hide it.
    Transcribe,
}

pub fn notice_title() -> String {
    tr!("No speech model yet")
}

// TRANSLATORS: FACT: owner review required (nothing is downloaded unless the user presses Download)
pub fn notice_body() -> String {
    tr!("Add a model on the AI page to transcribe with AI. Nothing is downloaded unless you press Download there.")
}

pub fn transcribe_hint() -> String {
    tr!("Transcribing needs a speech model. Add one on the AI page.")
}

// TRANSLATORS: FACT: owner review required (privacy statement)
pub fn privacy_copy() -> String {
    tr!("Everything here runs on this Mac. Nothing is downloaded unless you press Download, and nothing is sent anywhere unless you choose a remote text model.")
}

/// One download in flight or interrupted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadState {
    /// The user pressed Download; the row asks "Download N MB?" and nothing has started.
    Confirming,
    /// Started; `job_id` is known once `download_model` answered.
    Running {
        job_id: Option<String>,
        done: u64,
        total: u64,
    },
    /// Cancelled or interrupted after some bytes; the partial file is kept, so it can resume.
    Paused { done: u64, total: u64 },
    /// The download failed; Resume tries again from the partial file. Worded when drawn.
    Failed(ErrorNote),
}

/// A button on a model row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowAction {
    /// Opens the "Download N MB?" confirmation (starts nothing).
    Download,
    /// Confirms it: this is the only action that starts a download.
    ConfirmDownload,
    /// Closes the confirmation.
    CancelConfirm,
    /// Stops a running download (the partial file stays).
    Cancel,
    Resume,
    Import,
    Select,
    /// Opens the "Remove?" confirmation.
    Remove,
    ConfirmRemove,
    CancelRemove,
}

impl RowAction {
    pub fn label(&self, size_mb: u32) -> String {
        match self {
            RowAction::Download => tr_ctx!("speech model", "Download"),
            RowAction::ConfirmDownload => {
                tr!("Download {size}", size = size_label(size_mb))
            }
            RowAction::CancelConfirm | RowAction::CancelRemove | RowAction::Cancel => {
                tr_ctx!("speech model", "Cancel")
            }
            RowAction::Resume => tr_ctx!("speech model", "Resume"),
            RowAction::Import => tr!("Import from file"),
            RowAction::Select => tr!("Use this model"),
            RowAction::Remove => tr_ctx!("speech model", "Remove"),
            RowAction::ConfirmRemove => tr!("Remove model"),
        }
    }

    /// The primary button of its row (the one the kit paints as primary).
    pub fn is_primary(&self) -> bool {
        matches!(
            self,
            RowAction::ConfirmDownload | RowAction::Resume | RowAction::ConfirmRemove
        )
    }
}

/// What pressing a button asks the daemon for, once the model state has been updated. Only
/// [`Effect::StartDownload`] sends `download_model`, and [`ModelsVm::apply`] returns it only after
/// the user's second press (confirm or resume).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    StartDownload,
    /// Stop this download job.
    CancelJob(String),
    Import,
    Select,
    Remove,
}

/// What a row says about its model.
#[derive(Debug, Clone, PartialEq)]
pub enum RowStatus {
    NotInstalled,
    Installed,
    Active,
    Confirming,
    /// 0.0 to 1.0.
    Downloading(f32),
    Paused(f32),
    Failed(String),
    ConfirmingRemove,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelRow {
    pub id: String,
    pub label: String,
    pub size_mb: u32,
    pub hint: String,
    pub size: String,
    /// `Needs about 852 MB of memory` (empty when the catalogue gives no figure).
    pub memory: String,
    pub status: RowStatus,
    /// The progress line under a running or paused row, `120 MB of 466 MB`.
    pub progress: Option<String>,
    pub actions: Vec<RowAction>,
}

/// The one-line hint of a catalogue model in the current language; a model this build does not
/// know (a user's own catalogue) shows the hint its catalogue gives.
fn model_hint(id: &str, catalogue: &str) -> String {
    match id {
        "tiny" => tr!("Fastest, lowest accuracy. Good for quick drafts on any machine."),
        "base" => tr!("Fast, fine for clear speech in a quiet room."),
        "small" => tr!("Balanced accuracy and speed. A good choice on 8 GB Macs."),
        "medium" => tr!("Most accurate, slow. Best for accents and noisy audio."),
        "large" => tr!("Best accuracy and translation to English"),
        "large-v3-turbo" => {
            tr!("Recommended: fast and accurate for most recordings. Not for translation.")
        }
        _ => catalogue.to_owned(),
    }
}

/// `Needs about 852 MB of memory`; empty for 0 (unknown).
pub fn memory_label(ram_mb: u32) -> String {
    if ram_mb == 0 {
        return String::new();
    }
    tr!(
        "Needs about {memory} of memory",
        memory = size_label(ram_mb)
    )
}

/// `148 MB`, `1.5 GB`.
pub fn size_label(mb: u32) -> String {
    if mb >= 1000 {
        format!("{} GB", fmt::decimal(f64::from(mb) / 1000.0, 1))
    } else {
        format!("{mb} MB")
    }
}

fn bytes_label(bytes: u64) -> String {
    let mb = bytes as f64 / 1e6;
    if mb >= 1000.0 {
        format!("{} GB", fmt::decimal(mb / 1000.0, 1))
    } else {
        format!("{} MB", fmt::decimal(mb, 0))
    }
}

/// `120 MB of 466 MB`.
pub fn progress_label(done: u64, total: u64) -> String {
    if total == 0 {
        bytes_label(done)
    } else {
        tr!(
            "{done} of {total}",
            done = bytes_label(done),
            total = bytes_label(total)
        )
    }
}

/// 0.0 to 1.0; 0 when the total is unknown.
pub fn fraction(done: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0) as f32
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModelsVm {
    pub list: Option<ModelList>,
    /// Why the list could not be read (the AI actions are missing, or the daemon is down).
    pub load_error: Option<ErrorNote>,
    pub downloads: HashMap<String, DownloadState>,
    pub confirm_remove: Option<String>,
    /// The user closed the banners; remembered per viewer (see `ui::ai`).
    pub notice_dismissed: bool,
    /// `ai.auto_transcribe`.
    pub auto_transcribe: bool,
    /// `ai.glossary` (AI-6a): names and terms spelled the user's way in new transcripts.
    pub glossary: Vec<cm_ai::glossary::GlossaryEntry>,
    /// The voice part of the page: speaker labels and the text model (V8).
    pub voice: crate::ai::voice::VoiceVm,
    /// An import or select or remove call is running.
    pub busy: bool,
    /// The last error of an import, select or remove, shown on the page.
    pub action_error: Option<ErrorNote>,
}

/// Whether Whisper can translate into English with what is installed (AI-5), by the daemon's own
/// rule ([`cm_ai::translate::choose_translation_model`]): the active model when it can translate,
/// else `large` (large-v3), else `medium`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhisperTranslate {
    /// The model list has not arrived.
    Unknown,
    /// The model id the job would use.
    Ready(String),
    /// None can; `active` is the label of the active model when that one only transcribes
    /// (Large v3 Turbo), `large_mb` the download size of Large v3 when the catalogue has it.
    Missing {
        active: Option<String>,
        large_mb: Option<u32>,
    },
}

impl ModelsVm {
    pub fn whisper_translate(&self) -> WhisperTranslate {
        let Some(l) = self.list.as_ref() else {
            return WhisperTranslate::Unknown;
        };
        let installed: Vec<String> = l
            .models
            .iter()
            .filter(|m| m.installed)
            .map(|m| m.id.clone())
            .collect();
        match cm_ai::translate::choose_translation_model(&installed, l.active.as_deref(), None) {
            Ok(id) => WhisperTranslate::Ready(id),
            Err(_) => WhisperTranslate::Missing {
                active: self
                    .active()
                    .filter(|m| !cm_ai::translate::model_can_translate(&m.id))
                    .map(|m| m.label.clone()),
                large_mb: l.models.iter().find(|m| m.id == "large").map(|m| m.size_mb),
            },
        }
    }

    pub fn installed_any(&self) -> bool {
        self.list
            .as_ref()
            .is_some_and(|l| l.models.iter().any(|m| m.installed))
    }

    /// The installed models, for the model choice of a re-generate.
    pub fn installed_models(&self) -> Vec<&ModelInfo> {
        self.list
            .as_ref()
            .map(|l| l.models.iter().filter(|m| m.installed).collect())
            .unwrap_or_default()
    }

    pub fn active(&self) -> Option<&ModelInfo> {
        let l = self.list.as_ref()?;
        l.models.iter().find(|m| m.active && m.installed)
    }

    pub fn set_list(&mut self, list: ModelList) {
        self.downloads
            .retain(|id, _| !list.models.iter().any(|m| &m.id == id && m.installed));
        if let Some(id) = &self.confirm_remove {
            if !list.models.iter().any(|m| &m.id == id && m.installed) {
                self.confirm_remove = None;
            }
        }
        self.list = Some(list);
        self.load_error = None;
    }

    pub fn set_load_error(&mut self, message: impl Into<ErrorNote>) {
        self.load_error = Some(message.into());
    }

    /// The notice rules: no model installed, the list was read, and (for the banners) the user has
    /// not dismissed it.
    pub fn notice_visible(&self, site: NoticeSite) -> bool {
        if self.list.is_none() || self.installed_any() {
            return false;
        }
        match site {
            NoticeSite::AiPage | NoticeSite::Library => !self.notice_dismissed,
            NoticeSite::Transcribe => true,
        }
    }

    /// Whether Transcribe can be pressed. Before the list is read it is allowed (the daemon says
    /// `no_model` if it must).
    pub fn can_transcribe(&self) -> bool {
        self.list.is_none() || self.installed_any()
    }

    // ---- downloads -----------------------------------------------------------------------------

    /// Download pressed: asks for confirmation. Nothing starts.
    pub fn begin_download(&mut self, id: &str) -> bool {
        let installed = self
            .list
            .as_ref()
            .is_some_and(|l| l.models.iter().any(|m| m.id == id && m.installed));
        if installed || matches!(self.downloads.get(id), Some(DownloadState::Running { .. })) {
            return false;
        }
        self.downloads
            .insert(id.to_owned(), DownloadState::Confirming);
        true
    }

    /// The confirmation was accepted (or Resume pressed): the row is running. Returns `true`
    /// when the page should now send `download_model`.
    pub fn confirm_download(&mut self, id: &str) -> bool {
        let (done, total) = match self.downloads.get(id) {
            Some(DownloadState::Confirming) => (0, 0),
            Some(DownloadState::Paused { done, total }) => (*done, *total),
            Some(DownloadState::Failed(_)) => (0, 0),
            _ => return false,
        };
        self.downloads.insert(
            id.to_owned(),
            DownloadState::Running {
                job_id: None,
                done,
                total,
            },
        );
        true
    }

    pub fn cancel_confirm(&mut self, id: &str) {
        if matches!(self.downloads.get(id), Some(DownloadState::Confirming)) {
            self.downloads.remove(id);
        }
    }

    pub fn download_started(&mut self, id: &str, job: String) {
        if let Some(DownloadState::Running { job_id, .. }) = self.downloads.get_mut(id) {
            *job_id = Some(job);
        }
    }

    /// The running download's job id, for Cancel.
    pub fn job_of(&self, id: &str) -> Option<String> {
        match self.downloads.get(id) {
            Some(DownloadState::Running { job_id, .. }) => job_id.clone(),
            _ => None,
        }
    }

    pub fn progress(&mut self, id: &str, done: u64, total: u64) {
        match self.downloads.get_mut(id) {
            Some(DownloadState::Running {
                done: d, total: t, ..
            }) => {
                *d = done;
                *t = total;
            }
            // Progress for a download another client started (the CLI): show it.
            None => {
                self.downloads.insert(
                    id.to_owned(),
                    DownloadState::Running {
                        job_id: None,
                        done,
                        total,
                    },
                );
            }
            _ => {}
        }
    }

    /// Cancel pressed: the partial file stays, so the row offers Resume.
    pub fn cancelled(&mut self, id: &str) {
        if let Some(DownloadState::Running { done, total, .. }) = self.downloads.get(id).cloned() {
            self.downloads
                .insert(id.to_owned(), DownloadState::Paused { done, total });
        }
    }

    pub fn failed(&mut self, id: &str, message: impl Into<ErrorNote>) {
        self.downloads
            .insert(id.to_owned(), DownloadState::Failed(message.into()));
    }

    // ---- remove --------------------------------------------------------------------------------

    pub fn begin_remove(&mut self, id: &str) {
        self.confirm_remove = Some(id.to_owned());
    }

    pub fn cancel_remove(&mut self) {
        self.confirm_remove = None;
    }

    /// A row button was pressed: updates the state and says what, if anything, to send.
    pub fn apply(&mut self, id: &str, action: &RowAction) -> Option<Effect> {
        match action {
            RowAction::Download => {
                self.begin_download(id);
                None
            }
            RowAction::CancelConfirm => {
                self.cancel_confirm(id);
                None
            }
            RowAction::ConfirmDownload | RowAction::Resume => {
                self.confirm_download(id).then_some(Effect::StartDownload)
            }
            RowAction::Cancel => {
                let job = self.job_of(id);
                self.cancelled(id);
                job.map(Effect::CancelJob)
            }
            RowAction::Import => Some(Effect::Import),
            RowAction::Select => Some(Effect::Select),
            RowAction::Remove => {
                self.begin_remove(id);
                None
            }
            RowAction::CancelRemove => {
                self.cancel_remove();
                None
            }
            RowAction::ConfirmRemove => {
                self.cancel_remove();
                Some(Effect::Remove)
            }
        }
    }

    // ---- rows ----------------------------------------------------------------------------------

    pub fn rows(&self) -> Vec<ModelRow> {
        let Some(list) = &self.list else {
            return Vec::new();
        };
        list.models.iter().map(|m| self.row(m)).collect()
    }

    fn row(&self, m: &ModelInfo) -> ModelRow {
        let dl = self.downloads.get(&m.id);
        let (status, progress, actions) = if m.installed {
            if self.confirm_remove.as_deref() == Some(m.id.as_str()) {
                (
                    RowStatus::ConfirmingRemove,
                    None,
                    vec![RowAction::ConfirmRemove, RowAction::CancelRemove],
                )
            } else if m.active {
                (RowStatus::Active, None, vec![RowAction::Remove])
            } else {
                (
                    RowStatus::Installed,
                    None,
                    vec![RowAction::Select, RowAction::Remove],
                )
            }
        } else {
            match dl {
                Some(DownloadState::Confirming) => (
                    RowStatus::Confirming,
                    None,
                    vec![RowAction::ConfirmDownload, RowAction::CancelConfirm],
                ),
                Some(DownloadState::Running { done, total, .. }) => (
                    RowStatus::Downloading(fraction(*done, *total)),
                    Some(progress_label(*done, *total)),
                    vec![RowAction::Cancel],
                ),
                Some(DownloadState::Paused { done, total }) => (
                    RowStatus::Paused(fraction(*done, *total)),
                    Some(progress_label(*done, *total)),
                    vec![RowAction::Resume, RowAction::Import],
                ),
                Some(DownloadState::Failed(e)) => (
                    RowStatus::Failed(e.text()),
                    None,
                    vec![RowAction::Resume, RowAction::Import],
                ),
                None => (
                    RowStatus::NotInstalled,
                    None,
                    vec![RowAction::Download, RowAction::Import],
                ),
            }
        };
        ModelRow {
            id: m.id.clone(),
            label: m.label.clone(),
            size_mb: m.size_mb,
            hint: model_hint(&m.id, &m.hint),
            size: size_label(m.size_mb),
            memory: memory_label(m.ram_mb),
            status,
            progress,
            actions,
        }
    }
}

impl RowStatus {
    pub fn text(&self) -> String {
        match self {
            RowStatus::NotInstalled => tr!("Not installed"),
            RowStatus::Installed => tr_ctx!("speech model", "Installed"),
            RowStatus::Active => tr_ctx!("speech model", "In use"),
            RowStatus::Confirming => tr!("Ready to download"),
            RowStatus::Downloading(f) => {
                tr!(
                    "Downloading {percent}%",
                    percent = fmt::decimal(f64::from(*f * 100.0), 0)
                )
            }
            RowStatus::Paused(f) => {
                tr!(
                    "Paused at {percent}%",
                    percent = fmt::decimal(f64::from(*f * 100.0), 0)
                )
            }
            RowStatus::Failed(e) => tr!("Download failed: {reason}", reason = e),
            RowStatus::ConfirmingRemove => tr!("Remove this model?"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: &str, mb: u32, installed: bool, active: bool) -> ModelInfo {
        ModelInfo {
            id: id.into(),
            label: id.to_uppercase(),
            size_mb: mb,
            ram_mb: if id == "small" { 852 } else { 0 },
            hint: "hint".into(),
            installed,
            active,
            path: installed.then(|| format!("/m/{id}.bin")),
        }
    }

    fn list(models: Vec<ModelInfo>) -> ModelList {
        let active = models.iter().find(|m| m.active).map(|m| m.id.clone());
        ModelList {
            models,
            active,
            dir: "/m".into(),
        }
    }

    fn vm_with(models: Vec<ModelInfo>) -> ModelsVm {
        let mut vm = ModelsVm::default();
        vm.set_list(list(models));
        vm
    }

    /// AI-5: the Translate panel knows in advance whether Whisper can translate.
    #[test]
    fn whisper_translation_follows_the_daemons_model_rule() {
        let none = ModelsVm::default();
        assert_eq!(none.whisper_translate(), WhisperTranslate::Unknown);
        // Turbo only: missing, and the panel names the active model.
        let vm = vm_with(vec![
            info("large-v3-turbo", 1600, true, true),
            info("large", 3100, false, false),
        ]);
        assert_eq!(
            vm.whisper_translate(),
            WhisperTranslate::Missing {
                active: Some("LARGE-V3-TURBO".into()),
                large_mb: Some(3100)
            }
        );
        // Nothing installed.
        let vm = vm_with(vec![info("large", 3100, false, false)]);
        assert!(matches!(
            vm.whisper_translate(),
            WhisperTranslate::Missing { active: None, .. }
        ));
        // Medium is enough; large wins over it when turbo is the active one.
        let vm = vm_with(vec![
            info("large-v3-turbo", 1600, true, true),
            info("medium", 1500, true, false),
        ]);
        assert_eq!(
            vm.whisper_translate(),
            WhisperTranslate::Ready("medium".into())
        );
        let vm = vm_with(vec![
            info("large-v3-turbo", 1600, true, true),
            info("medium", 1500, true, false),
            info("large", 3100, true, false),
        ]);
        assert_eq!(
            vm.whisper_translate(),
            WhisperTranslate::Ready("large".into())
        );
        // The active model can translate: it is used.
        let vm = vm_with(vec![info("small", 466, true, true)]);
        assert_eq!(
            vm.whisper_translate(),
            WhisperTranslate::Ready("small".into())
        );
    }

    fn actions(vm: &ModelsVm, id: &str) -> Vec<RowAction> {
        vm.rows().into_iter().find(|r| r.id == id).unwrap().actions
    }

    #[test]
    fn sizes_and_progress_read_plainly() {
        assert_eq!(size_label(148), "148 MB");
        assert_eq!(size_label(1500), "1.5 GB");
        assert_eq!(progress_label(120_000_000, 466_000_000), "120 MB of 466 MB");
        assert_eq!(progress_label(0, 0), "0 MB");
        assert!((fraction(1, 4) - 0.25).abs() < 1e-6);
        assert_eq!(fraction(5, 0), 0.0);
        assert_eq!(fraction(9, 4), 1.0);
    }

    /// The notice appears only when the list is read and no model is installed; the banners can be
    /// dismissed, the hint beside the Transcribe button cannot (it explains a disabled button).
    #[test]
    fn notice_rules() {
        let mut vm = ModelsVm::default();
        // Not read yet: no notice, Transcribe allowed (the daemon decides).
        assert!(!vm.notice_visible(NoticeSite::AiPage));
        assert!(vm.can_transcribe());
        vm.set_list(list(vec![info("base", 148, false, false)]));
        assert!(vm.notice_visible(NoticeSite::AiPage));
        assert!(vm.notice_visible(NoticeSite::Library));
        assert!(vm.notice_visible(NoticeSite::Transcribe));
        assert!(!vm.can_transcribe());
        vm.notice_dismissed = true;
        assert!(!vm.notice_visible(NoticeSite::AiPage));
        assert!(!vm.notice_visible(NoticeSite::Library));
        assert!(vm.notice_visible(NoticeSite::Transcribe));
        vm.set_list(list(vec![info("base", 148, true, true)]));
        assert!(!vm.notice_visible(NoticeSite::Transcribe));
        assert!(vm.can_transcribe());
    }

    /// The memory line comes from the catalogue's figure (the whisper.cpp README's: small is about
    /// 852 MB, medium about 2.1 GB, large about 3.9 GB); 0 means unknown and shows nothing. Every
    /// shipped model has a translated hint, and the Medium hint no longer claims a 16 GB machine.
    #[test]
    fn the_memory_line_and_the_model_hints() {
        assert_eq!(memory_label(852), "Needs about 852 MB of memory");
        assert_eq!(memory_label(2100), "Needs about 2.1 GB of memory");
        assert_eq!(memory_label(3900), "Needs about 3.9 GB of memory");
        assert_eq!(memory_label(0), "");
        let mut vm = ModelsVm::default();
        vm.set_list(list(vec![info("small", 466, false, false)]));
        assert_eq!(vm.rows()[0].memory, "Needs about 852 MB of memory");
        for id in ["tiny", "base", "small", "medium", "large"] {
            assert_ne!(model_hint(id, "catalogue"), "catalogue", "{id}");
        }
        assert!(!model_hint("medium", "").contains("16 GB"));
        // A model from a user's own catalogue keeps its own hint.
        assert_eq!(model_hint("mine", "My hint"), "My hint");
    }

    /// Nothing starts until the confirm button: Download only opens the question.
    #[test]
    fn a_download_needs_two_presses() {
        let mut vm = vm_with(vec![info("small", 466, false, false)]);
        assert_eq!(
            actions(&vm, "small"),
            vec![RowAction::Download, RowAction::Import]
        );
        // Download pressed: the row asks, nothing runs, no job.
        assert!(vm.begin_download("small"));
        assert_eq!(vm.downloads.get("small"), Some(&DownloadState::Confirming));
        assert_eq!(vm.job_of("small"), None);
        assert_eq!(
            actions(&vm, "small"),
            vec![RowAction::ConfirmDownload, RowAction::CancelConfirm]
        );
        let row = &vm.rows()[0];
        assert_eq!(row.actions[0].label(row.size_mb), "Download 466 MB");
        // Cancelling the question leaves the row as it was.
        vm.cancel_confirm("small");
        assert!(vm.downloads.is_empty());
        // The second press is what starts it.
        vm.begin_download("small");
        assert!(vm.confirm_download("small"));
        assert!(
            !vm.confirm_download("small"),
            "a running download is not restarted"
        );
        vm.download_started("small", "job_9".into());
        assert_eq!(vm.job_of("small").as_deref(), Some("job_9"));
    }

    #[test]
    fn confirming_without_asking_starts_nothing() {
        let mut vm = vm_with(vec![info("small", 466, false, false)]);
        assert!(!vm.confirm_download("small"));
        assert!(vm.downloads.is_empty());
    }

    #[test]
    fn progress_cancel_resume_and_finish() {
        let mut vm = vm_with(vec![info("small", 466, false, false)]);
        vm.begin_download("small");
        vm.confirm_download("small");
        vm.progress("small", 120_000_000, 466_000_000);
        let r = &vm.rows()[0];
        assert_eq!(r.status.text(), "Downloading 26%");
        assert_eq!(r.progress.as_deref(), Some("120 MB of 466 MB"));
        assert_eq!(r.actions, vec![RowAction::Cancel]);
        // Cancel keeps the partial file: the row offers Resume and Import.
        vm.cancelled("small");
        let r = &vm.rows()[0];
        assert_eq!(r.status.text(), "Paused at 26%");
        assert_eq!(r.actions, vec![RowAction::Resume, RowAction::Import]);
        // Resume goes straight to running from where it stopped (the user already confirmed).
        assert!(vm.confirm_download("small"));
        assert_eq!(
            vm.downloads.get("small"),
            Some(&DownloadState::Running {
                job_id: None,
                done: 120_000_000,
                total: 466_000_000
            })
        );
        // The model appearing in the list ends the download.
        vm.set_list(list(vec![info("small", 466, true, false)]));
        assert!(vm.downloads.is_empty());
        assert_eq!(vm.rows()[0].status, RowStatus::Installed);
    }

    #[test]
    fn a_failed_download_offers_resume() {
        let mut vm = vm_with(vec![info("base", 148, false, false)]);
        vm.begin_download("base");
        vm.confirm_download("base");
        vm.failed("base", "network lost");
        let r = &vm.rows()[0];
        assert_eq!(r.status.text(), "Download failed: network lost");
        assert_eq!(r.actions, vec![RowAction::Resume, RowAction::Import]);
        assert!(vm.confirm_download("base"));
    }

    /// A download another client started (the CLI) shows up from its progress events.
    #[test]
    fn progress_from_another_client_is_shown() {
        let mut vm = vm_with(vec![info("base", 148, false, false)]);
        vm.progress("base", 10_000_000, 148_000_000);
        assert!(matches!(
            vm.downloads.get("base"),
            Some(DownloadState::Running { .. })
        ));
    }

    #[test]
    fn installed_rows_select_and_remove_with_a_confirmation() {
        let mut vm = vm_with(vec![
            info("base", 148, true, true),
            info("small", 466, true, false),
            info("medium", 1500, false, false),
        ]);
        assert_eq!(actions(&vm, "base"), vec![RowAction::Remove]);
        assert_eq!(
            actions(&vm, "small"),
            vec![RowAction::Select, RowAction::Remove]
        );
        assert_eq!(vm.active().map(|m| m.id.as_str()), Some("base"));
        vm.begin_remove("small");
        assert_eq!(
            actions(&vm, "small"),
            vec![RowAction::ConfirmRemove, RowAction::CancelRemove]
        );
        vm.cancel_remove();
        assert_eq!(
            actions(&vm, "small"),
            vec![RowAction::Select, RowAction::Remove]
        );
        // The removal confirmation does not outlive the model.
        vm.begin_remove("small");
        vm.set_list(list(vec![
            info("base", 148, true, true),
            info("small", 466, false, false),
        ]));
        assert_eq!(vm.confirm_remove, None);
    }

    #[test]
    fn an_installed_model_cannot_be_downloaded_again() {
        let mut vm = vm_with(vec![info("base", 148, true, true)]);
        assert!(!vm.begin_download("base"));
    }

    #[test]
    fn primary_buttons_are_the_committing_ones() {
        assert!(RowAction::ConfirmDownload.is_primary());
        assert!(RowAction::Resume.is_primary());
        assert!(!RowAction::Download.is_primary());
        assert!(!RowAction::Import.is_primary());
    }
}
