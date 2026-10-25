//! `.cmshot` bundles: create, open, edit (as undoable commands), save.

use crate::history::{Change, HistoryLog, StoredAction};
use crate::manifest::{
    CaptureKind, ManifestProblem, OriginalInfo, RenderInfo, ShotManifest, SCHEMA_VERSION,
};
use crate::ops::{visible_rect, AnnotationOp, OpKind, Rc};
use crate::util::{io_err, new_id, sanitize_title, sha256_file, sha256_hex, write_atomic};
use cm_core::{Action, Actor, CmError, PhysicalSize, Revision, UndoStack, DEFAULT_REVISION_LIMIT};
use std::fs;
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

pub const ORIGINAL_FILE: &str = "original.png";
pub const MANIFEST_FILE: &str = "manifest.json";
pub const BROKEN_MANIFEST_FILE: &str = "manifest.broken.json";
pub const RENDER_FILE: &str = "render.png";
pub const THUMBS_DIR: &str = "thumbs";
/// Revisions and undo steps kept (ANN-3: last 200).
pub const HISTORY_LIMIT: usize = DEFAULT_REVISION_LIMIT;

/// An open `.cmshot` bundle: manifest plus an in-memory undo stack that mirrors
/// `manifest.history`.
///
/// Edits change the in-memory manifest; [`ShotBundle::save`] persists it. `original.png` is
/// never written after [`ShotBundle::create`].
#[derive(Debug)]
pub struct ShotBundle {
    dir: PathBuf,
    manifest: ShotManifest,
    stack: UndoStack<ShotManifest>,
    undo_log: Vec<StoredAction>,
    redo_log: Vec<StoredAction>,
    warnings: Vec<String>,
}

fn png_size(bytes: &[u8], what: &str) -> Result<PhysicalSize, CmError> {
    if !bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Err(CmError::invalid_field(what, "not a PNG"));
    }
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| CmError::invalid_field(what, e))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|e| CmError::invalid_field(what, format!("invalid PNG: {e}")))?;
    Ok(PhysicalSize { width, height })
}

