use super::*;

// ---- the actions ---------------------------------------------------------------------------

/// One validated AI action.
#[derive(Debug, Clone, PartialEq)]
pub enum AiOp {
    ListModels,
    DownloadModel {
        model: String,
    },
    ImportModel {
        model: String,
        path: String,
    },
    SelectModel {
        model: String,
    },
    RemoveModel {
        model: String,
    },
    Transcribe {
        capture_id: String,
        /// `None`: detect.
        language: Option<String>,
        /// `None`: the active model.
        model: Option<String>,
    },
    GetTranscript {
        capture_id: String,
    },
    GetCaptions {
        capture_id: String,
        /// `None`: the original captions; a code: that translation (AI-5).
        language: Option<String>,
    },
    SetCaptions {
        capture_id: String,
        lines: Vec<CaptionLine>,
        language: Option<String>,
    },
    ReplaceInCaptions {
        capture_id: String,
        replacements: Vec<Replacement>,
        language: Option<String>,
    },
    ExportCaptions {
        capture_id: String,
        format: CaptionFormat,
        language: Option<String>,
    },
    /// AI-5: translate the captions into `target_language` (a job).
    TranslateCaptions {
        capture_id: String,
        target_language: String,
        engine: cm_ai::translate::EngineChoice,
    },
    SuggestZooms {
        capture_id: String,
    },
    SuggestCuts {
        capture_id: String,
    },
}

impl AiOp {
    pub fn method(&self) -> &'static str {
        match self {
            AiOp::ListModels => "list_models",
            AiOp::DownloadModel { .. } => "download_model",
            AiOp::ImportModel { .. } => "import_model",
            AiOp::SelectModel { .. } => "select_model",
            AiOp::RemoveModel { .. } => "remove_model",
            AiOp::Transcribe { .. } => "transcribe",
            AiOp::GetTranscript { .. } => "get_transcript",
            AiOp::GetCaptions { .. } => "get_captions",
            AiOp::SetCaptions { .. } => "set_captions",
            AiOp::ReplaceInCaptions { .. } => "replace_in_captions",
            AiOp::ExportCaptions { .. } => "export_captions",
            AiOp::SuggestZooms { .. } => "suggest_zooms",
            AiOp::SuggestCuts { .. } => "suggest_cuts",
            AiOp::TranslateCaptions { .. } => "translate_captions",
        }
    }

    /// The capture the action works on.
    pub fn capture_id(&self) -> Option<&str> {
        match self {
            AiOp::Transcribe { capture_id, .. }
            | AiOp::GetTranscript { capture_id }
            | AiOp::GetCaptions { capture_id, .. }
            | AiOp::SetCaptions { capture_id, .. }
            | AiOp::ReplaceInCaptions { capture_id, .. }
            | AiOp::ExportCaptions { capture_id, .. }
            | AiOp::TranslateCaptions { capture_id, .. }
            | AiOp::SuggestZooms { capture_id }
            | AiOp::SuggestCuts { capture_id } => Some(capture_id),
            _ => None,
        }
    }
}

/// One pair of `replace_in_captions`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Replacement {
    pub find: String,
    pub replace: String,
    pub match_case: bool,
    pub whole_word: bool,
}

fn insert_language(o: &mut Map<String, Value>, language: &Option<String>) {
    if let Some(l) = language {
        o.insert("language".into(), json!(l));
    }
}

/// An AI action with its idempotency key.
#[derive(Debug, Clone, PartialEq)]
pub struct AiParams {
    pub op: AiOp,
    pub request_id: Option<String>,
}

impl AiParams {
    /// The input as JSON (what the confirmation dialog and the activity log show).
    pub fn to_json(&self) -> Value {
        let mut o = Map::new();
        match &self.op {
            AiOp::ListModels => {}
            AiOp::DownloadModel { model }
            | AiOp::SelectModel { model }
            | AiOp::RemoveModel { model } => {
                o.insert("model".into(), json!(model));
            }
            AiOp::ImportModel { model, path } => {
                o.insert("model".into(), json!(model));
                o.insert("path".into(), json!(path));
            }
            AiOp::Transcribe {
                capture_id,
                language,
                model,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                if let Some(l) = language {
                    o.insert("language".into(), json!(l));
                }
                if let Some(m) = model {
                    o.insert("model".into(), json!(m));
                }
            }
            AiOp::GetTranscript { capture_id }
            | AiOp::SuggestZooms { capture_id }
            | AiOp::SuggestCuts { capture_id } => {
                o.insert("capture_id".into(), json!(capture_id));
            }
            AiOp::GetCaptions {
                capture_id,
                language,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                insert_language(&mut o, language);
            }
            AiOp::SetCaptions {
                capture_id,
                lines,
                language,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("lines".into(), json!(lines));
                insert_language(&mut o, language);
            }
            AiOp::ReplaceInCaptions {
                capture_id,
                replacements,
                language,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("replacements".into(), json!(replacements));
                insert_language(&mut o, language);
            }
            AiOp::ExportCaptions {
                capture_id,
                format,
                language,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("format".into(), json!(format.ext()));
                insert_language(&mut o, language);
            }
            AiOp::TranslateCaptions {
                capture_id,
                target_language,
                engine,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("target_language".into(), json!(target_language));
                o.insert("engine".into(), json!(engine));
            }
        }
        Value::Object(o)
    }
}

