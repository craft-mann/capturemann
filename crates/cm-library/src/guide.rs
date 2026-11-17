//! Click Guide bundles (GDE-1, GDE-2): `<title>.cmguide/`.
//!
//! ```text
//! <title>.cmguide/
//! ├─ manifest.json          schema 1, id, title, kind "guide", created
//! ├─ guide.json             the title, intro, numbering style, target and the ordered steps
//! ├─ steps/<name>.cmshot/   one ordinary screenshot bundle per image step (every editor tool
//! │                         works on it)
//! └─ .undo/<name>.cmshot/   step images that a deleted step left behind, kept while the undo
//!                           history can still bring them back
//! ```
//!
//! # `guide.json` (schema 1)
//!
//! ```json
//! { "schema": 1, "title": "Set up SSO", "intro": "", "numbering": "number",
//!   "target": { "kind": "display", "display_id": 1 },
//!   "step_seq": 2,
//!   "steps": [
//!     { "id": "step_1", "type": "image", "title": "Click \"Save\"", "note": "",
//!       "image": "step_1.cmshot", "click": { "x": 640, "y": 360, "button": "left", "count": 1 },
//!       "focus_crop": true, "has_proposals": false },
//!     { "id": "step_2", "type": "note", "title": "Type your password", "note": "" } ],
//!   "undo": [], "redo": [] }
//! ```
//!
//! `click` is in physical px of the step's `original.png`. `image` is the folder name under
//! `steps/`. Unknown top-level fields are kept (`extra`). A higher `schema` is refused.
//!
//! # History (GDE-2)
//!
//! Every change is one entry in `undo` (a snapshot of the guide state before it), capped at
//! [`HISTORY_LIMIT`]; `undo` and `redo` move snapshots between the two lists. A deleted step's
//! image folder moves to `.undo/` and comes back with the undo; it is removed for good once no
//! snapshot refers to it, at the next save (GDE-2.AC2: `steps/` never holds an orphan).

use crate::bundle::ShotBundle;
use crate::manifest::{CaptureKind, SCHEMA_VERSION};
use crate::util::{io_err, json_err, new_id, sanitize_title, write_atomic};
use cm_core::annotation::{AnnotationOp, OpKind, Pt, Rc, StepStyle};
use cm_core::{Actor, CmError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

pub const GUIDE_FILE: &str = "guide.json";
pub const MANIFEST_FILE: &str = "manifest.json";
pub const STEPS_DIR: &str = "steps";
pub const UNDO_DIR: &str = ".undo";
/// Undo entries kept in `guide.json`.
pub const HISTORY_LIMIT: usize = 50;
/// Most steps in one guide.
pub const MAX_STEPS: usize = 500;
/// Default focus crop, logical px (GDE-1): multiplied by the display scale factor of the step's
/// click, so a Retina display shows the same part of the screen as a 1x one.
pub const FOCUS_W: f64 = 1200.0;
pub const FOCUS_H: f64 = 800.0;
/// Longest title and note, in chars.
pub const MAX_TEXT_CHARS: usize = 2000;

/// `manifest.json` of a `.cmguide` folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuideManifest {
    pub schema: u32,
    pub id: String,
    pub title: String,
    pub kind: CaptureKind,
    #[serde(with = "time::serde::rfc3339")]
    pub created: OffsetDateTime,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl GuideManifest {
    pub fn from_json(text: &str) -> Result<Self, CmError> {
        let m: GuideManifest =
            serde_json::from_str(text).map_err(|e| json_err("guide manifest.json", &e))?;
        if m.schema > SCHEMA_VERSION {
            return Err(CmError::invalid_input(format!(
                "guide manifest schema {} is newer than this version ({SCHEMA_VERSION})",
                m.schema
            )));
        }
        if m.kind != CaptureKind::Guide {
            return Err(CmError::invalid_input("manifest kind is not `guide`"));
        }
        Ok(m)
    }
}

/// The click that made a step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClickPoint {
    pub x: f64,
    pub y: f64,
    /// `left`, `right` or `middle`.
    pub button: String,
    /// 1, or 2 for a double click.
    pub count: u32,
    /// Display scale factor of the followed target (physical px per logical px; 1 when the
    /// guide was recorded before this field existed).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub scale: f64,
}

fn one() -> f64 {
    1.0
}

fn is_one(v: &f64) -> bool {
    (*v - 1.0).abs() < f64::EPSILON
}

/// One step of a guide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuideStep {
    pub id: String,
    /// `image` or `note` (a step with no image).
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub note: String,
    /// Folder name under `steps/` (image steps).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub click: Option<ClickPoint>,
    #[serde(default)]
    pub focus_crop: bool,
    /// Kept only so the frozen schema 1 reads and writes unchanged; always false since 0.3.4
    /// (the redaction-proposals feature was withdrawn, maintainer decision Nov 15, 2026).
    #[serde(default)]
    pub has_proposals: bool,
    /// Where an automatic title came from: `screen_text` (named from the text recognized next
    /// to the click) or `generic` (`Click here`). Absent for a title a person or an agent wrote,
    /// and for guides recorded before this field existed. Optional, so schema 1 is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_source: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// [`GuideStep::title_source`] of a title named from text on the screen.
pub const TITLE_FROM_SCREEN: &str = "screen_text";
/// [`GuideStep::title_source`] of the wording used when no usable text was near the click.
pub const TITLE_GENERIC: &str = "generic";

impl GuideStep {
    pub fn is_note(&self) -> bool {
        self.kind == "note"
    }
}

/// One entry of `set_guide { steps }`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StepEdit {
    /// An existing step; `None` makes a new note step (then `title` is required).
    pub id: Option<String>,
    pub title: Option<String>,
    pub note: Option<String>,
    #[serde(default)]
    pub delete: bool,
    /// Adds a copy of the step right after it.
    #[serde(default)]
    pub duplicate: bool,
}

