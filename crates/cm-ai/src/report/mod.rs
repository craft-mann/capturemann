//! Summaries and reports (AI-9): meeting minutes, one-on-one and interview notes, a product
//! presentation recap, lecture notes and a plain summary, made from a transcript by the AI-7 text
//! model.
//!
//! * [`Template`]: versioned data in `data/reports/*.json`, a section list plus the JSON schema of
//!   the model's reply (`additionalProperties: false` everywhere). The Interview notes schema has
//!   no score, rating, ranking or recommendation field ([`Template::forbidden_judging_fields`] is
//!   the check the tests run).
//! * [`prompt`]: the instructions, the context the user typed, speaker names, markers and the
//!   transcript with a `[seconds]` marker on every line.
//! * [`check`]: **every item cites the recording**. An `at` time that does not fall inside a
//!   transcript line is removed, an item left without a citation is dropped and counted
//!   (`dropped_uncited`); an owner that is not an attendee, a speaker name or a word of the
//!   transcript becomes "Unassigned".
//! * [`generate`]: one call for a short recording, notes per chunk and merge passes for a long one
//!   (the shared [`crate::text_model::chunk`] helper), always the same schema.
//! * [`render`]: the Markdown of a report, plain text, golden-tested.
//! * [`store`]: `reports/<id>.json` and `reports/<id>.md` in the bundle, with the versions that
//!   make create, edit and delete undoable (the manifest key `reports` is the index; the daemon
//!   changes it with `rec_edit::apply_extra`).
//!
//! Nothing here talks to the network: [`generate::generate`] takes a [`crate::text_model::TextModel`].

pub mod check;
pub mod generate;
pub mod prompt;
pub mod render;
pub mod store;
#[cfg(test)]
mod tests;

use std::sync::OnceLock;

use cm_core::CmError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Bundle folder of the reports.
pub const REPORTS_DIR: &str = "reports";
/// Format version of `reports/<id>.json`.
pub const REPORT_VERSION: u32 = 1;
/// A transcript with fewer words than this is not worth a report (AI-9 edge cases).
pub const MIN_WORDS: usize = 30;
/// The message for it.
pub const TOO_LITTLE_SPEECH: &str = "Too little speech for a report";
/// What an empty section says.
pub const NOTHING_FOUND: &str = "Nothing found";
/// Seconds a citation may lie outside a transcript line and still count as inside it (a model
/// that cites the second after a line ended did not invent anything).
pub const CITE_TOLERANCE_S: f64 = 1.0;
/// The job type of a report run.
pub const JOB_REPORT: &str = "report";

/// Sections whose items can be ticked off (the tasks). `check_report_item` refuses any other.
pub const CHECKABLE_SECTIONS: [&str; 4] =
    ["action_items", "agreed_actions", "follow_ups", "next_steps"];

/// Whether the items of section `id` can be ticked.
pub fn is_checkable(id: &str) -> bool {
    CHECKABLE_SECTIONS.contains(&id)
}

/// Where a section's content comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// The text model writes it (it is part of the schema).
    #[default]
    Model,
    /// Built from `manifest.markers`, never by the model.
    Markers,
    /// Built from the context the user typed (role, people).
    Context,
}

/// How a field is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    #[default]
    Text,
    /// An array of short strings.
    List,
}

/// Which names a person field may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Person {
    /// An owner: a name that is not allowed becomes "Unassigned".
    Owner,
    /// An attendee: an item with a name that is not allowed is dropped.
    Attendee,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Field {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub kind: FieldKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<Person>,
    /// May be missing from a model item.
    #[serde(default)]
    pub optional: bool,
}

/// How [`render`] lays an item out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// `- first — Label: value · Label: value [12:03]`
    #[default]
    Inline,
    /// The first field on a line, every other field on its own indented line.
    Block,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Section {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub layout: Layout,
    /// Items may have no citation (the attendees of a meeting who never spoke).
    #[serde(default)]
    pub at_optional: bool,
    #[serde(default)]
    pub max_items: usize,
    pub fields: Vec<Field>,
}

/// One built-in template.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    /// Bumped when the sections or the schema change; stored in every report.
    pub version: u32,
    pub description: String,
    /// The context fields the report panel offers for it.
    pub context_fields: Vec<String>,
    /// Recording presets that suggest it after stop (VOC-1).
    pub suggested_for: Vec<String>,
    /// What the template adds to the instructions.
    pub instructions: String,
    /// The line the panel and the Markdown show (Interview notes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes_only: Option<String>,
    pub sections: Vec<Section>,
    /// The JSON schema of the model's reply.
    pub schema: Value,
}

