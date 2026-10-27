//! Caption translation (AI-5): the stored translations, the engine and model choice, Whisper's
//! translate task, and the pure line helpers a text model's output needs.
//!
//! * `translations/<lang>.json` in a bundle holds [`Translation`]: language, engine, model,
//!   created, lines (`start`, `end`, `text`) and `edited`. Each line also remembers the **source
//!   line** it was made from (`source`) so a hand edit can survive a re-translate when that source
//!   line is unchanged (AI-5.AC5, [`carry_over_edits`]). The extra line fields are additive.
//! * Two engines. **Whisper** re-runs the audio with the translate task (English only, offline);
//!   its output is cut into lines by the AI-1 rules ([`whisper_lines`]). A **text model** translates
//!   the source lines in batches ([`TextTranslator`], [`translate_with_text_model`]); the provider
//!   itself is AI-7 and is not part of this module: the trait is the seam.
//! * `large-v3-turbo` was not trained for translation and returns the source language, so it is
//!   never used for the task ([`choose_translation_model`]).
//! * [`fit_text`] re-wraps a translated line to 2 rows of 42 characters and 7 s and splits it at a
//!   word boundary, dividing the time by character count, when it does not fit.

use std::fs;
use std::path::{Path, PathBuf};

use cm_core::{CmError, MediaTime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::caption::{build_captions, rows_for, CaptionLine, MAX_LINE_SECS, MAX_ROWS};
use crate::recording::{translate_recording, TranscribeOptions};
use crate::textmatch;
use crate::transcribe::{Transcriber, Transcript};

/// The folder of translations inside a bundle.
pub const TRANSLATIONS_DIR: &str = "translations";
/// `translations/<lang>.json` format version.
pub const TRANSLATION_VERSION: u32 = 1;
/// Engine names as stored.
pub const ENGINE_WHISPER: &str = "whisper";
pub const ENGINE_TEXT_MODEL: &str = "text_model";
/// A stored line is kept over a new one when their spans overlap by at least this share of the
/// shorter one (the same rule as [`crate::reapply`]).
pub const MIN_OVERLAP: f64 = 0.5;

fn is_false(b: &bool) -> bool {
    !*b
}

/// One translated line, in session time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranslationLine {
    pub start: MediaTime,
    pub end: MediaTime,
    /// Rows joined by `\n`.
    pub text: String,
    /// The text of the source caption line this one was made from (the one it overlaps most).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The user (or an agent) changed this line by hand.
    #[serde(default, skip_serializing_if = "is_false")]
    pub edited: bool,
    /// The text model did not return this line (twice): `text` is the source text.
    #[serde(default, skip_serializing_if = "is_false")]
    pub untranslated: bool,
}

impl TranslationLine {
    pub fn to_caption(&self) -> CaptionLine {
        CaptionLine {
            start: self.start,
            end: self.end,
            text: self.text.clone(),
        }
    }
}

/// `translations/<lang>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Translation {
    #[serde(default = "version_one")]
    pub version: u32,
    pub language: String,
    /// `whisper` or `text_model`.
    pub engine: String,
    /// The Whisper model id, or the text model's name.
    pub model: String,
    /// RFC 3339, UTC.
    pub created: String,
    pub lines: Vec<TranslationLine>,
    /// Any line was edited by hand.
    pub edited: bool,
}

fn version_one() -> u32 {
    TRANSLATION_VERSION
}

impl Translation {
    /// The lines as caption lines (for SRT, VTT and TXT).
    pub fn captions(&self) -> Vec<CaptionLine> {
        self.lines.iter().map(TranslationLine::to_caption).collect()
    }

    /// Lines the text model did not return.
    pub fn untranslated(&self) -> usize {
        self.lines.iter().filter(|l| l.untranslated).count()
    }

    fn refresh_edited(&mut self) {
        self.edited = self.lines.iter().any(|l| l.edited);
    }
}

// ---- language codes and files ---------------------------------------------------------------

/// A language code made safe to use as a file name: lowercase, `-` between subtags, `en`, `vi`,
/// `pt-br`. `invalid_input` otherwise (`auto` is not a target).
pub fn language_code(raw: &str) -> Result<String, CmError> {
    let l = raw.trim().to_ascii_lowercase().replace('_', "-");
    let mut parts = l.split('-');
    let head_ok = parts
        .next()
        .is_some_and(|h| (2..=3).contains(&h.len()) && h.chars().all(|c| c.is_ascii_lowercase()));
    let tail_ok =
        parts.all(|t| (2..=8).contains(&t.len()) && t.chars().all(|c| c.is_ascii_alphanumeric()));
    if head_ok && tail_ok && l.len() <= 16 {
        Ok(l)
    } else {
        Err(CmError::invalid_field(
            "language",
            "must be a language code such as `en`, `vi`, `es` or `pt-br`",
        ))
    }
}

/// The part of a code before the first `-`: `pt-br` and `pt` are the same language for the
/// "already in that language" check.
pub fn base_language(code: &str) -> &str {
    code.split('-').next().unwrap_or(code)
}

/// `<bundle>/translations/<lang>.json` (the code must come from [`language_code`]).
pub fn translation_path(bundle: &Path, language: &str) -> PathBuf {
    bundle
        .join(TRANSLATIONS_DIR)
        .join(format!("{language}.json"))
}

/// Reads one translation; `None` when there is none for that language.
pub fn read_translation(bundle: &Path, language: &str) -> Result<Option<Translation>, CmError> {
    let path = translation_path(bundle, language);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(CmError::internal(format!("{}: {e}", path.display()))),
    };
    let t: Translation = serde_json::from_str(&text)
        .map_err(|e| CmError::internal(format!("{} is damaged: {e}", path.display())))?;
    if t.version > TRANSLATION_VERSION {
        return Err(CmError::invalid_input(format!(
            "{} was written by a newer CaptureMann (format {})",
            path.display(),
            t.version
        )));
    }
    Ok(Some(t))
}