/// The editable state a history entry stores.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuideState {
    pub title: String,
    pub intro: String,
    pub numbering: StepStyle,
    pub steps: Vec<GuideStep>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub name: String,
    pub state: GuideState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuideDoc {
    pub schema: u32,
    pub title: String,
    #[serde(default)]
    pub intro: String,
    #[serde(default)]
    pub numbering: StepStyle,
    /// What the guide follows (a display, a window or a region), as the daemon described it.
    #[serde(default)]
    pub target: Value,
    #[serde(default)]
    pub step_seq: u64,
    #[serde(default)]
    pub steps: Vec<GuideStep>,
    #[serde(default)]
    pub undo: Vec<Snapshot>,
    #[serde(default)]
    pub redo: Vec<Snapshot>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl GuideDoc {
    fn state(&self) -> GuideState {
        GuideState {
            title: self.title.clone(),
            intro: self.intro.clone(),
            numbering: self.numbering,
            steps: self.steps.clone(),
        }
    }
    fn restore(&mut self, s: GuideState) {
        self.title = s.title;
        self.intro = s.intro;
        self.numbering = s.numbering;
        self.steps = s.steps;
    }
}

/// An open guide folder.
#[derive(Debug)]
pub struct GuideBundle {
    dir: PathBuf,
    manifest: GuideManifest,
    doc: GuideDoc,
    /// Frame size (physical px) by step image folder, read lazily from the step bundles (not
    /// stored: the guide crop needs it for every step on every change).
    sizes: HashMap<String, (u32, u32)>,
    /// A click moved the guide crop while recording and the earlier steps still hold the old
    /// rectangle (rewriting all of them on every click makes a long guide slower with every
    /// step): [`GuideBundle::settle_crops`] makes them equal, once, when the recording ends.
    crop_dirty: bool,
}

/// The focus crop of a frame of `w` x `h` physical px around `click` (physical px): 1200 x 800
/// logical px (times `scale`) centred, clamped inside the frame so the click stays in it; `None`
/// when the frame is not larger than that (nothing to crop).
pub fn focus_rect(w: u32, h: u32, click: (f64, f64), scale: f64) -> Option<Rc> {
    let scale = if scale.is_finite() && scale >= 1.0 {
        scale
    } else {
        1.0
    };
    let (w, h) = (f64::from(w), f64::from(h));
    let (fw, fh) = ((FOCUS_W * scale).round(), (FOCUS_H * scale).round());
    if w <= fw && h <= fh {
        return None;
    }
    let cw = fw.min(w);
    let ch = fh.min(h);
    let x = (click.0 - cw / 2.0).clamp(0.0, w - cw).round();
    let y = (click.1 - ch / 2.0).clamp(0.0, h - ch).round();
    Some(Rc::new(x, y, cw, ch))
}

/// A click point (physical px) and the display scale factor it was made at.
pub type ClickAt = ((f64, f64), f64);

/// The ONE crop of every image step of a guide whose frames are `frame` physical px (GDE-1,
/// maintainer review Nov 15, 2026: a crop centred on each click made the framing jump between
/// steps). `clicks` are the click points (physical px) with their display scale factors. The box
/// is the bounding box of all click points padded by half the default focus size (600 x 400
/// logical px times each click's scale) on every side, so it is never smaller than the default
/// 1200 x 800 logical px; it is moved (not shrunk) to lie inside the frame. `None` when it covers
/// more than 85 % of the frame (nothing worth cropping) or there are no clicks.
pub fn guide_crop_rect(frame: (u32, u32), clicks: &[ClickAt]) -> Option<Rc> {
    let (w, h) = (f64::from(frame.0), f64::from(frame.1));
    if clicks.is_empty() || w < 1.0 || h < 1.0 {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &((cx, cy), scale) in clicks {
        let s = if scale.is_finite() && scale >= 1.0 {
            scale
        } else {
            1.0
        };
        let (cx, cy) = (cx.clamp(0.0, w), cy.clamp(0.0, h));
        let (px, py) = ((FOCUS_W * s / 2.0).round(), (FOCUS_H * s / 2.0).round());
        x0 = x0.min(cx - px);
        y0 = y0.min(cy - py);
        x1 = x1.max(cx + px);
        y1 = y1.max(cy + py);
    }
    let (bw, bh) = ((x1 - x0).min(w), (y1 - y0).min(h));
    let x = x0.clamp(0.0, w - bw).round();
    let y = y0.clamp(0.0, h - bh).round();
    if bw * bh > 0.85 * w * h {
        return None;
    }
    Some(Rc::new(x, y, bw.round(), bh.round()))
}

/// The badge and ring ops of a click on a frame of `w` x `h` (GDE-1): the hollow ring is centred
/// on the click point, so the thing that was clicked stays visible inside it, and the numbered
/// badge sits on the ring's upper-left (maintainer review of a real guide, Nov 14, 2026: a badge
/// centred on the click hid the label it pointed at).
pub fn click_ops(
    n: u32,
    click: (f64, f64),
    frame: (u32, u32),
    style: StepStyle,
    crop: Option<Rc>,
) -> Vec<AnnotationOp> {
    let long = f64::from(frame.0.max(frame.1));
    let size = (long * 0.02).clamp(36.0, 72.0).round();
    let r = (size * 1.6).round();
    // The badge's centre on the ring, up and to the left (45 degrees).
    let off = r * std::f64::consts::FRAC_1_SQRT_2;
    let at = Pt::new((click.0 - off).max(size), (click.1 - off).max(size));
    let mut ops = vec![
        AnnotationOp::new(
            "",
            OpKind::Ellipse {
                rect: Rc::new(click.0 - r, click.1 - r, r * 2.0, r * 2.0),
                color: cm_core::annotation::DEFAULT_COLOR,
                width: 5.0,
                // The design's 6 px glow around the 52 px ring, scaled with the ring.
                glow: (r * 0.23).round(),
                style: Default::default(),
            },
        ),
        AnnotationOp::new(
            "",
            OpKind::Step {
                at,
                n,
                size,
                color: cm_core::annotation::DEFAULT_COLOR,
                style,
                start: 1,
                text: None,
            },
        ),
    ];
    if let Some(rect) = crop {
        ops.push(AnnotationOp::new("", OpKind::Crop { rect }));
    }
    ops
}

fn clean(s: &str, field: &str) -> Result<String, CmError> {
    let s = s.trim();
    if s.chars().count() > MAX_TEXT_CHARS {
        return Err(CmError::invalid_field(
            field,
            format!("must be at most {MAX_TEXT_CHARS} characters"),
        ));
    }
    Ok(s.to_owned())
}

fn remove_dir_force(dir: &Path) {
    fn unlock(dir: &Path) {
        if let Ok(rd) = fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    unlock(&p);
                } else if let Ok(md) = fs::metadata(&p) {
                    let mut perms = md.permissions();
                    if perms.readonly() {
                        #[allow(clippy::permissions_set_readonly_false)]
                        perms.set_readonly(false);
                        let _ = fs::set_permissions(&p, perms);
                    }
                }
            }
        }
    }
    unlock(dir);
    let _ = fs::remove_dir_all(dir);
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), CmError> {
    fs::create_dir_all(dst).map_err(|e| io_err("cannot create", dst, &e))?;
    for e in fs::read_dir(src)
        .map_err(|e| io_err("cannot read", src, &e))?
        .flatten()
    {
        let (from, to) = (e.path(), dst.join(e.file_name()));
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| io_err("cannot copy", &from, &e))?;
        }
    }
    Ok(())
}

