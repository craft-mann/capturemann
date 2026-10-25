//! Undo/redo as persisted data.
//!
//! Every edit is a list of [`Change`]s. A `Change` carries *both* the forward data and the
//! inverse data (for example `ReplaceOp` holds `before` and `after`), so it is itself a cm-core
//! [`Command`] on the manifest and can also be stored in `manifest.history`. See the crate docs
//! for how a restarted session rebuilds its `UndoStack` from this.

use crate::manifest::ShotManifest;
use crate::ops::AnnotationOp;
use cm_core::{Actor, CmError, Command};
use serde::{Deserialize, Serialize};

/// One reversible mutation of `manifest.ops`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum Change {
    AddOp {
        index: usize,
        op: AnnotationOp,
    },
    RemoveOp {
        index: usize,
        op: AnnotationOp,
    },
    ReplaceOp {
        index: usize,
        before: AnnotationOp,
        after: AnnotationOp,
    },
    /// "Revert to original" (and the clear half of `annotate { replace: true }` in histories
    /// written before replace was stored as a diff).
    ClearOps {
        ops: Vec<AnnotationOp>,
    },
    /// The z-order changed and nothing else: the op ids before and after (the ops themselves are
    /// not repeated). Written by `annotate { replace: true }` when the same ops come back in a
    /// different order.
    ReorderOps {
        before: Vec<String>,
        after: Vec<String>,
    },
}

/// Puts `doc.ops` into the order of `ids`. `false` (and `doc` untouched) when the ids are not
/// exactly the ops present.
fn reorder_to(doc: &mut ShotManifest, ids: &[String]) -> bool {
    if ids.len() != doc.ops.len() {
        return false;
    }
    let mut by_id: std::collections::HashMap<&str, usize> = doc
        .ops
        .iter()
        .enumerate()
        .map(|(i, o)| (o.id.as_str(), i))
        .collect();
    let mut order = Vec::with_capacity(ids.len());
    for id in ids {
        match by_id.remove(id.as_str()) {
            Some(i) => order.push(i),
            None => return false,
        }
    }
    let mut slots: Vec<Option<AnnotationOp>> =
        std::mem::take(&mut doc.ops).into_iter().map(Some).collect();
    doc.ops = order.into_iter().filter_map(|i| slots[i].take()).collect();
    true
}

fn stale(what: &str) -> CmError {
    CmError::invalid_input(format!("history does not match the manifest: {what}"))
}

impl Command<ShotManifest> for Change {
    fn apply(&mut self, doc: &mut ShotManifest) -> Result<(), CmError> {
        match self {
            Change::AddOp { index, op } => {
                if *index > doc.ops.len() {
                    return Err(stale("add index out of range"));
                }
                if doc.ops.iter().any(|o| o.id == op.id) {
                    return Err(CmError::invalid_input(format!(
                        "duplicate op id `{}`",
                        op.id
                    )));
                }
                doc.ops.insert(*index, op.clone());
            }
            Change::RemoveOp { index, op } => {
                if doc.ops.get(*index).map(|o| &o.id) != Some(&op.id) {
                    return Err(stale("remove target moved"));
                }
                doc.ops.remove(*index);
            }
            Change::ReplaceOp {
                index,
                before,
                after,
            } => {
                if doc.ops.get(*index).map(|o| &o.id) != Some(&before.id) || before.id != after.id {
                    return Err(stale("replace target moved"));
                }
                doc.ops[*index] = after.clone();
            }
            Change::ReorderOps { before, after } => {
                let current: Vec<&str> = doc.ops.iter().map(|o| o.id.as_str()).collect();
                if current.len() != before.len()
                    || current.iter().zip(before.iter()).any(|(a, b)| *a != b)
                {
                    return Err(stale("reorder target moved"));
                }
                if !reorder_to(doc, after) {
                    return Err(stale("reorder ids do not match"));
                }
            }
            Change::ClearOps { .. } => {
                if doc.ops.is_empty() {
                    return Err(CmError::invalid_input("there are no annotations to clear"));
                }
                // The cleared ops were captured when the change was built.
                doc.ops.clear();
            }
        }
        Ok(())
    }

    fn undo(&mut self, doc: &mut ShotManifest) {
        match self {
            Change::AddOp { op, .. } => doc.ops.retain(|o| o.id != op.id),
            Change::RemoveOp { index, op } => {
                let at = (*index).min(doc.ops.len());
                doc.ops.insert(at, op.clone());
            }
            Change::ReplaceOp { before, .. } => {
                if let Some(slot) = doc.ops.iter_mut().find(|o| o.id == before.id) {
                    *slot = before.clone();
                }
            }
            Change::ClearOps { ops } => doc.ops = ops.clone(),
            Change::ReorderOps { before, .. } => {
                reorder_to(doc, before);
            }
        }
    }
}

/// One undoable step as stored in the manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredAction {
    pub name: String,
    pub by: Actor,
    pub changes: Vec<Change>,
}

/// `manifest.history`: undo steps oldest first; redo steps with the next redo last.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct HistoryLog {
    #[serde(default)]
    pub undo: Vec<StoredAction>,
    #[serde(default)]
    pub redo: Vec<StoredAction>,
    /// The stored history verbatim when this build could not read it (a newer build's change
    /// kinds). `undo`/`redo` are empty then; the raw value is written back untouched until
    /// the first edit replaces it (R1-4).
    #[serde(skip)]
    pub raw: Option<serde_json::Value>,
}