const SOURCES: &[&str] = &[
    include_str!("../../data/reports/summary.json"),
    include_str!("../../data/reports/minutes.json"),
    include_str!("../../data/reports/one_on_one.json"),
    include_str!("../../data/reports/interview.json"),
    include_str!("../../data/reports/product_recap.json"),
    include_str!("../../data/reports/lecture.json"),
];

/// The built-in templates, in the order the report panel lists them.
pub fn templates() -> &'static [Template] {
    static ALL: OnceLock<Vec<Template>> = OnceLock::new();
    ALL.get_or_init(|| {
        SOURCES
            .iter()
            .filter_map(|s| serde_json::from_str::<Template>(s).ok())
            .collect()
    })
}

/// A template by id (`minutes`), or by its name in any case with `-` or `_` for spaces
/// (`meeting-minutes`, `Interview notes`).
pub fn find_template(raw: &str) -> Option<&'static Template> {
    let key = raw.trim().to_lowercase().replace(['-', ' '], "_");
    templates()
        .iter()
        .find(|t| t.id == key || t.name.to_lowercase().replace(['-', ' '], "_") == key)
}

/// The error for an unknown template.
pub fn unknown_template(raw: &str) -> CmError {
    let ids: Vec<&str> = templates().iter().map(|t| t.id.as_str()).collect();
    CmError::invalid_field(
        "template",
        format!("unknown template `{raw}`; use one of: {}", ids.join(", ")),
    )
}

/// Words that must never be (part of) a property name of a template that only takes notes.
const JUDGING: &[&str] = &[
    "score",
    "rating",
    "rate",
    "rank",
    "ranking",
    "recommend",
    "recommendation",
    "hire",
    "grade",
    "verdict",
];

fn is_judging_word(name: &str) -> bool {
    name.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|w| JUDGING.contains(&w) || JUDGING.contains(&w.strip_suffix('s').unwrap_or(w)))
}

impl Template {
    /// The `list_report_templates` entry.
    pub fn summary_json(&self) -> Value {
        let mut v = json!({
            "id": self.id,
            "name": self.name,
            "version": self.version,
            "description": self.description,
            "sections": self.sections.iter().map(|s| json!({ "id": s.id, "title": s.title })).collect::<Vec<_>>(),
            "context_fields": self.context_fields,
            "suggested_for": self.suggested_for,
        });
        if let (Some(o), Some(n)) = (v.as_object_mut(), &self.notes_only) {
            o.insert("notes_only".into(), json!(n));
        }
        v
    }

