//! A scripted engine for the AI actions: a model store that "downloads" in small steps, a
//! transcriber that returns a fixed transcript, naive captions, and fixed suggestions. Tests share
//! one [`FakeAi`] with the daemon ([`FakeAi::services`]) and read what was asked of it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use cm_ai::caption::CaptionLine;
use cm_ai::model::{DownloadProgress, ModelInfo, ModelList};
use cm_ai::suggest::{CutSuggestion, ZoomSuggestion};
use cm_ai::transcribe::{Transcript, Word};
use cm_core::{CmError, EditList, MediaTime};

use crate::ai::{
    AiServices, CaptionFormat, CaptionService, ModelService, SuggestService, TranscribeRequest,
    TranscribeService, TranslateOutput, TranslateRequest, TranslateService,
};

struct Model {
    id: &'static str,
    label: &'static str,
    size_mb: u32,
    installed: bool,
}

struct State {
    models: Vec<Model>,
    active: Option<String>,
    /// Downloads advance in this many steps of `step_ms` each (0 = at once).
    download_steps: u32,
    step_ms: u64,
    downloads_started: Vec<String>,
    transcribe_calls: Vec<TranscribeRequest>,
    render_calls: Vec<(CaptionFormat, usize)>,
    fail_transcribe: Option<CmError>,
    /// Words every transcription returns instead of the built-in ones.
    words: Option<Vec<Word>>,
    translate_calls: Vec<TranslateRequest>,
    /// What the Whisper translate task returns (English words), `None` = a fixed sentence.
    translation: Option<Vec<Word>>,
}

/// The shared fake. Cheap to clone.
#[derive(Clone)]
pub struct FakeAi(Arc<Mutex<State>>);

impl Default for FakeAi {
    fn default() -> Self {
        FakeAi::new()
    }
}

fn t(s: f64) -> MediaTime {
    MediaTime::from_secs_f64(s).unwrap_or(MediaTime::ZERO)
}

impl FakeAi {
    /// Three models (`base`, `small`, `medium`), none installed.
    pub fn new() -> FakeAi {
        let m = |id, label, size_mb| Model {
            id,
            label,
            size_mb,
            installed: false,
        };
        FakeAi(Arc::new(Mutex::new(State {
            models: vec![
                m("base", "Base", 142),
                m("small", "Small", 466),
                m("medium", "Medium", 1500),
            ],
            active: None,
            download_steps: 0,
            step_ms: 0,
            downloads_started: vec![],
            transcribe_calls: vec![],
            render_calls: vec![],
            fail_transcribe: None,
            words: None,
            translate_calls: vec![],
            translation: None,
        })))
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `small` is installed and active.
    pub fn with_small_installed(self) -> FakeAi {
        {
            let mut s = self.state();
            for m in &mut s.models {
                if m.id == "small" {
                    m.installed = true;
                }
            }
            s.active = Some("small".into());
        }
        self
    }

    /// These models are installed (and `active` is the active one).
    pub fn with_installed(self, ids: &[&str], active: Option<&str>) -> FakeAi {
        {
            let mut s = self.state();
            // The large models of AI-5 are added on demand, so the default list stays three.
            for (id, label, size_mb) in [
                ("large", "Large v3", 3100),
                ("large-v3-turbo", "Large v3 Turbo", 1600),
            ] {
                if ids.contains(&id) && !s.models.iter().any(|m| m.id == id) {
                    s.models.push(Model {
                        id,
                        label,
                        size_mb,
                        installed: false,
                    });
                }
            }
            for m in &mut s.models {
                m.installed = ids.contains(&m.id);
            }
            s.active = active.map(str::to_owned);
        }
        self
    }

    /// The Whisper translate task returns these words from now on.
    pub fn set_translation(&self, words: Vec<Word>) {
        self.state().translation = Some(words);
    }

    /// Every translation run so far.
    pub fn translate_calls(&self) -> Vec<TranslateRequest> {
        self.state().translate_calls.clone()
    }

    /// Downloads take `steps` steps of `step_ms` milliseconds (so a test can cancel one).
    pub fn slow_downloads(self, steps: u32, step_ms: u64) -> FakeAi {
        {
            let mut s = self.state();
            s.download_steps = steps;
            s.step_ms = step_ms;
        }
        self
    }

    /// From now on runs (downloads and transcriptions) take `steps` steps of `step_ms` each.
    pub fn set_slow(&self, steps: u32, step_ms: u64) {
        let mut s = self.state();
        s.download_steps = steps;
        s.step_ms = step_ms;
    }

    /// The next transcription fails with this error.
    pub fn fail_transcribe_with(&self, e: CmError) {
        self.state().fail_transcribe = Some(e);
    }

    /// From now on every transcription returns these words.
    pub fn set_words(&self, words: Vec<Word>) {
        self.state().words = Some(words);
    }

    pub fn downloads_started(&self) -> Vec<String> {
        self.state().downloads_started.clone()
    }

    pub fn transcribe_calls(&self) -> Vec<TranscribeRequest> {
        self.state().transcribe_calls.clone()
    }

    /// `(format, number of lines)` of every render.
    pub fn render_calls(&self) -> Vec<(CaptionFormat, usize)> {
        self.state().render_calls.clone()
    }

    pub fn installed(&self) -> Vec<String> {
        self.state()
            .models
            .iter()
            .filter(|m| m.installed)
            .map(|m| m.id.to_owned())
            .collect()
    }

    pub fn active(&self) -> Option<String> {
        self.state().active.clone()
    }

    /// The services the daemon takes ([`crate::DaemonBuilder::ai`]).
    pub fn services(&self) -> AiServices {
        AiServices {
            models: Arc::new(self.clone()),
            transcribe: Arc::new(self.clone()),
            captions: Arc::new(self.clone()),
            suggest: Arc::new(self.clone()),
            translate: Arc::new(self.clone()),
        }
    }

    /// The transcript every transcription returns.
    pub fn transcript(model: &str) -> Transcript {
        let w = |text: &str, a: f64, b: f64| Word {
            text: text.to_owned(),
            start: t(a),
            end: t(b),
            confidence: 0.9,
        };
        Transcript {
            version: 1,
            language: "en".into(),
            model: model.to_owned(),
            words: vec![
                w("Hello", 1.0, 1.4),
                w("and", 1.5, 1.7),
                w("welcome", 1.8, 2.3),
                w("to", 2.4, 2.5),
                w("the", 2.6, 2.7),
                w("demo", 2.8, 3.4),
            ],
            note: None,
        }
    }
}

impl ModelService for FakeAi {
    fn list(&self) -> Result<ModelList, CmError> {
        let s = self.state();
        Ok(ModelList {
            models: s
                .models
                .iter()
                .map(|m| ModelInfo {
                    id: m.id.to_owned(),
                    label: m.label.to_owned(),
                    size_mb: m.size_mb,
                    ram_mb: 0,
                    hint: format!("{} model", m.label),
                    installed: m.installed,
                    active: s.active.as_deref() == Some(m.id),
                    path: m.installed.then(|| format!("/models/ggml-{}.bin", m.id)),
                })
                .collect(),
            active: s.active.clone(),
            dir: "/models".to_owned(),
        })
    }