/// Writes a translation atomically (a temporary file, then a rename). Returns the path.
pub fn write_translation(bundle: &Path, t: &Translation) -> Result<PathBuf, CmError> {
    let dir = bundle.join(TRANSLATIONS_DIR);
    fs::create_dir_all(&dir).map_err(|e| CmError::internal(format!("{}: {e}", dir.display())))?;
    let path = translation_path(bundle, &t.language);
    let tmp = dir.join(format!("{}.json.tmp", t.language));
    let body =
        serde_json::to_vec_pretty(t).map_err(|e| CmError::internal(format!("translation: {e}")))?;
    fs::write(&tmp, body).map_err(|e| CmError::internal(format!("{}: {e}", tmp.display())))?;
    fs::rename(&tmp, &path).map_err(|e| CmError::internal(format!("{}: {e}", path.display())))?;
    Ok(path)
}

/// The languages a bundle has translations for, sorted.
pub fn list_translations(bundle: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(bundle.join(TRANSLATIONS_DIR))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".json").map(str::to_owned)
        })
        .collect();
    out.sort();
    out
}

// ---- engine and model choice ----------------------------------------------------------------

/// What the caller asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineChoice {
    Auto,
    Whisper,
    TextModel,
}

impl EngineChoice {
    pub fn parse(s: &str) -> Result<EngineChoice, CmError> {
        match s {
            "auto" => Ok(EngineChoice::Auto),
            "whisper" => Ok(EngineChoice::Whisper),
            "text_model" | "text-model" => Ok(EngineChoice::TextModel),
            _ => Err(CmError::invalid_field(
                "engine",
                "must be `auto`, `whisper` or `text_model`",
            )),
        }
    }
}

/// The engine that will run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Whisper,
    TextModel,
}

impl Engine {
    pub fn as_str(self) -> &'static str {
        match self {
            Engine::Whisper => ENGINE_WHISPER,
            Engine::TextModel => ENGINE_TEXT_MODEL,
        }
    }
}

const NO_TEXT_MODEL: &str =
    "No text model is set up, so this recording cannot be translated into that language. \
     Whisper only translates into English.";
const TRANSLATION_MODEL_MISSING: &str =
    "Translating to English with Whisper needs the large-v3 or the medium model, and neither is \
     installed. large-v3-turbo cannot translate. Open the AI page to download one; nothing is \
     downloaded automatically.";
const SAME_LANGUAGE: &str = "The recording is already in that language";

/// `invalid_input` for a target no engine can reach (`data.reason = "no_text_model"`).
pub fn no_text_model() -> CmError {
    CmError::invalid_input(NO_TEXT_MODEL)
}

/// `invalid_input` for English with only the turbo model (or none that translates) installed
/// (`data.reason = "translation_model_missing"`).
pub fn translation_model_missing() -> CmError {
    CmError::invalid_input(TRANSLATION_MODEL_MISSING)
}

/// `invalid_input` for a target equal to the recording's language (`data.reason = "same_language"`).
pub fn same_language(language: &str) -> CmError {
    CmError::invalid_input(format!("{SAME_LANGUAGE} (`{language}`)"))
}

/// The error for naming a model that cannot translate.
pub fn turbo_cannot_translate(model: &str) -> CmError {
    CmError::invalid_input(format!(
        "The `{model}` model was not trained for translation and returns the spoken language. \
         Use large-v3 or medium."
    ))
}

/// The `data.reason` of the errors above, `None` for any other error.
pub fn error_reason(e: &CmError) -> Option<&'static str> {
    let CmError::InvalidInput { message } = e else {
        return None;
    };
    if message == NO_TEXT_MODEL {
        Some("no_text_model")
    } else if message == TRANSLATION_MODEL_MISSING {
        Some("translation_model_missing")
    } else if message.starts_with(SAME_LANGUAGE) {
        Some("same_language")
    } else if message.contains("was not trained for translation") {
        Some("translation_model_missing")
    } else {
        None
    }
}

/// The JSON-RPC `error.data` of an error above.
pub fn error_data(e: &CmError) -> Option<Value> {
    error_reason(e).map(|r| json!({ "reason": r }))
}

/// Whether a Whisper model id can run the translate task: every model except the turbo one.
pub fn model_can_translate(model_id: &str) -> bool {
    !model_id.to_ascii_lowercase().contains("turbo")
}

/// Picks the engine. Whisper translates into English only; everything else needs a text model
/// (`no_text_model` when `has_text_model` is false).
///
/// * `auto`: English target and a source that is not English: Whisper; any other target: the text
///   model.
/// * `whisper`: English target only.
/// * `text_model`: always the text model.
///
/// `source` is the recording's language (`transcript.language`). A target equal to the source
/// fails with `same_language`.
pub fn choose_engine(
    choice: EngineChoice,
    target: &str,
    source: &str,
    has_text_model: bool,
) -> Result<Engine, CmError> {
    if base_language(target) == base_language(source) {
        return Err(same_language(source));
    }
    let english = base_language(target) == "en";
    let engine = match choice {
        EngineChoice::Auto if english => Engine::Whisper,
        EngineChoice::Auto | EngineChoice::TextModel => Engine::TextModel,
        EngineChoice::Whisper if english => Engine::Whisper,
        EngineChoice::Whisper => {
            return Err(CmError::invalid_field(
                "engine",
                "Whisper translates into English only; use `text_model` for other languages",
            ))
        }
    };
    if engine == Engine::TextModel && !has_text_model {
        return Err(no_text_model());
    }
    Ok(engine)
}

/// The Whisper model for the translate task, from the installed model ids (as the model list
/// names them; `large` is large-v3).
///
/// * A named model must be installed and must be able to translate.
/// * Otherwise the active model, when it can translate (small and base work, with lower quality).
/// * Otherwise (the active model is turbo, or none is active): `large-v3`, else `medium`, else
///   `translation_model_missing`. Nothing is downloaded.
pub fn choose_translation_model(
    installed: &[String],
    active: Option<&str>,
    requested: Option<&str>,
) -> Result<String, CmError> {
    let has = |id: &str| installed.iter().any(|m| m == id);
    if let Some(r) = requested.filter(|r| !r.is_empty()) {
        if !model_can_translate(r) {
            return Err(turbo_cannot_translate(r));
        }
        return if has(r) {
            Ok(r.to_owned())
        } else {
            Err(translation_model_missing())
        };
    }
    if let Some(a) = active.filter(|a| has(a) && model_can_translate(a)) {
        return Ok(a.to_owned());
    }
    for id in ["large", "large-v3", "medium"] {
        if has(id) {
            return Ok(id.to_owned());
        }
    }
    Err(translation_model_missing())
}

