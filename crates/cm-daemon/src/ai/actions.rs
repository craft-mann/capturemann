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
    },
    SetCaptions {
        capture_id: String,
        lines: Vec<CaptionLine>,
    },
    ReplaceInCaptions {
        capture_id: String,
        replacements: Vec<Replacement>,
    },
    ExportCaptions {
        capture_id: String,
        format: CaptionFormat,
        /// SRT only: prefix every cue with `NAME:` (AI-8; default off).
        speaker_names: bool,
    },
    /// AI-8: a `diarize` job.
    Diarize {
        capture_id: String,
        hints: cm_ai::speakers::DiarizeHints,
        /// "My microphone only hears me" for this recording (AI-8.AC7).
        headset: bool,
    },
    /// AI-8: rename, merge and reassign speakers as one revision.
    SetSpeakers {
        capture_id: String,
        edit: cm_ai::speakers::SpeakerEdit,
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
            AiOp::Diarize { .. } => "diarize",
            AiOp::SetSpeakers { .. } => "set_speakers",
            AiOp::SuggestZooms { .. } => "suggest_zooms",
            AiOp::SuggestCuts { .. } => "suggest_cuts",
        }
    }

    /// The capture the action works on.
    pub fn capture_id(&self) -> Option<&str> {
        match self {
            AiOp::Transcribe { capture_id, .. }
            | AiOp::GetTranscript { capture_id }
            | AiOp::GetCaptions { capture_id }
            | AiOp::SetCaptions { capture_id, .. }
            | AiOp::ReplaceInCaptions { capture_id, .. }
            | AiOp::ExportCaptions { capture_id, .. }
            | AiOp::Diarize { capture_id, .. }
            | AiOp::SetSpeakers { capture_id, .. }
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
            | AiOp::GetCaptions { capture_id }
            | AiOp::SuggestZooms { capture_id }
            | AiOp::SuggestCuts { capture_id } => {
                o.insert("capture_id".into(), json!(capture_id));
            }
            AiOp::SetCaptions { capture_id, lines } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("lines".into(), json!(lines));
            }
            AiOp::ReplaceInCaptions {
                capture_id,
                replacements,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("replacements".into(), json!(replacements));
            }
            AiOp::ExportCaptions {
                capture_id,
                format,
                speaker_names,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("format".into(), json!(format.ext()));
                if *speaker_names {
                    o.insert("speaker_names".into(), json!(true));
                }
            }
            AiOp::Diarize {
                capture_id,
                hints,
                headset,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                for (k, v) in [
                    ("num_speakers", hints.num_speakers),
                    ("min_speakers", hints.min_speakers),
                    ("max_speakers", hints.max_speakers),
                ] {
                    if let Some(n) = v {
                        o.insert(k.into(), json!(n));
                    }
                }
                if *headset {
                    o.insert("headset".into(), json!(true));
                }
            }
            AiOp::SetSpeakers { capture_id, edit } => {
                o.insert("capture_id".into(), json!(capture_id));
                if !edit.rename.is_empty() {
                    let m: Map<String, Value> = edit
                        .rename
                        .iter()
                        .map(|(id, name)| (id.to_string(), json!(name)))
                        .collect();
                    o.insert("rename".into(), Value::Object(m));
                }
                if !edit.merge.is_empty() {
                    let v: Vec<Value> = edit
                        .merge
                        .iter()
                        .map(|(from, into)| json!({ "from": from, "into": into }))
                        .collect();
                    o.insert("merge".into(), json!(v));
                }
                if !edit.assign.is_empty() {
                    o.insert("assign".into(), json!(edit.assign));
                }
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
        AiOp::SetCaptions { capture_id, lines } => {
            format!("Set {} caption lines of {capture_id}", lines.len())
        }
        AiOp::ReplaceInCaptions {
            capture_id,
            replacements,
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
        AiOp::Diarize { capture_id, .. } => format!("Tell the speakers of {capture_id} apart"),
        AiOp::SetSpeakers { capture_id, .. } => format!("Change the speakers of {capture_id}"),
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
        AiOp::SetCaptions { capture_id, lines } => {
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

/// A speaker id: a whole number or a numeric string such as `"2"`.
fn speaker_id(v: &Value, field: &str) -> Result<u32, CmError> {
    let id = match v {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().parse::<u64>().ok(),
        _ => None,
    };
    id.filter(|n| *n <= u64::from(u32::MAX))
        .map(|n| n as u32)
        .ok_or_else(|| CmError::invalid_field(field, "must be a speaker id such as 2"))
}

/// `rename`, `merge` and `assign` of `set_speakers` (AI-8).
fn speaker_edit(f: &Fields<'_>) -> Result<cm_ai::speakers::SpeakerEdit, CmError> {
    use cm_ai::speakers::{Assign, SpeakerEdit, MAX_NAME_CHARS};
    let mut edit = SpeakerEdit::default();
    match f.obj.get("rename") {
        None | Some(Value::Null) => {}
        Some(Value::Object(o)) => {
            if o.len() > MAX_SPEAKER_CHANGES {
                return Err(CmError::invalid_field("rename", "too many speakers"));
            }
            for (k, v) in o {
                let id = speaker_id(&Value::String(k.clone()), "rename")?;
                let name = v
                    .as_str()
                    .ok_or_else(|| CmError::invalid_field("rename", "a name must be a string"))?;
                if name.chars().count() > MAX_NAME_CHARS {
                    return Err(CmError::invalid_field(
                        "rename",
                        format!("a name has at most {MAX_NAME_CHARS} characters"),
                    ));
                }
                edit.rename.push((id, name.to_owned()));
            }
        }
        Some(_) => {
            return Err(CmError::invalid_field(
                "rename",
                "must be an object such as {\"2\": \"Linh\"}",
            ))
        }
    }
    let list = |key: &str| -> Result<Vec<&Map<String, Value>>, CmError> {
        match f.obj.get(key) {
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(Value::Array(a)) if a.len() <= MAX_SPEAKER_CHANGES => a
                .iter()
                .map(|i| {
                    i.as_object()
                        .ok_or_else(|| CmError::invalid_field(key, "each item must be an object"))
                })
                .collect(),
            Some(Value::Array(_)) => Err(CmError::invalid_field(key, "too many items")),
            Some(_) => Err(CmError::invalid_field(key, "must be an array")),
        }
    };
    for o in list("merge")? {
        if let Some(k) = o.keys().find(|k| !matches!(k.as_str(), "from" | "into")) {
            return Err(CmError::invalid_field(
                &format!("merge.{k}"),
                "unknown parameter",
            ));
        }
        let get = |k: &str| {
            o.get(k)
                .ok_or_else(|| CmError::invalid_field(&format!("merge.{k}"), "is required"))
                .and_then(|v| speaker_id(v, &format!("merge.{k}")))
        };
        edit.merge.push((get("from")?, get("into")?));
    }
    for o in list("assign")? {
        if let Some(k) = o
            .keys()
            .find(|k| !matches!(k.as_str(), "start" | "end" | "speaker"))
        {
            return Err(CmError::invalid_field(
                &format!("assign.{k}"),
                "unknown parameter",
            ));
        }
        let time = |k: &str| match o.get(k) {
            Some(v) => parse_time(&format!("assign.{k}"), v),
            None => Err(CmError::invalid_field(
                &format!("assign.{k}"),
                "is required",
            )),
        };
        // `end` after `start` is checked when the edit runs (`apply_edit`).
        let (start, end) = (time("start")?, time("end")?);
        let speaker = o
            .get("speaker")
            .ok_or_else(|| CmError::invalid_field("assign.speaker", "is required"))
            .and_then(|v| speaker_id(v, "assign.speaker"))?;
        edit.assign.push(Assign {
            start,
            end,
            speaker,
        });
    }
    // An empty edit is refused when it runs (`Nothing to change`), not here: the tool's schema
    // lists all three as optional.
    Ok(edit)
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
        "get_transcript" | "get_captions" | "suggest_zooms" | "suggest_cuts" => {
            f.only(&["capture_id"])?;
            let capture_id = f.str_req("capture_id")?;
            match method {
                "get_transcript" => AiOp::GetTranscript { capture_id },
                "get_captions" => AiOp::GetCaptions { capture_id },
                "suggest_zooms" => AiOp::SuggestZooms { capture_id },
                _ => AiOp::SuggestCuts { capture_id },
            }
        }
        "set_captions" => {
            f.only(&["capture_id", "lines", "request_id"])?;
            AiOp::SetCaptions {
                capture_id: f.str_req("capture_id")?,
                lines: caption_lines(f)?,
            }
        }
        "replace_in_captions" => {
            f.only(&["capture_id", "replacements", "request_id"])?;
            AiOp::ReplaceInCaptions {
                capture_id: f.str_req("capture_id")?,
                replacements: replacements(f)?,
            }
        }
        "export_captions" => {
            f.only(&[
                "capture_id",
                "format",
                "timestamps",
                "speaker_names",
                "request_id",
            ])?;
            let timestamps = f.bool_or("timestamps", false)?;
            let speaker_names = f.bool_or("speaker_names", false)?;
            let format = match f.str_req("format")?.as_str() {
                "srt" => CaptionFormat::Srt,
                "vtt" => CaptionFormat::Vtt,
                "txt" if timestamps => CaptionFormat::TxtTimed,
                "txt" => CaptionFormat::Txt,
                "md" => CaptionFormat::Md,
                _ => {
                    return Err(CmError::invalid_field(
                        "format",
                        "must be `srt`, `vtt`, `txt` or `md`",
                    ))
                }
            };
            if timestamps && format != CaptionFormat::TxtTimed {
                return Err(CmError::invalid_field(
                    "timestamps",
                    "only applies to the `txt` format (SRT and VTT always have times, Markdown always has them)",
                ));
            }
            if speaker_names && format != CaptionFormat::Srt {
                return Err(CmError::invalid_field(
                    "speaker_names",
                    "only applies to `srt` (VTT, TXT and Markdown always name the speakers when they are known)",
                ));
            }
            AiOp::ExportCaptions {
                capture_id: f.str_req("capture_id")?,
                format,
                speaker_names,
            }
        }
        "diarize" => {
            f.only(&[
                "capture_id",
                "num_speakers",
                "min_speakers",
                "max_speakers",
                "headset",
                "request_id",
            ])?;
            let count = |key: &str| -> Result<Option<u32>, CmError> {
                match f.obj.get(key) {
                    None | Some(Value::Null) => Ok(None),
                    Some(v) => v
                        .as_u64()
                        .filter(|n| (1..=32).contains(n))
                        .map(|n| Some(n as u32))
                        .ok_or_else(|| {
                            CmError::invalid_field(key, "must be a whole number from 1 to 32")
                        }),
                }
            };
            let hints = cm_ai::speakers::DiarizeHints {
                num_speakers: count("num_speakers")?,
                min_speakers: count("min_speakers")?,
                max_speakers: count("max_speakers")?,
            };
            hints.check()?;
            AiOp::Diarize {
                capture_id: f.str_req("capture_id")?,
                hints,
                headset: f.bool_or("headset", false)?,
            }
        }
        "set_speakers" => {
            f.only(&["capture_id", "rename", "merge", "assign", "request_id"])?;
            AiOp::SetSpeakers {
                capture_id: f.str_req("capture_id")?,
                edit: speaker_edit(f)?,
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
