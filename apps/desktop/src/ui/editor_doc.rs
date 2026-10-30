//! The open capture of the editor: the display-free `EditorState`, the preview render
//! pipeline and the ordered queue of daemon calls. It is an entity of its own so queued
//! saves finish even if the user leaves the editor or opens another capture.
//!
//! Auto-save (ANN-3): every edit is sent to the daemon as soon as it is made (one `annotate`,
//! `undo` or `redo` per user action); nothing is held back until a "Save" click. The status
//! line shows "Saving..." while calls are queued.

use cm_core::{AnnotationOp, OpKind, Pt, Rc};
use cm_i18n::tr;
use gpui::{Context, Entity, RenderImage};
use image::{Frame, RgbaImage};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use crate::backend::Backend;
use crate::daemon_text::{AppNote, ErrorNote};
use crate::editor::session::{self, Job, JobResult};
use crate::editor::state::{Chrome, Edit, EditorState, EscapeOutcome};
use crate::editor::tools::{ParamValue, Styles, Tool};
use crate::editor::{preview, KeyCommand, Mods};
use crate::flows;
use crate::toast::{ToastKind, ToastSpec};
use crate::ui::app::{self, AppModel};

pub const STYLES_KEY: &str = "editor.tool_styles";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Loading,
    Ready,
    Failed(ErrorNote),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Saved,
    Saving,
    Failed(String),
}

/// Where Auto-redact is (ANN-2.AC4). The proposals themselves live in `state.proposals`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedactPhase {
    Idle,
    /// The recognizer is running off the UI thread.
    Running,
    /// It ran and found nothing worth redacting.
    NothingFound,
    Failed(ErrorNote),
}

enum Step {
    Job(Job),
    /// Copy the rendered image; the flag says whether to confirm with a toast.
    Copy(bool),
    /// CAP-4: "also save a PNG to a folder" after an annotation save (the step checks the
    /// setting itself).
    Folder,
    Save(PathBuf),
}

enum StepOut {
    Job(Box<Result<JobResult, String>>),
    Copy(bool, Result<(), String>),
    Folder(Result<crate::save_folder::FolderResult, String>),
    Save(Result<PathBuf, String>),
}

/// What a key command needs from the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Handled,
    Leave,
}

pub struct EditorDoc {
    pub capture_id: String,
    app: Entity<AppModel>,
    backend: Arc<Backend>,
    pub phase: Phase,
    pub title: String,
    pub dir: PathBuf,
    pub warnings: Vec<String>,
    pub revision: u64,
    pub state: EditorState,
    base: Option<Arc<RgbaImage>>,
    base_view: Option<Arc<RenderImage>>,
    preview: Option<Arc<RenderImage>>,
    /// Ops the visible preview was rendered from (compared ignoring ids).
    displayed: Vec<AnnotationOp>,
    requested: Vec<AnnotationOp>,
    wanted: Option<Vec<AnnotationOp>>,
    rendering: bool,
    queue: VecDeque<Step>,
    running: bool,
    remap: HashMap<String, String>,
    history_pending: usize,
    pub status: Status,
    pub redact: RedactPhase,
    /// Whether the Auto-redact kind chooser is open (the choice is `redact::session_kinds`).
    pub kinds_open: bool,
    /// Edits were made since the editor opened.
    edited: bool,
    closing: bool,
}

fn same_kinds(a: &[AnnotationOp], b: &[AnnotationOp]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.kind == y.kind)
}

fn bgra_image(img: RgbaImage) -> Arc<RenderImage> {
    Arc::new(RenderImage::new([Frame::new(img)]))
}

impl EditorDoc {
    pub fn open(app: Entity<AppModel>, capture_id: String, cx: &mut Context<Self>) -> Self {
        let backend = app.read(cx).backend.clone();
        let styles = backend
            .settings
            .snapshot()
            .get(STYLES_KEY)
            .map(Styles::from_json)
            .unwrap_or_default();
        let mut doc = EditorDoc {
            capture_id,
            app,
            backend,
            phase: Phase::Loading,
            title: String::new(),
            dir: PathBuf::new(),
            warnings: Vec::new(),
            revision: 0,
            state: EditorState::new((1, 1), Vec::new(), styles),
            base: None,
            base_view: None,
            preview: None,
            displayed: Vec::new(),
            requested: Vec::new(),
            wanted: None,
            rendering: false,
            queue: VecDeque::new(),
            running: false,
            remap: HashMap::new(),
            history_pending: 0,
            status: Status::Saved,
            redact: RedactPhase::Idle,
            kinds_open: false,
            edited: false,
            closing: false,
        };
        doc.start_load(cx);
        doc
    }

