//! Manifest types for `.cmshot` (full model) and `.cmrec` (typed header only, M3+).

use crate::history::HistoryLog;
use crate::ops::AnnotationOp;
use cm_core::{CmError, MediaTime, PhysicalSize, Revision};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use time::OffsetDateTime;

/// Manifest schema version written and understood by this crate.
pub const SCHEMA_VERSION: u32 = 1;

/// The kinds of capture bundle.
///
/// `Audio` (VOC-1) is a `.cmrec` bundle whose manifest says `"kind": "audio"`: it has the same
/// folder layout and tooling as a recording but no `screen` track. The folder extension cannot
/// tell the two apart (see [`CaptureKind::from_extension`]); the manifest does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureKind {
    Screenshot,
    Recording,
    Audio,
}

impl CaptureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CaptureKind::Screenshot => "screenshot",
            CaptureKind::Recording => "recording",
            CaptureKind::Audio => "audio",
        }
    }
    /// A `.cmrec` bundle: a screen recording or an audio recording. Both are edited, transcribed
    /// and captioned by the same code.
    pub fn is_recording(self) -> bool {
        matches!(self, CaptureKind::Recording | CaptureKind::Audio)
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "screenshot" => Some(CaptureKind::Screenshot),
            "recording" => Some(CaptureKind::Recording),
            "audio" => Some(CaptureKind::Audio),
            _ => None,
        }
    }
    /// Bundle folder extension without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            CaptureKind::Screenshot => "cmshot",
            CaptureKind::Recording | CaptureKind::Audio => "cmrec",
        }
    }
    /// The kind a folder extension stands for. `cmrec` gives `Recording`; an audio recording is
    /// told apart by the `kind` field of its manifest ([`RecManifest::capture_kind`]).
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "cmshot" => Some(CaptureKind::Screenshot),
            "cmrec" => Some(CaptureKind::Recording),
            _ => None,
        }
    }
}

/// The read-only master image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginalInfo {
    pub file: String,
    /// Physical px.
    pub size: PhysicalSize,
    pub bytes: u64,
    /// Lower-case hex SHA-256 of the file as captured (ANN-3.AC4 hash check).
    pub sha256: String,
}

/// The cached flattened render (`render.png`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderInfo {
    pub file: String,
    pub size: PhysicalSize,
    /// Revision number the render reflects; compare with the latest revision to detect staleness.
    pub rev: u64,
}

/// `manifest.json` of a `.cmshot` bundle.
///
/// Unknown top-level fields are preserved in `extra` and written back unchanged, so a newer
/// minor writer's additions survive an edit by this version. A higher `schema` number is refused
/// instead (see [`ShotManifest::from_json`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShotManifest {
    pub schema: u32,
    pub id: String,
    pub title: String,
    pub kind: CaptureKind,
    #[serde(with = "time::serde::rfc3339")]
    pub created: OffsetDateTime,
    pub original: OriginalInfo,
    #[serde(default)]
    pub render: Option<RenderInfo>,
    /// Annotation ops in z-order (first is bottom).
    #[serde(default)]
    pub ops: Vec<AnnotationOp>,
    /// Counter used to mint `op_<n>` ids; never reused, even after deletes.
    #[serde(default)]
    pub op_seq: u64,
    /// Revision log, newest last, capped at 200 entries.
    #[serde(default)]
    pub revisions: Vec<Revision>,
    /// Persisted undo/redo data; see the crate docs.
    #[serde(default)]
    pub history: HistoryLog,
    /// Ops this build cannot read (a newer build's op types, or an op that fails validation),
    /// kept as raw JSON and written back untouched at their original place (R1-4). They are
    /// not in `ops`, so they are neither rendered nor editable here.
    #[serde(skip)]
    pub unknown_ops: Vec<UnknownOp>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// An annotation op this build does not understand, preserved verbatim (R1-4).
#[derive(Debug, Clone, PartialEq)]
pub struct UnknownOp {
    /// Id of the known op it followed in `ops` when loaded (`None`: it came first). On save it
    /// is written right after that op if the op still exists, else at the end.
    pub after: Option<String>,
    /// The JSON value exactly as stored.
    pub raw: Value,
    /// Why it was not understood (shown in warnings and logs).
    pub error: String,
}

/// How a manifest failed to load.
#[derive(Debug)]
pub enum ManifestProblem {
    /// Newer schema than this build supports: never overwrite, surface to the user.
    TooNew(CmError),
    /// Unparseable or structurally invalid: callers may quarantine it as `manifest.broken.json`.
    Corrupt(CmError),
}

impl ManifestProblem {
    pub fn into_error(self) -> CmError {
        match self {
            ManifestProblem::TooNew(e) | ManifestProblem::Corrupt(e) => e,
        }
    }
}

