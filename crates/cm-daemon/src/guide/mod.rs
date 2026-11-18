//! Click Guide (GDE-1 to GDE-4): `start_guide`, `stop_guide`, `get_guide`, `set_guide`,
//! `export_guide`.
//!
//! A guide is a capture of the kind `guide` (see `cm_library::guide`). `start_guide` is
//! **Confirm** (it listens to clicks); the other four are Free. The HUD's controls are the same
//! actions: `set_guide { paused }`, `set_guide { note_step }`, `set_guide { remove_last }` and
//! `stop_guide`; the editor's are `set_guide { steps }` (reorder, titles, notes, delete,
//! duplicate, new note steps: one undo step) and the existing `undo` / `redo` on the guide id.
//! Step pictures are ordinary screenshots with the capture id `<guide_id>/<step_id>`, so
//! `annotate`, `get_capture`, `suggest_redactions` and `recognize_text` work on them.

use cm_core::annotation::StepStyle;
use cm_core::CmError;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::action::{Action, Fields, ScreenshotTarget};

pub mod access;
pub mod clicks;
pub mod export;
pub mod pdf;
pub mod ring;
pub mod session;
pub mod titles;

pub use access::{AccessibleNames, NoNames, ScriptedNames};
pub use clicks::{ClickSink, GuideClicks, LocalClick, NoClicks, ScriptedClicks};
pub(crate) use session::GuideRuntime;

pub const START: &str = "start_guide";
pub const STOP: &str = "stop_guide";
pub const GET: &str = "get_guide";
pub const SET: &str = "set_guide";
pub const EXPORT: &str = "export_guide";
/// The five tool names.
pub const METHODS: &[&str] = &[START, STOP, GET, SET, EXPORT];

/// Longest countdown before the first click is listened to, seconds.
pub const MAX_COUNTDOWN_S: f64 = 10.0;