    fn download(
        &self,
        model: &str,
        progress: &mut dyn FnMut(DownloadProgress) -> bool,
    ) -> Result<bool, CmError> {
        let (steps, step_ms) = {
            let mut s = self.state();
            s.downloads_started.push(model.to_owned());
            (s.download_steps.max(1), s.step_ms)
        };
        for i in 1..=steps {
            if step_ms > 0 {
                std::thread::sleep(Duration::from_millis(step_ms));
            }
            let keep_going = progress(DownloadProgress {
                model: model.to_owned(),
                bytes_done: u64::from(i),
                bytes_total: u64::from(steps),
            });
            if !keep_going {
                return Ok(false);
            }
        }
        let mut s = self.state();
        if let Some(m) = s.models.iter_mut().find(|m| m.id == model) {
            m.installed = true;
        }
        if s.active.is_none() {
            s.active = Some(model.to_owned());
        }
        Ok(true)
    }

    fn import(&self, model: &str, _path: &Path) -> Result<(), CmError> {
        let mut s = self.state();
        let m = s
            .models
            .iter_mut()
            .find(|m| m.id == model)
            .ok_or_else(|| CmError::invalid_field("model", "unknown model"))?;
        m.installed = true;
        if s.active.is_none() {
            s.active = Some(model.to_owned());
        }
        Ok(())
    }

    fn select(&self, model: &str) -> Result<(), CmError> {
        self.state().active = Some(model.to_owned());
        Ok(())
    }

