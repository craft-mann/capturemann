//! A typed client of the AI actions in the contract table of `crates/cm-ai/src/lib.rs`. The UI
//! talks to the daemon only through [`AiTransport`], so tests run it against a scripted fake and
//! the app against [`DaemonTransport`].

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use cm_ai::caption::CaptionLine;
use cm_ai::model::ModelList;
use cm_ai::suggest::{CutSuggestion, ZoomSuggestion};
use cm_ai::transcribe::Transcript;
use cm_core::Actor;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

/// A failed AI call. `reason` is the machine-readable `data.reason` (`"no_model"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiError {
    pub code: Option<String>,
    pub reason: Option<String>,
    pub message: String,
}

impl AiError {
    pub fn message(message: impl Into<String>) -> AiError {
        AiError {
            code: None,
            reason: None,
            message: message.into(),
        }
    }

    /// `transcribe` without an installed model: `invalid_input` with `data.reason == "no_model"`.
    pub fn is_no_model(&self) -> bool {
        self.reason.as_deref() == Some("no_model")
    }

    /// `diarize` without a speaker model (`data.reason == "no_speaker_model"`).
    pub fn is_no_speaker_model(&self) -> bool {
        self.reason.as_deref() == Some("no_speaker_model")
    }

    /// A report without a local text model (`data.reason == "no_text_model"`).
    pub fn is_no_text_model(&self) -> bool {
        self.reason.as_deref() == Some("no_text_model")
    }
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// One JSON-RPC style call: the method name and its params, the result object.
#[async_trait]
pub trait AiTransport: Send + Sync {
    async fn call(&self, method: &str, params: Value) -> Result<Value, AiError>;
}

/// Caption file formats of `export_captions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptionFormat {
    Srt,
    Vtt,
    /// Plain text for other uses (`<title>.txt`).
    Txt,
}

impl CaptionFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            CaptionFormat::Srt => "srt",
            CaptionFormat::Vtt => "vtt",
            CaptionFormat::Txt => "txt",
        }
    }

    /// Upper-case name for buttons and toasts.
    pub fn label(self) -> &'static str {
        match self {
            CaptionFormat::Srt => "SRT",
            CaptionFormat::Vtt => "VTT",
            CaptionFormat::Txt => "TXT",
        }
    }
}

/// The two caption switches of a recording (`set_caption_options`): show the captions in the
/// editor's preview, and burn them into exported videos by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CaptionOptionsInfo {
    pub enabled: bool,
    pub burn_in: bool,
}

impl CaptionOptionsInfo {
    /// Reads `{ enabled, burn_in }` (both default to off when missing).
    pub fn from_json(v: Option<&Value>) -> CaptionOptionsInfo {
        let flag = |k: &str| {
            v.and_then(|o| o.get(k))
                .and_then(Value::as_bool)
                .unwrap_or(false)
        };
        CaptionOptionsInfo {
            enabled: flag("enabled"),
            burn_in: flag("burn_in"),
        }
    }
}

/// What the recording details page shows about a stored transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptInfo {
    pub language: String,
    /// The model id that made it.
    pub model: String,
    pub words: usize,
    /// When it was written, Unix seconds (`None` when the daemon cannot say).
    pub made_at: Option<u64>,
    /// The user edited the caption lines (re-generating would discard those edits).
    pub edited: bool,
    /// The caption switches (show in the preview, burn into exports).
    pub options: CaptionOptionsInfo,
}

/// A transcription, speaker or report job that is waiting or running in the daemon.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveJob {
    pub job_id: String,
    pub kind: crate::ai::jobs::ActiveKind,
    pub capture_id: String,
    /// 0.0 to 1.0.
    pub progress: f32,
}

/// Which job `type` in `list_jobs` is which kind of run on the editor's tabs.
fn active_kind(job_type: &str) -> Option<crate::ai::jobs::ActiveKind> {
    use crate::ai::jobs::ActiveKind;
    match job_type {
        "transcribe" => Some(ActiveKind::Transcribe),
        "diarize" => Some(ActiveKind::Speakers),
        "report" => Some(ActiveKind::Report),
        _ => None,
    }
}