pub fn handles(name: &str) -> bool {
    METHODS.contains(&name)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GuideParams {
    pub op: GuideOp,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum GuideOp {
    Start {
        target: ScreenshotTarget,
        title: Option<String>,
        countdown_s: f64,
    },
    Stop {
        guide_id: Option<String>,
    },
    Get {
        guide_id: Option<String>,
    },
    Set(Box<SetArgs>),
    Export(Box<ExportArgs>),
}

impl GuideOp {
    pub fn method(&self) -> &'static str {
        match self {
            GuideOp::Start { .. } => START,
            GuideOp::Stop { .. } => STOP,
            GuideOp::Get { .. } => GET,
            GuideOp::Set(_) => SET,
            GuideOp::Export(_) => EXPORT,
        }
    }

    /// The guide this action works on (serializes actions per capture).
    pub fn guide_id(&self) -> Option<&str> {
        match self {
            GuideOp::Start { .. } => None,
            GuideOp::Stop { guide_id } | GuideOp::Get { guide_id } => guide_id.as_deref(),
            GuideOp::Set(a) => a.guide_id.as_deref(),
            GuideOp::Export(a) => Some(&a.guide_id),
        }
    }
}

/// A new note step from the HUD ("Type your password").
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoteArgs {
    pub title: String,
    pub note: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SetArgs {
    /// `None` means the guide that is being recorded (the HUD).
    pub guide_id: Option<String>,
    pub title: Option<String>,
    pub intro: Option<String>,
    pub numbering: Option<StepStyle>,
    pub steps: Option<Vec<cm_library::guide::StepEdit>>,
    /// HUD: Pause (`true`) and Resume (`false`).
    pub paused: Option<bool>,
    /// HUD: "Add note step".
    pub note_step: Option<NoteArgs>,
    /// HUD: "Undo last step".
    pub remove_last: bool,
    /// Focus crop on or off, for the steps named in `crop_steps`.
    pub focus_crop: Option<bool>,
    pub crop_steps: Vec<String>,
    /// Take a fresh frame of the followed target for these steps.
    pub retake: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Markdown,
    Html,
    Pdf,
    RichText,
}

impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Markdown => "markdown",
            Format::Html => "html",
            Format::Pdf => "pdf",
            Format::RichText => "rich_text",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Headings {
    /// `1. Title`
    #[default]
    Numbered,
    /// `Step 1 · Title`
    Step,
    /// `Title`
    TitleOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportArgs {
    pub guide_id: String,
    pub format: Format,
    /// A folder (Markdown) or a file or folder (HTML). Not used by `rich_text`.
    pub dest: Option<String>,
    pub headings: Headings,
    /// 1 or 2: the images are written at the reading size times this, never larger than the
    /// frame.
    pub image_scale: u32,
    pub intro: bool,
    pub notes: bool,
    pub note_steps: bool,
    /// PDF only: `a4` (default) or `letter`.
    pub paper: pdf::Paper,
}

pub(crate) fn parse(method: &str, f: &Fields<'_>) -> Result<Action, CmError> {
    let op = match method {
        START => {
            f.only(&["target", "title", "countdown_s", "request_id"])?;
            let target = ScreenshotTarget::parse(
                f.obj
                    .get("target")
                    .ok_or_else(|| CmError::invalid_field("target", "is required"))?,
            )?;
            let countdown_s = match f.obj.get("countdown_s") {
                None | Some(Value::Null) => 3.0,
                Some(v) => v
                    .as_f64()
                    .filter(|n| n.is_finite() && (0.0..=MAX_COUNTDOWN_S).contains(n))
                    .ok_or_else(|| {
                        CmError::invalid_field(
                            "countdown_s",
                            format!("must be between 0 and {MAX_COUNTDOWN_S}"),
                        )
                    })?,
            };
            GuideOp::Start {
                target,
                title: f.str_opt("title")?,
                countdown_s,
            }
        }
        STOP => {
            f.only(&["guide_id", "request_id"])?;
            GuideOp::Stop {
                guide_id: f.str_opt("guide_id")?,
            }
        }
        GET => {
            f.only(&["guide_id"])?;
            GuideOp::Get {
                guide_id: f.str_opt("guide_id")?,
            }
        }
        SET => GuideOp::Set(Box::new(parse_set(f)?)),
        EXPORT => GuideOp::Export(Box::new(parse_export(f)?)),
        other => {
            return Err(CmError::invalid_input(format!(
                "`{other}` is not a guide tool"
            )))
        }
    };
    let request_id = f.request_id()?;
    Ok(Action::Guide(GuideParams { op, request_id }))
}

fn string_list(f: &Fields<'_>, key: &str) -> Result<Vec<String>, CmError> {
    match f.obj.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => a
            .iter()
            .enumerate()
            .map(|(i, v)| {
                v.as_str().map(str::to_owned).ok_or_else(|| {
                    CmError::invalid_field(&format!("{key}[{i}]"), "must be a string")
                })
            })
            .collect(),
        Some(_) => Err(CmError::invalid_field(key, "must be an array of strings")),
    }
}

fn parse_set(f: &Fields<'_>) -> Result<SetArgs, CmError> {
    f.only(&[
        "guide_id",
        "title",
        "intro",
        "numbering",
        "steps",
        "paused",
        "note_step",
        "remove_last",
        "focus_crop",
        "crop_steps",
        "retake",
        "request_id",
    ])?;
    let numbering = match f.str_opt("numbering")?.as_deref() {
        None => None,
        Some("number" | "1") => Some(StepStyle::Number),
        Some("upper" | "A") => Some(StepStyle::Upper),
        Some("lower" | "a") => Some(StepStyle::Lower),
        Some(_) => {
            return Err(CmError::invalid_field(
                "numbering",
                "must be `number`, `upper` or `lower`",
            ))
        }
    };
    let steps = match f.obj.get("steps") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let arr = v
                .as_array()
                .ok_or_else(|| CmError::invalid_field("steps", "must be an array"))?;
            let mut out = Vec::with_capacity(arr.len());
            for (i, e) in arr.iter().enumerate() {
                let field = format!("steps[{i}]");
                let o = e
                    .as_object()
                    .ok_or_else(|| CmError::invalid_field(&field, "must be an object"))?;
                for k in o.keys() {
                    if !matches!(k.as_str(), "id" | "title" | "note" | "delete" | "duplicate") {
                        return Err(CmError::invalid_field(
                            &format!("{field}.{k}"),
                            "unknown parameter",
                        ));
                    }
                }
                let s = |k: &str| match o.get(k) {
                    None | Some(Value::Null) => Ok(None),
                    Some(Value::String(s)) => Ok(Some(s.clone())),
                    Some(_) => Err(CmError::invalid_field(
                        &format!("{field}.{k}"),
                        "must be a string",
                    )),
                };
                let b = |k: &str| match o.get(k) {
                    None | Some(Value::Null) => Ok(false),
                    Some(Value::Bool(b)) => Ok(*b),
                    Some(_) => Err(CmError::invalid_field(
                        &format!("{field}.{k}"),
                        "must be a boolean",
                    )),
                };
                out.push(cm_library::guide::StepEdit {
                    id: s("id")?,
                    title: s("title")?,
                    note: s("note")?,
                    delete: b("delete")?,
                    duplicate: b("duplicate")?,
                });
            }
            Some(out)
        }
    };
    let note_step = match f.obj.get("note_step") {
        None | Some(Value::Null) => None,
        Some(Value::Object(o)) => {
            let text = |k: &str| match o.get(k) {
                None | Some(Value::Null) => Ok(String::new()),
                Some(Value::String(s)) => Ok(s.clone()),
                Some(_) => Err(CmError::invalid_field(
                    &format!("note_step.{k}"),
                    "must be a string",
                )),
            };
            Some(NoteArgs {
                title: text("title")?,
                note: text("note")?,
            })
        }
        Some(_) => return Err(CmError::invalid_field("note_step", "must be an object")),
    };
    Ok(SetArgs {
        guide_id: f.str_opt("guide_id")?,
        title: f.str_opt("title")?,
        intro: f.str_opt("intro")?,
        numbering,
        steps,
        paused: f.bool_opt("paused")?,
        note_step,
        remove_last: f.bool_or("remove_last", false)?,
        focus_crop: f.bool_opt("focus_crop")?,
        crop_steps: string_list(f, "crop_steps")?,
        retake: string_list(f, "retake")?,
    })
}

