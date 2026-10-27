//! Where reports live in a bundle (AI-9).
//!
//! * `reports/<id>.json` is the report ([`Report`]) and `reports/<id>.md` its Markdown (the
//!   edited text when the user edited it). These two are what other tools read.
//! * `reports/versions/<id>.v<N>.json` keeps every version that was ever current, so that undo and
//!   redo can bring one back. They are small; nothing prunes them.
//! * The manifest key `reports` is the **index**: `[{ id, v, template, created, edited }]`, `v`
//!   naming the current version of each report. A create, an edit, a regenerate and a delete each
//!   change the index in one revision (`rec_edit::apply_extra`), so undo and redo just restore
//!   the index. [`sync_live`] then makes `reports/<id>.json` and `.md` match the index again;
//!   it runs after every change and on every read.

use std::fs;
use std::path::{Path, PathBuf};

use cm_core::CmError;
use cm_library::manifest::RecManifest;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::render::report_markdown;
use super::{ReportContext, REPORTS_DIR, REPORT_VERSION};

/// The manifest key of the index.
pub const MANIFEST_REPORTS: &str = "reports";
/// Folder of the kept versions, inside `reports/`.
pub const VERSIONS_DIR: &str = "versions";

/// A stored report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// Format version of this file.
    pub version: u32,
    pub id: String,
    /// The version of this report (1, 2, ... one per create, edit and regenerate).
    pub v: u32,
    pub template: String,
    pub template_name: String,
    pub template_version: u32,
    /// The text model that wrote it.
    pub model: String,
    /// `local`, `openai_compatible` or `anthropic`.
    pub provider: String,
    /// RFC 3339.
    pub created: String,
    /// The language of the note text (the transcript's unless one was asked for).
    pub language: String,
    pub context: ReportContext,
    /// `{ <section id>: [item] }`.
    pub content: Value,
    /// Items dropped because none of their citations matched the transcript.
    pub dropped_uncited: usize,
    #[serde(default)]
    pub reassigned_names: usize,
    /// Pieces the transcript was cut into (1 for a short recording).
    #[serde(default)]
    pub chunks: usize,
    /// The user edited the Markdown.
    #[serde(default)]
    pub edited: bool,
    /// Ticked task items as `<section id>:<index>` (the index counts the items of that section in
    /// `content`). Changed by `check_report_item`; a regenerate starts with none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checked: Vec<String>,
    /// The edited Markdown. Absent for a report nobody edited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,
}

/// The key of item `index` of `section` in [`Report::checked`].
pub fn item_key(section: &str, index: usize) -> String {
    format!("{section}:{index}")
}

impl Report {
    /// Whether item `index` of `section` is ticked.
    pub fn is_checked(&self, section: &str, index: usize) -> bool {
        let key = item_key(section, index);
        self.checked.contains(&key)
    }

    /// Number of items in `section` of the stored content.
    pub fn item_len(&self, section: &str) -> usize {
        self.content
            .get(section)
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    }

    /// Ticks or unticks an item. Returns whether anything changed. The caller checks the section
    /// and index first ([`is_checkable`](super::is_checkable), [`Report::item_len`]).
    pub fn set_checked(&mut self, section: &str, index: usize, on: bool) -> bool {
        let key = item_key(section, index);
        let was = self.checked.contains(&key);
        if on == was {
            return false;
        }
        if on {
            self.checked.push(key);
            self.checked.sort();
        } else {
            self.checked.retain(|k| *k != key);
        }
        true
    }

    /// The Markdown a reader sees.
    pub fn markdown_text(&self) -> String {
        report_markdown(self)
    }

    /// The entry in a list.
    pub fn summary_json(&self) -> Value {
        json!({
            "report_id": self.id,
            "template": self.template,
            "template_name": self.template_name,
            "created": self.created,
            "language": self.language,
            "model": self.model,
            "edited": self.edited,
            "dropped_uncited": self.dropped_uncited,
            "version": self.v,
        })
    }

    /// The full answer of `get_report`.
    pub fn full_json(&self, capture_id: &str) -> Value {
        json!({
            "capture_id": capture_id,
            "report_id": self.id,
            "report": self,
            "markdown": self.markdown_text(),
        })
    }
}

/// One line of the manifest index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub id: String,
    pub v: u32,
    pub template: String,
    pub created: String,
    #[serde(default)]
    pub edited: bool,
}

pub fn entry_of(r: &Report) -> IndexEntry {
    IndexEntry {
        id: r.id.clone(),
        v: r.v,
        template: r.template.clone(),
        created: r.created.clone(),
        edited: r.edited,
    }
}