// ---- lines ----------------------------------------------------------------------------------

fn overlap_secs(a: (MediaTime, MediaTime), b: (MediaTime, MediaTime)) -> f64 {
    let start = a.0.max(b.0);
    let end = a.1.min(b.1);
    (end - start).as_secs_f64().max(0.0)
}

fn overlap_share(a: (MediaTime, MediaTime), b: (MediaTime, MediaTime)) -> f64 {
    let shorter = (a.1 - a.0).as_secs_f64().min((b.1 - b.0).as_secs_f64());
    if shorter <= 0.0 {
        // A zero-length line overlaps when it lies inside the other one.
        return f64::from(u8::from(a.0 < b.1 && b.0 < a.1));
    }
    overlap_secs(a, b) / shorter
}

/// The source caption line a span overlaps most (`None` when it overlaps none).
pub fn best_source(
    source: &[CaptionLine],
    start: MediaTime,
    end: MediaTime,
) -> Option<&CaptionLine> {
    source
        .iter()
        .map(|s| (overlap_secs((s.start, s.end), (start, end)), s))
        .filter(|(o, _)| *o > 0.0)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, s)| s)
}

fn text_key(s: &str) -> String {
    textmatch::nfc(&s.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Re-wraps a translated line to the AI-1 limits. When it does not fit in 2 rows of 42 characters
/// and 7 s, it is split in two at the word boundary nearest to the middle, the time is divided by
/// character count, and each half is checked again. A single word is never split.
pub fn fit_text(start: MediaTime, end: MediaTime, text: &str) -> Vec<CaptionLine> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out = Vec::new();
    if !words.is_empty() {
        split_into(&words, start, end.max(start), &mut out);
    }
    out
}

fn char_len(words: &[&str]) -> usize {
    words.iter().map(|w| w.chars().count()).sum::<usize>() + words.len().saturating_sub(1)
}

fn split_into(words: &[&str], start: MediaTime, end: MediaTime, out: &mut Vec<CaptionLine>) {
    let joined = words.join(" ");
    let rows = rows_for(&joined);
    let fits = rows.len() <= MAX_ROWS && (end - start).as_secs_f64() <= MAX_LINE_SECS;
    if fits || words.len() < 2 {
        out.push(CaptionLine {
            start,
            end,
            text: rows.join("\n"),
        });
        return;
    }
    let total = char_len(words);
    // The split after word `i` that leaves the halves closest to equal in characters.
    let at = (1..words.len())
        .min_by_key(|&i| char_len(&words[..i]).abs_diff(total / 2))
        .unwrap_or(1);
    let left_chars = char_len(&words[..at]);
    let span = (end - start).ticks();
    let mid = MediaTime::from_ticks(
        start.ticks() + (span as f64 * left_chars as f64 / (total.max(1)) as f64).round() as i64,
    );
    split_into(&words[..at], start, mid, out);
    split_into(&words[at..], mid, end, out);
}

/// Whisper's translated words as caption lines by the AI-1 rules, each remembering its source line.
pub fn whisper_lines(translated: &Transcript, source: &[CaptionLine]) -> Vec<TranslationLine> {
    build_captions(&translated.words)
        .into_iter()
        .map(|l| TranslationLine {
            source: best_source(source, l.start, l.end).map(|s| s.text.clone()),
            start: l.start,
            end: l.end,
            text: l.text,
            edited: false,
            untranslated: false,
        })
        .collect()
}

/// What [`carry_over_edits`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Carried {
    /// Hand edits put back into the new translation.
    pub kept: usize,
    /// Hand edits whose source line changed or vanished.
    pub dropped: usize,
}

/// Puts the hand edits of `old` into `new` where their **source line is unchanged**: the new line
/// has the same source text (whitespace-normalised NFC) and overlaps the edited line by
/// [`MIN_OVERLAP`] of the shorter span. Each new line takes at most one edit. An edit whose source
/// changed is dropped and counted.
pub fn carry_over_edits(old: &Translation, new: &mut [TranslationLine]) -> Carried {
    let mut taken = vec![false; new.len()];
    let mut out = Carried::default();
    for o in old.lines.iter().filter(|l| l.edited) {
        let key = o.source.as_deref().map(text_key);
        let hit = key.as_ref().and_then(|k| {
            new.iter().enumerate().position(|(j, n)| {
                !taken[j]
                    && n.source.as_deref().map(text_key).as_ref() == Some(k)
                    && overlap_share((o.start, o.end), (n.start, n.end)) >= MIN_OVERLAP
            })
        });
        match hit {
            Some(j) => {
                taken[j] = true;
                new[j].text = o.text.clone();
                new[j].edited = true;
                new[j].untranslated = false;
                out.kept += 1;
            }
            None => out.dropped += 1,
        }
    }
    out
}

/// A finished translation from fresh lines: the old file's hand edits are carried over
/// (AI-5.AC5). `created` is an RFC 3339 time.
pub fn assemble(
    language: &str,
    engine: Engine,
    model: &str,
    created: &str,
    mut lines: Vec<TranslationLine>,
    old: Option<&Translation>,
) -> (Translation, Carried) {
    let carried = old.map_or(Carried::default(), |o| carry_over_edits(o, &mut lines));
    let mut t = Translation {
        version: TRANSLATION_VERSION,
        language: language.to_owned(),
        engine: engine.as_str().to_owned(),
        model: model.to_owned(),
        created: created.to_owned(),
        lines,
        edited: false,
    };
    t.refresh_edited();
    (t, carried)
}

/// The translation after the user (or an agent) replaced its lines (`set_captions` with a
/// language). A line that keeps its start and its text keeps its record; a line with the same start
/// and new text becomes `edited`; any other line is new and `edited`, with the source line it now
/// overlaps most.
pub fn with_edited_lines(
    old: &Translation,
    lines: &[CaptionLine],
    source: &[CaptionLine],
) -> Translation {
    let new_lines = lines
        .iter()
        .map(|l| match old.lines.iter().find(|o| o.start == l.start) {
            Some(o) => TranslationLine {
                start: l.start,
                end: l.end,
                text: l.text.clone(),
                source: o.source.clone(),
                edited: o.edited || o.text != l.text,
                untranslated: o.untranslated && o.text == l.text,
            },
            None => TranslationLine {
                start: l.start,
                end: l.end,
                text: l.text.clone(),
                source: best_source(source, l.start, l.end).map(|s| s.text.clone()),
                edited: true,
                untranslated: false,
            },
        })
        .collect();
    let mut t = Translation {
        lines: new_lines,
        ..old.clone()
    };
    t.refresh_edited();
    t
}