    /// The schema of the model's reply for one section (an array of items).
    pub fn section(&self, id: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.id == id)
    }

    /// The sections the model writes.
    pub fn model_sections(&self) -> impl Iterator<Item = &Section> {
        self.sections.iter().filter(|s| s.source == Source::Model)
    }

    /// Every property name (at any depth, also in `required` lists) of `value` that looks like a
    /// verdict on a person: `score`, `rating`, `rank`, `recommend`, `hire`, `grade`... AI-9.AC4
    /// asserts this is empty for the Interview notes schema and for every stored Interview report.
    pub fn forbidden_judging_fields(value: &Value) -> Vec<String> {
        fn walk(v: &Value, out: &mut Vec<String>) {
            match v {
                Value::Object(m) => {
                    for (k, child) in m {
                        if is_judging_word(k) {
                            out.push(k.clone());
                        }
                        if k == "required" {
                            if let Some(names) = child.as_array() {
                                out.extend(
                                    names
                                        .iter()
                                        .filter_map(Value::as_str)
                                        .filter(|n| is_judging_word(n))
                                        .map(str::to_owned),
                                );
                            }
                        }
                        walk(child, out);
                    }
                }
                Value::Array(a) => a.iter().for_each(|c| walk(c, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        walk(value, &mut out);
        out
    }
}

/// What the user types in the Report panel (all optional).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct ReportContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attendees: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agenda: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub competencies: Vec<String>,
}

pub const MAX_TITLE_CHARS: usize = 200;
pub const MAX_ATTENDEES: usize = 50;
pub const MAX_NAME_CHARS: usize = 80;
pub const MAX_AGENDA_CHARS: usize = 4000;
pub const MAX_ROLE_CHARS: usize = 200;
pub const MAX_COMPETENCIES: usize = 30;
pub const MAX_COMPETENCY_CHARS: usize = 120;

impl ReportContext {
    /// The context fields that are set.
    pub fn used_fields(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.title.is_some() {
            v.push("title");
        }
        if !self.attendees.is_empty() {
            v.push("attendees");
        }
        if self.agenda.is_some() {
            v.push("agenda");
        }
        if self.role.is_some() {
            v.push("role");
        }
        if !self.competencies.is_empty() {
            v.push("competencies");
        }
        v
    }

    /// Trims everything, drops empty entries and checks the limits and that `template` uses every
    /// field that is set.
    pub fn checked(mut self, template: &Template) -> Result<ReportContext, CmError> {
        fn one(field: &str, v: Option<String>, max: usize) -> Result<Option<String>, CmError> {
            let Some(s) = v.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()) else {
                return Ok(None);
            };
            if s.chars().count() > max {
                return Err(CmError::invalid_field(
                    &format!("context.{field}"),
                    format!("at most {max} characters"),
                ));
            }
            Ok(Some(s))
        }
        fn many(
            field: &str,
            v: Vec<String>,
            count: usize,
            max: usize,
        ) -> Result<Vec<String>, CmError> {
            let v: Vec<String> = v
                .into_iter()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect();
            if v.len() > count {
                return Err(CmError::invalid_field(
                    &format!("context.{field}"),
                    format!("at most {count} entries"),
                ));
            }
            if v.iter().any(|s| s.chars().count() > max) {
                return Err(CmError::invalid_field(
                    &format!("context.{field}"),
                    format!("each entry at most {max} characters"),
                ));
            }
            Ok(v)
        }
        self.title = one("title", self.title, MAX_TITLE_CHARS)?;
        self.agenda = one("agenda", self.agenda, MAX_AGENDA_CHARS)?;
        self.role = one("role", self.role, MAX_ROLE_CHARS)?;
        self.attendees = many("attendees", self.attendees, MAX_ATTENDEES, MAX_NAME_CHARS)?;
        self.competencies = many(
            "competencies",
            self.competencies,
            MAX_COMPETENCIES,
            MAX_COMPETENCY_CHARS,
        )?;
        for used in self.used_fields() {
            if !template.context_fields.iter().any(|f| f == used) {
                return Err(CmError::invalid_field(
                    &format!("context.{used}"),
                    format!(
                        "is not used by {} (it uses: {})",
                        template.name,
                        template.context_fields.join(", ")
                    ),
                ));
            }
        }
        Ok(self)
    }
}

/// A transcript line as the report sees it: caption lines (the edited ones win) with the speaker
/// who said most of it. Times are seconds of session time.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub start: f64,
    pub end: f64,
    pub speaker: Option<String>,
    pub text: String,
}

/// A marker (`manifest.markers`) in session seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerIn {
    pub at: f64,
    pub label: Option<String>,
}

/// The words of the lines.
pub fn word_count(lines: &[Line]) -> usize {
    lines
        .iter()
        .map(|l| l.text.split_whitespace().count())
        .sum()
}

/// `invalid_input` when the lines have fewer than [`MIN_WORDS`] words.
pub fn check_enough_speech(lines: &[Line]) -> Result<(), CmError> {
    if word_count(lines) < MIN_WORDS {
        return Err(CmError::invalid_input(TOO_LITTLE_SPEECH));
    }
    Ok(())
}

/// The English name of a language code for the prompt (`vi` becomes `Vietnamese`); an unknown code
/// is passed through as the code.
pub fn language_name(code: &str) -> String {
    let base = code.split('-').next().unwrap_or(code);
    let name = match base {
        "en" => "English",
        "vi" => "Vietnamese",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "it" => "Italian",
        "pt" => "Portuguese",
        "nl" => "Dutch",
        "ru" => "Russian",
        "uk" => "Ukrainian",
        "pl" => "Polish",
        "tr" => "Turkish",
        "ar" => "Arabic",
        "hi" => "Hindi",
        "id" => "Indonesian",
        "th" => "Thai",
        "ja" => "Japanese",
        "ko" => "Korean",
        "zh" => "Chinese",
        _ => return code.to_owned(),
    };
    if code.contains('-') {
        format!("{name} ({code})")
    } else {
        name.to_owned()
    }
}