/// Splits a stored `ops` array into the ops this build understands and the rest (R1-4).
///
/// Unlike [`crate::ops::parse_ops`] (used for client input, where an unknown op is an error),
/// a stored op that does not parse or validate is kept as raw JSON so a newer build's
/// additions at the same schema version survive an edit by this one. A non-array `ops` is
/// structural corruption.
pub(crate) fn split_ops(value: &Value) -> Result<(Vec<AnnotationOp>, Vec<UnknownOp>), CmError> {
    let arr = value
        .as_array()
        .ok_or_else(|| CmError::invalid_field("ops", "must be an array"))?;
    let mut ops: Vec<AnnotationOp> = Vec::new();
    let mut unknown = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (i, v) in arr.iter().enumerate() {
        let parsed = serde_json::from_value::<AnnotationOp>(v.clone())
            .map_err(|e| {
                let tag = v.get("op").and_then(Value::as_str).unwrap_or("?");
                format!("ops[{i}]: invalid `{tag}` op: {e}")
            })
            .and_then(|op| {
                cm_core::validate_op(i, &op)
                    .map(|()| op)
                    .map_err(|e| e.to_string())
            });
        match parsed {
            Ok(op) if op.id.is_empty() || seen.insert(op.id.clone()) => ops.push(op),
            Ok(op) => unknown.push(UnknownOp {
                after: ops.last().map(|o| o.id.clone()),
                raw: v.clone(),
                error: format!("ops[{i}]: duplicate id `{}`", op.id),
            }),
            Err(error) => unknown.push(UnknownOp {
                after: ops.last().map(|o| o.id.clone()),
                raw: v.clone(),
                error,
            }),
        }
    }
    Ok((ops, unknown))
}

/// Merges preserved unknown ops back into the serialized `ops` array.
fn merge_unknown_ops(known: Vec<Value>, unknown: &[UnknownOp]) -> Vec<Value> {
    let mut out = Vec::with_capacity(known.len() + unknown.len());
    let mut placed = vec![false; unknown.len()];
    let mut place = |after: Option<&str>, out: &mut Vec<Value>| {
        for (i, u) in unknown.iter().enumerate() {
            if !placed[i] && u.after.as_deref() == after {
                out.push(u.raw.clone());
                placed[i] = true;
            }
        }
    };
    place(None, &mut out);
    for v in known {
        let id = v.get("id").and_then(Value::as_str).map(str::to_owned);
        out.push(v);
        if let Some(id) = id {
            place(Some(&id), &mut out);
        }
    }
    for (i, u) in unknown.iter().enumerate() {
        if !placed[i] {
            out.push(u.raw.clone());
        }
    }
    out
}

/// Checks the `schema` field of a manifest JSON value.
pub(crate) fn check_schema(v: &Value, what: &str) -> Result<(), ManifestProblem> {
    let Some(schema) = v.get("schema").and_then(Value::as_u64) else {
        return Err(ManifestProblem::Corrupt(CmError::invalid_input(format!(
            "{what}: missing or invalid `schema` field"
        ))));
    };
    if schema > u64::from(SCHEMA_VERSION) {
        return Err(ManifestProblem::TooNew(CmError::invalid_input(format!(
            "{what}: manifest schema {schema} is newer than the supported schema {SCHEMA_VERSION}; \
             update CaptureMann to open this capture"
        ))));
    }
    if schema == 0 {
        return Err(ManifestProblem::Corrupt(CmError::invalid_input(format!(
            "{what}: invalid schema 0"
        ))));
    }
    Ok(())
}

impl ShotManifest {
    /// Parses manifest JSON text with the schema check and per-index op errors.
    pub fn from_json(text: &str) -> Result<Self, ManifestProblem> {
        let corrupt = |e: serde_json::Error| {
            ManifestProblem::Corrupt(CmError::invalid_input(format!("manifest.json: {e}")))
        };
        let mut v: Value = serde_json::from_str(text).map_err(corrupt)?;
        check_schema(&v, "manifest.json")?;
        let (ops_val, history_val) = match v.as_object_mut() {
            Some(m) => (m.remove("ops"), m.remove("history")),
            None => (None, None),
        };
        let mut m: ShotManifest = serde_json::from_value(v).map_err(corrupt)?;
        if let Some(ops) = ops_val {
            let (known, unknown) = split_ops(&ops).map_err(ManifestProblem::Corrupt)?;
            m.ops = known;
            m.unknown_ops = unknown;
        }
        // R1-4: history this build cannot read (new change kinds, new op types inside it) is
        // kept raw and written back as is until the user edits; the ops are authoritative.
        if let Some(h) = history_val {
            m.history = match serde_json::from_value::<HistoryLog>(h.clone()) {
                Ok(h) => h,
                Err(_) => HistoryLog {
                    raw: Some(h),
                    ..HistoryLog::default()
                },
            };
        }
        Ok(m)
    }