    pub fn is_ready(&self) -> bool {
        self.phase == Phase::Ready && !self.closing
    }

    /// Edits are refused while an undo or redo is in flight: the local list is stale until
    /// the daemon answers.
    pub fn locked(&self) -> bool {
        !self.is_ready() || self.history_pending > 0
    }

    pub fn pending(&self) -> usize {
        self.queue.len() + usize::from(self.running)
    }

    /// The image to show: the latest preview, else the untouched original.
    pub fn display_image(&self) -> Option<Arc<RenderImage>> {
        self.preview.clone().or_else(|| self.base_view.clone())
    }

    pub fn image_size(&self) -> (u32, u32) {
        self.state.image
    }

    /// Redaction rects whose real render has not reached the screen yet. The view paints an
    /// opaque mask over them so the pixels underneath are never shown unredacted (ANN-2).
    pub fn pending_redactions(&self) -> Vec<Rc> {
        self.state
            .ops
            .iter()
            .filter(|o| o.kind.is_redaction())
            .filter(|o| !self.displayed.iter().any(|d| d.kind == o.kind))
            .filter_map(|o| match &o.kind {
                OpKind::Blur { rect, .. } | OpKind::Pixelate { rect, .. } => {
                    Some(rect.normalized())
                }
                _ => None,
            })
            .collect()
    }

    // ---- loading ------------------------------------------------------------------------