/// The active ones among the jobs of a `list_jobs` answer (queued, running or paused), those of a
/// recording.
pub fn active_jobs_of(answer: &Value) -> Vec<ActiveJob> {
    answer["jobs"]
        .as_array()
        .map(|jobs| {
            jobs.iter()
                .filter(|j| matches!(j["state"].as_str(), Some("queued" | "running" | "paused")))
                .filter_map(|j| {
                    Some(ActiveJob {
                        job_id: j["job_id"].as_str()?.to_owned(),
                        kind: active_kind(j["type"].as_str()?)?,
                        capture_id: j["inputs"]["capture_id"].as_str()?.to_owned(),
                        progress: j["progress"].as_f64().unwrap_or(0.0) as f32,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone)]
pub struct AiClient {
    transport: Arc<dyn AiTransport>,
}

fn parse<T: DeserializeOwned>(method: &str, v: Value) -> Result<T, AiError> {
    serde_json::from_value(v)
        .map_err(|e| AiError::message(format!("{method}: unexpected answer: {e}")))
}

fn job_id(method: &str, v: &Value) -> Result<String, AiError> {
    v.get("job_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AiError::message(format!("{method}: the answer has no job_id")))
}

impl AiClient {
    pub fn new(transport: Arc<dyn AiTransport>) -> AiClient {
        AiClient { transport }
    }

    /// `get_text_model` (AI-7): provider, model, the built-in engine's state and limits.
    /// Never the key.
    pub async fn get_text_model(&self) -> Result<Value, AiError> {
        self.transport.call("get_text_model", json!({})).await
    }

    /// `test_text_model` (AI-7): one short prompt to the configured provider only.
    pub async fn test_text_model(&self) -> Result<Value, AiError> {
        self.transport.call("test_text_model", json!({})).await
    }

    /// `list_text_models` (AI-7): the built-in text models by memory tier.
    pub async fn list_text_models(
        &self,
    ) -> Result<cm_ai::text_model::store::TextModelList, AiError> {
        let v = self.transport.call("list_text_models", json!({})).await?;
        parse("list_text_models", v)
    }

    /// Starts a user-requested text-model download; returns the job id (progress arrives as
    /// `model.download.progress` events with `kind: "text"`).
    pub async fn download_text_model(&self, model: &str) -> Result<String, AiError> {
        let v = self
            .transport
            .call("download_text_model", json!({ "model": model }))
            .await?;
        job_id("download_text_model", &v)
    }

    pub async fn select_text_model(&self, model: &str) -> Result<(), AiError> {
        self.transport
            .call("select_text_model", json!({ "model": model }))
            .await
            .map(|_| ())
    }

    pub async fn remove_text_model(&self, model: &str) -> Result<(), AiError> {
        self.transport
            .call("remove_text_model", json!({ "model": model }))
            .await
            .map(|_| ())
    }

    pub async fn list_models(&self) -> Result<ModelList, AiError> {
        let v = self.transport.call("list_models", json!({})).await?;
        parse("list_models", v)
    }

    /// Starts a user-requested download; returns the job id (progress arrives as
    /// `model.download.progress` events).
    pub async fn download_model(&self, model: &str) -> Result<String, AiError> {
        let v = self
            .transport
            .call("download_model", json!({ "model": model }))
            .await?;
        job_id("download_model", &v)
    }

    /// The transcription, speaker and report jobs that are waiting or running (the daemon picks
    /// up the ones a restart left in its queue, and nothing tells the window about those).
    pub async fn active_voice_jobs(&self) -> Vec<ActiveJob> {
        let mut out = Vec::new();
        for job_type in ["transcribe", "diarize", "report"] {
            if let Ok(v) = self
                .transport
                .call("list_jobs", json!({ "type": job_type, "limit": 20 }))
                .await
            {
                out.extend(active_jobs_of(&v));
            }
        }
        out
    }

    pub async fn cancel_job(&self, job_id: &str) -> Result<(), AiError> {
        self.transport
            .call("cancel_job", json!({ "job_id": job_id }))
            .await
            .map(|_| ())
    }

    pub async fn import_model(&self, model: &str, path: &str) -> Result<(), AiError> {
        self.transport
            .call("import_model", json!({ "model": model, "path": path }))
            .await
            .map(|_| ())
    }

    pub async fn select_model(&self, model: &str) -> Result<(), AiError> {
        self.transport
            .call("select_model", json!({ "model": model }))
            .await
            .map(|_| ())
    }

    pub async fn remove_model(&self, model: &str) -> Result<(), AiError> {
        self.transport
            .call("remove_model", json!({ "model": model }))
            .await
            .map(|_| ())
    }

    pub async fn transcribe(
        &self,
        capture_id: &str,
        language: Option<&str>,
        model: Option<&str>,
    ) -> Result<String, AiError> {
        let mut p = json!({ "capture_id": capture_id });
        if let Some(l) = language {
            p["language"] = json!(l);
        }
        if let Some(m) = model {
            p["model"] = json!(m);
        }
        let v = self.transport.call("transcribe", p).await?;
        job_id("transcribe", &v)
    }

    /// The stored transcript, or `None` when the recording has none yet.
    pub async fn get_transcript(&self, capture_id: &str) -> Result<Option<Transcript>, AiError> {
        match self
            .transport
            .call("get_transcript", json!({ "capture_id": capture_id }))
            .await
        {
            Ok(Value::Null) => Ok(None),
            // The daemon answers `{ "capture_id": ..., "transcript": {...} }`; a bare transcript is
            // accepted too (the scripted tests and older callers use it).
            Ok(v) => parse("get_transcript", v.get("transcript").cloned().unwrap_or(v)).map(Some),
            Err(e) if e.code.as_deref() == Some("not_found") => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The summary of the stored transcript (language, model, word count, when, edited captions),
    /// or `None` when the recording has none. Read from the same `get_transcript` answer.
    pub async fn transcript_info(
        &self,
        capture_id: &str,
    ) -> Result<Option<TranscriptInfo>, AiError> {
        let v = match self
            .transport
            .call("get_transcript", json!({ "capture_id": capture_id }))
            .await
        {
            Ok(Value::Null) => return Ok(None),
            Ok(v) => v,
            Err(e) if e.code.as_deref() == Some("not_found") => return Ok(None),
            Err(e) => return Err(e),
        };
        let t = v.get("transcript").unwrap_or(&v);
        let text = |k: &str| t.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
        Ok(Some(TranscriptInfo {
            language: text("language"),
            model: text("model"),
            words: t.get("words").and_then(Value::as_array).map_or(0, Vec::len),
            made_at: v.get("made_at").and_then(Value::as_u64),
            edited: v.get("captions_source").and_then(Value::as_str) == Some("edited"),
            options: CaptionOptionsInfo::from_json(v.get("caption_options")),
        }))
    }

    /// The stored transcript together with the recording's caption switches (one read), or
    /// `None` when the recording has no transcript yet.
    pub async fn get_transcript_with_options(
        &self,
        capture_id: &str,
    ) -> Result<Option<(Transcript, CaptionOptionsInfo)>, AiError> {
        let v = match self
            .transport
            .call("get_transcript", json!({ "capture_id": capture_id }))
            .await
        {
            Ok(Value::Null) => return Ok(None),
            Ok(v) => v,
            Err(e) if e.code.as_deref() == Some("not_found") => return Ok(None),
            Err(e) => return Err(e),
        };
        let options = CaptionOptionsInfo::from_json(v.get("caption_options"));
        let t: Transcript = parse("get_transcript", v.get("transcript").cloned().unwrap_or(v))?;
        Ok(Some((t, options)))
    }

    /// Sets the caption switches (`None` keeps a value): one undoable revision in the daemon.
    /// Returns the switches as they are now.
    pub async fn set_caption_options(
        &self,
        capture_id: &str,
        enabled: Option<bool>,
        burn_in: Option<bool>,
    ) -> Result<CaptionOptionsInfo, AiError> {
        let mut p = json!({ "capture_id": capture_id });
        if let Some(e) = enabled {
            p["enabled"] = json!(e);
        }
        if let Some(b) = burn_in {
            p["burn_in"] = json!(b);
        }
        let v = self.transport.call("set_caption_options", p).await?;
        Ok(CaptionOptionsInfo::from_json(Some(&v)))
    }

    /// The stored caption lines (as the user last saved them, or as the daemon built them), or
    /// `None` when none are stored or the daemon has no `get_captions`. The UI then builds lines
    /// from the transcript words ([`crate::ai::captions::lines_from_words`]).
    pub async fn get_captions(
        &self,
        capture_id: &str,
    ) -> Result<Option<Vec<CaptionLine>>, AiError> {
        match self
            .transport
            .call("get_captions", json!({ "capture_id": capture_id }))
            .await
        {
            Ok(Value::Null) => Ok(None),
            Ok(v) => parse(
                "get_captions",
                v.get("lines").cloned().unwrap_or(Value::Array(vec![])),
            )
            .map(|l: Vec<CaptionLine>| (!l.is_empty()).then_some(l)),
            // No such method (an older daemon) or nothing stored: both mean "build them".
            Err(e) if e.code.is_none() || e.code.as_deref() == Some("not_found") => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn set_captions(
        &self,
        capture_id: &str,
        lines: &[CaptionLine],
    ) -> Result<(), AiError> {
        // The daemon reads a plain number as SECONDS; ticks must be `"t:<ticks>"` (it answers with
        // plain ticks, so the lines it returned would otherwise come back as huge times).
        let lines: Vec<Value> = lines
            .iter()
            .map(|l| {
                json!({
                    "start": format!("t:{}", l.start.ticks()),
                    "end": format!("t:{}", l.end.ticks()),
                    "text": l.text,
                })
            })
            .collect();
        self.transport
            .call(
                "set_captions",
                json!({ "capture_id": capture_id, "lines": lines }),
            )
            .await
            .map(|_| ())
    }

    /// `timestamps` only applies to [`CaptionFormat::Txt`] (`[mm:ss]` before every line).
    pub async fn export_captions(
        &self,
        capture_id: &str,
        format: CaptionFormat,
        timestamps: bool,
    ) -> Result<PathBuf, AiError> {
        self.export_captions_for(capture_id, format, timestamps, None, false)
            .await
    }

    /// [`Self::export_captions`] for one language of the transcript (`None`: the original) and,
    /// for SRT, with the speakers' names before each line (VOC-2, AI-8; VTT and TXT always name
    /// the speakers when they are known, so the daemon refuses the flag for them).
    pub async fn export_captions_for(
        &self,
        capture_id: &str,
        format: CaptionFormat,
        timestamps: bool,
        language: Option<&str>,
        speaker_names: bool,
    ) -> Result<PathBuf, AiError> {
        let mut p = json!({ "capture_id": capture_id, "format": format.as_str() });
        if timestamps && format == CaptionFormat::Txt {
            p["timestamps"] = json!(true);
        }
        if let Some(l) = language {
            p["language"] = json!(l);
        }
        if speaker_names && format == CaptionFormat::Srt {
            p["speaker_names"] = json!(true);
        }
        let v = self.transport.call("export_captions", p).await?;
        v.get("path")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| AiError::message("export_captions: the answer has no path"))
    }

    /// `translate_captions`: starts the `translate` job for `target_language`; the engine is
    /// chosen by the daemon (AI-5). The result arrives as `translation.ready`.
    pub async fn translate_captions(
        &self,
        capture_id: &str,
        target_language: &str,
    ) -> Result<String, AiError> {
        let v = self
            .transport
            .call(
                "translate_captions",
                json!({ "capture_id": capture_id, "target_language": target_language }),
            )
            .await?;
        job_id("translate_captions", &v)
    }

    /// `diarize` (AI-8): starts the speaker job. `num_speakers` is the "How many people?" answer
    /// (`None`: not sure); `headset` is "My microphone only hears me". The result arrives as
    /// `speakers.ready`. Without a speaker model the daemon answers `no_speaker_model`.
    pub async fn diarize(
        &self,
        capture_id: &str,
        num_speakers: Option<u32>,
        min_speakers: Option<u32>,
        headset: bool,
    ) -> Result<String, AiError> {
        let mut p = json!({ "capture_id": capture_id });
        if let Some(n) = num_speakers {
            p["num_speakers"] = json!(n);
        }
        if let Some(n) = min_speakers {
            p["min_speakers"] = json!(n);
        }
        if headset {
            p["headset"] = json!(true);
        }
        let v = self.transport.call("diarize", p).await?;
        job_id("diarize", &v)
    }

    /// `set_speakers` (AI-8): renames (`id` to name), merges (`from` into `into`) and reassigns
    /// time ranges, all as one undo step. Returns whether anything changed.
    pub async fn set_speakers(
        &self,
        capture_id: &str,
        rename: &[(u32, String)],
        merge: &[(u32, u32)],
    ) -> Result<bool, AiError> {
        let mut p = json!({ "capture_id": capture_id });
        if !rename.is_empty() {
            let mut m = serde_json::Map::new();
            for (id, name) in rename {
                m.insert(id.to_string(), json!(name));
            }
            p["rename"] = Value::Object(m);
        }
        if !merge.is_empty() {
            p["merge"] = Value::Array(
                merge
                    .iter()
                    .map(|(from, into)| json!({ "from": from, "into": into }))
                    .collect(),
            );
        }
        let v = self.transport.call("set_speakers", p).await?;
        Ok(v.get("changed").and_then(Value::as_bool).unwrap_or(true))
    }

    /// `generate_report` (AI-9): starts the `report` job; the result arrives as `report.ready`.
    /// `context` holds only the fields the template lists. `report_id` regenerates that report.
    pub async fn generate_report(
        &self,
        capture_id: &str,
        template: &str,
        language: Option<&str>,
        context: Value,
        report_id: Option<&str>,
    ) -> Result<String, AiError> {
        let mut p = json!({ "capture_id": capture_id, "template": template });
        if let Some(l) = language {
            p["language"] = json!(l);
        }
        if context.as_object().is_some_and(|o| !o.is_empty()) {
            p["context"] = context;
        }
        if let Some(r) = report_id {
            p["report_id"] = json!(r);
        }
        let v = self.transport.call("generate_report", p).await?;
        job_id("generate_report", &v)
    }

    /// `export_report` (AI-9): a Markdown (`md`) or text (`txt`) file in the recording's
    /// `exports/` folder.
    pub async fn export_report(
        &self,
        capture_id: &str,
        report_id: &str,
        format: &str,
        include_times: bool,
    ) -> Result<PathBuf, AiError> {
        let mut params =
            json!({ "capture_id": capture_id, "report_id": report_id, "format": format });
        if include_times {
            params["include_times"] = json!(true);
        }
        let v = self.transport.call("export_report", params).await?;
        v.get("path")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| AiError::message("export_report: the answer has no path"))
    }

    /// `delete_report` (AI-9): one undo step.
    pub async fn delete_report(&self, capture_id: &str, report_id: &str) -> Result<(), AiError> {
        self.transport
            .call(
                "delete_report",
                json!({ "capture_id": capture_id, "report_id": report_id }),
            )
            .await
            .map(|_| ())
    }

    /// `edit_report` (AI-9): replaces the report's Markdown; one undo step.
    pub async fn edit_report(
        &self,
        capture_id: &str,
        report_id: &str,
        markdown: &str,
    ) -> Result<(), AiError> {
        self.transport
            .call(
                "edit_report",
                json!({ "capture_id": capture_id, "report_id": report_id, "markdown": markdown }),
            )
            .await
            .map(|_| ())
    }

    /// `check_report_item` (AI-9): ticks or unticks a task of a report; one undo step.
    pub async fn check_report_item(
        &self,
        capture_id: &str,
        report_id: &str,
        section: &str,
        index: usize,
        checked: bool,
    ) -> Result<(), AiError> {
        self.transport
            .call(
                "check_report_item",
                json!({ "capture_id": capture_id, "report_id": report_id,
                        "section": section, "index": index, "checked": checked }),
            )
            .await
            .map(|_| ())
    }

    /// `export_audio` (VOC-2): starts the audio export job; the result arrives as `export.done`.
    pub async fn export_audio(&self, params: Value) -> Result<String, AiError> {
        let v = self.transport.call("export_audio", params).await?;
        job_id("export_audio", &v)
    }

    pub async fn suggest_zooms(&self, capture_id: &str) -> Result<Vec<ZoomSuggestion>, AiError> {
        let v = self
            .transport
            .call("suggest_zooms", json!({ "capture_id": capture_id }))
            .await?;
        parse(
            "suggest_zooms",
            v.get("suggestions")
                .cloned()
                .unwrap_or(Value::Array(vec![])),
        )
    }

    pub async fn suggest_cuts(&self, capture_id: &str) -> Result<Vec<CutSuggestion>, AiError> {
        let v = self
            .transport
            .call("suggest_cuts", json!({ "capture_id": capture_id }))
            .await?;
        parse(
            "suggest_cuts",
            v.get("suggestions")
                .cloned()
                .unwrap_or(Value::Array(vec![])),
        )
    }
}

/// The real transport: the in-process daemon, called as the user.
pub struct DaemonTransport {
    pub daemon: cm_daemon::Daemon,
    pub rt: tokio::runtime::Handle,
}

#[async_trait]
impl AiTransport for DaemonTransport {
    async fn call(&self, method: &str, params: Value) -> Result<Value, AiError> {
        let daemon = self.daemon.clone();
        let method = method.to_owned();
        self.rt
            .spawn(async move {
                daemon
                    .execute_method(Actor::User, 0, &method, &params)
                    .await
                    .map_err(|e| {
                        let reason = match &e {
                            cm_daemon::DaemonError::WithData { data, .. } => data
                                .get("reason")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                            cm_daemon::DaemonError::Declined { reason, .. } => {
                                Some((*reason).to_owned())
                            }
                            _ => None,
                        };
                        AiError {
                            code: e.code().map(|c| c.as_str().to_owned()),
                            reason,
                            message: e.to_string(),
                        }
                    })
            })
            .await
            .unwrap_or_else(|e| Err(AiError::message(e.to_string())))
    }
}

type Answers = HashMap<String, VecDeque<Result<Value, AiError>>>;

/// A scripted transport for tests: answers are queued per method (the last one repeats) and every
/// call is recorded.
#[derive(Default)]
pub struct ScriptedTransport {
    answers: Mutex<Answers>,
    calls: Mutex<Vec<(String, Value)>>,
}

impl ScriptedTransport {
    pub fn new() -> Arc<ScriptedTransport> {
        Arc::new(ScriptedTransport::default())
    }

    pub fn answer(&self, method: &str, r: Result<Value, AiError>) {
        self.answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(method.to_owned())
            .or_default()
            .push_back(r);
    }

    pub fn calls(&self) -> Vec<(String, Value)> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn calls_of(&self, method: &str) -> Vec<Value> {
        self.calls()
            .into_iter()
            .filter(|(m, _)| m == method)
            .map(|(_, p)| p)
            .collect()
    }
}

#[async_trait]
impl AiTransport for ScriptedTransport {
    async fn call(&self, method: &str, params: Value) -> Result<Value, AiError> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((method.to_owned(), params));
        let mut a = self.answers.lock().unwrap_or_else(PoisonError::into_inner);
        match a.get_mut(method) {
            Some(q) if q.len() > 1 => q.pop_front().unwrap_or(Ok(Value::Null)),
            Some(q) => q.front().cloned().unwrap_or(Ok(Value::Null)),
            None => Err(AiError::message(format!("no scripted answer for {method}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::MediaTime;

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(f)
    }

    fn models_json() -> Value {
        json!({"models": [
            {"id": "base", "label": "Base", "size_mb": 148, "hint": "Fast", "installed": false,
             "active": false, "path": null}
        ], "active": null, "dir": "/models"})
    }

    /// AI-1 contract: `list_models` answers parse and the call carries no params.
    #[test]
    fn list_models_parses_the_contract_shape() {
        let t = ScriptedTransport::new();
        t.answer("list_models", Ok(models_json()));
        let c = AiClient::new(t.clone());
        let l = run(c.list_models()).unwrap();
        assert_eq!(l.models[0].id, "base");
        assert_eq!(l.active, None);
        assert_eq!(t.calls_of("list_models"), vec![json!({})]);
    }

    /// The download is a user action: it sends exactly `{model}` and returns the job id.
    #[test]
    fn download_sends_the_model_and_returns_the_job() {
        let t = ScriptedTransport::new();
        t.answer("download_model", Ok(json!({"job_id": "job_7"})));
        let c = AiClient::new(t.clone());
        assert_eq!(run(c.download_model("small")).unwrap(), "job_7");
        assert_eq!(
            t.calls_of("download_model"),
            vec![json!({"model": "small"})]
        );
    }

    #[test]
    fn transcribe_without_a_model_is_recognised() {
        let t = ScriptedTransport::new();
        t.answer(
            "transcribe",
            Err(AiError {
                code: Some("invalid_input".into()),
                reason: Some("no_model".into()),
                message: "Add a speech model first".into(),
            }),
        );
        let c = AiClient::new(t.clone());
        let e = run(c.transcribe("cap_1", None, None)).unwrap_err();
        assert!(e.is_no_model());
        assert_eq!(
            t.calls_of("transcribe"),
            vec![json!({"capture_id": "cap_1"})]
        );
    }

    #[test]
    fn transcribe_passes_language_and_model_when_given() {
        let t = ScriptedTransport::new();
        t.answer("transcribe", Ok(json!({"job_id": "j"})));
        let c = AiClient::new(t.clone());
        run(c.transcribe("cap_1", Some("vi"), Some("small"))).unwrap();
        assert_eq!(
            t.calls_of("transcribe"),
            vec![json!({"capture_id": "cap_1", "language": "vi", "model": "small"})]
        );
    }

    #[test]
    fn a_recording_without_a_transcript_is_none() {
        let t = ScriptedTransport::new();
        t.answer(
            "get_transcript",
            Err(AiError {
                code: Some("not_found".into()),
                reason: None,
                message: "no transcript".into(),
            }),
        );
        let c = AiClient::new(t);
        assert_eq!(run(c.get_transcript("cap_1")).unwrap(), None);
    }

    /// Regression: the real daemon wraps the transcript (`{ capture_id, transcript }`). Reading the
    /// wrapper as a transcript failed silently and the editor showed no captions after a
    /// successful transcription.
    #[test]
    fn the_daemons_wrapped_transcript_is_read() {
        let t = ScriptedTransport::new();
        let words = json!([{"text": "Hello", "start": 0, "end": 60000, "confidence": 0.9}]);
        let tr =
            json!({"version": 1, "language": "en", "model": "small", "words": words, "note": null});
        t.answer(
            "get_transcript",
            Ok(json!({"capture_id": "cap_1", "transcript": tr.clone()})),
        );
        t.answer("get_transcript", Ok(tr));
        let c = AiClient::new(t);
        let wrapped = run(c.get_transcript("cap_1")).unwrap().unwrap();
        assert_eq!(wrapped.words.len(), 1);
        assert_eq!(wrapped.language, "en");
        let bare = run(c.get_transcript("cap_1")).unwrap().unwrap();
        assert_eq!(bare, wrapped);
    }

    #[test]
    fn stored_captions_come_back_or_are_none() {
        let t = ScriptedTransport::new();
        t.answer(
            "get_captions",
            Ok(json!({"lines": [{"start": 120000, "end": 360000, "text": "Hi"}]})),
        );
        let c = AiClient::new(t.clone());
        let lines = run(c.get_captions("cap_1")).unwrap().unwrap();
        assert_eq!(lines[0].text, "Hi");
        // An older daemon without the action: no stored lines, not an error.
        let t2 = ScriptedTransport::new();
        t2.answer("get_captions", Err(AiError::message("method not found")));
        assert_eq!(run(AiClient::new(t2).get_captions("cap_1")).unwrap(), None);
        // An empty list is "none stored".
        let t3 = ScriptedTransport::new();
        t3.answer("get_captions", Ok(json!({"lines": []})));
        assert_eq!(run(AiClient::new(t3).get_captions("cap_1")).unwrap(), None);
    }

    #[test]
    fn captions_travel_as_session_ticks() {
        let t = ScriptedTransport::new();
        t.answer("set_captions", Ok(json!({"revision": 3})));
        let c = AiClient::new(t.clone());
        let lines = [CaptionLine {
            start: MediaTime::from_secs(1),
            end: MediaTime::from_secs(3),
            text: "Hello".into(),
        }];
        run(c.set_captions("cap_1", &lines)).unwrap();
        let p = &t.calls_of("set_captions")[0];
        assert_eq!(p["capture_id"], "cap_1");
        assert_eq!(
            p["lines"][0]["start"],
            format!("t:{}", MediaTime::from_secs(1).ticks()),
            "ticks go as `t:<ticks>`: a plain number would be read as seconds"
        );
        assert_eq!(p["lines"][0]["text"], "Hello");
    }

    #[test]
    fn export_returns_the_path_and_names_the_format() {
        let t = ScriptedTransport::new();
        t.answer("export_captions", Ok(json!({"path": "/x/Lesson.srt"})));
        let c = AiClient::new(t.clone());
        let p = run(c.export_captions("cap_1", CaptionFormat::Srt, true)).unwrap();
        assert_eq!(p, PathBuf::from("/x/Lesson.srt"));
        assert_eq!(
            t.calls_of("export_captions"),
            vec![json!({"capture_id": "cap_1", "format": "srt"})]
        );
    }

    /// TXT: `timestamps` is sent only for `txt` and only when on.
    #[test]
    fn txt_export_sends_timestamps_only_when_asked() {
        let t = ScriptedTransport::new();
        t.answer("export_captions", Ok(json!({"path": "/x/Lesson.txt"})));
        let c = AiClient::new(t.clone());
        run(c.export_captions("cap_1", CaptionFormat::Txt, false)).unwrap();
        run(c.export_captions("cap_1", CaptionFormat::Txt, true)).unwrap();
        assert_eq!(
            t.calls_of("export_captions"),
            vec![
                json!({"capture_id": "cap_1", "format": "txt"}),
                json!({"capture_id": "cap_1", "format": "txt", "timestamps": true}),
            ]
        );
    }

    /// The details page summary is read from the daemon's real `get_transcript` answer.
    #[test]
    fn the_transcript_summary_comes_from_the_daemons_answer() {
        let t = ScriptedTransport::new();
        let words: Vec<_> = (0..27)
            .map(|i| json!({"text": "w", "start": i * 100, "end": i * 100 + 50, "confidence": 0.9}))
            .collect();
        t.answer(
            "get_transcript",
            Ok(json!({"capture_id": "cap_1", "made_at": 1_792_000_000u64,
                "transcript": {"version": 1, "language": "en", "model": "small", "words": words},
                "captions_source": "edited",
                "caption_options": {"enabled": true, "burn_in": false}})),
        );
        t.answer(
            "get_transcript",
            Err(AiError {
                code: Some("not_found".into()),
                reason: None,
                message: "no transcript".into(),
            }),
        );
        let c = AiClient::new(t);
        let info = run(c.transcript_info("cap_1")).unwrap().unwrap();
        assert_eq!(
            info,
            TranscriptInfo {
                language: "en".into(),
                model: "small".into(),
                words: 27,
                made_at: Some(1_792_000_000),
                edited: true,
                options: CaptionOptionsInfo {
                    enabled: true,
                    burn_in: false,
                },
            }
        );
        // A recording without a transcript is `None`, not an error.
        assert_eq!(run(c.transcript_info("cap_2")).unwrap(), None);
    }

    /// The caption switches travel with the transcript, and `set_caption_options` sends only what
    /// changes and returns the switches as they are now.
    #[test]
    fn caption_options_are_read_with_the_transcript_and_set_by_name() {
        let t = ScriptedTransport::new();
        let tr = json!({"version": 1, "language": "en", "model": "small",
            "words": [{"text": "Hi", "start": 0, "end": 60000, "confidence": 0.9}], "note": null});
        t.answer(
            "get_transcript",
            Ok(json!({"capture_id": "cap_1", "transcript": tr,
                "caption_options": {"enabled": true, "burn_in": false}})),
        );
        t.answer(
            "get_transcript",
            Ok(json!({"capture_id": "cap_1", "transcript": tr})),
        );
        t.answer(
            "set_caption_options",
            Ok(json!({"capture_id": "cap_1", "enabled": true, "burn_in": true, "changed": true})),
        );
        let c = AiClient::new(t.clone());
        let (_, o) = run(c.get_transcript_with_options("cap_1"))
            .unwrap()
            .unwrap();
        assert_eq!(
            o,
            CaptionOptionsInfo {
                enabled: true,
                burn_in: false
            }
        );
        // An older answer without the field means both off.
        let (_, o) = run(c.get_transcript_with_options("cap_1"))
            .unwrap()
            .unwrap();
        assert_eq!(o, CaptionOptionsInfo::default());
        let now = run(c.set_caption_options("cap_1", None, Some(true))).unwrap();
        assert_eq!(
            now,
            CaptionOptionsInfo {
                enabled: true,
                burn_in: true
            }
        );
        assert_eq!(
            t.calls_of("set_caption_options"),
            vec![json!({"capture_id": "cap_1", "burn_in": true})],
            "only the changed switch is sent"
        );
    }

    #[test]
    fn suggestions_parse_and_an_empty_answer_is_an_empty_list() {
        let t = ScriptedTransport::new();
        t.answer("suggest_zooms", Ok(json!({"suggestions": []})));
        t.answer(
            "suggest_cuts",
            Ok(json!({"suggestions": [
                {"start": 1200000, "end": 2400000, "kind": "silence", "detail": "1.0 s of silence"}
            ]})),
        );
        let c = AiClient::new(t);
        assert!(run(c.suggest_zooms("cap_1")).unwrap().is_empty());
        let cuts = run(c.suggest_cuts("cap_1")).unwrap();
        assert_eq!(cuts[0].kind, "silence");
        assert_eq!(cuts[0].start, MediaTime::from_ticks(1_200_000));
    }

    /// VOC-2: the transcript's language and the speakers' names go out only when asked for, and
    /// the names only for SRT (the daemon refuses them for the other formats).
    #[test]
    fn export_captions_for_sends_language_and_speaker_names_only_when_asked() {
        let t = ScriptedTransport::new();
        t.answer("export_captions", Ok(json!({"path": "/x/a.srt"})));
        let c = AiClient::new(t.clone());
        run(c.export_captions("cap_1", CaptionFormat::Srt, false)).unwrap();
        run(c.export_captions_for("cap_1", CaptionFormat::Srt, false, Some("vi"), true)).unwrap();
        run(c.export_captions_for("cap_1", CaptionFormat::Vtt, false, None, true)).unwrap();
        let calls = t.calls_of("export_captions");
        assert_eq!(calls[0], json!({"capture_id": "cap_1", "format": "srt"}));
        assert_eq!(
            calls[1],
            json!({"capture_id": "cap_1", "format": "srt", "language": "vi", "speaker_names": true})
        );
        assert_eq!(calls[2], json!({"capture_id": "cap_1", "format": "vtt"}));
    }

    /// Seen on Oct 28, 2026: a long transcription the daemon picked up again after a restart was
    /// invisible in the editor. The window asks the daemon which AI jobs are active, and takes
    /// only the transcription, speaker and report jobs that are waiting, running or paused.
    #[test]
    fn the_active_voice_jobs_come_from_the_daemons_list() {
        let job = |id: &str, ty: &str, state: &str, cap: Option<&str>, p: f64| {
            json!({"job_id": id, "type": ty, "state": state, "progress": p,
                   "inputs": cap.map_or(json!({}), |c| json!({"capture_id": c}))})
        };
        let list = |ty: &str| match ty {
            "transcribe" => json!({"jobs": [
                job("job_1", "transcribe", "running", Some("rec_a"), 0.4),
                job("job_2", "transcribe", "queued", Some("rec_b"), 0.0),
                job("job_3", "transcribe", "done", Some("rec_a"), 1.0),
                job("job_4", "transcribe", "failed", Some("rec_c"), 0.2),
                job("job_5", "transcribe", "running", None, 0.1),
            ]}),
            "diarize" => json!({"jobs": [job("job_6", "diarize", "paused", Some("rec_a"), 0.7)]}),
            _ => json!({"jobs": [job("job_7", "report", "running", Some("rec_a"), 0.3),
                                  job("job_8", "export", "running", Some("rec_a"), 0.5)]}),
        };
        let t = ScriptedTransport::new();
        for ty in ["transcribe", "diarize", "report"] {
            t.answer("list_jobs", Ok(list(ty)));
        }
        let c = AiClient::new(t.clone());
        let jobs = run(c.active_voice_jobs());
        let summary: Vec<(String, crate::ai::jobs::ActiveKind, String)> = jobs
            .iter()
            .map(|j| (j.job_id.clone(), j.kind, j.capture_id.clone()))
            .collect();
        use crate::ai::jobs::ActiveKind::{Report, Speakers, Transcribe};
        assert_eq!(
            summary,
            vec![
                ("job_1".into(), Transcribe, "rec_a".into()),
                ("job_2".into(), Transcribe, "rec_b".into()),
                ("job_6".into(), Speakers, "rec_a".into()),
                ("job_7".into(), Report, "rec_a".into()),
            ],
            "only active jobs of the three kinds that name a recording"
        );
        assert_eq!(jobs[0].progress, 0.4);
        let asked = t.calls_of("list_jobs");
        assert_eq!(asked.len(), 3);
        assert_eq!(asked[0], json!({"type": "transcribe", "limit": 20}));
    }

    /// VOC-2: Translate sends the target language; Export audio sends its parameters and returns
    /// the job.
    #[test]
    fn translate_and_export_audio_return_their_jobs() {
        let t = ScriptedTransport::new();
        t.answer("translate_captions", Ok(json!({"job_id": "job_7"})));
        t.answer(
            "export_audio",
            Ok(json!({"job_id": "job_8", "state": "queued"})),
        );
        let c = AiClient::new(t.clone());
        assert_eq!(run(c.translate_captions("cap_1", "en")).unwrap(), "job_7");
        assert_eq!(
            t.calls_of("translate_captions"),
            vec![json!({"capture_id": "cap_1", "target_language": "en"})]
        );
        let p = json!({"capture_id": "cap_1", "format": "wav"});
        assert_eq!(run(c.export_audio(p.clone())).unwrap(), "job_8");
        assert_eq!(t.calls_of("export_audio"), vec![p]);
    }
    /// AI-8 and AI-9: the speaker and report calls send only what was chosen, and the typed
    /// "not available" reasons are told apart.
    #[test]
    fn speaker_and_report_calls_send_only_what_was_chosen() {
        let t = ScriptedTransport::new();
        t.answer("diarize", Ok(json!({"job_id": "job_1"})));
        t.answer(
            "set_speakers",
            Ok(json!({"changed": false, "speakers": []})),
        );
        t.answer("generate_report", Ok(json!({"job_id": "job_2"})));
        t.answer("export_report", Ok(json!({"path": "/x/r.md"})));
        let c = AiClient::new(t.clone());
        assert_eq!(run(c.diarize("cap_1", None, None, false)).unwrap(), "job_1");
        assert_eq!(
            run(c.diarize("cap_1", Some(3), None, true)).unwrap(),
            "job_1"
        );
        assert_eq!(
            t.calls_of("diarize"),
            vec![
                json!({"capture_id": "cap_1"}),
                json!({"capture_id": "cap_1", "num_speakers": 3, "headset": true})
            ]
        );
        let changed = run(c.set_speakers("cap_1", &[(2, "Linh".into())], &[(4, 3)])).unwrap();
        assert!(!changed);
        assert_eq!(
            t.calls_of("set_speakers"),
            vec![json!({"capture_id": "cap_1", "rename": {"2": "Linh"},
                "merge": [{"from": 4, "into": 3}]})]
        );
        run(c.generate_report(
            "cap_1",
            "minutes",
            None,
            json!({"title": "Sync"}),
            Some("rpt_00000001"),
        ))
        .unwrap();
        run(c.generate_report("cap_1", "summary", Some("vi"), json!({}), None)).unwrap();
        assert_eq!(
            t.calls_of("generate_report"),
            vec![
                json!({"capture_id": "cap_1", "template": "minutes",
                    "context": {"title": "Sync"}, "report_id": "rpt_00000001"}),
                json!({"capture_id": "cap_1", "template": "summary", "language": "vi"})
            ]
        );
        assert_eq!(
            run(c.export_report("cap_1", "rpt_00000001", "md", false)).unwrap(),
            PathBuf::from("/x/r.md")
        );
        let no_model = AiError {
            code: Some("invalid_input".into()),
            reason: Some("no_speaker_model".into()),
            message: String::new(),
        };
        assert!(no_model.is_no_speaker_model() && !no_model.is_no_text_model());
    }
}
