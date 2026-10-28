use super::*;

// ---- the services --------------------------------------------------------------------------

/// Caption file formats of `export_captions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionFormat {
    Srt,
    Vtt,
    /// Plain text, one caption line per row, no timestamps.
    Txt,
    /// Plain text with a `[mm:ss]` (or `[hh:mm:ss]`) prefix on every line.
    TxtTimed,
    /// Markdown, one paragraph per caption line (per speaker turn when speakers are known).
    Md,
}

impl CaptionFormat {
    pub fn ext(self) -> &'static str {
        match self {
            CaptionFormat::Srt => "srt",
            CaptionFormat::Vtt => "vtt",
            CaptionFormat::Txt | CaptionFormat::TxtTimed => "txt",
            CaptionFormat::Md => "md",
        }
    }

    pub fn is_text(self) -> bool {
        matches!(
            self,
            CaptionFormat::Txt | CaptionFormat::TxtTimed | CaptionFormat::Md
        )
    }
}

/// What a transcription needs: the bundle (the service finds and decodes the audio) and choices.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscribeRequest {
    pub capture_id: String,
    pub bundle_dir: PathBuf,
    /// `None`: detect the language.
    pub language: Option<String>,
    /// An installed model id.
    pub model: String,
    /// The user's glossary (setting `ai.glossary`): the service primes the model with its terms
    /// ([`cm_ai::glossary::initial_prompt`]). The job replaces the heard-as spellings afterwards.
    pub glossary: Vec<cm_ai::glossary::GlossaryEntry>,
    /// The checkpoint the job saved before it was held or interrupted (`cm_ai::pieces::Checkpoint`
    /// as JSON): the run goes on from the last finished piece. `None`: from the start.
    pub resume: Option<serde_json::Value>,
    /// Read the microphone as it was recorded, not the noise-reduced copy.
    pub original_audio: bool,
}

/// The speech-model store (`cm_ai::model`). Blocking: the daemon calls it off the async threads.
pub trait ModelService: Send + Sync {
    /// Whether this build can manage models at all.
    fn available(&self) -> bool {
        true
    }
    fn list(&self) -> Result<ModelList, CmError>;
    /// Downloads a model, resuming a partial file. `progress` returns `false` to interrupt:
    /// the service stops, keeps the partial file for the next try and returns `Ok(false)`.
    /// `Ok(true)` means the model is installed and verified. Network failures are
    /// `CmError::network` (retried by the queue).
    fn download(
        &self,
        model: &str,
        progress: &mut dyn FnMut(DownloadProgress) -> bool,
    ) -> Result<bool, CmError>;
    /// Installs a model from a file the user has (checks it is a usable model).
    fn import(&self, model: &str, path: &Path) -> Result<(), CmError>;
    fn select(&self, model: &str) -> Result<(), CmError>;
    fn remove(&self, model: &str) -> Result<(), CmError>;
}

/// The text-model store of the built-in engine (`cm_ai::text_model::store`, AI-7). The same shape as
/// [`ModelService`]; the models are GGUF files for the bundled `llama-server`. Blocking.
pub trait TextModelService: Send + Sync {
    /// Whether this build can manage text models at all.
    fn available(&self) -> bool {
        true
    }
    fn list(&self) -> Result<TextModelList, CmError>;
    /// Downloads a model, resuming a partial file (see [`ModelService::download`]).
    fn download(
        &self,
        model: &str,
        progress: &mut dyn FnMut(DownloadProgress) -> bool,
    ) -> Result<bool, CmError>;
    fn select(&self, model: &str) -> Result<(), CmError>;
    fn remove(&self, model: &str) -> Result<(), CmError>;
    /// The installed model to run: the named one, else the active one. `invalid_input` when
    /// nothing is downloaded.
    fn resolve(&self, requested: Option<&str>) -> Result<ResolvedTextModel, CmError>;
}