// ---- Whisper ---------------------------------------------------------------------------------

/// Runs Whisper's translate task on a recording's audio and returns English lines (AI-1 rules).
/// `source_language` is the spoken language (a hint). The transcriber is already loaded with the
/// model [`choose_translation_model`] picked. Nothing is stored.
pub fn translate_with_whisper(
    bundle: &Path,
    transcriber: &dyn Transcriber,
    source_language: Option<&str>,
    model_id: &str,
    source: &[CaptionLine],
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<Vec<TranslationLine>, CmError> {
    let opts = TranscribeOptions {
        language: source_language,
        include_system: false,
        model_id: Some(model_id),
        prompt: None,
    };
    let t = translate_recording(bundle, transcriber, &opts, progress)?;
    Ok(whisper_lines(&t, source))
}

// ---- text model ------------------------------------------------------------------------------

/// Source lines per request to a text model, and the lines of context on each side.
pub const BATCH_LINES: usize = 20;
pub const CONTEXT_LINES: usize = 3;

/// One batch for a text model.
#[derive(Debug, Clone, PartialEq)]
pub struct TextTranslateRequest<'a> {
    pub source_language: &'a str,
    pub target_language: &'a str,
    /// The lines to translate, in order.
    pub lines: &'a [String],
    /// Lines just before and after, for context only (do not translate them).
    pub before: &'a [String],
    pub after: &'a [String],
    /// Glossary terms to leave as written.
    pub keep_as_written: &'a [String],
}