impl ShotBundle {
    /// Creates `<parent>/<title>.cmshot/` (with a ` (2)`, ` (3)`… suffix if the name is taken)
    /// holding `original.png` (the given bytes, verbatim, made read-only) and a fresh manifest.
    pub fn create(
        parent: &Path,
        title: &str,
        png: &[u8],
        now: OffsetDateTime,
    ) -> Result<Self, CmError> {
        let size = png_size(png, "png")?;
        let base = sanitize_title(title)?;
        fs::create_dir_all(parent).map_err(|e| io_err("cannot create", parent, &e))?;
        let dir = create_unique_dir(parent, &base, CaptureKind::Screenshot)?;
        let res = (|| {
            let orig = dir.join(ORIGINAL_FILE);
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&orig)
                .map_err(|e| io_err("cannot create", &orig, &e))?;
            std::io::Write::write_all(&mut f, png)
                .map_err(|e| io_err("cannot write", &orig, &e))?;
            f.sync_all()
                .map_err(|e| io_err("cannot write", &orig, &e))?;
            drop(f);
            let mut perms = fs::metadata(&orig)
                .map_err(|e| io_err("cannot stat", &orig, &e))?
                .permissions();
            perms.set_readonly(true);
            fs::set_permissions(&orig, perms).map_err(|e| io_err("cannot protect", &orig, &e))?;
            let manifest = ShotManifest {
                schema: SCHEMA_VERSION,
                id: new_id("shot", now),
                title: title.trim().to_owned(),
                kind: CaptureKind::Screenshot,
                created: now,
                original: OriginalInfo {
                    file: ORIGINAL_FILE.to_owned(),
                    size,
                    bytes: png.len() as u64,
                    sha256: sha256_hex(png),
                },
                render: None,
                ops: Vec::new(),
                op_seq: 0,
                revisions: Vec::new(),
                history: HistoryLog::default(),
                unknown_ops: Vec::new(),
                extra: Default::default(),
            };
            write_atomic(&dir.join(MANIFEST_FILE), manifest.to_json()?.as_bytes())?;
            Ok(manifest)
        })();
        match res {
            Ok(manifest) => Ok(Self::assemble(dir, manifest, Vec::new())),
            Err(e) => {
                // Best effort: the original is read-only, so lift that first.
                let orig = dir.join(ORIGINAL_FILE);
                if let Ok(md) = fs::metadata(&orig) {
                    let mut p = md.permissions();
                    #[allow(clippy::permissions_set_readonly_false)]
                    p.set_readonly(false);
                    let _ = fs::set_permissions(&orig, p);
                }
                let _ = fs::remove_dir_all(&dir);
                Err(e)
            }
        }
    }

    /// Creates a bundle from raw RGBA8 pixels (encodes `original.png`).
    pub fn create_from_rgba(
        parent: &Path,
        title: &str,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        now: OffsetDateTime,
    ) -> Result<Self, CmError> {
        let img = image::RgbaImage::from_raw(width, height, rgba).ok_or_else(|| {
            CmError::invalid_field("rgba", "length does not match width x height x 4")
        })?;
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| CmError::internal(format!("cannot encode PNG: {e}")))?;
        Self::create(parent, title, &png, now)
    }

    /// Opens a bundle folder.
    ///
    /// * Manifest schema newer than [`SCHEMA_VERSION`]: `invalid_input` naming both versions;
    ///   nothing is touched.
    /// * Ops (and undo history) this build cannot read are kept as raw JSON and written back
    ///   untouched (R1-4); they are not rendered, and each one adds a warning.
    /// * Missing or corrupt manifest (ANN-3 edge case): a corrupt file is moved to
    ///   `manifest.broken.json` (`manifest.broken-<unix_ts>.json` when that name is taken, an
    ///   earlier quarantine is never overwritten), a fresh manifest without ops is created from `original.png`,
    ///   and [`ShotBundle::warnings`] says so.
    /// * Stored undo history that no longer matches the ops is dropped with a warning (the ops
    ///   themselves are authoritative).
    pub fn open(dir: &Path) -> Result<Self, CmError> {
        if !dir.is_dir() {
            return Err(CmError::not_found(format!(
                "no bundle at `{}`",
                dir.display()
            )));
        }
        let mpath = dir.join(MANIFEST_FILE);
        let mut warnings = Vec::new();
        let manifest = match fs::read_to_string(&mpath) {
            Ok(text) => match ShotManifest::from_json(&text) {
                Ok(m) => m,
                Err(ManifestProblem::TooNew(e)) => return Err(e),
                Err(ManifestProblem::Corrupt(e)) => {
                    let broken = broken_manifest_path(dir);
                    fs::rename(&mpath, &broken)
                        .map_err(|err| io_err("cannot move", &mpath, &err))?;
                    let kept = broken
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(BROKEN_MANIFEST_FILE);
                    warnings.push(format!(
                        "The manifest was damaged ({e}); opened the original without annotations and kept the old file as {kept}."
                    ));
                    Self::fresh_manifest(dir)?
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warnings.push(
                    "The manifest was missing; created a new one from the original.".to_owned(),
                );
                Self::fresh_manifest(dir)?
            }
            Err(e) => return Err(io_err("cannot read", &mpath, &e)),
        };
        for u in &manifest.unknown_ops {
            warnings.push(format!(
                "Kept an annotation this version cannot show ({}); it is preserved unchanged.",
                u.error
            ));
        }
        if manifest.history.raw.is_some() {
            warnings.push(
                "The undo history was written by a newer version; undo is unavailable until the next edit."
                    .to_owned(),
            );
        }
        Ok(Self::assemble(dir.to_owned(), manifest, warnings))
    }

    fn fresh_manifest(dir: &Path) -> Result<ShotManifest, CmError> {
        let orig = dir.join(ORIGINAL_FILE);
        let bytes = fs::read(&orig).map_err(|e| io_err("cannot read", &orig, &e))?;
        let size = png_size(&bytes, ORIGINAL_FILE)?;
        let title = dir
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Screenshot")
            .to_owned();
        let now = OffsetDateTime::now_utc();
        let m = ShotManifest {
            schema: SCHEMA_VERSION,
            id: new_id("shot", now),
            title,
            kind: CaptureKind::Screenshot,
            created: now,
            original: OriginalInfo {
                file: ORIGINAL_FILE.to_owned(),
                size,
                bytes: bytes.len() as u64,
                sha256: sha256_hex(&bytes),
            },
            render: None,
            ops: Vec::new(),
            op_seq: 0,
            revisions: Vec::new(),
            history: HistoryLog::default(),
            unknown_ops: Vec::new(),
            extra: Default::default(),
        };
        write_atomic(&dir.join(MANIFEST_FILE), m.to_json()?.as_bytes())?;
        Ok(m)
    }

    fn assemble(dir: PathBuf, manifest: ShotManifest, mut warnings: Vec<String>) -> Self {
        let undo_log = manifest.history.undo.clone();
        let redo_log = manifest.history.redo.clone();
        let (stack, undo_log, redo_log) = match rebuild_stack(&manifest, &undo_log, &redo_log) {
            Some(stack) => (stack, undo_log, redo_log),
            None => {
                if !undo_log.is_empty() || !redo_log.is_empty() {
                    warnings.push("Undo history could not be restored and was cleared.".to_owned());
                }
                (UndoStack::new(), Vec::new(), Vec::new())
            }
        };
        ShotBundle {
            dir,
            manifest,
            stack,
            undo_log,
            redo_log,
            warnings,
        }
    }

    pub(crate) fn push_warning(&mut self, w: String) {
        self.warnings.push(w);
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn manifest(&self) -> &ShotManifest {
        &self.manifest
    }
    pub fn ops(&self) -> &[AnnotationOp] {
        &self.manifest.ops
    }
    pub fn revisions(&self) -> &[Revision] {
        &self.manifest.revisions
    }
    /// Non-fatal problems found while opening (damaged manifest, dropped history).
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
    pub fn original_path(&self) -> PathBuf {
        self.dir.join(ORIGINAL_FILE)
    }
    pub fn render_path(&self) -> PathBuf {
        self.dir.join(RENDER_FILE)
    }
    pub fn thumbs_dir(&self) -> PathBuf {
        self.dir.join(THUMBS_DIR)
    }
    pub fn can_undo(&self) -> bool {
        self.stack.can_undo()
    }
    pub fn can_redo(&self) -> bool {
        self.stack.can_redo()
    }
    pub fn next_undo_name(&self) -> Option<&str> {
        self.stack.next_undo_name()
    }
    /// The visible rect set by the last crop op, `None` for the full image.
    pub fn visible_rect(&self) -> Option<Rc> {
        visible_rect(&self.manifest.ops)
    }
    /// True when `render.png` is missing or older than the latest revision.
    pub fn render_is_stale(&self) -> bool {
        match &self.manifest.render {
            Some(r) => r.rev != self.manifest.latest_rev() || !self.render_path().is_file(),
            None => true,
        }
    }

    /// Re-hashes `original.png` against the manifest (ANN-3.AC4).
    pub fn verify_original(&self) -> Result<bool, CmError> {
        Ok(sha256_file(&self.original_path())? == self.manifest.original.sha256)
    }

    // ---- edits (each is one undo step) -------------------------------------------------

    /// Appends ops as one step. Blank ids are assigned. `replace` first clears all ops (the
    /// `annotate { replace: true }` semantics), still as one step.
    pub fn annotate(
        &mut self,
        ops: Vec<AnnotationOp>,
        replace: bool,
        by: Actor,
        at: OffsetDateTime,
    ) -> Result<Revision, CmError> {
        let name = if replace {
            "annotate_replace"
        } else {
            "annotate"
        };
        self.add_ops_named(name, ops, replace, by, at)
    }

    /// Appends ops as one step named `add_op`.
    pub fn add_ops(
        &mut self,
        ops: Vec<AnnotationOp>,
        by: Actor,
        at: OffsetDateTime,
    ) -> Result<Revision, CmError> {
        self.add_ops_named("add_op", ops, false, by, at)
    }

    fn add_ops_named(
        &mut self,
        name: &str,
        ops: Vec<AnnotationOp>,
        replace: bool,
        by: Actor,
        at: OffsetDateTime,
    ) -> Result<Revision, CmError> {
        if ops.is_empty() {
            return Err(CmError::invalid_field("ops", "must not be empty"));
        }
        if replace && !self.manifest.ops.is_empty() {
            return self.replace_ops(name, ops, by, at);
        }
        let mut changes = Vec::new();
        let mut scratch = self.manifest.clone();
        let base = scratch.ops.len();
        for (i, mut op) in ops.into_iter().enumerate() {
            cm_core::validate_op(i, &op)?;
            if op.id.is_empty() {
                op.id = scratch.mint_op_id();
            }
            if scratch.ops.iter().any(|o| o.id == op.id) {
                return Err(CmError::invalid_field(
                    &format!("ops[{i}]"),
                    format!("duplicate id `{}`", op.id),
                ));
            }
            scratch.ops.push(op.clone());
            changes.push(Change::AddOp {
                index: base + i,
                op,
            });
        }
        // Keep the id counter ahead of anything we minted (it is not part of undo).
        self.manifest.op_seq = self.manifest.op_seq.max(scratch.op_seq);
        self.execute(name, by, at, changes)
    }

    /// `annotate { replace: true }` over existing ops, stored as a diff by op id so one GUI edit
    /// costs the ops that changed, not the whole list: unchanged ops are untouched, changed ones
    /// become `ReplaceOp`, vanished ones `RemoveOp`, new ones `AddOp`, and a different order of
    /// the surviving ops one `ReorderOps`. Still one undo step. When nothing differs there is
    /// nothing to record and the latest revision is returned unchanged.
    fn replace_ops(
        &mut self,
        name: &str,
        ops: Vec<AnnotationOp>,
        by: Actor,
        at: OffsetDateTime,
    ) -> Result<Revision, CmError> {
        use std::collections::{HashMap, HashSet};
        // Ids the new list names itself must not be handed out to ops without one.
        let mut taken: HashSet<String> = self.manifest.ops.iter().map(|o| o.id.clone()).collect();
        taken.extend(
            ops.iter()
                .filter(|o| !o.id.is_empty())
                .map(|o| o.id.clone()),
        );
        let floor = taken
            .iter()
            .filter_map(|id| id.strip_prefix("op_")?.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        let mut seq = self.manifest.op_seq.max(floor);
        let mut new: Vec<AnnotationOp> = Vec::with_capacity(ops.len());
        let mut seen: HashSet<String> = HashSet::new();
        for (i, mut op) in ops.into_iter().enumerate() {
            cm_core::validate_op(i, &op)?;
            if op.id.is_empty() {
                loop {
                    seq += 1;
                    let id = format!("op_{seq}");
                    if !taken.contains(&id) {
                        op.id = id;
                        break;
                    }
                }
            }
            if !seen.insert(op.id.clone()) {
                return Err(CmError::invalid_field(
                    &format!("ops[{i}]"),
                    format!("duplicate id `{}`", op.id),
                ));
            }
            new.push(op);
        }
        self.manifest.op_seq = self.manifest.op_seq.max(seq);

        let new_by_id: HashMap<&str, &AnnotationOp> =
            new.iter().map(|o| (o.id.as_str(), o)).collect();
        let old_ids: HashSet<&str> = self.manifest.ops.iter().map(|o| o.id.as_str()).collect();
        let mut working: Vec<AnnotationOp> = self.manifest.ops.clone();
        let mut changes = Vec::new();
        // Removed ops, last first, so the stored indexes stay valid while applying.
        for i in (0..working.len()).rev() {
            if !new_by_id.contains_key(working[i].id.as_str()) {
                let op = working.remove(i);
                changes.push(Change::RemoveOp { index: i, op });
            }
        }
        // Changed ops.
        for (i, w) in working.iter_mut().enumerate() {
            if let Some(nw) = new_by_id.get(w.id.as_str()) {
                if **nw != *w {
                    changes.push(Change::ReplaceOp {
                        index: i,
                        before: w.clone(),
                        after: (*nw).clone(),
                    });
                    *w = (*nw).clone();
                }
            }
        }
        // Added ops, and the order.
        let survivors_in_new: Vec<&str> = new
            .iter()
            .filter(|o| old_ids.contains(o.id.as_str()))
            .map(|o| o.id.as_str())
            .collect();
        let same_order = working
            .iter()
            .map(|o| o.id.as_str())
            .eq(survivors_in_new.iter().copied());
        if same_order {
            for (j, op) in new.iter().enumerate() {
                if !old_ids.contains(op.id.as_str()) {
                    working.insert(j, op.clone());
                    changes.push(Change::AddOp {
                        index: j,
                        op: op.clone(),
                    });
                }
            }
        } else {
            for op in new.iter().filter(|o| !old_ids.contains(o.id.as_str())) {
                changes.push(Change::AddOp {
                    index: working.len(),
                    op: op.clone(),
                });
                working.push(op.clone());
            }
            changes.push(Change::ReorderOps {
                before: working.iter().map(|o| o.id.clone()).collect(),
                after: new.iter().map(|o| o.id.clone()).collect(),
            });
        }
        if changes.is_empty() {
            return Ok(self.manifest.revisions.last().cloned().unwrap_or(Revision {
                rev: 0,
                action: name.to_owned(),
                by,
                at,
            }));
        }
        self.execute(name, by, at, changes)
    }

    /// Replaces the payload of op `id` (recolor, move, resize) as one step.
    pub fn update_op(
        &mut self,
        id: &str,
        kind: OpKind,
        by: Actor,
        at: OffsetDateTime,
    ) -> Result<Revision, CmError> {
        kind.validate()?;
        let (index, before) = self.find_op(id)?;
        let after = AnnotationOp {
            id: before.id.clone(),
            kind,
        };
        self.execute(
            "update_op",
            by,
            at,
            vec![Change::ReplaceOp {
                index,
                before,
                after,
            }],
        )
    }

    /// Deletes op `id`. Deleting a step badge renumbers later badges (ANN-1.AC2), all in the
    /// same undo step.
    pub fn remove_op(
        &mut self,
        id: &str,
        by: Actor,
        at: OffsetDateTime,
    ) -> Result<Revision, CmError> {
        let (index, op) = self.find_op(id)?;
        let mut changes = vec![Change::RemoveOp {
            index,
            op: op.clone(),
        }];
        if let OpKind::Step { n: removed, .. } = op.kind {
            for (i, o) in self.manifest.ops.iter().enumerate() {
                if let OpKind::Step {
                    at: p,
                    n,
                    size,
                    color,
                } = &o.kind
                {
                    if *n > removed {
                        let after = AnnotationOp {
                            id: o.id.clone(),
                            kind: OpKind::Step {
                                at: *p,
                                n: n - 1,
                                size: *size,
                                color: *color,
                            },
                        };
                        // Index after the removal above.
                        let idx = if i > index { i - 1 } else { i };
                        changes.push(Change::ReplaceOp {
                            index: idx,
                            before: o.clone(),
                            after,
                        });
                    }
                }
            }
        }
        self.execute("remove_op", by, at, changes)
    }

    /// Clears all ops as one undoable step ("Revert to original").
    pub fn revert_to_original(
        &mut self,
        by: Actor,
        at: OffsetDateTime,
    ) -> Result<Revision, CmError> {
        let ops = self.manifest.ops.clone();
        self.execute("revert_to_original", by, at, vec![Change::ClearOps { ops }])
    }

    fn find_op(&self, id: &str) -> Result<(usize, AnnotationOp), CmError> {
        self.manifest
            .ops
            .iter()
            .enumerate()
            .find(|(_, o)| o.id == id)
            .map(|(i, o)| (i, o.clone()))
            .ok_or_else(|| CmError::not_found(format!("no annotation `{id}`")))
    }

    fn execute(
        &mut self,
        name: &str,
        by: Actor,
        at: OffsetDateTime,
        changes: Vec<Change>,
    ) -> Result<Revision, CmError> {
        let mut action = Action::new(name, by.clone());
        for c in &changes {
            action.push(c.clone());
        }
        let r = self.stack.execute(&mut self.manifest, action, at)?;
        self.undo_log.push(StoredAction {
            name: name.to_owned(),
            by,
            changes,
        });
        self.redo_log.clear();
        Ok(self.record(r))
    }

    /// Undoes the last step. `None` when there is nothing to undo.
    pub fn undo(&mut self, by: Actor, at: OffsetDateTime) -> Option<Revision> {
        let r = self.stack.undo(&mut self.manifest, by, at)?;
        if let Some(a) = self.undo_log.pop() {
            self.redo_log.push(a);
        }
        Some(self.record(r))
    }

    /// Redoes the last undone step. `Ok(None)` when there is nothing to redo.
    pub fn redo(&mut self, by: Actor, at: OffsetDateTime) -> Result<Option<Revision>, CmError> {
        let Some(r) = self.stack.redo(&mut self.manifest, by, at)? else {
            return Ok(None);
        };
        if let Some(a) = self.redo_log.pop() {
            self.undo_log.push(a);
        }
        Ok(Some(self.record(r)))
    }

    /// Appends to the manifest's revision log with its own numbering (the in-memory stack
    /// restarts at 1 every session), keeping the last [`HISTORY_LIMIT`].
    fn record(&mut self, mut r: Revision) -> Revision {
        r.rev = self.manifest.latest_rev() + 1;
        self.manifest.revisions.push(r.clone());
        let excess = self.manifest.revisions.len().saturating_sub(HISTORY_LIMIT);
        self.manifest.revisions.drain(..excess);
        r
    }

    // ---- persistence ---------------------------------------------------------------------

    /// Writes `manifest.json` atomically, including the (capped) undo/redo history.
    pub fn save(&mut self) -> Result<(), CmError> {
        let cap = |v: &[StoredAction]| v[v.len().saturating_sub(HISTORY_LIMIT)..].to_vec();
        let untouched = self.undo_log.is_empty() && self.redo_log.is_empty();
        self.manifest.history = HistoryLog {
            undo: cap(&self.undo_log),
            redo: cap(&self.redo_log),
            // Unreadable history from a newer build stays as stored until an edit happens.
            raw: if untouched {
                self.manifest.history.raw.take()
            } else {
                None
            },
        };
        write_atomic(
            &self.dir.join(MANIFEST_FILE),
            self.manifest.to_json()?.as_bytes(),
        )
    }

    /// Stores a freshly rendered flattened PNG (from `cm-render`) as `render.png` and records
    /// which revision it reflects. Not an undo step. The manifest is saved by this call (a
    /// following [`ShotBundle::save`] is harmless).
    ///
    /// Crash-safe order (R1-18): the PNG is written to a temp file; the manifest is saved with
    /// no render recorded (so the render counts as stale and the ops are on disk); the temp
    /// file replaces `render.png`; only then the manifest records the render at the current
    /// revision. A failure at any step leaves the render stale, never a current-looking
    /// `render.png` that disagrees with the ops (which could show a redaction that was removed
    /// or miss one that was added).
    pub fn write_render(&mut self, png: &[u8]) -> Result<(), CmError> {
        let size = png_size(png, "render")?;
        let target = self.render_path();
        let mut tmp = target.as_os_str().to_owned();
        tmp.push(format!(".tmp-{}", std::process::id()));
        let tmp = PathBuf::from(tmp);
        let res = (|| {
            fs::write(&tmp, png).map_err(|e| io_err("cannot write", &tmp, &e))?;
            self.manifest.render = None;
            self.save()?;
            fs::rename(&tmp, &target).map_err(|e| io_err("cannot write", &target, &e))?;
            self.manifest.render = Some(RenderInfo {
                file: RENDER_FILE.to_owned(),
                size,
                rev: self.manifest.latest_rev(),
            });
            self.save()
        })();
        if res.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        res
    }
}

pub(crate) fn create_unique_dir(
    parent: &Path,
    base: &str,
    kind: CaptureKind,
) -> Result<PathBuf, CmError> {
    for n in 1..10_000u32 {
        let name = if n == 1 {
            format!("{base}.{}", kind.extension())
        } else {
            format!("{base} ({n}).{}", kind.extension())
        };
        let p = parent.join(name);
        match fs::create_dir(&p) {
            Ok(()) => return Ok(p),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(io_err("cannot create", &p, &e)),
        }
    }
    Err(CmError::busy("too many bundles with the same title"))
}

/// Picks a free `<title>.<ext>` path under `parent` without creating it (used for rename).
pub(crate) fn free_bundle_path(
    parent: &Path,
    base: &str,
    kind: CaptureKind,
    own: &Path,
) -> PathBuf {
    for n in 1..10_000u32 {
        let name = if n == 1 {
            format!("{base}.{}", kind.extension())
        } else {
            format!("{base} ({n}).{}", kind.extension())
        };
        let p = parent.join(name);
        if same_path(&p, own) || !p.exists() {
            return p;
        }
    }
    parent.join(format!("{base}.{}", kind.extension()))
}

/// Path equality as the file system sees it: case-insensitive on macOS (default APFS) and
/// Windows, so a case-only rename ("k8s probes" to "K8S Probes") keeps the folder name
/// instead of becoming "K8S Probes (2)" (R1-14).
pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        return a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase();
    }
    false
}

