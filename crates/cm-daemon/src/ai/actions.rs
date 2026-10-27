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
        /// SRT only: prefix every cue with `NAME:` (AI-8; default off).
        speaker_names: bool,
    },
    /// AI-5: translate the captions into `target_language` (a job).
    TranslateCaptions {
        capture_id: String,
        target_language: String,
        engine: cm_ai::translate::EngineChoice,
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
    /// AI-9: the built-in report templates.
    ListReportTemplates,
    /// AI-9: a `report` job. `report_id` regenerates that report (replaced only on success).
    GenerateReport {
        capture_id: String,
        /// A template id.
        template: String,
        language: Option<String>,
        context: cm_ai::report::ReportContext,
        report_id: Option<String>,
    },
    GetReport {
        capture_id: String,
        report_id: Option<String>,
    },
    ExportReport {
        capture_id: String,
        report_id: String,
        format: ReportFormat,
        out_dir: Option<String>,
    },
    DeleteReport {
        capture_id: String,
        report_id: String,
    },
    /// The Report panel's "edit as Markdown".
    EditReport {
        capture_id: String,
        report_id: String,
        markdown: String,
    },
    /// Tick or untick one task item of a report.
    CheckReportItem {
        capture_id: String,
        report_id: String,
        section: String,
        index: usize,
        checked: bool,
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
            AiOp::TranslateCaptions { .. } => "translate_captions",
            AiOp::ListReportTemplates => "list_report_templates",
            AiOp::GenerateReport { .. } => "generate_report",
            AiOp::GetReport { .. } => "get_report",
            AiOp::ExportReport { .. } => "export_report",
            AiOp::DeleteReport { .. } => "delete_report",
            AiOp::EditReport { .. } => "edit_report",
            AiOp::CheckReportItem { .. } => "check_report_item",
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
            | AiOp::Diarize { capture_id, .. }
            | AiOp::SetSpeakers { capture_id, .. }
            | AiOp::GenerateReport { capture_id, .. }
            | AiOp::GetReport { capture_id, .. }
            | AiOp::ExportReport { capture_id, .. }
            | AiOp::DeleteReport { capture_id, .. }
            | AiOp::EditReport { capture_id, .. }
            | AiOp::CheckReportItem { capture_id, .. }
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
                speaker_names,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("format".into(), json!(format.ext()));
                insert_language(&mut o, language);
                if *speaker_names {
                    o.insert("speaker_names".into(), json!(true));
                }
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
            AiOp::ListReportTemplates => {}
            AiOp::GenerateReport {
                capture_id,
                template,
                language,
                context,
                report_id,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("template".into(), json!(template));
                insert_language(&mut o, language);
                if !context.used_fields().is_empty() {
                    o.insert("context".into(), json!(context));
                }
                if let Some(r) = report_id {
                    o.insert("report_id".into(), json!(r));
                }
            }
            AiOp::GetReport {
                capture_id,
                report_id,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                if let Some(r) = report_id {
                    o.insert("report_id".into(), json!(r));
                }
            }
            AiOp::ExportReport {
                capture_id,
                report_id,
                format,
                out_dir,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("report_id".into(), json!(report_id));
                o.insert("format".into(), json!(format.ext()));
                if let Some(d) = out_dir {
                    o.insert("out_dir".into(), json!(d));
                }
            }
            AiOp::DeleteReport {
                capture_id,
                report_id,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("report_id".into(), json!(report_id));
            }
            AiOp::EditReport {
                capture_id,
                report_id,
                markdown,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("report_id".into(), json!(report_id));
                o.insert("characters".into(), json!(markdown.chars().count()));
            }
            AiOp::CheckReportItem {
                capture_id,
                report_id,
                section,
                index,
                checked,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("report_id".into(), json!(report_id));
                o.insert("section".into(), json!(section));
                o.insert("index".into(), json!(index));
                o.insert("checked".into(), json!(checked));
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
        AiOp::Diarize { capture_id, .. } => format!("Tell the speakers of {capture_id} apart"),
        AiOp::SetSpeakers { capture_id, .. } => format!("Change the speakers of {capture_id}"),
        AiOp::GenerateReport {
            capture_id,
            template,
            ..
        } => format!("Write a `{template}` report of {capture_id}"),
        AiOp::DeleteReport {
            capture_id,
            report_id,
        } => format!("Delete the report {report_id} of {capture_id}"),
        AiOp::EditReport {
            capture_id,
            report_id,
            ..
        } => format!("Edit the report {report_id} of {capture_id}"),
        AiOp::CheckReportItem {
            capture_id,
            report_id,
            ..
        } => format!("Tick an item of the report {report_id} of {capture_id}"),
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

fn report_id_opt(f: &Fields<'_>) -> Result<Option<String>, CmError> {
    match f.str_opt("report_id")? {
        None => Ok(None),
        Some(r) if cm_ai::report::store::valid_id(&r) => Ok(Some(r)),
        Some(_) => Err(CmError::invalid_field(
            "report_id",
            "must be a report id such as `rpt_1a2b3c4d` (see `get_report`)",
        )),
    }
}

/// The `context` object of `generate_report`: `title`, `attendees`, `agenda`, `role`,
/// `competencies`; the template must use every field that is set.
fn report_context(
    f: &Fields<'_>,
    template: &cm_ai::report::Template,
) -> Result<cm_ai::report::ReportContext, CmError> {
    let ctx = match f.obj.get("context") {
        None | Some(Value::Null) => return Ok(Default::default()),
        Some(v) => v,
    };
    let o = ctx
        .as_object()
        .ok_or_else(|| CmError::invalid_field("context", "must be an object"))?;
    for k in o.keys() {
        if !matches!(
            k.as_str(),
            "title" | "attendees" | "agenda" | "role" | "competencies"
        ) {
            return Err(CmError::invalid_field(
                &format!("context.{k}"),
                "unknown parameter",
            ));
        }
    }
    let string = |k: &str| -> Result<Option<String>, CmError> {
        match o.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(CmError::invalid_field(
                &format!("context.{k}"),
                "must be a string",
            )),
        }
    };
    let list = |k: &str| -> Result<Vec<String>, CmError> {
        match o.get(k) {
            None | Some(Value::Null) => Ok(vec![]),
            Some(Value::Array(a)) => a
                .iter()
                .map(|x| {
                    x.as_str().map(str::to_owned).ok_or_else(|| {
                        CmError::invalid_field(
                            &format!("context.{k}"),
                            "must be an array of strings",
                        )
                    })
                })
                .collect(),
            Some(_) => Err(CmError::invalid_field(
                &format!("context.{k}"),
                "must be an array of strings",
            )),
        }
    };
    cm_ai::report::ReportContext {
        title: string("title")?,
        attendees: list("attendees")?,
        agenda: string("agenda")?,
        role: string("role")?,
        competencies: list("competencies")?,
    }
    .checked(template)
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
                language: translation_language(f)?,
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
        "list_report_templates" => {
            f.only(&[])?;
            AiOp::ListReportTemplates
        }
        "generate_report" => {
            f.only(&[
                "capture_id",
                "template",
                "language",
                "context",
                "report_id",
                "request_id",
            ])?;
            let raw = f.str_req("template")?;
            let template = cm_ai::report::find_template(&raw)
                .ok_or_else(|| cm_ai::report::unknown_template(&raw))?;
            AiOp::GenerateReport {
                capture_id: f.str_req("capture_id")?,
                template: template.id.clone(),
                language: translation_language(f)?,
                context: report_context(f, template)?,
                report_id: report_id_opt(f)?,
            }
        }
        "get_report" => {
            f.only(&["capture_id", "report_id"])?;
            AiOp::GetReport {
                capture_id: f.str_req("capture_id")?,
                report_id: report_id_opt(f)?,
            }
        }
        "export_report" => {
            f.only(&["capture_id", "report_id", "format", "out_dir", "request_id"])?;
            let format = match f.str_req("format")?.as_str() {
                "md" => ReportFormat::Md,
                "txt" => ReportFormat::Txt,
                _ => return Err(CmError::invalid_field("format", "must be `md` or `txt`")),
            };
            AiOp::ExportReport {
                capture_id: f.str_req("capture_id")?,
                report_id: report_id_opt(f)?
                    .ok_or_else(|| CmError::invalid_field("report_id", "is required"))?,
                format,
                out_dir: f.str_opt("out_dir")?,
            }
        }
        "delete_report" => {
            f.only(&["capture_id", "report_id", "request_id"])?;
            AiOp::DeleteReport {
                capture_id: f.str_req("capture_id")?,
                report_id: report_id_opt(f)?
                    .ok_or_else(|| CmError::invalid_field("report_id", "is required"))?,
            }
        }
        "edit_report" => {
            f.only(&["capture_id", "report_id", "markdown", "request_id"])?;
            let markdown = f.str_req("markdown")?;
            if markdown.chars().count() > MAX_REPORT_MARKDOWN_CHARS {
                return Err(CmError::invalid_field(
                    "markdown",
                    format!("at most {MAX_REPORT_MARKDOWN_CHARS} characters"),
                ));
            }
            AiOp::EditReport {
                capture_id: f.str_req("capture_id")?,
                report_id: report_id_opt(f)?
                    .ok_or_else(|| CmError::invalid_field("report_id", "is required"))?,
                markdown,
            }
        }
        "check_report_item" => {
            f.only(&[
                "capture_id",
                "report_id",
                "section",
                "index",
                "checked",
                "request_id",
            ])?;
            let index = match f.obj.get("index") {
                Some(v) => v
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .filter(|n| *n < 1000)
                    .ok_or_else(|| {
                        CmError::invalid_field("index", "must be a whole number from 0 to 999")
                    })?,
                None => return Err(CmError::invalid_field("index", "is required")),
            };
            AiOp::CheckReportItem {
                capture_id: f.str_req("capture_id")?,
                report_id: report_id_opt(f)?
                    .ok_or_else(|| CmError::invalid_field("report_id", "is required"))?,
                section: f.str_req("section")?,
                index,
                checked: f.bool_or("checked", true)?,
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