/// The seam for the AI-7 text model. An implementation returns exactly one translation per line
/// of `req.lines`, in order; a different count counts as a failed batch (one retry, then the lines
/// stay in the source language, flagged). Provider failures are errors and stop the job.
pub trait TextTranslator: Send + Sync {
    /// The model name stored in the translation.
    fn model_name(&self) -> String;
    fn translate(&self, req: &TextTranslateRequest<'_>) -> Result<Vec<String>, CmError>;
}

/// What a text-model run produced.
#[derive(Debug, Clone, PartialEq)]
pub struct TextTranslation {
    pub lines: Vec<TranslationLine>,
    /// Source lines left untranslated.
    pub untranslated: usize,
}

/// Translates the source caption lines in batches with the surrounding lines as context. Every
/// translated line keeps its source line's start and end; one that is too long is re-wrapped and
/// split ([`fit_text`]). `progress` gets 0.0 to 1.0 and returns `false` to cancel
/// ([`crate::transcribe::cancelled`]).
pub fn translate_with_text_model(
    translator: &dyn TextTranslator,
    source: &[CaptionLine],
    source_language: &str,
    target_language: &str,
    keep_as_written: &[String],
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<TextTranslation, CmError> {
    let texts: Vec<String> = source.iter().map(|l| l.text.replace('\n', " ")).collect();
    let mut lines = Vec::new();
    let mut untranslated = 0;
    let mut from = 0;
    while from < source.len() {
        if !progress(from as f32 / source.len() as f32) {
            return Err(crate::transcribe::cancelled());
        }
        let to = (from + BATCH_LINES).min(source.len());
        let req = TextTranslateRequest {
            source_language,
            target_language,
            lines: &texts[from..to],
            before: &texts[from.saturating_sub(CONTEXT_LINES)..from],
            after: &texts[to..(to + CONTEXT_LINES).min(source.len())],
            keep_as_written,
        };
        // One try, and one retry when the count is wrong or a glossary term is lost (AI-5.AC4).
        let mut answer = translator.translate(&req)?;
        let mut checked = enforce_batch(req.lines, answer, keep_as_written);
        if checked.is_none()
            || checked
                .as_ref()
                .is_some_and(|c| c.iter().any(Option::is_none))
        {
            answer = translator.translate(&req)?;
            checked = enforce_batch(req.lines, answer, keep_as_written);
        }
        let ok = checked.is_some();
        let answer: Vec<Option<String>> = checked.unwrap_or_default();
        for (k, line) in source[from..to].iter().enumerate() {
            // A line that lost a glossary term for the second time keeps the source text
            // (which has the term), flagged like a dropped line.
            let text = match answer.get(k) {
                Some(Some(t)) if ok => t.as_str(),
                _ => texts[from + k].as_str(),
            };
            let missing = !ok || answer[k].is_none() || text.trim().is_empty();
            let text = if missing {
                texts[from + k].as_str()
            } else {
                text
            };
            untranslated += usize::from(missing);
            for piece in fit_text(line.start, line.end, text) {
                lines.push(TranslationLine {
                    start: piece.start,
                    end: piece.end,
                    text: piece.text,
                    source: Some(line.text.clone()),
                    edited: false,
                    untranslated: missing,
                });
            }
        }
        from = to;
    }
    let _ = progress(1.0);
    Ok(TextTranslation {
        lines,
        untranslated,
    })
}

// ---- glossary terms (AI-5.AC4) ---------------------------------------------------------------

/// Whether `text` holds `term`, ignoring case.
fn has_term_ci(text: &str, term: &str) -> bool {
    replace_term_ci(text, term).is_some()
}

fn same_char_ci(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// `text` with every case-insensitive occurrence of `term` written exactly as `term`; `None` when
/// there is none. (Also the test for "contains", so both use the same matching.)
fn replace_term_ci(text: &str, term: &str) -> Option<String> {
    let t: Vec<char> = term.chars().collect();
    if t.is_empty() {
        return None;
    }
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let (mut i, mut found) = (0, false);
    while i < c.len() {
        if i + t.len() <= c.len()
            && c[i..i + t.len()]
                .iter()
                .zip(&t)
                .all(|(a, b)| same_char_ci(*a, *b))
        {
            out.push_str(term);
            i += t.len();
            found = true;
        } else {
            out.push(c[i]);
            i += 1;
        }
    }
    found.then_some(out)
}

/// The glossary check for one translated line: every term that the source line holds must be in
/// the translation exactly as the glossary writes it. A term that came back in another case is
/// put right; a term that is gone gives `None`.
pub fn keep_terms(source: &str, translated: &str, terms: &[String]) -> Option<String> {
    let mut text = translated.to_owned();
    for term in terms.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        if !has_term_ci(source, term) || text.contains(term) {
            continue;
        }
        text = replace_term_ci(&text, term)?;
    }
    Some(text)
}

/// A batch answer checked against the glossary: `None` when the line count is wrong; else one
/// entry per line, `None` for a line that lost a term.
fn enforce_batch(
    sources: &[String],
    answer: Vec<String>,
    terms: &[String],
) -> Option<Vec<Option<String>>> {
    if answer.len() != sources.len() {
        return None;
    }
    Some(
        sources
            .iter()
            .zip(answer)
            .map(|(s, a)| keep_terms(s, &a, terms))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::{media_secs, ScriptedTranscriber};
    use std::sync::Mutex;

    fn t(s: f64) -> MediaTime {
        media_secs(s)
    }

    fn cap(a: f64, b: f64, text: &str) -> CaptionLine {
        CaptionLine {
            start: t(a),
            end: t(b),
            text: text.into(),
        }
    }

    fn tl(a: f64, b: f64, text: &str, source: &str, edited: bool) -> TranslationLine {
        TranslationLine {
            start: t(a),
            end: t(b),
            text: text.into(),
            source: Some(source.into()),
            edited,
            untranslated: false,
        }
    }

    fn installed(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| (*s).to_owned()).collect()
    }

    // ---- fit_text -----------------------------------------------------------------------

    #[test]
    fn a_line_that_fits_is_only_wrapped() {
        let out = fit_text(t(0.0), t(3.0), "This is a short translated caption line.");
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].start, out[0].end), (t(0.0), t(3.0)));
        // 40 characters: one row.
        assert!(!out[0].text.contains('\n'));
        let two = fit_text(
            t(0.0),
            t(4.0),
            "A rather longer caption that needs two rows to be shown",
        );
        assert_eq!(two.len(), 1);
        let rows: Vec<&str> = two[0].text.split('\n').collect();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.chars().count() <= 42));
    }

    #[test]
    fn a_long_line_is_split_at_a_word_with_time_by_characters() {
        // 3 x 40 = 120+ characters cannot be two rows of 42.
        let text = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb cccccccccccccccccccccccccccccccccccccccc dddddddddddddddddddddddddddddddddddddddd";
        let out = fit_text(t(10.0), t(14.0), text);
        assert!(out.len() >= 2, "{out:?}");
        // Contiguous, covering exactly the source span, in order.
        assert_eq!(out.first().unwrap().start, t(10.0));
        assert_eq!(out.last().unwrap().end, t(14.0));
        for w in out.windows(2) {
            assert_eq!(w[0].end, w[1].start);
        }
        for l in &out {
            assert!(l.text.split('\n').count() <= 2);
            assert!(l.text.split('\n').all(|r| r.chars().count() <= 42));
        }
        // No word is lost.
        let joined: Vec<String> = out
            .iter()
            .flat_map(|l| {
                l.text
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(joined.join(" "), text);
        // Two equal halves share the time equally.
        let even = fit_text(
            t(0.0),
            t(4.0),
            &format!("{} {}", "a".repeat(40), "b".repeat(40)),
        );
        assert_eq!(even.len(), 1, "80 characters still fit in two rows");
        let four = format!(
            "{} {} {} {}",
            "a".repeat(40),
            "b".repeat(40),
            "c".repeat(40),
            "d".repeat(40)
        );
        let halves = fit_text(t(0.0), t(8.0), &four);
        assert_eq!(halves.len(), 2);
        // 81 of 163 characters (the space between the halves is not counted): 8 s x 81/163.
        let first = (halves[0].end - halves[0].start).as_secs_f64();
        assert!((first - 8.0 * 81.0 / 163.0).abs() < 0.001, "{first}");
    }

    #[test]
    fn a_line_over_seven_seconds_is_split_even_when_it_is_short() {
        let out = fit_text(t(0.0), t(12.0), "one two three four five six seven eight");
        assert!(out.len() >= 2);
        assert!(out
            .iter()
            .all(|l| (l.end - l.start).as_secs_f64() <= MAX_LINE_SECS));
        assert_eq!(out.last().unwrap().end, t(12.0));
    }

    #[test]
    fn fit_text_edge_cases() {
        assert!(fit_text(t(0.0), t(1.0), "   ").is_empty());
        // One enormous word cannot be split; it keeps its span.
        let w = "x".repeat(100);
        let out = fit_text(t(0.0), t(2.0), &w);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, w);
        // An end before the start is clamped.
        assert_eq!(fit_text(t(5.0), t(4.0), "hi")[0].end, t(5.0));
    }

    // ---- engine and model choice ----------------------------------------------------------

    #[test]
    fn the_engine_follows_the_target() {
        use EngineChoice::*;
        assert_eq!(
            choose_engine(Auto, "en", "vi", false).unwrap(),
            Engine::Whisper
        );
        assert_eq!(
            choose_engine(Whisper, "en", "vi", false).unwrap(),
            Engine::Whisper
        );
        assert_eq!(
            choose_engine(Auto, "vi", "en", true).unwrap(),
            Engine::TextModel
        );
        assert_eq!(
            choose_engine(TextModel, "en", "vi", true).unwrap(),
            Engine::TextModel
        );
        // No text model: reason no_text_model, for auto and explicit.
        for c in [Auto, TextModel] {
            let e = choose_engine(c, "vi", "en", false).unwrap_err();
            assert_eq!(error_reason(&e), Some("no_text_model"));
            assert_eq!(e.code().as_str(), "invalid_input");
        }
        // Explicit text_model to English needs the model too.
        assert!(choose_engine(TextModel, "en", "vi", false).is_err());
        // Whisper cannot reach other languages.
        assert!(choose_engine(Whisper, "vi", "en", true).is_err());
        // The same language is refused, region aside.
        let e = choose_engine(Auto, "pt-br", "pt", true).unwrap_err();
        assert_eq!(error_reason(&e), Some("same_language"));
        assert_eq!(EngineChoice::parse("text-model").unwrap(), TextModel);
        assert!(EngineChoice::parse("gpt").is_err());
    }

    /// AI-5.AC2: the turbo rule.
    #[test]
    fn turbo_is_never_used_for_translation() {
        // Active turbo with large-v3 installed: large-v3.
        let all = installed(&["small", "medium", "large", "large-v3-turbo"]);
        assert_eq!(
            choose_translation_model(&all, Some("large-v3-turbo"), None).unwrap(),
            "large"
        );
        // Without large-v3: medium.
        let no_large = installed(&["small", "medium", "large-v3-turbo"]);
        assert_eq!(
            choose_translation_model(&no_large, Some("large-v3-turbo"), None).unwrap(),
            "medium"
        );
        // Neither: translation_model_missing, even with small installed.
        let few = installed(&["small", "large-v3-turbo"]);
        let e = choose_translation_model(&few, Some("large-v3-turbo"), None).unwrap_err();
        assert_eq!(error_reason(&e), Some("translation_model_missing"));
        assert_eq!(
            error_data(&e).unwrap()["reason"],
            "translation_model_missing"
        );
        // Only turbo installed.
        assert!(choose_translation_model(&installed(&["large-v3-turbo"]), None, None).is_err());
        // A translating active model is honoured.
        assert_eq!(
            choose_translation_model(&all, Some("small"), None).unwrap(),
            "small"
        );
        // Naming the turbo model is refused, naming a missing one too.
        assert!(choose_translation_model(&all, None, Some("large-v3-turbo")).is_err());
        assert!(choose_translation_model(&no_large, None, Some("large")).is_err());
        assert_eq!(
            choose_translation_model(&all, None, Some("medium")).unwrap(),
            "medium"
        );
        assert!(!model_can_translate("large-v3-turbo"));
        assert!(model_can_translate("large"));
    }

    #[test]
    fn language_codes_are_file_safe() {
        assert_eq!(language_code("EN").unwrap(), "en");
        assert_eq!(language_code("pt_BR").unwrap(), "pt-br");
        for bad in ["", "e", "../x", "en/vi", "auto1", "english", "en-", "a b"] {
            assert!(language_code(bad).is_err(), "{bad}");
        }
        assert_eq!(base_language("pt-br"), "pt");
    }

    // ---- sources and edits ------------------------------------------------------------------

    #[test]
    fn a_line_remembers_the_source_it_overlaps_most() {
        let src = [cap(0.0, 3.0, "one"), cap(3.0, 6.0, "two")];
        assert_eq!(best_source(&src, t(2.0), t(5.0)).unwrap().text, "two");
        assert_eq!(best_source(&src, t(0.5), t(1.0)).unwrap().text, "one");
        assert!(best_source(&src, t(8.0), t(9.0)).is_none());
    }

    /// AI-5.AC5: an edited line is kept when its source line is unchanged.
    #[test]
    fn edits_survive_a_retranslate_when_the_source_line_is_unchanged() {
        let old = Translation {
            version: 1,
            language: "vi".into(),
            engine: "text_model".into(),
            model: "m".into(),
            created: "2026-10-26T00:00:00Z".into(),
            lines: vec![
                tl(0.0, 2.0, "Xin chào các bạn", "Hello everyone", true),
                tl(2.0, 4.0, "Hôm nay chúng ta học", "Today we learn", false),
                tl(4.0, 6.0, "Bản sửa của tôi", "Old third line", true),
            ],
            edited: true,
        };
        let mut new = vec![
            tl(0.0, 2.0, "Chào mọi người", "Hello everyone", false),
            tl(2.0, 4.0, "Hôm nay ta học", "Today we learn", false),
            // The source line was edited: the translation of the third line is new text.
            tl(4.0, 6.0, "Dòng thứ ba mới", "New third line", false),
        ];
        let carried = carry_over_edits(&old, &mut new);
        assert_eq!(
            carried,
            Carried {
                kept: 1,
                dropped: 1
            }
        );
        assert_eq!(new[0].text, "Xin chào các bạn");
        assert!(new[0].edited);
        assert_eq!(new[1].text, "Hôm nay ta học");
        assert!(!new[1].edited);
        assert_eq!(new[2].text, "Dòng thứ ba mới");
        let (t2, c2) = assemble("vi", Engine::TextModel, "m", "now", new, Some(&old));
        assert_eq!(c2.kept, 1);
        assert!(t2.edited);
    }

    #[test]
    fn a_carried_edit_needs_the_same_place_in_time_and_is_used_once() {
        let old = Translation {
            version: 1,
            language: "vi".into(),
            engine: "whisper".into(),
            model: "m".into(),
            created: String::new(),
            lines: vec![tl(0.0, 2.0, "mine", "same", true)],
            edited: true,
        };
        // Same source text but far away in time: not the same line.
        let mut far = vec![tl(30.0, 32.0, "other", "same", false)];
        assert_eq!(carry_over_edits(&old, &mut far).kept, 0);
        // Whitespace and Unicode form of the source do not matter.
        let mut spaced = vec![tl(0.2, 2.1, "other", "same ", false)];
        assert_eq!(carry_over_edits(&old, &mut spaced).kept, 1);
        // A split line: the first half takes the edit, the second does not.
        let mut split = vec![
            tl(0.0, 1.0, "a", "same", false),
            tl(1.0, 2.0, "b", "same", false),
        ];
        let c = carry_over_edits(&old, &mut split);
        assert_eq!(c.kept, 1);
        assert_eq!(split[0].text, "mine");
        assert_eq!(split[1].text, "b");
    }

    #[test]
    fn editing_lines_marks_only_the_changed_ones() {
        let old = Translation {
            version: 1,
            language: "en".into(),
            engine: "whisper".into(),
            model: "large".into(),
            created: String::new(),
            lines: vec![
                tl(0.0, 2.0, "Hello", "Xin chào", false),
                tl(2.0, 4.0, "World", "Thế giới", false),
            ],
            edited: false,
        };
        let src = [
            cap(0.0, 2.0, "Xin chào"),
            cap(2.0, 4.0, "Thế giới"),
            cap(4.0, 6.0, "Nữa"),
        ];
        let edited = with_edited_lines(
            &old,
            &[
                cap(0.0, 2.0, "Hello"),
                cap(2.0, 4.0, "Planet"),
                cap(4.0, 6.0, "More"),
            ],
            &src,
        );
        assert!(edited.edited);
        assert!(!edited.lines[0].edited);
        assert!(edited.lines[1].edited);
        assert_eq!(edited.lines[1].source.as_deref(), Some("Thế giới"));
        assert!(edited.lines[2].edited);
        assert_eq!(edited.lines[2].source.as_deref(), Some("Nữa"));
        // Nothing changed: not edited.
        let same = with_edited_lines(&old, &old.captions(), &src);
        assert!(!same.edited);
        assert_eq!(same, old);
    }

    // ---- files ----------------------------------------------------------------------------

    #[test]
    fn a_translation_round_trips_in_the_bundle() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_translation(dir.path(), "vi").unwrap().is_none());
        assert!(list_translations(dir.path()).is_empty());
        let (tr, _) = assemble(
            "vi",
            Engine::TextModel,
            "llama",
            "2026-10-26T01:02:03Z",
            vec![tl(0.0, 2.0, "Xin chào", "Hello", false)],
            None,
        );
        let path = write_translation(dir.path(), &tr).unwrap();
        assert!(path.ends_with("translations/vi.json"));
        assert_eq!(read_translation(dir.path(), "vi").unwrap().unwrap(), tr);
        assert_eq!(list_translations(dir.path()), ["vi"]);
        // The stored shape is the spec's, plus the additive per-line source.
        let v: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        for k in ["language", "engine", "model", "created", "lines", "edited"] {
            assert!(v.get(k).is_some(), "{k}");
        }
        assert_eq!(v["lines"][0]["text"], "Xin chào");
        assert!(v["lines"][0].get("edited").is_none());
        fs::write(translation_path(dir.path(), "vi"), "{ nope").unwrap();
        assert!(read_translation(dir.path(), "vi")
            .unwrap_err()
            .to_string()
            .contains("damaged"));
    }

    // ---- Whisper -----------------------------------------------------------------------------

    #[test]
    fn whisper_output_is_cut_by_the_caption_rules() {
        let s = ScriptedTranscriber::from_secs(&[("unused", 0.0, 0.1)]).with_translation(&[
            ("Hello", 0.0, 0.4),
            ("everyone.", 0.5, 1.2),
            ("Today", 3.0, 3.4),
            ("we", 3.5, 3.6),
            ("learn.", 3.7, 4.4),
        ]);
        let mut seen = Vec::new();
        let tr = s
            .translate_to_english(
                crate::transcribe::Pcm16k {
                    samples: &[],
                    start: t(10.0),
                },
                Some("vi"),
                &mut |p| {
                    seen.push(p);
                    true
                },
            )
            .unwrap();
        assert_eq!(tr.language, "en");
        assert_eq!(seen.last().copied(), Some(1.0));
        assert_eq!(
            s.translate_runs.lock().unwrap().as_slice(),
            [Some("vi".to_owned())]
        );
        let src = [
            cap(10.0, 12.0, "Xin chào mọi người."),
            cap(13.0, 15.0, "Hôm nay."),
        ];
        let lines = whisper_lines(&tr, &src);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Hello everyone.");
        assert_eq!(lines[0].source.as_deref(), Some("Xin chào mọi người."));
        assert_eq!(lines[0].start, t(10.0));
        assert!(lines.iter().all(|l| {
            l.text.split('\n').count() <= 2
                && l.text.split('\n').all(|r| r.chars().count() <= 42)
                && (l.end - l.start).as_secs_f64() <= MAX_LINE_SECS
        }));
        // An engine without the task says so.
        let plain = ScriptedTranscriber::from_secs(&[("a", 0.0, 0.1)]);
        let e = plain
            .translate_to_english(
                crate::transcribe::Pcm16k {
                    samples: &[],
                    start: MediaTime::ZERO,
                },
                None,
                &mut |_| true,
            )
            .unwrap_err();
        assert_eq!(e.code().as_str(), "not_implemented");
    }

    // ---- text model --------------------------------------------------------------------------

    /// Answers with `[tgt] text`, and can be told to drop a line on given calls.
    struct Scripted {
        calls: Mutex<Vec<(usize, usize, usize)>>,
        drop_on_call: Vec<usize>,
        always_drop: bool,
        fail: bool,
    }

    impl Scripted {
        fn new() -> Scripted {
            Scripted {
                calls: Mutex::new(Vec::new()),
                drop_on_call: vec![],
                always_drop: false,
                fail: false,
            }
        }
    }

    impl TextTranslator for Scripted {
        fn model_name(&self) -> String {
            "scripted".into()
        }
        fn translate(&self, req: &TextTranslateRequest<'_>) -> Result<Vec<String>, CmError> {
            if self.fail {
                return Err(CmError::network("unreachable"));
            }
            let mut calls = self.calls.lock().unwrap();
            calls.push((req.lines.len(), req.before.len(), req.after.len()));
            let n = calls.len();
            let mut out: Vec<String> = req
                .lines
                .iter()
                .map(|l| format!("[{}] {l}", req.target_language))
                .collect();
            if self.always_drop || self.drop_on_call.contains(&n) {
                out.pop();
            }
            Ok(out)
        }
    }

    fn many(n: usize) -> Vec<CaptionLine> {
        (0..n)
            .map(|i| cap(i as f64 * 2.0, i as f64 * 2.0 + 1.5, &format!("line {i}")))
            .collect()
    }

    #[test]
    fn a_text_model_keeps_the_source_times_and_sees_context() {
        let src = many(45);
        let tr = Scripted::new();
        let mut last = 0.0;
        let out =
            translate_with_text_model(&tr, &src, "en", "vi", &["CaptureMann".into()], &mut |p| {
                last = p;
                true
            })
            .unwrap();
        assert_eq!(out.untranslated, 0);
        assert_eq!(out.lines.len(), 45);
        assert_eq!(last, 1.0);
        for (o, s) in out.lines.iter().zip(&src) {
            assert_eq!((o.start, o.end), (s.start, s.end));
            assert_eq!(o.source.as_deref(), Some(s.text.as_str()));
            assert!(o.text.starts_with("[vi] "));
        }
        // Batches of 20, 20 and 5 with 3 lines of context on each side where there are any.
        assert_eq!(
            tr.calls.lock().unwrap().as_slice(),
            [(20, 0, 3), (20, 3, 3), (5, 3, 0)]
        );
    }

    #[test]
    fn a_batch_that_drops_a_line_is_retried_once_then_flagged() {
        let src = many(4);
        // Dropped on the first try only: the retry fixes it.
        let once = Scripted {
            drop_on_call: vec![1],
            ..Scripted::new()
        };
        let out = translate_with_text_model(&once, &src, "en", "vi", &[], &mut |_| true).unwrap();
        assert_eq!(out.untranslated, 0);
        assert_eq!(once.calls.lock().unwrap().len(), 2);
        // Dropped both times: the whole batch keeps the source text, flagged.
        let always = Scripted {
            always_drop: true,
            ..Scripted::new()
        };
        let out = translate_with_text_model(&always, &src, "en", "vi", &[], &mut |_| true).unwrap();
        assert_eq!(out.untranslated, 4);
        assert!(out
            .lines
            .iter()
            .all(|l| l.untranslated && l.text.starts_with("line ")));
        // A provider failure stops the run.
        let down = Scripted {
            fail: true,
            ..Scripted::new()
        };
        assert!(translate_with_text_model(&down, &src, "en", "vi", &[], &mut |_| true).is_err());
        // Cancelling.
        let e = translate_with_text_model(&Scripted::new(), &src, "en", "vi", &[], &mut |_| false)
            .unwrap_err();
        assert!(crate::transcribe::is_cancelled(&e));
    }

    #[test]
    fn ai_5_ac4_glossary_terms_stay_as_written() {
        // AI-5.AC4: a glossary term in a source line is in the translated line exactly as
        // written. A term that came back in another case is put right; a term that is gone gets
        // one retry, then the line keeps its source text, flagged.
        let terms = vec!["CaptureMann".to_owned()];
        assert_eq!(
            keep_terms("Open CaptureMann now", "Mở CaptureMann ngay", &terms).as_deref(),
            Some("Mở CaptureMann ngay")
        );
        assert_eq!(
            keep_terms("Open CaptureMann now", "Mở capturemann ngay", &terms).as_deref(),
            Some("Mở CaptureMann ngay")
        );
        assert_eq!(
            keep_terms("Open CaptureMann now", "Mở ứng dụng", &terms),
            None
        );
        // A line without the term is not checked.
        assert_eq!(
            keep_terms("Hello", "Xin chào", &terms).as_deref(),
            Some("Xin chào")
        );

        struct Terms {
            calls: Mutex<usize>,
            /// Calls on which the term is translated away.
            lose_on: Vec<usize>,
            /// Calls on which the term comes back in lower case.
            lower_on: Vec<usize>,
        }
        impl TextTranslator for Terms {
            fn model_name(&self) -> String {
                "scripted".into()
            }
            fn translate(&self, req: &TextTranslateRequest<'_>) -> Result<Vec<String>, CmError> {
                let mut n = self.calls.lock().unwrap();
                *n += 1;
                Ok(req
                    .lines
                    .iter()
                    .map(|l| {
                        if self.lose_on.contains(&n) {
                            l.replace("CaptureMann", "the app")
                        } else if self.lower_on.contains(&n) {
                            l.replace("CaptureMann", "capturemann")
                        } else {
                            l.clone()
                        }
                    })
                    .collect())
            }
        }
        let src = [
            cap(0.0, 2.0, "Open CaptureMann now"),
            cap(2.0, 4.0, "Then record"),
        ];
        let make = |lose_on: Vec<usize>, lower_on: Vec<usize>| Terms {
            calls: Mutex::new(0),
            lose_on,
            lower_on,
        };
        let run = |m: &Terms| {
            translate_with_text_model(m, &src, "en", "vi", &terms, &mut |_| true).unwrap()
        };
        // Fine on the first try: one call.
        let ok = make(vec![], vec![]);
        let out = run(&ok);
        assert_eq!(*ok.calls.lock().unwrap(), 1);
        assert_eq!(out.untranslated, 0);
        // Case drift is repaired without a retry.
        let lower = make(vec![], vec![1]);
        let out = run(&lower);
        assert_eq!(*lower.calls.lock().unwrap(), 1);
        assert_eq!(out.lines[0].text, "Open CaptureMann now");
        // Lost once: the retry brings it back.
        let once = make(vec![1], vec![]);
        let out = run(&once);
        assert_eq!(*once.calls.lock().unwrap(), 2);
        assert_eq!(out.untranslated, 0);
        assert!(out.lines[0].text.contains("CaptureMann"));
        // Lost twice: that line keeps its source text (with the term) and is flagged; the other
        // line of the batch keeps its translation.
        let twice = make(vec![1, 2], vec![]);
        let out = run(&twice);
        assert_eq!(out.untranslated, 1);
        assert!(out.lines[0].untranslated && out.lines[0].text.contains("CaptureMann"));
        assert!(!out.lines[1].untranslated);
        for (o, s) in out.lines.iter().zip(&src) {
            assert_eq!((o.start, o.end), (s.start, s.end));
        }
    }

    #[test]
    fn a_long_translation_is_split_on_the_source_span() {
        struct Long;
        impl TextTranslator for Long {
            fn model_name(&self) -> String {
                "long".into()
            }
            fn translate(&self, req: &TextTranslateRequest<'_>) -> Result<Vec<String>, CmError> {
                Ok(req.lines.iter().map(|_| "palabra ".repeat(25)).collect())
            }
        }
        let src = [cap(0.0, 6.0, "short")];
        let out = translate_with_text_model(&Long, &src, "en", "es", &[], &mut |_| true).unwrap();
        assert!(out.lines.len() >= 2);
        assert_eq!(out.lines.first().unwrap().start, t(0.0));
        assert_eq!(out.lines.last().unwrap().end, t(6.0));
        assert!(out
            .lines
            .iter()
            .all(|l| l.source.as_deref() == Some("short")));
    }
}