impl Serialize for AiParams {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut v = self.to_json();
        if let (Some(o), Some(r)) = (v.as_object_mut(), &self.request_id) {
            o.insert("request_id".into(), json!(r));
        }
        v.serialize(s)
    }
}

/// One sentence for the dialog and the activity log.
pub(crate) fn summary(p: &AiParams) -> String {
    match &p.op {
        AiOp::DownloadModel { model } => {
            format!("Download the speech model `{model}` from the internet")
        }
        AiOp::RemoveModel { model } => {
            format!("Delete the speech model `{model}` from this computer")
        }
        AiOp::ImportModel { model, .. } => {
            format!("Install the speech model `{model}` from a file")
        }
        AiOp::SelectModel { model } => format!("Use the speech model `{model}`"),
        AiOp::Transcribe { capture_id, .. } => format!("Transcribe {capture_id}"),
        AiOp::SetCaptions {
            capture_id, lines, ..
        } => {
            format!("Set {} caption lines of {capture_id}", lines.len())
        }
        AiOp::TranslateCaptions {
            capture_id,
            target_language,
            ..
        } => format!("Translate the captions of {capture_id} into `{target_language}`"),
        AiOp::ReplaceInCaptions {
            capture_id,
            replacements,
            ..
        } => format!(
            "Replace text in the captions of {capture_id} ({} pairs)",
            replacements.len()
        ),
        AiOp::ExportCaptions {
            capture_id, format, ..
        } => {
            format!(
                "Export {capture_id} captions as {}",
                format.ext().to_uppercase()
            )
        }
        other => format!("Run {}", other.method()),
    }
}

/// The typed parameters of an AI action's confirmation (P3): the model, capture and format the
/// English `summary` names.
pub(crate) fn confirm_params(p: &AiParams) -> Value {
    match &p.op {
        AiOp::DownloadModel { model }
        | AiOp::RemoveModel { model }
        | AiOp::ImportModel { model, .. }
        | AiOp::SelectModel { model } => json!({ "model": model }),
        AiOp::Transcribe { capture_id, .. } => json!({ "capture_id": capture_id }),
        AiOp::SetCaptions {
            capture_id, lines, ..
        } => {
            json!({ "capture_id": capture_id, "lines": lines.len() })
        }
        AiOp::ExportCaptions {
            capture_id, format, ..
        } => {
            json!({ "capture_id": capture_id, "format": format.ext().to_uppercase() })
        }
        _ => json!({}),
    }
}

fn model_id(f: &Fields<'_>, key: &str) -> Result<String, CmError> {
    let m = f.str_req(key)?;
    let ok = m.len() <= 40
        && m.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && m.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(m)
    } else {
        Err(CmError::invalid_field(
            key,
            "must be a model id such as `base`, `small` or `medium`",
        ))
    }
}

fn language(f: &Fields<'_>) -> Result<Option<String>, CmError> {
    match f.str_opt("language")? {
        None => Ok(None),
        Some(l) if l.eq_ignore_ascii_case("auto") => Ok(None),
        Some(l) => {
            let l = l.to_ascii_lowercase();
            let mut parts = l.split('-');
            let head_ok = parts.next().is_some_and(|h| {
                (2..=3).contains(&h.len()) && h.chars().all(|c| c.is_ascii_lowercase())
            });
            let tail_ok = parts.all(|t| {
                (2..=4).contains(&t.len()) && t.chars().all(|c| c.is_ascii_alphanumeric())
            });
            if head_ok && tail_ok && l.len() <= 12 {
                Ok(Some(l))
            } else {
                Err(CmError::invalid_field(
                    "language",
                    "must be `auto` or a language code such as `en`, `vi` or `es`",
                ))
            }
        }
    }
}

/// The optional `language` of the caption actions: a translation's code (`vi`, `pt-br`). Absent
/// means the original captions; `auto` is not a translation.
fn translation_language(f: &Fields<'_>) -> Result<Option<String>, CmError> {
    f.str_opt("language")?
        .map(|l| cm_ai::translate::language_code(&l))
        .transpose()
}