/// Runs a transcription (`cm_ai::transcribe`): decodes the audio of the bundle and calls the
/// model. Blocking.
pub trait TranscribeService: Send + Sync {
    fn available(&self) -> bool {
        true
    }
    /// `progress` gets 0.0 to 1.0 and returns `false` to interrupt, which gives `Ok(None)`.
    fn transcribe(
        &self,
        req: &TranscribeRequest,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<Transcript>, CmError>;

    /// [`transcribe`](Self::transcribe) that goes on from `req.resume` and hands `on_checkpoint`
    /// a new checkpoint after every finished piece (the job saves it). A service that cannot
    /// resume ignores both and runs from the start.
    fn transcribe_resumable(
        &self,
        req: &TranscribeRequest,
        on_checkpoint: &mut dyn FnMut(serde_json::Value),
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<Transcript>, CmError> {
        let _ = on_checkpoint;
        self.transcribe(req, progress)
    }
}

/// What a diarization needs: the bundle (the service decodes the audio) and the choices.
#[derive(Debug, Clone, PartialEq)]
pub struct DiarizeRequest {
    pub capture_id: String,
    pub bundle_dir: PathBuf,
    pub hints: cm_ai::speakers::DiarizeHints,
    /// "My microphone only hears me": the mic track is the user, only the system track is
    /// diarized (AI-8.AC7).
    pub headset: bool,
    /// The user's name for headset mode (`profile.display_name`, default "Me").
    pub display_name: String,
    /// RFC 3339 time for `diarization.json`.
    pub created: String,
}

/// Runs a diarization (`cm_ai::speakers`): decodes the audio of the bundle and calls the speaker
/// model. Blocking. Nothing here may keep a voiceprint (AI-8 privacy).
pub trait SpeakerService: Send + Sync {
    fn available(&self) -> bool {
        true
    }
    /// Whether a speaker model is installed: `Err` with [`cm_ai::speakers::no_speaker_model`]
    /// (`invalid_input`, `data.reason = "no_speaker_model"`) when not. Checked before a job is
    /// queued.
    fn ready(&self) -> Result<(), CmError> {
        Ok(())
    }
    /// `progress` gets 0.0 to 1.0 and returns `false` to interrupt, which gives `Ok(None)`.
    fn diarize(
        &self,
        req: &DiarizeRequest,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<cm_ai::speakers::Diarized>, CmError>;
}

/// Caption lines and files (`cm_ai::caption`).
pub trait CaptionService: Send + Sync {
    fn available(&self) -> bool {
        true
    }
    /// Builds caption lines from a transcript (42 characters, 2 rows, 1 to 7 s each).
    fn lines(&self, transcript: &Transcript) -> Result<Vec<CaptionLine>, CmError>;
    /// The SRT or VTT text for these lines after the edit list is applied: cuts remove lines and
    /// shift later ones, speed scales times, and nothing references cut content (AI-1.AC4).
    fn render(
        &self,
        lines: &[CaptionLine],
        format: CaptionFormat,
        edits: &EditList,
    ) -> Result<String, CmError>;
}

/// Deterministic suggestions (`cm_ai::suggest`). Blocking.
pub trait SuggestService: Send + Sync {
    fn available(&self) -> bool {
        true
    }
    fn zooms(&self, bundle_dir: &Path) -> Result<Vec<ZoomSuggestion>, CmError>;
    /// Silence from the audio, filler words from the transcript when there is one.
    fn cuts(
        &self,
        bundle_dir: &Path,
        transcript: Option<&Transcript>,
    ) -> Result<Vec<CutSuggestion>, CmError>;
}

/// The hint every unavailable call carries.
pub const NOT_BUILT: &str =
    "local AI is not available in this build yet: the speech engine has not been plugged into the daemon";

pub(super) fn not_built() -> CmError {
    CmError::not_implemented(NOT_BUILT)
}

/// The default of every service: not available, every call fails with `not_implemented`.
pub struct Unavailable;

impl ModelService for Unavailable {
    fn available(&self) -> bool {
        false
    }
    fn list(&self) -> Result<ModelList, CmError> {
        Err(not_built())
    }
    fn download(
        &self,
        _: &str,
        _: &mut dyn FnMut(DownloadProgress) -> bool,
    ) -> Result<bool, CmError> {
        Err(not_built())
    }
    fn import(&self, _: &str, _: &Path) -> Result<(), CmError> {
        Err(not_built())
    }
    fn select(&self, _: &str) -> Result<(), CmError> {
        Err(not_built())
    }
    fn remove(&self, _: &str) -> Result<(), CmError> {
        Err(not_built())
    }
}

impl TextModelService for Unavailable {
    fn available(&self) -> bool {
        false
    }
    fn list(&self) -> Result<TextModelList, CmError> {
        Err(not_built())
    }
    fn download(
        &self,
        _: &str,
        _: &mut dyn FnMut(DownloadProgress) -> bool,
    ) -> Result<bool, CmError> {
        Err(not_built())
    }
    fn select(&self, _: &str) -> Result<(), CmError> {
        Err(not_built())
    }
    fn remove(&self, _: &str) -> Result<(), CmError> {
        Err(not_built())
    }
    fn resolve(&self, _: Option<&str>) -> Result<ResolvedTextModel, CmError> {
        Err(cm_ai::text_model::store::no_text_model(None))
    }
}

/// The text engine of a build that has none: the built-in provider answers `engine_missing`.
impl TextEngine for Unavailable {
    fn acquire(&self, _: &EngineSpec) -> Result<String, TextModelError> {
        Err(TextModelError {
            kind: cm_ai::text_model::ErrorKind::EngineMissing,
            message: "The built-in text engine is not available in this program.".to_owned(),
        })
    }
    fn release(&self) {}
    fn status(&self) -> EngineStatus {
        EngineStatus {
            state: EngineState::Stopped,
            model: None,
            port: None,
            in_flight: 0,
            restarts: 0,
            helper: None,
            reason: None,
            error: None,
            idle_stop_s: 0,
        }
    }
    fn stop(&self) {}
    fn reap_idle(&self) -> bool {
        false
    }
}

impl TranscribeService for Unavailable {
    fn available(&self) -> bool {
        false
    }
    fn transcribe(
        &self,
        _: &TranscribeRequest,
        _: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<Transcript>, CmError> {
        Err(not_built())
    }
}

impl SpeakerService for Unavailable {
    fn available(&self) -> bool {
        false
    }
    fn ready(&self) -> Result<(), CmError> {
        Err(not_built())
    }
    fn diarize(
        &self,
        _: &DiarizeRequest,
        _: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<cm_ai::speakers::Diarized>, CmError> {
        Err(not_built())
    }
}

impl CaptionService for Unavailable {
    fn available(&self) -> bool {
        false
    }
    fn lines(&self, _: &Transcript) -> Result<Vec<CaptionLine>, CmError> {
        Err(not_built())
    }
    fn render(&self, _: &[CaptionLine], _: CaptionFormat, _: &EditList) -> Result<String, CmError> {
        Err(not_built())
    }
}

impl SuggestService for Unavailable {
    fn available(&self) -> bool {
        false
    }
    fn zooms(&self, _: &Path) -> Result<Vec<ZoomSuggestion>, CmError> {
        Err(not_built())
    }
    fn cuts(&self, _: &Path, _: Option<&Transcript>) -> Result<Vec<CutSuggestion>, CmError> {
        Err(not_built())
    }
}

/// The engines behind the AI actions ([`crate::DaemonBuilder::ai`]).
#[derive(Clone)]
pub struct AiServices {
    pub models: Arc<dyn ModelService>,
    /// The built-in text models (AI-7, provider `embedded`).
    pub text_models: Arc<dyn TextModelService>,
    /// The helper process that runs them.
    pub text_engine: Arc<dyn TextEngine>,
    pub transcribe: Arc<dyn TranscribeService>,
    pub captions: Arc<dyn CaptionService>,
    pub suggest: Arc<dyn SuggestService>,
    /// Caption translation (AI-5).
    pub translate: Arc<dyn TranslateService>,
    /// Speaker labels (AI-8).
    pub speakers: Arc<dyn SpeakerService>,
}

impl Default for AiServices {
    fn default() -> Self {
        AiServices {
            models: Arc::new(Unavailable),
            text_models: Arc::new(Unavailable),
            text_engine: Arc::new(Unavailable),
            transcribe: Arc::new(Unavailable),
            captions: Arc::new(Unavailable),
            suggest: Arc::new(Unavailable),
            translate: Arc::new(Unavailable),
            speakers: Arc::new(Unavailable),
        }
    }
}

impl AiServices {
    /// Whether the engine behind a Confirm-tier AI action exists (checked before any dialog).
    pub(crate) fn supports(&self, method: &str) -> bool {
        match method {
            "list_models" | "download_model" | "import_model" | "select_model" | "remove_model" => {
                self.models.available()
            }
            "list_text_models"
            | "download_text_model"
            | "select_text_model"
            | "remove_text_model" => self.text_models.available(),
            "diarize" => self.speakers.available(),
            _ => true,
        }
    }
}