    /// Pretty JSON. Unknown ops and unreadable history kept by [`ShotManifest::from_json`]
    /// are written back unchanged.
    pub fn to_json(&self) -> Result<String, CmError> {
        let err = |e: serde_json::Error| crate::util::json_err("manifest.json", &e);
        let mut v = serde_json::to_value(self).map_err(err)?;
        if let Some(obj) = v.as_object_mut() {
            if !self.unknown_ops.is_empty() {
                let known = match obj.remove("ops") {
                    Some(Value::Array(a)) => a,
                    _ => Vec::new(),
                };
                obj.insert(
                    "ops".to_owned(),
                    Value::Array(merge_unknown_ops(known, &self.unknown_ops)),
                );
            }
            if let Some(raw) = &self.history.raw {
                if self.history.undo.is_empty() && self.history.redo.is_empty() {
                    obj.insert("history".to_owned(), raw.clone());
                }
            }
        }
        // The manifest is pretty for humans, the undo history is not: it is machine data, and
        // indenting every op of every step made it several times bigger. The history value is
        // swapped for a token, the rest is pretty-printed, and the token is replaced by the
        // history as one compact line.
        let history = match v.as_object_mut().and_then(|o| o.remove("history")) {
            Some(h) => h,
            None => return serde_json::to_string_pretty(&v).map_err(err),
        };
        const TOKEN: &str = "__cm_compact_history_5d1f9c__";
        if let Some(o) = v.as_object_mut() {
            o.insert("history".to_owned(), Value::String(TOKEN.to_owned()));
        }
        let pretty = serde_json::to_string_pretty(&v).map_err(err)?;
        let quoted = format!("\"{TOKEN}\"");
        if pretty.matches(&quoted).count() != 1 {
            // The token appears in user text: fall back to the plain pretty form.
            if let Some(o) = v.as_object_mut() {
                o.insert("history".to_owned(), history);
            }
            return serde_json::to_string_pretty(&v).map_err(err);
        }
        let compact = serde_json::to_string(&history).map_err(err)?;
        Ok(pretty.replacen(&quoted, &compact, 1))
    }

    /// Next unused `op_<n>` id (advances `op_seq`).
    pub fn mint_op_id(&mut self) -> String {
        let floor = self
            .ops
            .iter()
            .filter_map(|o| o.id.strip_prefix("op_")?.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        self.op_seq = self.op_seq.max(floor) + 1;
        format!("op_{}", self.op_seq)
    }

    /// Latest revision number, 0 when none.
    pub fn latest_rev(&self) -> u64 {
        self.revisions.last().map_or(0, |r| r.rev)
    }
}

/// `manifest.json` of a `.cmrec` bundle (specs/01-technical-design.md, "Data model").
///
/// Only what the library needs is deeply typed; each track entry and each edit stays JSON so a
/// rescan or rename never loses data and the track crates own their own `details`. Unknown
/// top-level fields are kept in `extra`. Written by [`crate::rec`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecManifest {
    pub schema: u32,
    pub id: String,
    pub title: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created: OffsetDateTime,
    #[serde(default)]
    pub tick_rate: Option<u32>,
    /// Total length of the master; needed for the library's duration column.
    ///
    /// Not in the spec's abridged manifest: see the open questions in the crate docs.
    #[serde(default)]
    pub duration: Option<MediaTime>,
    /// Track entries by name (`screen`, `camera`, `mic`, `system`, `cursor`). A track whose
    /// recovery failed has `"missing": true` (REC-8).
    #[serde(default)]
    pub tracks: Map<String, Value>,
    /// Edit list; the default is one full-length segment at speed 1.
    #[serde(default)]
    pub edits: Vec<Value>,
    #[serde(default)]
    pub revisions: Vec<Revision>,
    /// Recording settings as used (size, fps, codec, quality, source, ...).
    #[serde(default)]
    pub settings: Value,
    /// Pause gaps in session time. Media files have continuous timestamps; session time =
    /// file time + the gaps that precede it (REC-4).
    #[serde(default)]
    pub gaps: Vec<cm_core::PauseGap>,
    #[serde(default)]
    pub captions: Value,
    /// `"audio"` for an audio-only recording (VOC-1); absent for a screen recording.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Markers the user or an agent set (VOC-1), sorted by `at`. Times are the recording clock
    /// (the HUD timer: pauses removed), in ticks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<Marker>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One marker of `manifest.markers` (VOC-1.AC4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    /// Recorded time (pauses removed), ticks.
    pub at: MediaTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Who set it: `user` or `agent:<name>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
}

impl RecManifest {
    /// `Audio` when the manifest says so, else `Recording`.
    pub fn capture_kind(&self) -> CaptureKind {
        if self.kind.as_deref() == Some("audio") {
            CaptureKind::Audio
        } else {
            CaptureKind::Recording
        }
    }

    pub fn from_json(text: &str) -> Result<Self, ManifestProblem> {
        let corrupt = |e: serde_json::Error| {
            ManifestProblem::Corrupt(CmError::invalid_input(format!("manifest.json: {e}")))
        };
        let v: Value = serde_json::from_str(text).map_err(corrupt)?;
        check_schema(&v, "manifest.json")?;
        serde_json::from_value(v).map_err(corrupt)
    }
}