/// Rebuilds an `UndoStack` whose undo/redo stacks match the stored logs.
///
/// `UndoStack` can only grow by executing actions, so the chain is replayed from the oldest
/// retained state: rewind the manifest copy through every stored change, execute the chain
/// forward, then undo the redo part. Returns `None` if the logs do not fit the ops.
/// `manifest.broken.json`, or `manifest.broken-<unix_ts>[-n].json` when that already exists.
fn broken_manifest_path(dir: &Path) -> PathBuf {
    let first = dir.join(BROKEN_MANIFEST_FILE);
    if !first.exists() {
        return first;
    }
    let ts = OffsetDateTime::now_utc().unix_timestamp();
    let mut p = dir.join(format!("manifest.broken-{ts}.json"));
    let mut n = 1;
    while p.exists() {
        p = dir.join(format!("manifest.broken-{ts}-{n}.json"));
        n += 1;
    }
    p
}

fn rebuild_stack(
    manifest: &ShotManifest,
    undo_log: &[StoredAction],
    redo_log: &[StoredAction],
) -> Option<UndoStack<ShotManifest>> {
    use cm_core::Command;
    let mut scratch = manifest.clone();
    // Fast-forward through the redo steps (next redo is last in the log).
    for a in redo_log.iter().rev() {
        for c in &a.changes {
            c.clone().apply(&mut scratch).ok()?;
        }
    }
    // Rewind the whole chain: redo steps (newest = first in log), then undo steps newest first.
    for a in redo_log.iter().chain(undo_log.iter().rev()) {
        for c in a.changes.iter().rev() {
            c.clone().undo(&mut scratch);
        }
    }
    let mut stack = UndoStack::new();
    for a in undo_log.iter().chain(redo_log.iter().rev()) {
        let mut action = Action::new(a.name.clone(), a.by.clone());
        for c in &a.changes {
            action.push(c.clone());
        }
        stack.execute(&mut scratch, action, manifest.created).ok()?;
    }
    for _ in redo_log {
        stack.undo(&mut scratch, Actor::User, manifest.created)?;
    }
    (scratch.ops == manifest.ops).then_some(stack)
}