/// `lines` of `set_captions`: times as seconds or `"t:<ticks>"`, in order, not overlapping, at
/// most 2 rows of 42 characters and 7 s each (AI-1).
fn caption_lines(f: &Fields<'_>) -> Result<Vec<CaptionLine>, CmError> {
    let arr = f
        .obj
        .get("lines")
        .ok_or_else(|| CmError::invalid_field("lines", "is required"))?
        .as_array()
        .ok_or_else(|| CmError::invalid_field("lines", "must be an array"))?;
    if arr.len() > MAX_CAPTION_LINES {
        return Err(CmError::invalid_field(
            "lines",
            format!("at most {MAX_CAPTION_LINES} lines"),
        ));
    }
    let mut out: Vec<CaptionLine> = Vec::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        let at = |k: &str| format!("lines[{i}].{k}");
        let o = item
            .as_object()
            .ok_or_else(|| CmError::invalid_field(&format!("lines[{i}]"), "must be an object"))?;
        for k in o.keys() {
            if !matches!(k.as_str(), "start" | "end" | "text") {
                return Err(CmError::invalid_field(&at(k), "unknown parameter"));
            }
        }
        let time = |k: &str| match o.get(k) {
            Some(v) => parse_time(&at(k), v),
            None => Err(CmError::invalid_field(&at(k), "is required")),
        };
        let (start, end) = (time("start")?, time("end")?);
        if end <= start {
            return Err(CmError::invalid_field(&at("end"), "must be after start"));
        }
        if secs(end) - secs(start) > MAX_CAPTION_SECONDS {
            return Err(CmError::invalid_field(
                &at("end"),
                format!("a line lasts at most {MAX_CAPTION_SECONDS} s"),
            ));
        }
        let text = match o.get("text") {
            Some(Value::String(t)) => t.trim().to_owned(),
            Some(_) => return Err(CmError::invalid_field(&at("text"), "must be a string")),
            None => return Err(CmError::invalid_field(&at("text"), "is required")),
        };
        let rows: Vec<&str> = text.split('\n').collect();
        if text.is_empty() {
            return Err(CmError::invalid_field(&at("text"), "must not be empty"));
        }
        if rows.len() > MAX_CAPTION_ROWS
            || rows
                .iter()
                .any(|r| r.chars().count() > MAX_CAPTION_ROW_CHARS)
        {
            return Err(CmError::invalid_field(
                &at("text"),
                format!("at most {MAX_CAPTION_ROWS} rows of {MAX_CAPTION_ROW_CHARS} characters"),
            ));
        }
        if let Some(prev) = out.last() {
            if start < prev.end {
                return Err(CmError::invalid_field(
                    &at("start"),
                    "lines must be in order and must not overlap",
                ));
            }
        }
        out.push(CaptionLine { start, end, text });
    }
    Ok(out)
}

/// `replacements` of `replace_in_captions`.
fn replacements(f: &Fields<'_>) -> Result<Vec<Replacement>, CmError> {
    let arr = f
        .obj
        .get("replacements")
        .ok_or_else(|| CmError::invalid_field("replacements", "is required"))?
        .as_array()
        .ok_or_else(|| CmError::invalid_field("replacements", "must be an array"))?;
    if arr.is_empty() || arr.len() > MAX_REPLACEMENTS {
        return Err(CmError::invalid_field(
            "replacements",
            format!("give 1 to {MAX_REPLACEMENTS} pairs"),
        ));
    }
    let mut out = Vec::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        let at = |k: &str| format!("replacements[{i}].{k}");
        let o = item.as_object().ok_or_else(|| {
            CmError::invalid_field(&format!("replacements[{i}]"), "must be an object")
        })?;
        for k in o.keys() {
            if !matches!(k.as_str(), "find" | "replace" | "match_case" | "whole_word") {
                return Err(CmError::invalid_field(&at(k), "unknown parameter"));
            }
        }
        let text = |k: &str, required_non_empty: bool| match o.get(k) {
            Some(Value::String(s)) => {
                if s.chars().count() > MAX_REPLACE_CHARS {
                    Err(CmError::invalid_field(
                        &at(k),
                        format!("at most {MAX_REPLACE_CHARS} characters"),
                    ))
                } else if required_non_empty && s.trim().is_empty() {
                    Err(CmError::invalid_field(&at(k), "must not be empty"))
                } else {
                    Ok(s.clone())
                }
            }
            Some(_) => Err(CmError::invalid_field(&at(k), "must be a string")),
            None => Err(CmError::invalid_field(&at(k), "is required")),
        };
        let flag = |k: &str| match o.get(k) {
            None => Ok(false),
            Some(Value::Bool(b)) => Ok(*b),
            Some(_) => Err(CmError::invalid_field(&at(k), "must be true or false")),
        };
        out.push(Replacement {
            find: text("find", true)?,
            replace: text("replace", false)?,
            match_case: flag("match_case")?,
            whole_word: flag("whole_word")?,
        });
    }
    Ok(out)
}