    fn start_load(&mut self, cx: &mut Context<Self>) {
        let Some(daemon) = self.backend.daemon.clone() else {
            self.phase = Phase::Failed(ErrorNote::app(AppNote::LibraryUnavailableDot));
            return;
        };
        let rt = self.backend.rt.clone();
        let id = self.capture_id.clone();
        cx.spawn(async move |this, cx| {
            let loaded = rt
                .spawn(async move {
                    let snap = session::fetch_snapshot(&daemon, &id).await?;
                    let img = session::load_original(&snap.dir).await?;
                    Ok::<_, ErrorNote>((snap, img))
                })
                .await
                .map_err(|e| ErrorNote::plain(e.to_string()))
                .and_then(|r| r);
            match loaded {
                Ok((snap, img)) => {
                    let img = Arc::new(img);
                    let src = img.clone();
                    let bgra = cx
                        .background_executor()
                        .spawn(async move { preview::into_bgra((*src).clone()) })
                        .await;
                    this.update(cx, |d, cx| d.loaded(snap, img, bgra, cx)).ok();
                }
                Err(e) => {
                    cx.update(|cx| crate::ui::app::note_open_failed(e.message(), cx));
                    this.update(cx, |d, cx| {
                        d.phase = Phase::Failed(e);
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn loaded(
        &mut self,
        snap: session::Snapshot,
        img: Arc<RgbaImage>,
        bgra: RgbaImage,
        cx: &mut Context<Self>,
    ) {
        if self.closing {
            return;
        }
        let styles = self.state.styles.clone();
        self.state = EditorState::new((img.width(), img.height()), snap.ops.clone(), styles);
        self.state.can_undo = snap.can_undo;
        self.state.can_redo = snap.can_redo;
        self.title = snap.title;
        self.dir = snap.dir;
        self.warnings = snap.warnings;
        self.revision = snap.revision;
        self.base = Some(img);
        self.base_view = Some(bgra_image(bgra));
        self.phase = Phase::Ready;
        self.sync_preview(cx);
        cx.notify();
    }

    /// Another client (CLI, MCP) changed the capture: adopt its state when we have nothing
    /// in flight.
    pub fn refresh_if_changed(&mut self, cx: &mut Context<Self>) {
        if !self.is_ready() || self.pending() > 0 || self.state.busy() {
            return;
        }
        let Some(daemon) = self.backend.daemon.clone() else {
            return;
        };
        let rt = self.backend.rt.clone();
        let id = self.capture_id.clone();
        cx.spawn(async move |this, cx| {
            let snap = rt
                .spawn(async move { session::fetch_snapshot(&daemon, &id).await })
                .await
                .map_err(|e| ErrorNote::plain(e.to_string()))
                .and_then(|r| r);
            if let Ok(snap) = snap {
                this.update(cx, |d, cx| {
                    if d.pending() == 0 && !d.state.busy() && snap.revision != d.revision {
                        d.adopt(snap);
                        d.sync_preview(cx);
                        cx.notify();
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    fn adopt(&mut self, snap: session::Snapshot) {
        self.revision = snap.revision;
        self.state.adopt_ops(snap.ops);
        self.state.can_undo = snap.can_undo;
        self.state.can_redo = snap.can_redo;
        self.warnings = snap.warnings;
        self.title = snap.title;
    }

    // ---- preview ------------------------------------------------------------------------

    /// Re-renders the preview when the op list changed.
    pub fn sync_preview(&mut self, cx: &mut Context<Self>) {
        if self.closing || self.phase != Phase::Ready {
            return;
        }
        let ops = self.state.render_ops();
        if same_kinds(&ops, &self.requested) {
            return;
        }
        self.requested = ops.clone();
        self.wanted = Some(ops);
        self.pump_render(cx);
    }

    fn pump_render(&mut self, cx: &mut Context<Self>) {
        if self.rendering {
            return;
        }
        let (Some(ops), Some(base)) = (self.wanted.take(), self.base.clone()) else {
            return;
        };
        self.rendering = true;
        let inflight = ops.clone();
        cx.spawn(async move |this, cx| {
            let out = cx
                .background_executor()
                .spawn(async move { preview::render_bgra(&base, &ops) })
                .await;
            this.update(cx, |d, cx| {
                d.rendering = false;
                if d.closing {
                    return;
                }
                match out {
                    Ok(img) => {
                        let new = if inflight.is_empty() {
                            None
                        } else {
                            Some(bgra_image(img))
                        };
                        if let Some(old) = std::mem::replace(&mut d.preview, new) {
                            cx.drop_image(old, None);
                        }
                        d.displayed = inflight;
                    }
                    Err(e) => d.status = Status::Failed(e),
                }
                cx.notify();
                d.pump_render(cx);
            })
            .ok();
        })
        .detach();
    }

    // ---- daemon queue -------------------------------------------------------------------

    fn submit(&mut self, edits: Vec<Edit>, cx: &mut Context<Self>) {
        let any = !edits.is_empty();
        for e in edits {
            self.edited = true;
            self.queue.push_back(Step::Job(Job::Edit(e)));
        }
        if any {
            self.queue_auto_copy(cx);
        }
        self.pump(cx);
    }

    /// CAP-4: with "copy after capture" on, the rendered image is copied after each annotation
    /// save. Saves queued back to back share one copy, so a burst of edits copies once.
    fn queue_auto_copy(&mut self, cx: &mut Context<Self>) {
        if self.app.read(cx).copy_after_capture {
            self.queue.retain(|s| !matches!(s, Step::Copy(false)));
            self.queue.push_back(Step::Copy(false));
        }
        // The folder step is a no-op while the setting is off; like the copy it runs once for a
        // burst of edits.
        self.queue.retain(|s| !matches!(s, Step::Folder));
        self.queue.push_back(Step::Folder);
    }

    fn pump(&mut self, cx: &mut Context<Self>) {
        if self.running {
            return;
        }
        let Some(step) = self.queue.pop_front() else {
            self.drained(cx);
            return;
        };
        let Some(daemon) = self.backend.daemon.clone() else {
            return;
        };
        self.running = true;
        if matches!(step, Step::Job(_)) {
            self.status = Status::Saving;
        }
        cx.notify();
        let backend = self.backend.clone();
        let id = self.capture_id.clone();
        let remap = self.remap.clone();
        let keep = cx.entity();
        let rt = backend.rt.clone();
        cx.spawn(async move |this, cx| {
            let _keep = keep;
            let is_history = matches!(step, Step::Job(Job::Undo | Job::Redo));
            let out = rt
                .spawn(async move {
                    match step {
                        Step::Job(job) => StepOut::Job(Box::new(
                            session::run_job(&daemon, &id, &job, remap).await,
                        )),
                        Step::Copy(confirm) => {
                            StepOut::Copy(confirm, backend_copy(&backend, &id).await)
                        }
                        Step::Folder => StepOut::Folder(backend.save_capture_to_folder(&id).await),
                        Step::Save(path) => StepOut::Save(save_png(&daemon, &id, path).await),
                    }
                })
                .await;
            let out = out.unwrap_or_else(|e| StepOut::Job(Box::new(Err(e.to_string()))));
            this.update(cx, |d, cx| d.step_done(out, is_history, cx))
                .ok();
        })
        .detach();
    }

    fn step_done(&mut self, out: StepOut, is_history: bool, cx: &mut Context<Self>) {
        self.running = false;
        if is_history {
            self.history_pending = self.history_pending.saturating_sub(1);
        }
        match out {
            StepOut::Job(res) => match *res {
                Ok(r) => {
                    self.remap = r.remap;
                    self.state.apply_id_map(&self.remap);
                    self.revision = r.snapshot.revision;
                    self.state.can_undo = r.snapshot.can_undo;
                    self.state.can_redo = r.snapshot.can_redo;
                    self.warnings = r.snapshot.warnings.clone();
                    let more_edits = self.queue.iter().any(|s| matches!(s, Step::Job(_)));
                    if !more_edits && !self.state.busy() {
                        self.state.adopt_ops(r.snapshot.ops);
                    }
                    self.sync_preview(cx);
                }
                Err(e) => {
                    // Later queued edits were computed against a state that no longer
                    // holds: drop them and take the daemon's version.
                    self.queue.clear();
                    self.history_pending = 0;
                    self.status = Status::Failed(e.clone());
                    app::push_toast(
                        &self.app,
                        flows::error_toast_live(|| tr!("Couldn\u{2019}t save the change"), e),
                        cx,
                    );
                    self.resync(cx);
                }
            },
            StepOut::Copy(confirm, res) => {
                if confirm {
                    match res {
                        Ok(()) => app::push_toast(
                            &self.app,
                            ToastSpec::new(ToastKind::Success, tr!("Copied to clipboard")),
                            cx,
                        ),
                        Err(e) => app::push_toast(
                            &self.app,
                            flows::error_toast_live(|| tr!("Couldn\u{2019}t copy"), e),
                            cx,
                        ),
                    }
                } else if let Err(e) = res {
                    log::warn!("clipboard copy after editing failed: {e}");
                }
            }
            StepOut::Folder(res) => match res {
                Ok(r) => {
                    if let Some(toast) = flows::folder_warning_toast(&r) {
                        app::push_toast(&self.app, toast, cx);
                    }
                }
                Err(e) => log::warn!("folder save after editing failed: {e}"),
            },
            StepOut::Save(res) => match res {
                Ok(path) => app::push_toast(
                    &self.app,
                    ToastSpec::new(ToastKind::Success, tr!("Saved the image"))
                        .body(path.display().to_string()),
                    cx,
                ),
                Err(e) => app::push_toast(
                    &self.app,
                    flows::error_toast_live(|| tr!("Couldn\u{2019}t save the image"), e),
                    cx,
                ),
            },
        }
        cx.notify();
        self.pump(cx);
    }

    fn drained(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.status, Status::Failed(_)) {
            self.status = Status::Saved;
        }
        cx.notify();
    }

    fn resync(&mut self, cx: &mut Context<Self>) {
        let Some(daemon) = self.backend.daemon.clone() else {
            return;
        };
        let rt = self.backend.rt.clone();
        let id = self.capture_id.clone();
        cx.spawn(async move |this, cx| {
            let snap = rt
                .spawn(async move { session::fetch_snapshot(&daemon, &id).await })
                .await
                .map_err(|e| ErrorNote::plain(e.to_string()))
                .and_then(|r| r);
            if let Ok(snap) = snap {
                this.update(cx, |d, cx| {
                    if d.pending() == 0 {
                        d.state.cancel_interaction();
                        d.state.text = None;
                        d.adopt(snap);
                        d.sync_preview(cx);
                        cx.notify();
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    // ---- input --------------------------------------------------------------------------

    fn after_input(&mut self, edits: Vec<Edit>, cx: &mut Context<Self>) {
        self.persist_styles_if_changed(cx);
        self.submit(edits, cx);
        self.sync_preview(cx);
        cx.notify();
    }

    fn persist_styles_if_changed(&mut self, cx: &mut Context<Self>) {
        if self.state.take_styles_dirty() {
            app::set_setting(&self.app, STYLES_KEY, self.state.styles.to_json(), cx);
        }
    }

    pub fn pointer_down(
        &mut self,
        p: Pt,
        mods: Mods,
        clicks: usize,
        tol: f64,
        cx: &mut Context<Self>,
    ) {
        if self.locked() {
            return;
        }
        // A click inside a dashed Auto-redact rectangle accepts that one (ANN-2.AC4).
        if self.state.text.is_none() {
            if let Some(i) = self.state.proposal_at(p) {
                self.accept_proposal(Some(i), cx);
                return;
            }
        }
        let edits = self.state.pointer_down(p, mods, clicks, tol);
        self.after_input(edits, cx);
    }

    pub fn pointer_move(&mut self, p: Pt, mods: Mods, tol: f64, cx: &mut Context<Self>) {
        if self.phase != Phase::Ready {
            return;
        }
        self.state.pointer_move(p, mods, tol);
        self.sync_preview(cx);
        cx.notify();
    }

    pub fn pointer_up(&mut self, p: Pt, mods: Mods, tol: f64, cx: &mut Context<Self>) {
        if self.phase != Phase::Ready {
            return;
        }
        let edits = self.state.pointer_up(p, mods, tol);
        self.after_input(edits, cx);
    }

    pub fn pointer_leave(&mut self, cx: &mut Context<Self>) {
        self.state.hover = None;
        cx.notify();
    }

    pub fn set_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let edits = self.state.set_tool(tool);
        self.after_input(edits, cx);
    }

    pub fn set_param(&mut self, v: ParamValue, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let edit = self.state.set_param(v);
        self.after_input(edit.into_iter().collect(), cx);
    }

    pub fn clear_crop(&mut self, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let edit = self.state.clear_crop();
        self.after_input(edit.into_iter().collect(), cx);
    }

    pub fn delete_selected(&mut self, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let edit = self.state.delete_selected();
        self.after_input(edit.into_iter().collect(), cx);
    }

    pub fn duplicate(&mut self, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let edit = self.state.duplicate();
        self.after_input(edit.into_iter().collect(), cx);
    }

    /// ANN-3: "Revert to original" is one undoable step.
    pub fn revert(&mut self, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let edit = self.state.revert();
        self.after_input(edit.into_iter().collect(), cx);
    }

    // ---- Auto-redact (ANN-2.AC4) ----------------------------------------------------------

    /// Runs the text recognizer in the daemon (off the UI thread) and shows what it proposes as
    /// dashed rectangles. Nothing is applied.
    pub fn auto_redact(&mut self, cx: &mut Context<Self>) {
        if self.locked() || self.redact == RedactPhase::Running {
            return;
        }
        let Some(daemon) = self.backend.daemon.clone() else {
            return;
        };
        self.redact = RedactPhase::Running;
        self.state.dismiss_proposals();
        cx.notify();
        let rt = self.backend.rt.clone();
        let id = self.capture_id.clone();
        let kinds = crate::editor::redact::session_kinds();
        cx.spawn(async move |this, cx| {
            let found = rt
                .spawn(async move { session::fetch_proposals(&daemon, &id, &kinds).await })
                .await
                .map_err(|e| ErrorNote::plain(e.to_string()))
                .and_then(|r| r);
            this.update(cx, |d, cx| {
                if d.closing {
                    return;
                }
                match found {
                    Ok(p) if p.is_empty() => d.redact = RedactPhase::NothingFound,
                    Ok(p) => {
                        d.redact = RedactPhase::Idle;
                        d.state.set_proposals(p);
                    }
                    Err(e) => d.redact = RedactPhase::Failed(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Opens or closes the kind chooser of the Auto-redact card.
    pub fn toggle_kinds_open(&mut self, cx: &mut Context<Self>) {
        self.kinds_open = !self.kinds_open;
        cx.notify();
    }

    /// Edits the remembered choice of Auto-redact kinds.
    pub fn edit_redact_kinds(
        &mut self,
        f: impl FnOnce(&mut crate::editor::redact::KindChoice),
        cx: &mut Context<Self>,
    ) {
        let mut c = crate::editor::redact::session_kinds();
        f(&mut c);
        crate::editor::redact::set_session_kinds(c);
        cx.notify();
    }

    /// Accepts one proposal (`Some(i)`) or all (`None`): blur ops in one undo step.
    pub fn accept_proposal(&mut self, which: Option<usize>, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let edit = self.state.accept_proposals(which);
        self.after_input(edit.into_iter().collect(), cx);
    }

    /// Dismisses the proposals (and a "nothing found" or error note) without applying any.
    pub fn dismiss_redactions(&mut self, cx: &mut Context<Self>) {
        self.state.dismiss_proposals();
        if self.redact != RedactPhase::Running {
            self.redact = RedactPhase::Idle;
        }
        cx.notify();
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) {
        self.history(Job::Undo, cx);
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) {
        self.history(Job::Redo, cx);
    }

    fn history(&mut self, job: Job, cx: &mut Context<Self>) {
        if self.locked() {
            return;
        }
        let can = match job {
            Job::Undo => self.state.can_undo || self.pending() > 0,
            _ => self.state.can_redo,
        };
        if !can {
            return;
        }
        // Finish what is being typed or dragged first, so it is its own undo step.
        let mut edits: Vec<Edit> = self.state.finish_text().into_iter().collect();
        self.state.cancel_interaction();
        for e in edits.drain(..) {
            self.edited = true;
            self.queue.push_back(Step::Job(Job::Edit(e)));
        }
        self.history_pending += 1;
        self.edited = true;
        self.queue.push_back(Step::Job(job));
        self.queue_auto_copy(cx);
        self.pump(cx);
        self.sync_preview(cx);
        cx.notify();
    }

    /// Key commands the document handles. View-level ones (zoom, back) never get here.
    pub fn command(&mut self, cmd: KeyCommand, center: Pt, cx: &mut Context<Self>) -> Outcome {
        match cmd {
            KeyCommand::SetTool(t) => self.set_tool(t, cx),
            KeyCommand::Delete => self.delete_selected(cx),
            KeyCommand::Undo => self.undo(cx),
            KeyCommand::Redo => self.redo(cx),
            KeyCommand::Copy => {
                if !self.state.copy_selection() {
                    self.copy_image(cx);
                }
            }
            KeyCommand::CopyImage => self.copy_image(cx),
            KeyCommand::Paste => {
                if !self.locked() {
                    let e = self.state.paste();
                    self.after_input(e.into_iter().collect(), cx);
                }
            }
            KeyCommand::Duplicate => self.duplicate(cx),
            KeyCommand::SelectNext | KeyCommand::SelectPrev => {
                self.state.select_step(cmd == KeyCommand::SelectNext);
                cx.notify();
            }
            KeyCommand::Nudge {
                dx,
                dy,
                resize,
                big,
            } => {
                if !self.locked() {
                    let e = self.state.nudge(dx, dy, resize, big);
                    self.after_input(e.into_iter().collect(), cx);
                }
            }
            KeyCommand::PlaceDefault => {
                if !self.locked() {
                    let e = self.state.place_default(center);
                    self.after_input(e, cx);
                }
            }
            KeyCommand::Escape => {
                if self.locked() {
                    return Outcome::Handled;
                }
                if !self.state.proposals.is_empty() && self.state.text.is_none() {
                    self.dismiss_redactions(cx);
                    return Outcome::Handled;
                }
                match self.state.escape() {
                    EscapeOutcome::Edits(e) => self.after_input(e, cx),
                    EscapeOutcome::Leave => return Outcome::Leave,
                }
            }
            KeyCommand::ZoomIn
            | KeyCommand::ZoomOut
            | KeyCommand::ZoomFit
            | KeyCommand::ZoomActual
            | KeyCommand::Back
            | KeyCommand::SaveAs => {}
        }
        Outcome::Handled
    }

    /// Typed text while the text tool is open. Returns whether the key was consumed.
    pub fn text_key(
        &mut self,
        key: &str,
        key_char: Option<&str>,
        mods: Mods,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.state.text.is_none() {
            return false;
        }
        let used = self.state.text_key(key, key_char, mods);
        if used {
            self.sync_preview(cx);
            cx.notify();
        }
        used
    }

    pub fn text_paste(&mut self, s: &str, cx: &mut Context<Self>) {
        self.state.text_insert(s);
        self.sync_preview(cx);
        cx.notify();
    }

    // ---- output -------------------------------------------------------------------------

    /// CAP-4: copies the rendered PNG once pending saves have landed.
    pub fn copy_image(&mut self, cx: &mut Context<Self>) {
        if !self.is_ready() {
            return;
        }
        self.finish_pending_input(cx);
        self.queue.push_back(Step::Copy(true));
        self.pump(cx);
    }

    /// Writes the rendered PNG to `path` once pending saves have landed.
    pub fn save_to(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.is_ready() {
            return;
        }
        self.finish_pending_input(cx);
        self.queue.push_back(Step::Save(path));
        self.pump(cx);
    }

    fn finish_pending_input(&mut self, cx: &mut Context<Self>) {
        let edits: Vec<Edit> = self.state.finish_text().into_iter().collect();
        self.after_input(edits, cx);
    }

    /// The editor was left. Queued saves still finish; the images are released now.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        if self.phase == Phase::Ready {
            self.finish_pending_input(cx);
        }
        self.closing = true;
        self.pump(cx);
        for img in [self.preview.take(), self.base_view.take()]
            .into_iter()
            .flatten()
        {
            cx.drop_image(img, None);
        }
        self.base = None;
    }
}

async fn backend_copy(backend: &Backend, id: &str) -> Result<(), String> {
    backend.copy_capture(id).await
}

async fn save_png(daemon: &cm_daemon::Daemon, id: &str, path: PathBuf) -> Result<PathBuf, String> {
    let (_, png) = Backend::capture_png(daemon, id).await?;
    let p = path.clone();
    tokio::task::spawn_blocking(move || session::write_png(&p, &png))
        .await
        .map_err(|e| e.to_string())??;
    Ok(path)
}

/// Where the text caret and the text frame go, in image px.
#[derive(Debug, Clone, PartialEq)]
pub struct TextGeom {
    pub frame: Rc,
    pub caret: Pt,
    pub caret_h: f64,
}

/// Everything the canvas overlay paints on top of the preview, in image px.
pub struct OverlayData {
    pub chrome: Option<Chrome>,
    pub draft: Option<OpKind>,
    pub masks: Vec<Rc>,
    /// Auto-redact proposals, drawn dashed (not applied).
    pub proposals: Vec<Rc>,
    pub crop: Option<Rc>,
    pub text: Option<TextGeom>,
}

impl EditorDoc {
    /// Gathers the overlay shapes for this frame. Takes `&mut` because text extents are
    /// measured (and cached) by the renderer.
    pub fn overlay_data(&mut self) -> OverlayData {
        let draft = self.state.draft();
        let crop_draft = match &draft {
            Some(OpKind::Crop { rect }) => Some(rect.normalized()),
            _ => None,
        };
        let crop = if self.state.tool == Tool::Crop {
            crop_draft.or_else(|| self.state.crop_rect())
        } else {
            None
        };
        let text = self.state.text.clone().map(|t| {
            let m = &mut self.state.metrics;
            let lh = m.line_height(t.size);
            let byte = t
                .text
                .char_indices()
                .nth(t.cursor)
                .map_or(t.text.len(), |(b, _)| b);
            let before = &t.text[..byte];
            let line_idx = before.matches('\n').count();
            let line = before.rsplit('\n').next().unwrap_or("");
            let cx0 = if line.is_empty() {
                0.0
            } else {
                let b = m.measure(line, t.size);
                b.dx + b.w
            };
            let lines = t.text.split('\n').count().max(1);
            let b = m.measure(&t.text, t.size);
            let w = if t.text.is_empty() {
                t.size * 0.6
            } else {
                b.dx + b.w
            };
            TextGeom {
                frame: Rc::new(t.at.x, t.at.y, w.max(t.size * 0.6), lh * lines as f64),
                caret: Pt::new(t.at.x + cx0, t.at.y + lh * line_idx as f64),
                caret_h: lh,
            }
        });
        OverlayData {
            chrome: self.state.chrome(),
            draft: draft.filter(|d| !matches!(d, OpKind::Crop { .. })),
            masks: self.pending_redactions(),
            proposals: self.state.proposals.iter().map(|p| p.rect).collect(),
            crop,
            text,
        }
    }
}