fn parse_export(f: &Fields<'_>) -> Result<ExportArgs, CmError> {
    f.only(&[
        "guide_id",
        "format",
        "dest",
        "headings",
        "image_scale",
        "intro",
        "notes",
        "note_steps",
        "paper",
        "request_id",
    ])?;
    let paper = match f.str_opt("paper")?.as_deref() {
        None | Some("a4") => pdf::Paper::A4,
        Some("letter") => pdf::Paper::Letter,
        Some(_) => return Err(CmError::invalid_field("paper", "must be `a4` or `letter`")),
    };
    let format = match f.str_req("format")?.as_str() {
        "markdown" => Format::Markdown,
        "html" => Format::Html,
        "pdf" => Format::Pdf,
        "rich_text" => Format::RichText,
        _ => {
            return Err(CmError::invalid_field(
                "format",
                "must be `markdown`, `html`, `pdf` or `rich_text`",
            ))
        }
    };
    let headings = match f.str_opt("headings")?.as_deref() {
        None | Some("numbered") => Headings::Numbered,
        Some("step") => Headings::Step,
        Some("title_only") => Headings::TitleOnly,
        Some(_) => {
            return Err(CmError::invalid_field(
                "headings",
                "must be `numbered`, `step` or `title_only`",
            ))
        }
    };
    let image_scale = match f.obj.get("image_scale") {
        None | Some(Value::Null) => 1,
        Some(v) => match v.as_u64() {
            Some(n @ (1 | 2)) => n as u32,
            _ => return Err(CmError::invalid_field("image_scale", "must be 1 or 2")),
        },
    };
    // The MCP schema lists `dest` as required for every format; `rich_text` ignores it, so the
    // parser lets the CLI and the app leave it out there.
    let dest = f.str_opt("dest")?;
    if dest.is_none() && format != Format::RichText {
        return Err(CmError::invalid_field("dest", "is required"));
    }
    Ok(ExportArgs {
        guide_id: f.str_req("guide_id")?,
        format,
        dest,
        headings,
        image_scale,
        intro: f.bool_or("intro", true)?,
        notes: f.bool_or("notes", true)?,
        note_steps: f.bool_or("note_steps", true)?,
        paper,
    })
}

