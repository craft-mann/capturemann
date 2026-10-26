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

fn not_built() -> CmError {
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
    pub transcribe: Arc<dyn TranscribeService>,
    pub captions: Arc<dyn CaptionService>,
    pub suggest: Arc<dyn SuggestService>,
    /// Speaker labels (AI-8).
    pub speakers: Arc<dyn SpeakerService>,
}

impl Default for AiServices {
    fn default() -> Self {
        AiServices {
            models: Arc::new(Unavailable),
            transcribe: Arc::new(Unavailable),
            captions: Arc::new(Unavailable),
            suggest: Arc::new(Unavailable),
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
            "diarize" => self.speakers.available(),
            _ => true,
        }
    }
}