    fn remove(&self, model: &str) -> Result<(), CmError> {
        let mut s = self.state();
        if let Some(m) = s.models.iter_mut().find(|m| m.id == model) {
            m.installed = false;
        }
        if s.active.as_deref() == Some(model) {
            s.active = None;
        }
        Ok(())
    }
}

impl TranscribeService for FakeAi {
    fn transcribe(
        &self,
        req: &TranscribeRequest,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<Transcript>, CmError> {
        let (steps, step_ms, fail, words) = {
            let mut s = self.state();
            s.transcribe_calls.push(req.clone());
            (
                s.download_steps.max(4),
                s.step_ms,
                s.fail_transcribe.take(),
                s.words.clone(),
            )
        };
        for i in 1..=steps {
            if step_ms > 0 {
                std::thread::sleep(Duration::from_millis(step_ms));
            }
            if !progress(i as f32 / steps as f32) {
                return Ok(None);
            }
        }
        if let Some(e) = fail {
            return Err(e);
        }
        let mut t = FakeAi::transcript(&req.model);
        if let Some(w) = words {
            t.words = w;
        }
        Ok(Some(t))
    }
}

impl TranslateService for FakeAi {
    fn whisper(
        &self,
        req: &TranslateRequest,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<TranslateOutput>, CmError> {
        let (words, steps, step_ms) = {
            let mut s = self.state();
            s.translate_calls.push(req.clone());
            (s.translation.clone(), s.download_steps.max(4), s.step_ms)
        };
        for i in 1..=steps {
            if step_ms > 0 {
                std::thread::sleep(Duration::from_millis(step_ms));
            }
            if !progress(i as f32 / steps as f32) {
                return Ok(None);
            }
        }
        let w = |text: &str, a: f64, b: f64| Word {
            text: text.to_owned(),
            start: t(a),
            end: t(b),
            confidence: 0.9,
        };
        let words = words.unwrap_or_else(|| {
            vec![
                w("Hello", 1.0, 1.4),
                w("and", 1.5, 1.7),
                w("welcome", 1.8, 2.3),
                w("to", 2.4, 2.5),
                w("the", 2.6, 2.7),
                w("demo.", 2.8, 3.4),
            ]
        });
        let english = Transcript {
            version: 1,
            language: "en".into(),
            model: req.model.clone(),
            words,
            note: None,
        };
        Ok(Some(TranslateOutput {
            lines: cm_ai::translate::whisper_lines(&english, &req.source_lines),
            model: req.model.clone(),
            untranslated: 0,
        }))
    }
}

impl CaptionService for FakeAi {
    fn lines(&self, transcript: &Transcript) -> Result<Vec<CaptionLine>, CmError> {
        // Three words to a line.
        Ok(transcript
            .words
            .chunks(3)
            .filter_map(|c| {
                Some(CaptionLine {
                    start: c.first()?.start,
                    end: c.last()?.end,
                    text: c
                        .iter()
                        .map(|w| w.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                })
            })
            .collect())
    }

    fn render(
        &self,
        lines: &[CaptionLine],
        format: CaptionFormat,
        _edits: &EditList,
    ) -> Result<String, CmError> {
        self.state().render_calls.push((format, lines.len()));
        if format.is_text() {
            // The fake skips the cut mapping; the real service applies it (see ai_engine).
            let mut out = String::new();
            for l in lines {
                if format == CaptionFormat::TxtTimed {
                    let secs = l.start.ticks() / 120_000;
                    out.push_str(&format!("[{:02}:{:02}] ", secs / 60, secs % 60));
                }
                out.push_str(&l.text.replace('\n', " "));
                out.push('\n');
            }
            return Ok(out);
        }
        let stamp = |m: MediaTime| {
            let ms = (m.ticks() as f64 / 120.0).round() as i64;
            let sep = if format == CaptionFormat::Srt {
                ','
            } else {
                '.'
            };
            format!(
                "{:02}:{:02}:{:02}{sep}{:03}",
                ms / 3_600_000,
                ms / 60_000 % 60,
                ms / 1000 % 60,
                ms % 1000
            )
        };
        let mut out = String::new();
        if format == CaptionFormat::Vtt {
            out.push_str("WEBVTT\n\n");
        }
        for (i, l) in lines.iter().enumerate() {
            if format == CaptionFormat::Srt {
                out.push_str(&format!("{}\n", i + 1));
            }
            out.push_str(&format!(
                "{} --> {}\n{}\n\n",
                stamp(l.start),
                stamp(l.end),
                l.text
            ));
        }
        Ok(out)
    }
}

impl SuggestService for FakeAi {
    fn zooms(&self, _bundle_dir: &Path) -> Result<Vec<ZoomSuggestion>, CmError> {
        Ok(vec![ZoomSuggestion {
            start: t(10.0),
            end: t(14.0),
            focus: (0.4, 0.6),
            scale: 2.0,
            reason: "click".into(),
        }])
    }

    fn cuts(
        &self,
        _bundle_dir: &Path,
        transcript: Option<&Transcript>,
    ) -> Result<Vec<CutSuggestion>, CmError> {
        let mut v = vec![CutSuggestion {
            start: t(20.0),
            end: t(23.0),
            kind: "silence".into(),
            detail: "3.0 s of silence".into(),
        }];
        if transcript.is_some() {
            v.push(CutSuggestion {
                start: t(30.0),
                end: t(30.3),
                kind: "filler".into(),
                detail: "um".into(),
            });
        }
        Ok(v)
    }
}

/// A path that exists in tests for `import_model` (any file works for the fake).
pub fn some_file(dir: &Path) -> PathBuf {
    let p = dir.join("ggml-small.bin");
    let _ = std::fs::write(&p, b"model bytes");
    p
}