/// One sentence for the confirmation dialog and the activity log.
pub fn summary(p: &GuideParams) -> String {
    match &p.op {
        GuideOp::Start { .. } => {
            "Start a Click Guide: CaptureMann will take a screenshot of each mouse click \
             (clicks only; keystrokes are never recorded)"
                .to_owned()
        }
        GuideOp::Stop { .. } => "Stop the Click Guide".to_owned(),
        GuideOp::Get { guide_id } => {
            format!("Read guide {}", guide_id.as_deref().unwrap_or("(current)"))
        }
        GuideOp::Set(a) => format!(
            "Edit guide {}",
            a.guide_id.as_deref().unwrap_or("(current)")
        ),
        GuideOp::Export(a) => format!("Export guide {} as {}", a.guide_id, a.format.as_str()),
    }
}

/// The rows of the start dialog (`start_guide`): flat English text a client words itself from
/// [`confirm_params`] (the raw serialized parameters are nested JSON, not rows).
pub fn confirm_details(p: &GuideParams) -> Option<Value> {
    // BUG-3: the export dialog names the exact place, made absolute and free of `..` and links,
    // so what is shown is what is written to.
    if let GuideOp::Export(a) = &p.op {
        let mut rows = serde_json::Map::new();
        rows.insert("guide".into(), json!(a.guide_id));
        rows.insert("format".into(), json!(a.format.as_str()));
        if let Some(d) = a.dest.as_deref().filter(|d| !d.is_empty()) {
            let shown = export::resolve_path(std::path::Path::new(d));
            rows.insert("path".into(), json!(export::plain_path(&shown)));
        }
        return Some(Value::Object(rows));
    }
    let GuideOp::Start {
        target,
        title,
        countdown_s,
    } = &p.op
    else {
        return None;
    };
    let target = match target {
        ScreenshotTarget::Display { display_id } => format!("display {display_id}"),
        ScreenshotTarget::Window { window_id } => format!("window {window_id}"),
        ScreenshotTarget::Region { display_id, .. } => format!("area of display {display_id}"),
    };
    let mut rows = serde_json::Map::new();
    rows.insert("target".into(), json!(target));
    if let Some(t) = title.as_deref().filter(|t| !t.trim().is_empty()) {
        rows.insert("title".into(), json!(t));
    }
    rows.insert(
        "countdown".into(),
        json!(format!("{} s", countdown_s.round())),
    );
    rows.insert("records".into(), json!("mouse clicks only"));
    Some(Value::Object(rows))
}

pub fn confirm_params(p: &GuideParams) -> Value {
    match &p.op {
        GuideOp::Start {
            target,
            title,
            countdown_s,
        } => json!({ "target": target, "title": title, "countdown_s": countdown_s,
                     "records": "mouse clicks only" }),
        _ => json!({}),
    }
}

/// The JSON of a guide for `get_guide` and the replies of the other tools.
pub(crate) fn guide_json(id: &str, g: &cm_library::guide::GuideBundle, state: &str) -> Value {
    let doc = g.doc();
    let steps: Vec<Value> = doc
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut o = Map::new();
            o.insert("id".into(), json!(s.id));
            o.insert("number".into(), json!(i + 1));
            o.insert(
                "label".into(),
                json!(cm_core::annotation::step_label(
                    doc.numbering,
                    1,
                    u32::try_from(i + 1).unwrap_or(u32::MAX)
                )),
            );
            o.insert("type".into(), json!(s.kind));
            o.insert("title".into(), json!(s.title));
            // `accessibility`, `screen_text` or `generic` for a title CaptureMann made; absent for one a person
            // or an agent wrote.
            if let Some(src) = &s.title_source {
                o.insert("title_source".into(), json!(src));
            }
            o.insert("note".into(), json!(s.note));
            if let Some(img) = &s.image {
                o.insert("image_id".into(), json!(format!("{id}/{}", s.id)));
                o.insert(
                    "image_file".into(),
                    json!(format!("steps/{img}/original.png")),
                );
            }
            if let Some(c) = &s.click {
                o.insert("click".into(), json!(c));
            }
            o.insert("focus_crop".into(), json!(s.focus_crop));
            Value::Object(o)
        })
        .collect();
    json!({
        "guide_id": id,
        "path": g.dir(),
        "title": doc.title,
        "intro": doc.intro,
        "numbering": doc.numbering,
        "state": state,
        "step_count": steps.len(),
        "steps": steps,
        "can_undo": g.can_undo(),
        "can_redo": g.can_redo(),
    })
}