/// The index of a recording, oldest report first.
pub fn index_of(m: &RecManifest) -> Vec<IndexEntry> {
    m.extra
        .get(MANIFEST_REPORTS)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

/// The manifest value of an index.
pub fn index_value(entries: &[IndexEntry]) -> Value {
    serde_json::to_value(entries).unwrap_or_else(|_| json!([]))
}

/// A report id is `rpt_` and eight hex digits.
pub fn valid_id(s: &str) -> bool {
    s.strip_prefix("rpt_")
        .is_some_and(|h| h.len() == 8 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// A new id that is not in `taken` (derived from `seed`, which should differ per call).
pub fn new_id(taken: &[IndexEntry], seed: &str) -> String {
    let mut n = 0u32;
    loop {
        let digest = Sha256::digest(format!("{seed}:{n}").as_bytes());
        let id = format!(
            "rpt_{:02x}{:02x}{:02x}{:02x}",
            digest[0], digest[1], digest[2], digest[3]
        );
        if !taken.iter().any(|e| e.id == id) {
            return id;
        }
        n += 1;
    }
}

fn io(path: &Path, e: std::io::Error) -> CmError {
    CmError::internal(format!("cannot write `{}`: {e}", path.display()))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CmError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.tmp"));
    fs::write(&tmp, bytes).map_err(|e| io(&tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io(path, e)
    })
}

pub fn reports_dir(bundle: &Path) -> PathBuf {
    bundle.join(REPORTS_DIR)
}

pub fn version_path(bundle: &Path, id: &str, v: u32) -> PathBuf {
    reports_dir(bundle)
        .join(VERSIONS_DIR)
        .join(format!("{id}.v{v}.json"))
}

pub fn live_json(bundle: &Path, id: &str) -> PathBuf {
    reports_dir(bundle).join(format!("{id}.json"))
}

pub fn live_md(bundle: &Path, id: &str) -> PathBuf {
    reports_dir(bundle).join(format!("{id}.md"))
}

fn to_bytes(r: &Report) -> Result<Vec<u8>, CmError> {
    serde_json::to_vec_pretty(r).map_err(|e| CmError::internal(format!("report: {e}")))
}

/// Keeps `r` as a version and makes it the live `reports/<id>.json` and `.md`.
pub fn write(bundle: &Path, r: &Report) -> Result<(), CmError> {
    let bytes = to_bytes(r)?;
    write_atomic(&version_path(bundle, &r.id, r.v), &bytes)?;
    materialize(bundle, r)
}

/// Writes the live files of `r` (the version file must already exist).
pub fn materialize(bundle: &Path, r: &Report) -> Result<(), CmError> {
    write_atomic(&live_json(bundle, &r.id), &to_bytes(r)?)?;
    write_atomic(&live_md(bundle, &r.id), r.markdown_text().as_bytes())
}

/// A kept version.
pub fn read_version(bundle: &Path, id: &str, v: u32) -> Result<Report, CmError> {
    let path = version_path(bundle, id, v);
    let text = fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            CmError::not_found(format!(
                "report `{id}` version {v} is missing from the recording"
            ))
        } else {
            CmError::internal(format!("cannot read `{}`: {e}", path.display()))
        }
    })?;
    serde_json::from_str(&text)
        .map_err(|e| CmError::internal(format!("`{}` is not a report: {e}", path.display())))
}

/// The current version of the report `id` named by the index.
pub fn read(bundle: &Path, entries: &[IndexEntry], id: &str) -> Result<Report, CmError> {
    let e = entries.iter().find(|e| e.id == id).ok_or_else(|| {
        CmError::not_found(format!(
            "this recording has no report `{id}`; `get_report` without a report id lists them"
        ))
    })?;
    read_version(bundle, &e.id, e.v)
}

/// Makes `reports/<id>.json` and `.md` match the index: a report whose live file is another
/// version (after an undo) is written again, and the live files of a report that is not in the
/// index (deleted, or its creation undone) are removed. Best effort: a file that cannot be
/// written is left for the next call.
pub fn sync_live(bundle: &Path, entries: &[IndexEntry]) {
    for e in entries {
        let live = fs::read_to_string(live_json(bundle, &e.id))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok());
        let current = live.as_ref().and_then(|v| v["v"].as_u64()) == Some(u64::from(e.v))
            && live_md(bundle, &e.id).is_file();
        if !current {
            if let Ok(r) = read_version(bundle, &e.id, e.v) {
                let _ = materialize(bundle, &r);
            }
        }
    }
    let Ok(dir) = fs::read_dir(reports_dir(bundle)) else {
        return;
    };
    for f in dir.flatten() {
        let path = f.path();
        if !path.is_file() {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let is_report_file = valid_id(&stem)
            && matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("json" | "md")
            );
        if is_report_file && !entries.iter().any(|e| e.id == stem) {
            let _ = fs::remove_file(&path);
        }
    }
}

/// A new report struct for `template` with the fields every creator sets.
#[allow(clippy::too_many_arguments)]
pub fn new_report(
    id: String,
    v: u32,
    template: &super::Template,
    model: &str,
    provider: &str,
    created: &str,
    language: &str,
    context: ReportContext,
    generated: &super::generate::Generated,
) -> Report {
    Report {
        version: REPORT_VERSION,
        id,
        v,
        template: template.id.clone(),
        template_name: template.name.clone(),
        template_version: template.version,
        model: model.to_owned(),
        provider: provider.to_owned(),
        created: created.to_owned(),
        language: language.to_owned(),
        context,
        content: generated.content.clone(),
        dropped_uncited: generated.dropped_uncited,
        reassigned_names: generated.reassigned_names,
        chunks: generated.chunks,
        edited: false,
        checked: Vec::new(),
        markdown: None,
    }
}