/// Validates the input of one AI method.
pub(crate) fn parse(method: &str, f: &Fields<'_>) -> Result<Action, CmError> {
    let op = match method {
        "list_models" => {
            f.only(&[])?;
            AiOp::ListModels
        }
        "download_model" | "select_model" | "remove_model" => {
            f.only(&["model", "request_id"])?;
            let model = model_id(f, "model")?;
            match method {
                "download_model" => AiOp::DownloadModel { model },
                "select_model" => AiOp::SelectModel { model },
                _ => AiOp::RemoveModel { model },
            }
        }
        "import_model" => {
            f.only(&["model", "path", "request_id"])?;
            AiOp::ImportModel {
                model: model_id(f, "model")?,
                path: f.str_req("path")?,
            }
        }
        "transcribe" => {
            f.only(&["capture_id", "language", "model", "request_id"])?;
            let model = match f.str_opt("model")? {
                None => None,
                Some(_) => Some(model_id(f, "model")?),
            };
            AiOp::Transcribe {
                capture_id: f.str_req("capture_id")?,
                language: language(f)?,
                model,
            }
        }
        "get_transcript" | "suggest_zooms" | "suggest_cuts" => {
            f.only(&["capture_id"])?;
            let capture_id = f.str_req("capture_id")?;
            match method {
                "get_transcript" => AiOp::GetTranscript { capture_id },
                "suggest_zooms" => AiOp::SuggestZooms { capture_id },
                _ => AiOp::SuggestCuts { capture_id },
            }
        }
        "get_captions" => {
            f.only(&["capture_id", "language"])?;
            AiOp::GetCaptions {
                capture_id: f.str_req("capture_id")?,
                language: translation_language(f)?,
            }
        }
        "set_captions" => {
            f.only(&["capture_id", "lines", "language", "request_id"])?;
            AiOp::SetCaptions {
                capture_id: f.str_req("capture_id")?,
                lines: caption_lines(f)?,
                language: translation_language(f)?,
            }
        }
        "replace_in_captions" => {
            f.only(&["capture_id", "replacements", "language", "request_id"])?;
            AiOp::ReplaceInCaptions {
                capture_id: f.str_req("capture_id")?,
                replacements: replacements(f)?,
                language: translation_language(f)?,
            }
        }
        "translate_captions" => {
            f.only(&["capture_id", "target_language", "engine", "request_id"])?;
            let target = f.str_req("target_language")?;
            let target_language = cm_ai::translate::language_code(&target).map_err(|_| {
                CmError::invalid_field(
                    "target_language",
                    "must be a language code such as `en`, `vi`, `es` or `pt-br`",
                )
            })?;
            let engine = match f.str_opt("engine")? {
                None => cm_ai::translate::EngineChoice::Auto,
                Some(e) => cm_ai::translate::EngineChoice::parse(&e)?,
            };
            AiOp::TranslateCaptions {
                capture_id: f.str_req("capture_id")?,
                target_language,
                engine,
            }
        }
        "export_captions" => {
            f.only(&[
                "capture_id",
                "format",
                "timestamps",
                "language",
                "request_id",
            ])?;
            let timestamps = f.bool_or("timestamps", false)?;
            let format = match f.str_req("format")?.as_str() {
                "srt" => CaptionFormat::Srt,
                "vtt" => CaptionFormat::Vtt,
                "txt" if timestamps => CaptionFormat::TxtTimed,
                "txt" => CaptionFormat::Txt,
                _ => {
                    return Err(CmError::invalid_field(
                        "format",
                        "must be `srt`, `vtt` or `txt`",
                    ))
                }
            };
            if timestamps && !format.is_text() {
                return Err(CmError::invalid_field(
                    "timestamps",
                    "only applies to the `txt` format (SRT and VTT always have times)",
                ));
            }
            AiOp::ExportCaptions {
                capture_id: f.str_req("capture_id")?,
                format,
                language: translation_language(f)?,
            }
        }
        other => {
            return Err(CmError::invalid_input(format!(
                "unknown AI action `{other}`"
            )))
        }
    };
    Ok(Action::Ai(AiParams {
        op,
        request_id: f.request_id()?,
    }))
}