impl GuideBundle {
    /// Creates `<parent>/<title>.cmguide/` with an empty guide.
    pub fn create(
        parent: &Path,
        title: &str,
        target: Value,
        now: OffsetDateTime,
    ) -> Result<Self, CmError> {
        let base = sanitize_title(title)?;
        fs::create_dir_all(parent).map_err(|e| io_err("cannot create", parent, &e))?;
        let mut dir = None;
        for n in 1..10_000u32 {
            let name = if n == 1 {
                format!("{base}.cmguide")
            } else {
                format!("{base} ({n}).cmguide")
            };
            let p = parent.join(name);
            match fs::create_dir(&p) {
                Ok(()) => {
                    dir = Some(p);
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(io_err("cannot create", &p, &e)),
            }
        }
        let dir = dir.ok_or_else(|| CmError::busy("too many guides with the same title"))?;
        let manifest = GuideManifest {
            schema: SCHEMA_VERSION,
            id: new_id("guide", now),
            title: title.trim().to_owned(),
            kind: CaptureKind::Guide,
            created: now,
            extra: Map::new(),
        };
        let doc = GuideDoc {
            schema: SCHEMA_VERSION,
            title: title.trim().to_owned(),
            intro: String::new(),
            numbering: StepStyle::Number,
            target,
            step_seq: 0,
            steps: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            extra: Map::new(),
        };
        let mut g = GuideBundle {
            dir,
            manifest,
            doc,
            sizes: HashMap::new(),
            crop_dirty: false,
        };
        let res = fs::create_dir_all(g.dir.join(STEPS_DIR))
            .map_err(|e| io_err("cannot create", &g.dir, &e))
            .and_then(|()| g.save());
        if let Err(e) = res {
            remove_dir_force(&g.dir);
            return Err(e);
        }
        Ok(g)
    }

    pub fn open(dir: &Path) -> Result<Self, CmError> {
        let mp = dir.join(MANIFEST_FILE);
        let text = fs::read_to_string(&mp).map_err(|e| io_err("cannot read", &mp, &e))?;
        let manifest = GuideManifest::from_json(&text)?;
        let gp = dir.join(GUIDE_FILE);
        let text = fs::read_to_string(&gp).map_err(|e| io_err("cannot read", &gp, &e))?;
        let mut doc: GuideDoc =
            serde_json::from_str(&text).map_err(|e| json_err("guide.json", &e))?;
        // The manifest is what the library (rename_capture) edits: it wins.
        if doc.title != manifest.title {
            doc.title.clone_from(&manifest.title);
        }
        if doc.schema > SCHEMA_VERSION {
            return Err(CmError::invalid_input(format!(
                "guide.json schema {} is newer than this version ({SCHEMA_VERSION})",
                doc.schema
            )));
        }
        Ok(GuideBundle {
            dir: dir.to_owned(),
            manifest,
            doc,
            sizes: HashMap::new(),
            crop_dirty: false,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn id(&self) -> &str {
        &self.manifest.id
    }
    pub fn manifest(&self) -> &GuideManifest {
        &self.manifest
    }
    pub fn doc(&self) -> &GuideDoc {
        &self.doc
    }
    pub fn title(&self) -> &str {
        &self.doc.title
    }
    pub fn steps(&self) -> &[GuideStep] {
        &self.doc.steps
    }
    pub fn step(&self, id: &str) -> Result<&GuideStep, CmError> {
        self.doc
            .steps
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| CmError::not_found(format!("no step `{id}` in this guide")))
    }
    pub fn can_undo(&self) -> bool {
        !self.doc.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.doc.redo.is_empty()
    }
    pub fn next_undo_name(&self) -> Option<&str> {
        self.doc.undo.last().map(|s| s.name.as_str())
    }
    /// The folder of a step's screenshot bundle.
    pub fn step_dir(&self, step_id: &str) -> Result<PathBuf, CmError> {
        let s = self.step(step_id)?;
        let img = s
            .image
            .as_ref()
            .ok_or_else(|| CmError::invalid_input(format!("step `{step_id}` has no image")))?;
        Ok(self.dir.join(STEPS_DIR).join(img))
    }
    pub fn open_step(&self, step_id: &str) -> Result<ShotBundle, CmError> {
        ShotBundle::open(&self.step_dir(step_id)?)
    }

    /// Writes `guide.json` and `manifest.json`, then settles the image folders (see the module
    /// docs).
    pub fn save(&mut self) -> Result<(), CmError> {
        self.manifest.title.clone_from(&self.doc.title);
        let over = self.doc.undo.len().saturating_sub(HISTORY_LIMIT);
        self.doc.undo.drain(..over);
        let over = self.doc.redo.len().saturating_sub(HISTORY_LIMIT);
        self.doc.redo.drain(..over);
        self.reconcile()?;
        let text =
            serde_json::to_string_pretty(&self.doc).map_err(|e| json_err("guide.json", &e))?;
        write_atomic(&self.dir.join(GUIDE_FILE), text.as_bytes())?;
        let text = serde_json::to_string_pretty(&self.manifest)
            .map_err(|e| json_err("manifest.json", &e))?;
        write_atomic(&self.dir.join(MANIFEST_FILE), text.as_bytes())
    }

    fn images(steps: &[GuideStep]) -> impl Iterator<Item = &str> {
        steps.iter().filter_map(|s| s.image.as_deref())
    }

    /// Image folders are in `steps/` exactly when the current steps use them, in `.undo/` while
    /// a history entry still needs them, and gone otherwise.
    fn reconcile(&self) -> Result<(), CmError> {
        let current: BTreeSet<&str> = Self::images(&self.doc.steps).collect();
        let mut kept: BTreeSet<&str> = current.clone();
        for snap in self.doc.undo.iter().chain(&self.doc.redo) {
            kept.extend(Self::images(&snap.state.steps));
        }
        let steps = self.dir.join(STEPS_DIR);
        let undo = self.dir.join(UNDO_DIR);
        let list = |d: &Path| -> Vec<String> {
            fs::read_dir(d)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| e.path().is_dir())
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default()
        };
        for name in list(&undo) {
            let from = undo.join(&name);
            if current.contains(name.as_str()) {
                fs::create_dir_all(&steps).map_err(|e| io_err("cannot create", &steps, &e))?;
                fs::rename(&from, steps.join(&name))
                    .map_err(|e| io_err("cannot move", &from, &e))?;
            } else if !kept.contains(name.as_str()) {
                remove_dir_force(&from);
            }
        }
        for name in list(&steps) {
            if current.contains(name.as_str()) {
                continue;
            }
            let from = steps.join(&name);
            if kept.contains(name.as_str()) {
                fs::create_dir_all(&undo).map_err(|e| io_err("cannot create", &undo, &e))?;
                fs::rename(&from, undo.join(&name))
                    .map_err(|e| io_err("cannot move", &from, &e))?;
            } else {
                remove_dir_force(&from);
            }
        }
        if list(&undo).is_empty() {
            let _ = fs::remove_dir(&undo);
        }
        Ok(())
    }

    fn mint_step_id(&mut self) -> String {
        self.doc.step_seq += 1;
        format!("step_{}", self.doc.step_seq)
    }

    /// Makes the badge numbers follow the step order (GDE-2): the badge of the step in position
    /// p carries `n = p`, styled with the guide's numbering. Only changed badges are written.
    fn renumber(&self, by: &Actor, at: OffsetDateTime) -> Result<(), CmError> {
        for (i, s) in self.doc.steps.iter().enumerate() {
            let Some(img) = &s.image else { continue };
            let dir = self.dir.join(STEPS_DIR).join(img);
            if !dir.is_dir() {
                continue;
            }
            let mut b = ShotBundle::open(&dir)?;
            let want = u32::try_from(i + 1).unwrap_or(u32::MAX);
            let mut edits = Vec::new();
            for op in b.ops() {
                if let OpKind::Step {
                    at: p,
                    n,
                    size,
                    color,
                    style,
                    start,
                    text,
                } = &op.kind
                {
                    if *n != want || *style != self.doc.numbering {
                        edits.push((
                            op.id.clone(),
                            OpKind::Step {
                                at: *p,
                                n: want,
                                size: *size,
                                color: *color,
                                style: self.doc.numbering,
                                start: *start,
                                text: text.clone(),
                            },
                        ));
                    }
                }
            }
            if edits.is_empty() {
                continue;
            }
            for (id, kind) in edits {
                b.update_op(&id, kind, by.clone(), at)?;
            }
            b.save()?;
        }
        Ok(())
    }

    /// Runs one change as one undo step: snapshot, change, renumber, save.
    fn change(
        &mut self,
        name: &str,
        by: &Actor,
        at: OffsetDateTime,
        f: impl FnOnce(&mut Self) -> Result<(), CmError>,
    ) -> Result<(), CmError> {
        let before = self.doc.state();
        f(self)?;
        if self.doc.state() == before {
            return Ok(());
        }
        self.doc.undo.push(Snapshot {
            name: name.to_owned(),
            state: before,
        });
        self.doc.redo.clear();
        self.renumber(by, at)?;
        self.save()
    }

    // ---- the guide crop (GDE-1) ----------------------------------------------------------

    /// Frame size of the image folder `img` (cached).
    fn frame_size(&mut self, img: &str) -> Option<(u32, u32)> {
        if let Some(s) = self.sizes.get(img) {
            return Some(*s);
        }
        let dir = self.dir.join(STEPS_DIR).join(img);
        if !dir.is_dir() {
            return None;
        }
        let b = ShotBundle::open(&dir).ok()?;
        let z = b.manifest().original.size;
        self.sizes.insert(img.to_owned(), (z.width, z.height));
        Some((z.width, z.height))
    }

    /// Click points (physical px, with their display scale) of the image steps, by image folder.
    fn image_clicks(&self) -> Vec<(String, ClickAt)> {
        self.doc
            .steps
            .iter()
            .filter_map(|s| {
                let c = s.click.as_ref()?;
                Some((s.image.clone()?, ((c.x, c.y), c.scale)))
            })
            .collect()
    }

    /// The click points of the image steps whose frame is `frame`.
    fn group_clicks(&mut self, frame: (u32, u32)) -> Vec<ClickAt> {
        self.image_clicks()
            .into_iter()
            .filter(|(img, _)| self.frame_size(img) == Some(frame))
            .map(|(_, c)| c)
            .collect()
    }

    /// The guide follows one chosen window (its target names a `window_id`): every step keeps the
    /// whole window, with no automatic crop.
    pub fn follows_window(&self) -> bool {
        self.doc.target.get("window_id").is_some()
    }

    /// The guide crop of every frame size in the guide: the click points of ALL image steps (not
    /// only those whose crop is on) grouped by frame size, because steps with different frame
    /// sizes (the display changed in between) cannot share one rectangle.
    fn guide_rects(&mut self) -> HashMap<(u32, u32), Option<Rc>> {
        if self.follows_window() {
            return HashMap::new();
        }
        let mut groups: HashMap<(u32, u32), Vec<ClickAt>> = HashMap::new();
        for (img, c) in self.image_clicks() {
            if let Some(size) = self.frame_size(&img) {
                groups.entry(size).or_default().push(c);
            }
        }
        groups
            .into_iter()
            .map(|(size, clicks)| (size, guide_crop_rect(size, &clicks)))
            .collect()
    }

    /// Makes the `crop` op of every image step whose crop is on equal the guide crop of its
    /// frame size; `only` limits it to one step. Old guides are not touched until an edit calls
    /// this. Ring and badge ops are in original image coordinates and need no change: the crop
    /// only chooses the visible part.
    fn apply_guide_crop(
        &mut self,
        only: Option<&str>,
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<(), CmError> {
        let rects = self.guide_rects();
        let steps: Vec<String> = self
            .doc
            .steps
            .iter()
            .filter(|s| s.focus_crop && only.is_none_or(|o| o == s.id))
            .filter_map(|s| s.image.clone())
            .collect();
        for img in steps {
            let Some(size) = self.frame_size(&img) else {
                continue;
            };
            let want = rects.get(&size).copied().flatten();
            let mut b = ShotBundle::open(&self.dir.join(STEPS_DIR).join(&img))?;
            let have: Vec<(String, Rc)> = b
                .ops()
                .iter()
                .filter_map(|o| match &o.kind {
                    OpKind::Crop { rect } => Some((o.id.clone(), *rect)),
                    _ => None,
                })
                .collect();
            let same = match want {
                Some(r) => have.len() == 1 && have[0].1 == r,
                None => have.is_empty(),
            };
            if same {
                continue;
            }
            for (id, _) in &have {
                b.remove_op(id, by.clone(), at)?;
            }
            if let Some(rect) = want {
                b.add_ops(
                    vec![AnnotationOp::new("", OpKind::Crop { rect })],
                    by.clone(),
                    at,
                )?;
            }
            b.save()?;
        }
        Ok(())
    }

    // ---- recording (not undo steps: the user is clicking through a task) -----------------

    /// Adds an image step made by a click: its own screenshot bundle with the badge and ring
    /// (and the focus crop) as ordinary ops. `title` is the draft title.
    pub fn add_click_step(
        &mut self,
        png: &[u8],
        click: ClickPoint,
        title: &str,
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<GuideStep, CmError> {
        self.add_click_step_titled(png, click, title, None, by, at)
    }

    /// [`GuideBundle::add_click_step`] saying where the draft title came from
    /// ([`TITLE_FROM_SCREEN`] or [`TITLE_GENERIC`]).
    pub fn add_click_step_titled(
        &mut self,
        png: &[u8],
        click: ClickPoint,
        title: &str,
        title_source: Option<&str>,
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<GuideStep, CmError> {
        if self.doc.steps.len() >= MAX_STEPS {
            return Err(CmError::invalid_input(format!(
                "a guide holds at most {MAX_STEPS} steps"
            )));
        }
        let id = self.mint_step_id();
        let steps = self.dir.join(STEPS_DIR);
        let mut b = ShotBundle::create(&steps, &id, png, at)?;
        let size = b.manifest().original.size;
        let n = u32::try_from(self.doc.steps.len() + 1).unwrap_or(u32::MAX);
        // The crop is on when the frame is larger than the default focus; its rectangle is the
        // guide crop (all clicks of this frame size, this one included).
        // A guide of a chosen window keeps the whole window in every step (maintainer decision
        // Nov 16, 2026): the crop and zoom are the user's to set later in the editor.
        let crop = !self.follows_window()
            && focus_rect(size.width, size.height, (click.x, click.y), click.scale).is_some();
        let frame = (size.width, size.height);
        let mut clicks = self.group_clicks(frame);
        let before = guide_crop_rect(frame, &clicks);
        clicks.push(((click.x, click.y), click.scale));
        let rect = guide_crop_rect(frame, &clicks);
        let res = b
            .add_ops(
                click_ops(
                    n,
                    (click.x, click.y),
                    frame,
                    self.doc.numbering,
                    if crop { rect } else { None },
                ),
                by.clone(),
                at,
            )
            .and_then(|_| b.save());
        if let Err(e) = res {
            remove_dir_force(b.dir());
            return Err(e);
        }
        let image = b
            .dir()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        let step = GuideStep {
            id,
            kind: "image".to_owned(),
            title: clean(title, "title")?,
            note: String::new(),
            image,
            click: Some(click),
            focus_crop: crop,
            has_proposals: false,
            title_source: title_source.map(str::to_owned),
            extra: Map::new(),
        };
        if let Some(name) = &step.image {
            self.sizes.insert(name.clone(), frame);
        }
        self.doc.steps.push(step.clone());
        self.save()?;
        // A click outside the old box moves the rectangle of the earlier steps too; that is
        // done once, when the recording ends (every step already shows its own click).
        if rect != before {
            self.crop_dirty = true;
        }
        Ok(step)
    }

    /// Makes the crop of every step the guide crop again after clicks moved it during the
    /// recording. Call it when the recording ends; a no-op when nothing moved.
    pub fn settle_crops(&mut self, by: &Actor, at: OffsetDateTime) -> Result<(), CmError> {
        if !self.crop_dirty {
            return Ok(());
        }
        self.crop_dirty = false;
        self.apply_guide_crop(None, by, at)
    }

    /// HUD "Add note step": a step with no image.
    pub fn add_note_step(&mut self, title: &str, note: &str) -> Result<GuideStep, CmError> {
        let id = self.mint_step_id();
        let step = GuideStep {
            id,
            kind: "note".to_owned(),
            title: clean(title, "title")?,
            note: clean(note, "note")?,
            image: None,
            click: None,
            focus_crop: false,
            has_proposals: false,
            title_source: None,
            extra: Map::new(),
        };
        self.doc.steps.push(step.clone());
        self.save()?;
        Ok(step)
    }

    /// HUD "Undo last step": drops the newest step and its image for good.
    pub fn remove_last_step(
        &mut self,
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<Option<GuideStep>, CmError> {
        let Some(s) = self.doc.steps.pop() else {
            return Ok(None);
        };
        self.save()?;
        if s.click.is_some() {
            // The crop of the steps left is made equal when the recording ends.
            self.crop_dirty = true;
        }
        let _ = (by, at);
        Ok(Some(s))
    }

    /// A second mouse-down at the same spot is a double click: the newest image step keeps its
    /// frame and takes the new title and click count (GDE-1).
    pub fn amend_last_click(
        &mut self,
        count: u32,
        title: &str,
        title_source: Option<&str>,
    ) -> Result<Option<String>, CmError> {
        let title = clean(title, "title")?;
        let Some(s) = self.doc.steps.iter_mut().rev().find(|s| s.click.is_some()) else {
            return Ok(None);
        };
        if let Some(c) = &mut s.click {
            c.count = count;
        }
        s.title = title;
        s.title_source = title_source.map(str::to_owned);
        let id = s.id.clone();
        self.save()?;
        Ok(Some(id))
    }

    /// Replaces the automatic title of a step while the guide records (the text read next to
    /// the click arrived after the step was written). Not an undo step. `false` when the step
    /// is gone or its title is no longer automatic (a person changed it).
    pub fn set_draft_title(
        &mut self,
        step_id: &str,
        title: &str,
        title_source: &str,
    ) -> Result<bool, CmError> {
        let title = clean(title, "title")?;
        let Some(s) = self
            .doc
            .steps
            .iter_mut()
            .find(|s| s.id == step_id && s.title_source.is_some())
        else {
            return Ok(false);
        };
        s.title = title;
        s.title_source = Some(title_source.to_owned());
        self.save()?;
        Ok(true)
    }

    /// The click of an image step (`None` for a note or a missing step).
    pub fn click_of(&self, step_id: &str) -> Option<ClickPoint> {
        self.doc
            .steps
            .iter()
            .find(|s| s.id == step_id)
            .and_then(|s| s.click.clone())
    }

    // ---- editing (GDE-2): every call is one undo step ------------------------------------

    pub fn set_title(
        &mut self,
        title: &str,
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<(), CmError> {
        let t = clean(title, "title")?;
        if t.is_empty() {
            return Err(CmError::invalid_field("title", "must not be empty"));
        }
        self.change("guide_title", by, at, |g| {
            g.doc.title = t;
            Ok(())
        })
    }

    pub fn set_intro(
        &mut self,
        intro: &str,
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<(), CmError> {
        let t = clean(intro, "intro")?;
        self.change("guide_intro", by, at, |g| {
            g.doc.intro = t;
            Ok(())
        })
    }

    pub fn set_numbering(
        &mut self,
        style: StepStyle,
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<(), CmError> {
        self.change("guide_numbering", by, at, |g| {
            g.doc.numbering = style;
            Ok(())
        })
    }

    pub fn set_step_text(
        &mut self,
        id: &str,
        title: Option<&str>,
        note: Option<&str>,
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<(), CmError> {
        let title = title.map(|t| clean(t, "title")).transpose()?;
        let note = note.map(|t| clean(t, "note")).transpose()?;
        self.step(id)?;
        self.change("step_text", by, at, |g| {
            let s = g
                .doc
                .steps
                .iter_mut()
                .find(|s| s.id == id)
                .expect("checked");
            if let Some(t) = title {
                s.title = t;
                // A title somebody wrote is not a draft any more.
                s.title_source = None;
            }
            if let Some(n) = note {
                s.note = n;
            }
            Ok(())
        })
    }

    /// Shows or hides the guide crop on an image step (a `crop` op in its bundle).
    pub fn set_focus_crop(
        &mut self,
        id: &str,
        on: bool,
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<(), CmError> {
        self.step_dir(id)?;
        self.change("focus_crop", by, at, |g| {
            let s = g
                .doc
                .steps
                .iter_mut()
                .find(|s| s.id == id)
                .expect("checked");
            s.focus_crop = on;
            Ok(())
        })?;
        if on {
            return self.apply_guide_crop(Some(id), by, at);
        }
        let mut b = self.open_step(id)?;
        let crops: Vec<String> = b
            .ops()
            .iter()
            .filter(|o| matches!(o.kind, OpKind::Crop { .. }))
            .map(|o| o.id.clone())
            .collect();
        for c in crops {
            b.remove_op(&c, by.clone(), at)?;
        }
        b.save()
    }

    pub fn delete_step(&mut self, id: &str, by: &Actor, at: OffsetDateTime) -> Result<(), CmError> {
        self.step(id)?;
        self.change("delete_step", by, at, |g| {
            g.doc.steps.retain(|s| s.id != id);
            Ok(())
        })?;
        self.apply_guide_crop(None, by, at)
    }

    /// Moves a step to `to` (0-based position in the new order).
    pub fn move_step(
        &mut self,
        id: &str,
        to: usize,
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<(), CmError> {
        self.step(id)?;
        self.change("move_step", by, at, |g| {
            let from = g
                .doc
                .steps
                .iter()
                .position(|s| s.id == id)
                .expect("checked");
            let s = g.doc.steps.remove(from);
            let to = to.min(g.doc.steps.len());
            g.doc.steps.insert(to, s);
            Ok(())
        })
    }

    /// Puts the steps in the order of `ids`; steps not named follow in their old order.
    pub fn reorder(
        &mut self,
        ids: &[String],
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<(), CmError> {
        for id in ids {
            self.step(id)?;
        }
        self.change("reorder_steps", by, at, |g| {
            let mut rest = std::mem::take(&mut g.doc.steps);
            let mut out = Vec::with_capacity(rest.len());
            for id in ids {
                if let Some(p) = rest.iter().position(|s| &s.id == id) {
                    out.push(rest.remove(p));
                }
            }
            out.extend(rest);
            g.doc.steps = out;
            Ok(())
        })
    }

    /// Makes a copy of `src` with a new id and its own image folder (not yet in the step list).
    fn copy_step(&mut self, src: &GuideStep, at: OffsetDateTime) -> Result<GuideStep, CmError> {
        if self.doc.steps.len() >= MAX_STEPS {
            return Err(CmError::invalid_input("the guide is full"));
        }
        let new_id = self.mint_step_id();
        let mut copy = src.clone();
        copy.id.clone_from(&new_id);
        if let Some(img) = &src.image {
            let steps = self.dir.join(STEPS_DIR);
            let name = format!("{new_id}.cmshot");
            let dst = steps.join(&name);
            copy_tree(&steps.join(img), &dst)?;
            let mp = dst.join(crate::bundle::MANIFEST_FILE);
            let res = (|| {
                let text = fs::read_to_string(&mp).map_err(|e| io_err("cannot read", &mp, &e))?;
                let mut v: Value =
                    serde_json::from_str(&text).map_err(|e| json_err("manifest.json", &e))?;
                if let Some(m) = v.as_object_mut() {
                    m.insert("id".into(), Value::String(new_id_for_shot(at)));
                    m.insert("title".into(), Value::String(new_id.clone()));
                }
                let out =
                    serde_json::to_string_pretty(&v).map_err(|e| json_err("manifest.json", &e))?;
                write_atomic(&mp, out.as_bytes())
            })();
            if let Err(e) = res {
                remove_dir_force(&dst);
                return Err(e);
            }
            copy.image = Some(name);
        }
        Ok(copy)
    }

    /// Copies a step (image bundle included) right after the original.
    pub fn duplicate_step(
        &mut self,
        id: &str,
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<String, CmError> {
        let src = self.step(id)?.clone();
        let copy = self.copy_step(&src, at)?;
        let new_id = copy.id.clone();
        self.change("duplicate_step", by, at, |g| {
            let p = g
                .doc
                .steps
                .iter()
                .position(|s| s.id == id)
                .expect("checked");
            g.doc.steps.insert(p + 1, copy);
            Ok(())
        })?;
        Ok(new_id)
    }

    /// `set_guide { steps }`: the new order, titles, notes, deletions, duplicates and new note
    /// steps as ONE undo step. Steps the list does not name keep their order after the named
    /// ones.
    pub fn edit_steps(
        &mut self,
        edits: &[StepEdit],
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<(), CmError> {
        let mut copies: Vec<Option<GuideStep>> = Vec::with_capacity(edits.len());
        for (i, e) in edits.iter().enumerate() {
            let field = format!("steps[{i}]");
            match &e.id {
                Some(id) => {
                    self.step(id)
                        .map_err(|_| CmError::invalid_field(&field, format!("no step `{id}`")))?;
                    if let Some(t) = &e.title {
                        clean(t, &field)?;
                    }
                    if let Some(t) = &e.note {
                        clean(t, &field)?;
                    }
                }
                None if e.title.is_none() => {
                    return Err(CmError::invalid_field(
                        &field,
                        "a step without `id` is a new note step and needs a `title`",
                    ))
                }
                None => {}
            }
            copies.push(None);
        }
        for (i, e) in edits.iter().enumerate() {
            if let (true, false, Some(id)) = (e.duplicate, e.delete, &e.id) {
                let src = self.step(id)?.clone();
                copies[i] = Some(self.copy_step(&src, at)?);
            }
        }
        let mut notes = Vec::new();
        for (e, c) in edits.iter().zip(&copies) {
            if e.id.is_none() && c.is_none() {
                let id = self.mint_step_id();
                notes.push(GuideStep {
                    id,
                    kind: "note".to_owned(),
                    title: clean(e.title.as_deref().unwrap_or_default(), "title")?,
                    note: clean(e.note.as_deref().unwrap_or_default(), "note")?,
                    image: None,
                    click: None,
                    focus_crop: false,
                    has_proposals: false,
                    title_source: None,
                    extra: Map::new(),
                });
            }
        }
        self.change("edit_steps", by, at, |g| {
            let mut rest = std::mem::take(&mut g.doc.steps);
            let mut out = Vec::with_capacity(rest.len() + edits.len());
            let mut notes = notes.into_iter();
            for (e, copy) in edits.iter().zip(copies) {
                let Some(id) = &e.id else {
                    out.extend(notes.next());
                    continue;
                };
                let Some(p) = rest.iter().position(|s| &s.id == id) else {
                    continue;
                };
                let mut s = rest.remove(p);
                if e.delete {
                    continue;
                }
                if let Some(t) = &e.title {
                    // The editor sends every title back: only a changed one stops being a draft.
                    if s.title != t.trim() {
                        s.title = t.trim().to_owned();
                        s.title_source = None;
                    }
                }
                if let Some(t) = &e.note {
                    s.note = t.trim().to_owned();
                }
                out.push(s);
                out.extend(copy);
            }
            out.extend(rest);
            g.doc.steps = out;
            Ok(())
        })?;
        self.reconcile()?;
        self.apply_guide_crop(None, by, at)
    }

    /// Inserts a note step after `after` (or at the end).
    pub fn insert_note_step(
        &mut self,
        after: Option<&str>,
        title: &str,
        note: &str,
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<String, CmError> {
        let (title, note) = (clean(title, "title")?, clean(note, "note")?);
        if let Some(a) = after {
            self.step(a)?;
        }
        let id = self.mint_step_id();
        let step = GuideStep {
            id: id.clone(),
            kind: "note".to_owned(),
            title,
            note,
            image: None,
            click: None,
            focus_crop: false,
            has_proposals: false,
            title_source: None,
            extra: Map::new(),
        };
        self.change("add_note_step", by, at, |g| {
            let p = after
                .and_then(|a| g.doc.steps.iter().position(|s| s.id == a))
                .map_or(g.doc.steps.len(), |p| p + 1);
            g.doc.steps.insert(p, step);
            Ok(())
        })?;
        Ok(id)
    }

    /// Replaces a step's picture with a fresh frame. The badge, ring and crop are made again
    /// from the click point, so earlier edits of the old picture are not carried over.
    pub fn retake_step(
        &mut self,
        id: &str,
        png: &[u8],
        by: &Actor,
        at: OffsetDateTime,
    ) -> Result<(), CmError> {
        let step = self.step(id)?.clone();
        let click = step
            .click
            .clone()
            .ok_or_else(|| CmError::invalid_input("a note step has no picture to retake"))?;
        let steps = self.dir.join(STEPS_DIR);
        let mut b = ShotBundle::create(&steps, &format!("{id}-retake"), png, at)?;
        let size = b.manifest().original.size;
        let pos = self
            .doc
            .steps
            .iter()
            .position(|s| s.id == id)
            .expect("checked");
        let crop = step.focus_crop
            && focus_rect(size.width, size.height, (click.x, click.y), click.scale).is_some();
        let res = b
            .add_ops(
                // The crop op is set by the guide crop below, once the new frame is the step's.
                click_ops(
                    u32::try_from(pos + 1).unwrap_or(u32::MAX),
                    (click.x, click.y),
                    (size.width, size.height),
                    self.doc.numbering,
                    None,
                ),
                by.clone(),
                at,
            )
            .and_then(|_| b.save());
        if let Err(e) = res {
            remove_dir_force(b.dir());
            return Err(e);
        }
        let name = b
            .dir()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        self.change("retake_step", by, at, |g| {
            let s = g
                .doc
                .steps
                .iter_mut()
                .find(|s| s.id == id)
                .expect("checked");
            s.image = name;
            s.focus_crop = crop;
            s.has_proposals = false;
            Ok(())
        })?;
        self.apply_guide_crop(None, by, at)
    }

    // ---- undo and redo -------------------------------------------------------------------

    pub fn undo(&mut self, by: &Actor, at: OffsetDateTime) -> Result<Option<String>, CmError> {
        let Some(snap) = self.doc.undo.pop() else {
            return Ok(None);
        };
        self.doc.redo.push(Snapshot {
            name: snap.name.clone(),
            state: self.doc.state(),
        });
        self.doc.restore(snap.state);
        self.save()?;
        self.renumber(by, at)?;
        self.apply_guide_crop(None, by, at)?;
        Ok(Some(snap.name))
    }

    pub fn redo(&mut self, by: &Actor, at: OffsetDateTime) -> Result<Option<String>, CmError> {
        let Some(snap) = self.doc.redo.pop() else {
            return Ok(None);
        };
        self.doc.undo.push(Snapshot {
            name: snap.name.clone(),
            state: self.doc.state(),
        });
        self.doc.restore(snap.state);
        self.save()?;
        self.renumber(by, at)?;
        self.apply_guide_crop(None, by, at)?;
        Ok(Some(snap.name))
    }
}

fn new_id_for_shot(at: OffsetDateTime) -> String {
    new_id("shot", at)
}

/// The `.cmguide` folder that holds the step bundle at `step_dir` (`<guide>/steps/<name>`).
pub fn guide_dir_of_step(step_dir: &Path) -> Option<PathBuf> {
    let steps = step_dir.parent()?;
    if steps.file_name()? != STEPS_DIR {
        return None;
    }
    let guide = steps.parent()?;
    (guide.extension()? == "cmguide").then(|| guide.to_owned())
}
