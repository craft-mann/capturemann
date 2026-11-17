//! The 0.3.4 editor flows that sit beside the drawing tools: the Zoom Lens limit (ANN-8),
//! Retype (ANN-9), Move area (ANN-10), the Blueprint panel (ANN-11) and the pointer side of
//! text selection on the canvas (ANN-1.AC9 to AC11).
//!
//! Retype, Move area and Blueprint are **base edits**: they are inserted before the first
//! non-base op and the whole list goes to the daemon (`Edit::Replace`), because `annotate`
//! refuses a base edit after another op. Fills are measured once, from the picture with the
//! earlier base edits applied; the measuring itself needs the pixels, so the state only
//! *asks* for it (`take_measure_request`) and the document answers (`touch_measured`) from a
//! background task.

use super::*;
use crate::editor::touchup::{
    self, BlueprintPanel, BusyPrompt, Line, MeasureFor, MeasureRequest, Measured, MovePending,
    SnapGuide,
};
use cm_core::annotation::{
    BlueprintColor, BlueprintStyle, Fill, FillKind, MagnifyShape, TextAlign,
};

/// A drag of the contents of a chosen Move area source.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveDrag {
    /// The pointer at the press.
    pub start: Pt,
    /// Where the contents were at the press.
    pub dest0: Pt,
    pub moved: bool,
}

/// Everything the touch-up tools and the Blueprint panel keep between events.
#[derive(Debug, Clone, PartialEq)]
pub struct Touch {
    /// The recognized text lines (Retype's dashed boxes), once asked for.
    pub lines: Option<Vec<Line>>,
    /// The base edits `lines` were recognized on; a change of them makes the lines stale.
    lines_for: Vec<OpKind>,
    pub lines_loading: bool,
    pub lines_error: Option<String>,
    request: Option<MeasureRequest>,
    /// A measurement is running off the UI thread.
    pub measuring: bool,
    /// "This area has an image or a control in it, so the fill may show" is on screen.
    pub busy_prompt: Option<BusyPrompt>,
    /// A press landed while the busy sheet was open: its release must not reach the canvas.
    pub sheet_press: bool,
    /// The chosen Move area source.
    pub mv: Option<MovePending>,
    pub mv_drag: Option<MoveDrag>,
    /// "Snap to the source row and column" (on by default).
    pub snap_on: bool,
    /// The pink guide of a snapped Move area drag (canvas only).
    pub guide: Option<SnapGuide>,
    /// The Blueprint panel, when open.
    pub bp: Option<BlueprintPanel>,
    pub bp_loading: bool,
    /// A short refusal for the inspector ("4 lenses on this image", "text recognition ...").
    pub notice: Option<String>,
}

impl Touch {
    pub fn new() -> Self {
        Touch {
            lines: None,
            lines_for: Vec::new(),
            lines_loading: false,
            lines_error: None,
            request: None,
            measuring: false,
            busy_prompt: None,
            sheet_press: false,
            mv: None,
            mv_drag: None,
            snap_on: true,
            guide: None,
            bp: None,
            bp_loading: false,
            notice: None,
        }
    }

    /// Switching tools drops what belongs to the old one (nothing is stored).
    pub fn leave_tool(&mut self) {
        self.mv = None;
        self.mv_drag = None;
        self.busy_prompt = None;
        self.guide = None;
        self.notice = None;
        self.request = None;
        self.measuring = false;
        self.bp = None;
    }

    pub fn cancel_drag(&mut self) {
        self.mv_drag = None;
        self.guide = None;
        if let Some(m) = self.mv.as_mut() {
            m.dest = None;
        }
    }

    /// Esc: closes the busy sheet, the Blueprint panel or the chosen source, one at a time.
    /// Returns whether it did something.
    pub fn escape(&mut self) -> bool {
        if self.busy_prompt.take().is_some() {
            return true;
        }
        if self.bp.take().is_some() {
            return true;
        }
        if self.mv.take().is_some() {
            self.guide = None;
            return true;
        }
        false
    }

    /// Something of the touch-up flows is in progress (the document must not replace the ops).
    pub fn busy(&self) -> bool {
        self.mv_drag.is_some()
            || self.mv.is_some()
            || self.busy_prompt.is_some()
            || self.bp.is_some()
            || self.measuring
            || self.bp_loading
    }

    /// The daemon's op list was adopted: a Blueprint panel of a vanished op closes, and lines
    /// recognized on other base edits are stale.
    pub fn ops_adopted(&mut self, ops: &[AnnotationOp]) {
        if let Some(id) = self.bp.as_ref().and_then(|b| b.existing.clone()) {
            if !ops.iter().any(|o| o.id == id) {
                self.bp = None;
            }
        }
        let base: Vec<OpKind> = touchup::base_ops(ops).into_iter().map(|o| o.kind).collect();
        if base != self.lines_for {
            self.lines = None;
        }
    }
}

impl Default for Touch {
    fn default() -> Self {
        Touch::new()
    }
}

/// What the Zoom Lens panel shows (see [`EditorState::lens_info`]).
#[derive(Debug, Clone, PartialEq)]
pub struct LensInfo {
    pub shape: MagnifyShape,
    pub connector: bool,
    pub badge: bool,
    /// The source of the selected lens, physical px.
    pub source: Option<Rc>,
    /// Lenses on the image, and the limit.
    pub count: usize,
    pub max: usize,
    pub has_op: bool,
    pub notice: Option<String>,
}

/// What the Retype panel shows (see [`EditorState::retype_info`]).
#[derive(Debug, Clone, PartialEq)]
pub struct RetypeInfo {
    pub text: String,
    pub size: f64,
    pub color: Color,
    pub weight: u16,
    pub align: TextAlign,
    pub fill_kind: FillKind,
    pub fill_color: Color,
    /// The text is being typed on the canvas.
    pub editing: bool,
    pub has_op: bool,
    /// The recognizer is not available or failed: the box can still be dragged by hand.
    pub note: Option<String>,
}

/// What a click on the Retype panel's Text field does (QA E4). The field is a plain box, not an
/// input: typing always goes through the canvas's input handler, which needs the canvas focused,
/// and a press on any non-input element moves the focus to the editor root. So every click on the
/// field must hand the focus back to the canvas while a text is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldClick {
    /// A stored retype is selected: open it for typing (and give the canvas the focus).
    Edit,
    /// The text is open already: only give the canvas the focus back.
    Refocus,
    /// Nothing to type into yet.
    Nothing,
}

impl RetypeInfo {
    pub fn field_click(&self) -> FieldClick {
        if self.editing {
            FieldClick::Refocus
        } else if self.has_op {
            FieldClick::Edit
        } else {
            FieldClick::Nothing
        }
    }
}

/// What the Move area panel shows (see [`EditorState::move_info`]).
#[derive(Debug, Clone, PartialEq)]
pub struct MoveInfo {
    pub source: Option<Rc>,
    pub to: Option<Pt>,
    pub fill_kind: FillKind,
    pub fill_color: Color,
    pub snap: bool,
    /// The source is chosen; the contents are not moved yet.
    pub pending: bool,
    /// The op exists and removes the contents.
    pub removes: bool,
}

/// What the Blueprint panel shows (see [`EditorState::blueprint_info`]).
#[derive(Debug, Clone, PartialEq)]
pub struct BlueprintInfo {
    pub found: usize,
    pub bars: usize,
    pub kept: usize,
    pub style: BlueprintStyle,
    pub color: BlueprintColor,
    pub radius: f64,
    /// The kept lines: index in the panel and the recognized text.
    pub kept_rows: Vec<(usize, String)>,
    /// A stored op is being edited (each change is one undo step).
    pub existing: bool,
    /// The last keep-list change of a draft can be taken back.
    pub can_undo: bool,
    /// The line under the pointer: its text and whether it is a bar now.
    pub hover: Option<(String, bool)>,
}

/// One dashed box of the Retype overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct LineBox {
    pub rect: Rc,
    pub hovered: bool,
}

/// What the Move area overlay draws.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveOverlay {
    pub source: Rc,
    /// `filled · #FFFFFF`.
    pub tag: String,
    pub dest: Option<Rc>,
}

/// What the Blueprint overlay draws: every line, whether it is kept, and the one hovered.
#[derive(Debug, Clone, PartialEq)]
pub struct BlueprintBox {
    pub rect: Rc,
    pub kept: bool,
    pub hovered: bool,
}

fn inside(r: Rc, p: Pt, pad: f64) -> bool {
    let r = r.normalized();
    p.x >= r.x - pad && p.x <= r.x + r.w + pad && p.y >= r.y - pad && p.y <= r.y + r.h + pad
}

fn fill_summary(f: &Fill) -> (FillKind, Color) {
    (f.kind, f.colors.first().copied().unwrap_or(Color::WHITE))
}

impl EditorState {
    // ---- Zoom Lens (ANN-8) -----------------------------------------------------------------

    /// How many Zoom Lenses the image has.
    pub fn lens_count(&self) -> usize {
        self.ops
            .iter()
            .filter(|o| matches!(o.kind, OpKind::Magnify { .. }))
            .count()
    }

    /// Adds a lens at the end of the list (it sees what is drawn before it). Refused with a
    /// notice at four lenses, or when the schema would refuse it (ANN-8.AC5).
    pub(super) fn add_lens(&mut self, kind: OpKind) -> Option<Edit> {
        if self.lens_count() >= cm_core::annotation::MAX_MAGNIFY {
            self.touch.notice = Some(tr!(
                "This image already has 4 zoom lenses, the most it can have. Delete one to add another."
            ));
            return None;
        }
        if !geom::lens_ok(&kind) {
            self.touch.notice = Some(tr!(
                "That lens would be too large (16,384 px a side, 32 megapixels). Draw a smaller source."
            ));
            return None;
        }
        self.touch.notice = None;
        let id = self.tmp_id();
        let op = AnnotationOp::new(id.clone(), kind);
        self.ops.push(op.clone());
        self.selected = Some(id);
        self.lens_part = 0;
        Some(Edit::Append(vec![op]))
    }

    /// The Zoom Lens panel: shown for the Zoom Lens tool, and for a selected lens.
    pub fn lens_info(&self) -> Option<LensInfo> {
        if self.inspector_tool() != Tool::Magnify || self.text.is_some() {
            return None;
        }
        let st = self.styles.get(Tool::Magnify);
        let (shape, connector, badge, source, has_op) = match self.selected_kind() {
            Some(OpKind::Magnify {
                shape,
                connector,
                badge,
                source,
                ..
            }) => (*shape, *connector, *badge, Some(source.normalized()), true),
            _ => (st.lens_shape, st.connector, st.badge, None, false),
        };
        Some(LensInfo {
            shape,
            connector,
            badge,
            source,
            count: self.lens_count(),
            max: cm_core::annotation::MAX_MAGNIFY,
            has_op,
            notice: self.touch.notice.clone(),
        })
    }

    /// Circle, Rounded or Rect: the selected lens (one undo step) and new ones.
    pub fn set_lens_shape(&mut self, shape: MagnifyShape) -> Option<Edit> {
        self.styles.set_lens(Some(shape), None, None);
        self.styles_dirty = true;
        self.edit_selected_lens(|k| match k {
            OpKind::Magnify { shape: s, .. } if *s != shape => {
                *s = shape;
                true
            }
            _ => false,
        })
    }

    /// The connector lines from the source to the loupe.
    pub fn set_lens_connector(&mut self, on: bool) -> Option<Edit> {
        self.styles.set_lens(None, Some(on), None);
        self.styles_dirty = true;
        self.edit_selected_lens(|k| match k {
            OpKind::Magnify { connector, .. } if *connector != on => {
                *connector = on;
                true
            }
            _ => false,
        })
    }

    /// The `×3` badge on the loupe.
    pub fn set_lens_badge(&mut self, on: bool) -> Option<Edit> {
        self.styles.set_lens(None, None, Some(on));
        self.styles_dirty = true;
        self.edit_selected_lens(|k| match k {
            OpKind::Magnify { badge, .. } if *badge != on => {
                *badge = on;
                true
            }
            _ => false,
        })
    }

    fn edit_selected_lens(&mut self, f: impl FnOnce(&mut OpKind) -> bool) -> Option<Edit> {
        let i = self.selected_index()?;
        if !matches!(self.ops[i].kind, OpKind::Magnify { .. }) {
            return None;
        }
        if !f(&mut self.ops[i].kind) {
            return None;
        }
        Some(Edit::Replace(self.ops.clone()))
    }

    // ---- drafts for the preview ------------------------------------------------------------

    /// The ops that exist only while the user is dragging or has a panel open: the lens being
    /// dragged out, the moved area, the Blueprint draft. The preview renders them with the
    /// rest, so what is shown is what the renderer makes.
    pub(super) fn touch_draft_ops(&self, out: &mut Vec<AnnotationOp>) {
        if self.tool == Tool::Magnify && self.lens_count() < cm_core::annotation::MAX_MAGNIFY {
            if let Interaction::Drawing { start, cur, shift } = &self.interaction {
                if let Some(k) = geom::shape_from_drag(
                    Tool::Magnify,
                    &self.styles.get(Tool::Magnify),
                    *start,
                    *cur,
                    *shift,
                    self.image,
                ) {
                    if geom::lens_ok(&k) {
                        out.push(AnnotationOp::new("~lens", k));
                    }
                }
            }
        }
        if let Some(mv) = &self.touch.mv {
            if let Some(d) = mv.dest {
                out.push(AnnotationOp::new("~move", mv.op(Some(d))));
            }
        }
        if let Some(bp) = &self.touch.bp {
            if bp.existing.is_none() {
                if let Some(k) = bp.op_kind() {
                    out.push(AnnotationOp::new("~blueprint", k));
                }
            }
        }
    }

    /// Selection box of a chosen Move area (the moved contents get the blue outline).
    pub(super) fn touch_chrome(&mut self) -> Option<Chrome> {
        let mv = self.touch.mv.as_ref()?;
        let s = mv.source.normalized();
        let bounds = match mv.dest {
            Some(d) => Rc::new(d.x, d.y, s.w, s.h),
            None => s,
        };
        Some(Chrome {
            bounds,
            handles: Vec::new(),
            loupe: None,
        })
    }

    // ---- inserting base edits --------------------------------------------------------------

    /// Puts a base edit after the earlier base edits and before everything else, selects it
    /// and returns the edit that sends the whole list (ANN-9, base-edit rules 1).
    fn insert_base_edit(&mut self, kind: OpKind) -> Edit {
        let id = self.tmp_id();
        let at = touchup::insert_at(&self.ops);
        self.ops.insert(at, AnnotationOp::new(id.clone(), kind));
        self.selected = Some(id);
        Edit::Replace(self.ops.clone())
    }

    // ---- recognized lines (Retype) ---------------------------------------------------------

    /// The Retype tool is open and the recognized lines are missing or stale: the document
    /// asks the daemon (`suggest_blueprint`).
    pub fn lines_wanted(&self) -> bool {
        self.tool == Tool::Retype
            && !self.touch.lines_loading
            && (self.touch.lines.is_none() || self.touch.lines_for != self.base_kinds())
    }

    fn base_kinds(&self) -> Vec<OpKind> {
        touchup::base_ops(&self.ops)
            .into_iter()
            .map(|o| o.kind)
            .collect()
    }

    pub fn set_lines_loading(&mut self) {
        self.touch.lines_loading = true;
        self.touch.lines_error = None;
    }

    /// The daemon answered. On an error the tool still works by dragging a box.
    pub fn set_lines(&mut self, res: Result<Vec<Line>, String>) {
        self.touch.lines_loading = false;
        self.touch.lines_for = self.base_kinds();
        match res {
            Ok(l) => {
                self.touch.lines = Some(l);
                self.touch.lines_error = None;
            }
            Err(e) => {
                self.touch.lines = Some(Vec::new());
                self.touch.lines_error = Some(e);
            }
        }
    }

    /// The dashed boxes of the Retype overlay: every recognized line, the hovered one marked.
    pub fn retype_boxes(&self) -> Vec<LineBox> {
        if self.tool != Tool::Retype
            || self.text.is_some()
            || self.interaction != Interaction::Idle
            || self.touch.busy_prompt.is_some()
        {
            return Vec::new();
        }
        let Some(lines) = &self.touch.lines else {
            return Vec::new();
        };
        let hov = self.hover.and_then(|p| Self::line_at(lines, p));
        lines
            .iter()
            .enumerate()
            .map(|(i, l)| LineBox {
                rect: l.rect.normalized(),
                hovered: hov == Some(i),
            })
            .collect()
    }

    /// The smallest line under `p`.
    fn line_at(lines: &[Line], p: Pt) -> Option<usize> {
        lines
            .iter()
            .enumerate()
            .filter(|(_, l)| inside(l.rect, p, 0.0))
            .min_by(|a, b| (a.1.rect.w * a.1.rect.h).total_cmp(&(b.1.rect.w * b.1.rect.h)))
            .map(|(i, _)| i)
    }

    // ---- the pointer tools: a box was drawn or clicked -------------------------------------

    /// A Retype or Move area drag ended (or was a click).
    pub(super) fn touch_drawn(&mut self, start: Pt, cur: Pt) {
        let rect = crate::editor::copytext::selection_rect(start, cur, self.image);
        match self.tool {
            Tool::Retype => {
                let (rect, text) = match rect {
                    // A dragged box over text the recognizer missed: the text starts empty.
                    Some(r) => (r, String::new()),
                    None => {
                        // A click: the recognized line under it.
                        let Some(lines) = &self.touch.lines else {
                            return;
                        };
                        let Some(i) = Self::line_at(lines, start) else {
                            return;
                        };
                        (lines[i].rect.normalized(), lines[i].editable_text())
                    }
                };
                self.ask(MeasureFor::Retype, rect, text);
            }
            Tool::MoveArea => {
                if let Some(r) = rect {
                    self.ask(MeasureFor::MoveSource, r, String::new());
                } else {
                    self.touch.mv = None;
                }
            }
            _ => {}
        }
    }

    fn ask(&mut self, what: MeasureFor, rect: Rc, text: String) {
        self.touch.request = Some(MeasureRequest { what, rect, text });
        self.touch.measuring = true;
        self.touch.notice = None;
    }

    /// The measurement the editor wants run (once): the document does it off the UI thread and
    /// answers with [`EditorState::touch_measured`].
    pub fn take_measure_request(&mut self) -> Option<MeasureRequest> {
        self.touch.request.take()
    }

    /// The base edits a measurement is made on: everything before where a new base edit goes.
    pub fn measure_base(&self) -> Vec<AnnotationOp> {
        touchup::base_ops(&self.ops)
    }

    /// The measurement finished.
    pub fn touch_measured(&mut self, req: MeasureRequest, res: Result<Measured, String>) {
        self.touch.measuring = false;
        match res {
            Err(e) => self.touch.notice = Some(e),
            Ok(m) if m.busy => {
                // Shown before the op is created; Cancel (the default) stores nothing.
                self.touch.busy_prompt = Some(BusyPrompt {
                    request: req,
                    measured: m,
                });
            }
            Ok(m) => self.proceed(req, m),
        }
    }

    /// The busy-background sheet: Cancel stores nothing, Use anyway goes on with the fill that
    /// fits best (ANN-9.AC3).
    pub fn busy_choice(&mut self, use_anyway: bool) {
        let Some(p) = self.touch.busy_prompt.take() else {
            return;
        };
        if use_anyway {
            self.proceed(p.request, p.measured);
        }
    }

    fn proceed(&mut self, req: MeasureRequest, m: Measured) {
        match req.what {
            MeasureFor::Retype => {
                let st = self.styles.get(Tool::Retype);
                let kind = OpKind::Retype {
                    rect: req.rect,
                    fill: m.fill,
                    text: req.text.clone(),
                    size: m.size,
                    color: m.color,
                    weight: st.weight,
                    align: st.align,
                };
                self.selected = None;
                self.text = Some(Self::retype_edit(kind, None, Some(req.text)));
                self.last_text_input = Some(std::time::Instant::now());
            }
            MeasureFor::MoveSource => {
                self.selected = None;
                self.touch.mv = Some(MovePending {
                    source: req.rect,
                    fill: m.fill,
                    dest: None,
                });
            }
        }
    }

    // ---- Retype ----------------------------------------------------------------------------

    /// Typing the new text of a retype (one line, selected so typing replaces it).
    pub(super) fn retype_edit(
        kind: OpKind,
        existing: Option<String>,
        original: Option<String>,
    ) -> TextEdit {
        let (rect, text, size, color) = match &kind {
            OpKind::Retype {
                rect,
                text,
                size,
                color,
                ..
            } => (rect.normalized(), text.clone(), *size, *color),
            _ => (
                Rc::new(0.0, 0.0, 1.0, 1.0),
                String::new(),
                14.0,
                Color::BLACK,
            ),
        };
        TextEdit {
            target: TextTarget::Retype {
                base: kind,
                original,
            },
            existing,
            // The text is centred vertically in the box.
            at: Pt::new(rect.x, rect.y + (rect.h - size * 1.25) / 2.0),
            cursor: text.chars().count(),
            sel_all: !text.is_empty(),
            text,
            size,
            color,
            outline: None,
            bg: None,
            marked: None,
            anchor: None,
        }
    }

    /// Enter or a click elsewhere: the retype is stored (before the first non-base op), or
    /// nothing is stored when the text did not change or is empty.
    pub(super) fn finish_retype(&mut self, t: &TextEdit, original: Option<String>) -> Option<Edit> {
        let kind = self.fit_retype(t.kind());
        match t.existing.clone() {
            Some(id) => {
                let i = self.index_of(&id)?;
                if t.text.is_empty() {
                    self.ops.remove(i);
                    self.selected = None;
                    return Some(Edit::Replace(self.ops.clone()));
                }
                self.selected = Some(id);
                if self.ops[i].kind == kind {
                    return None;
                }
                self.ops[i].kind = kind;
                Some(Edit::Replace(self.ops.clone()))
            }
            None => {
                if t.text.is_empty() || original.as_deref() == Some(t.text.as_str()) {
                    return None;
                }
                Some(self.insert_base_edit(kind))
            }
        }
    }

    /// A text wider than its box widens the box (the fill with it) instead of being cut: to the
    /// right for left-aligned text, to the left for right-aligned, both ways for centred, kept
    /// inside the picture (ANN-9).
    pub(super) fn fit_retype(&self, mut kind: OpKind) -> OpKind {
        let Some((w, _)) = cm_render::touchup::retype_text_extent(&kind) else {
            return kind;
        };
        let OpKind::Retype { rect, align, .. } = &mut kind else {
            return kind;
        };
        let r = rect.normalized();
        let max_w = f64::from(self.image.0);
        let need = (w.ceil() + 2.0).min(max_w);
        if need <= r.w || max_w <= 0.0 {
            return kind;
        }
        let grow = need - r.w;
        let left = match align {
            TextAlign::Left => 0.0,
            TextAlign::Right => grow,
            _ => grow / 2.0,
        };
        let x = (r.x - left).clamp(0.0, (max_w - need).max(0.0));
        *rect = Rc::new(x, r.y, need, r.h);
        kind
    }

    /// Where the text of the retype being typed starts (image px, the x the renderer lays it
    /// out from): it follows the widened box and the alignment, so the caret and the selection
    /// sit on the drawn text, not on the unwidened box. `None` when no retype is open.
    pub fn retype_edit_origin_x(&self) -> Option<f64> {
        let t = self.text.as_ref()?;
        if !matches!(t.target, TextTarget::Retype { .. }) {
            return None;
        }
        let kind = self.fit_retype(t.kind());
        let w = cm_render::touchup::retype_text_extent(&kind).map_or(0.0, |(w, _)| w);
        let OpKind::Retype { rect, align, .. } = &kind else {
            return None;
        };
        let n = rect.normalized();
        Some(match align {
            TextAlign::Left => n.x,
            TextAlign::Center => n.x + (n.w - w) / 2.0,
            TextAlign::Right => n.x + n.w - w,
        })
    }

    /// What the Retype panel shows: the text being typed, the selected retype, or the tool's
    /// settings.
    pub fn retype_info(&self) -> Option<RetypeInfo> {
        if self.inspector_tool() != Tool::Retype {
            return None;
        }
        let (kind, editing, has_op) = match (&self.text, self.selected_kind()) {
            (Some(t), _) if matches!(t.target, TextTarget::Retype { .. }) => {
                (Some(t.kind()), true, t.existing.is_some())
            }
            (None, Some(k @ OpKind::Retype { .. })) => (Some(k.clone()), false, true),
            _ => (None, false, false),
        };
        let note = self.touch.lines_error.clone();
        match kind {
            Some(
                k @ OpKind::Retype {
                    weight,
                    align,
                    size,
                    color,
                    ..
                },
            ) => {
                let OpKind::Retype { text, fill, .. } = &k else {
                    return None;
                };
                let (fk, fc) = fill_summary(fill);
                Some(RetypeInfo {
                    text: text.clone(),
                    size,
                    color,
                    weight,
                    align,
                    fill_kind: fk,
                    fill_color: fc,
                    editing,
                    has_op,
                    note,
                })
            }
            _ => {
                let st = self.styles.get(Tool::Retype);
                Some(RetypeInfo {
                    text: String::new(),
                    size: 0.0,
                    color: st.color,
                    weight: st.weight,
                    align: st.align,
                    fill_kind: FillKind::Solid,
                    fill_color: Color::WHITE,
                    editing: false,
                    has_op: false,
                    note,
                })
            }
        }
    }

    /// Regular, Medium or Bold: the retype being typed or selected, and new ones.
    pub fn set_retype_weight(&mut self, weight: u16) -> Option<Edit> {
        self.styles.set_retype(Some(weight), None);
        self.styles_dirty = true;
        self.edit_retype(|w, _| *w = weight)
    }

    /// Left, Center or Right.
    pub fn set_retype_align(&mut self, align: TextAlign) -> Option<Edit> {
        self.styles.set_retype(None, Some(align));
        self.styles_dirty = true;
        self.edit_retype(|_, a| *a = align)
    }

    fn edit_retype(&mut self, f: impl Fn(&mut u16, &mut TextAlign)) -> Option<Edit> {
        let apply = |k: &mut OpKind| {
            if let OpKind::Retype { weight, align, .. } = k {
                f(weight, align);
            }
        };
        if let Some(t) = self.text.as_mut() {
            if let TextTarget::Retype { base, .. } = &mut t.target {
                apply(base);
            }
            return None;
        }
        let i = self.selected_index()?;
        if !matches!(self.ops[i].kind, OpKind::Retype { .. }) {
            return None;
        }
        let before = self.ops[i].kind.clone();
        apply(&mut self.ops[i].kind);
        (self.ops[i].kind != before).then(|| Edit::Replace(self.ops.clone()))
    }

    // ---- Move area -------------------------------------------------------------------------

    /// A press inside the chosen source starts dragging its contents.
    pub(super) fn begin_move_drag(&mut self, p: Pt, tol: f64) -> bool {
        let Some(mv) = &self.touch.mv else {
            return false;
        };
        let s = mv.source.normalized();
        let cur = mv.dest.unwrap_or(Pt::new(s.x, s.y));
        let inside_now = inside(Rc::new(cur.x, cur.y, s.w, s.h), p, tol);
        if !inside_now {
            return false;
        }
        self.touch.mv_drag = Some(MoveDrag {
            start: p,
            dest0: cur,
            moved: false,
        });
        true
    }

    /// The pointer moved while the contents are dragged: snapped, shown by the preview.
    pub(super) fn move_drag_to(&mut self, p: Pt, mods: Mods, tol: f64) -> bool {
        let Some(d) = self.touch.mv_drag.clone() else {
            return false;
        };
        let Some(mv) = self.touch.mv.as_mut() else {
            return false;
        };
        let delta = (p.x - d.start.x, p.y - d.start.y);
        let moved = d.moved || delta.0.hypot(delta.1) >= tol / 2.0;
        if let Some(dr) = self.touch.mv_drag.as_mut() {
            dr.moved = moved;
        }
        if moved {
            let (to, guide) = touchup::moved_to(
                mv.source,
                d.dest0,
                delta,
                mods.shift,
                self.touch.snap_on,
                tol * touchup::SNAP_PX / 6.0,
                self.image,
            );
            mv.dest = Some(to);
            self.touch.guide = guide;
        }
        true
    }

    /// The contents were dropped: one `move_area` op, inserted among the base edits.
    pub(super) fn end_move_drag(&mut self) -> Option<Edit> {
        let d = self.touch.mv_drag.take()?;
        self.touch.guide = None;
        if !d.moved {
            return None;
        }
        let mv = self.touch.mv.take()?;
        let to = mv.dest?;
        Some(self.insert_base_edit(mv.op(Some(to))))
    }

    /// Delete on a chosen source, or "Remove contents" on a moved area: the contents go and the
    /// hole is filled (`to: null`, ANN-10.AC2).
    pub fn remove_contents(&mut self) -> Option<Edit> {
        if let Some(mv) = self.touch.mv.take() {
            self.touch.guide = None;
            return Some(self.insert_base_edit(mv.op(None)));
        }
        let i = self.selected_index()?;
        match &mut self.ops[i].kind {
            OpKind::MoveArea { to, .. } if to.is_some() => {
                *to = None;
                Some(Edit::Replace(self.ops.clone()))
            }
            _ => None,
        }
    }

    pub fn set_snap_on(&mut self, on: bool) {
        self.touch.snap_on = on;
    }

    /// What the Move area panel shows: the chosen source, the selected op, or the tool's
    /// settings.
    pub fn move_info(&self) -> Option<MoveInfo> {
        if self.inspector_tool() != Tool::MoveArea || self.text.is_some() {
            return None;
        }
        if let Some(mv) = &self.touch.mv {
            let (fk, fc) = fill_summary(&mv.fill);
            return Some(MoveInfo {
                source: Some(mv.source.normalized()),
                to: mv.dest,
                fill_kind: fk,
                fill_color: fc,
                snap: self.touch.snap_on,
                pending: true,
                removes: false,
            });
        }
        match self.selected_kind() {
            Some(OpKind::MoveArea { source, to, fill }) => {
                let (fk, fc) = fill_summary(fill);
                Some(MoveInfo {
                    source: Some(source.normalized()),
                    to: *to,
                    fill_kind: fk,
                    fill_color: fc,
                    snap: self.touch.snap_on,
                    pending: false,
                    removes: to.is_none(),
                })
            }
            _ => Some(MoveInfo {
                source: None,
                to: None,
                fill_kind: FillKind::Solid,
                fill_color: Color::WHITE,
                snap: self.touch.snap_on,
                pending: false,
                removes: false,
            }),
        }
    }

    /// The source outline with its `filled · #FFFFFF` tag, and where the contents are now.
    pub fn move_overlay(&self) -> Option<MoveOverlay> {
        let (source, fill, dest) = if let Some(mv) = &self.touch.mv {
            (mv.source.normalized(), &mv.fill, mv.dest)
        } else {
            match self.selected_kind()? {
                OpKind::MoveArea { source, to, fill } => (source.normalized(), fill, *to),
                _ => return None,
            }
        };
        let hex = fill
            .colors
            .first()
            .map_or_else(|| "#FFFFFF".to_owned(), |c| c.to_hex());
        Some(MoveOverlay {
            source,
            tag: tr!("filled \u{B7} {hex}", hex = hex),
            dest: dest.map(|d| Rc::new(d.x, d.y, source.w, source.h)),
        })
    }

    // ---- Blueprint -------------------------------------------------------------------------

    /// The recognizer answered: the panel opens with every line a bar.
    pub fn blueprint_open(&mut self, lines: &[Line]) {
        self.touch.bp_loading = false;
        if lines.is_empty() {
            self.touch.notice = Some(tr!(
                "No text was found in this image, so there is nothing to turn into bars."
            ));
            return;
        }
        self.text = None;
        self.selected = None;
        self.touch.bp = Some(BlueprintPanel::from_lines(lines));
    }

    /// Opens the panel on the image's stored Blueprint, if it has one.
    pub fn blueprint_open_existing(&mut self) -> bool {
        let found = self.ops.iter().find_map(|o| {
            matches!(o.kind, OpKind::Blueprint { .. })
                .then(|| BlueprintPanel::from_op(&o.id, &o.kind))
                .flatten()
        });
        match found {
            Some(p) => {
                self.selected = None;
                self.touch.bp = Some(p);
                true
            }
            None => false,
        }
    }

    pub fn set_blueprint_loading(&mut self, on: bool) {
        self.touch.bp_loading = on;
    }

    pub fn blueprint_active(&self) -> bool {
        self.touch.bp.is_some()
    }

    /// A click on a line keeps its real text, or makes it a bar again.
    pub(super) fn blueprint_click(&mut self, p: Pt) -> Option<Edit> {
        let i = self.touch.bp.as_ref()?.line_at(p)?;
        self.blueprint_toggle(i)
    }

    /// One keep-list change. A draft only changes the panel; a stored Blueprint is one op
    /// update and so one undo step (ANN-11.AC3); keeping every line removes the op.
    pub fn blueprint_toggle(&mut self, i: usize) -> Option<Edit> {
        let bp = self.touch.bp.as_mut()?;
        if !bp.toggle(i) {
            return None;
        }
        let id = bp.existing.clone()?;
        let kind = bp.op_kind();
        let idx = self.index_of(&id)?;
        match kind {
            Some(k) => self.ops[idx].kind = k,
            None => {
                self.ops.remove(idx);
                self.touch.bp = None;
            }
        }
        Some(Edit::Replace(self.ops.clone()))
    }

    /// Takes back the last keep-list change of a draft (Cmd/Ctrl+Z while the panel is open).
    pub fn blueprint_undo_keep(&mut self) -> bool {
        match self.touch.bp.as_mut() {
            Some(bp) if bp.existing.is_none() => bp.undo_keep(),
            _ => false,
        }
    }

    /// Bars or Boxes.
    pub fn set_blueprint_style(&mut self, style: BlueprintStyle) -> Option<Edit> {
        self.edit_blueprint(|b| b.style = style)
    }

    /// Auto (each bar takes its line's colour) or one colour.
    pub fn set_blueprint_color(&mut self, color: BlueprintColor) -> Option<Edit> {
        self.edit_blueprint(|b| b.color = color)
    }

    pub fn set_blueprint_radius(&mut self, radius: f64) -> Option<Edit> {
        let r = radius.clamp(0.0, 40.0);
        self.edit_blueprint(|b| b.radius = r)
    }

    fn edit_blueprint(&mut self, f: impl FnOnce(&mut BlueprintPanel)) -> Option<Edit> {
        let bp = self.touch.bp.as_mut()?;
        f(bp);
        let id = bp.existing.clone()?;
        let kind = bp.op_kind()?;
        let idx = self.index_of(&id)?;
        if self.ops[idx].kind == kind {
            return None;
        }
        self.ops[idx].kind = kind;
        Some(Edit::Replace(self.ops.clone()))
    }

    /// Apply Blueprint: one op, inserted among the base edits, one undo step (ANN-11).
    pub fn blueprint_apply(&mut self) -> Option<Edit> {
        let bp = self.touch.bp.take()?;
        if bp.existing.is_some() {
            return None;
        }
        let kind = bp.op_kind()?;
        let edit = self.insert_base_edit(kind);
        self.selected = None;
        Some(edit)
    }

    /// Cancel: the draft goes, nothing is stored.
    pub fn blueprint_cancel(&mut self) {
        self.touch.bp = None;
    }

    pub fn blueprint_info(&self) -> Option<BlueprintInfo> {
        let bp = self.touch.bp.as_ref()?;
        let (found, bars, kept) = bp.counts();
        Some(BlueprintInfo {
            found,
            bars,
            kept,
            style: bp.style,
            color: bp.color,
            radius: bp.radius,
            kept_rows: bp
                .boxes
                .iter()
                .enumerate()
                .filter(|(_, b)| !b.replaced)
                .map(|(i, b)| (i, b.text.clone()))
                .collect(),
            existing: bp.existing.is_some(),
            can_undo: bp.can_undo_keep(),
            hover: bp
                .hover
                .and_then(|i| bp.boxes.get(i))
                .map(|b| (b.text.clone(), b.replaced)),
        })
    }

    /// Every line of the open panel for the overlay: kept ones get the green dashed outline.
    pub fn blueprint_boxes(&self) -> Vec<BlueprintBox> {
        let Some(bp) = &self.touch.bp else {
            return Vec::new();
        };
        bp.boxes
            .iter()
            .enumerate()
            .map(|(i, b)| BlueprintBox {
                rect: b.rect.normalized(),
                kept: !b.replaced,
                hovered: bp.hover == Some(i),
            })
            .collect()
    }

    // ---- text selection with the pointer (ANN-1.AC9 to AC11) -------------------------------

    /// The box of the text being typed, for the press test: a text box or a callout (a step
    /// label and a retype are one line and keep to the keyboard).
    fn text_frame(&mut self) -> Option<Rc> {
        let t = self.text.clone()?;
        match &t.target {
            TextTarget::Callout(_) => cm_render::callout_box(&t.kind()),
            TextTarget::Text => {
                let m = &mut self.metrics;
                let lh = m.line_height(t.size);
                let lines = t.text.split('\n').count().max(1);
                let b = m.measure(&t.text, t.size);
                let w = if t.text.is_empty() {
                    t.size * 0.6
                } else {
                    (b.dx + b.w).max(t.size * 0.6)
                };
                Some(Rc::new(t.at.x, t.at.y, w, lh * lines as f64))
            }
            _ => None,
        }
    }

    /// The char index of the text being typed nearest to `p` (image px).
    pub(super) fn char_index_at(&mut self, p: Pt) -> Option<usize> {
        let t = self.text.clone()?;
        if !matches!(t.target, TextTarget::Text | TextTarget::Callout(_)) {
            return None;
        }
        let lh = self.metrics.line_height(t.size).max(1.0);
        let lines: Vec<&str> = t.text.split('\n').collect();
        let li = (((p.y - t.at.y) / lh).floor().max(0.0) as usize).min(lines.len() - 1);
        let before: usize = lines.iter().take(li).map(|l| l.chars().count() + 1).sum();
        let line: Vec<char> = lines[li].chars().collect();
        let x = p.x - t.at.x;
        let m = &mut self.metrics;
        let mut width = |k: usize| -> f64 {
            if k == 0 {
                return 0.0;
            }
            let pre: String = line.iter().take(k).collect();
            let b = m.measure(&pre, t.size);
            b.dx + b.w
        };
        // The first boundary at or past x, then the nearer of it and the one before.
        let (mut lo, mut hi) = (0usize, line.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            if width(mid) >= x {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        let k = if lo > 0 && x - width(lo - 1) < width(lo) - x {
            lo - 1
        } else {
            lo
        };
        Some(before + k)
    }

    /// A press inside the text being edited: caret, shift-extend, double-click word. Returns
    /// whether the press was handled (outside the box it commits the text as before).
    pub(super) fn press_in_text(&mut self, p: Pt, mods: Mods, clicks: usize, tol: f64) -> bool {
        let Some(frame) = self.text_frame() else {
            return false;
        };
        if !inside(frame, p, tol) {
            return false;
        }
        let Some(i) = self.char_index_at(p) else {
            return false;
        };
        let Some(t) = self.text.as_mut() else {
            return false;
        };
        if clicks >= 3 {
            t.select_line_at(i);
        } else if clicks == 2 {
            t.select_word_at(i);
        } else {
            t.press_at(i, mods.shift);
            // The pointer is captured until the button goes up.
            self.text_drag = true;
        }
        self.last_text_input = Some(std::time::Instant::now());
        true
    }

    /// The pointer went up while selecting: the caret ends where it is.
    pub(super) fn pointer_move_text_end(&mut self, p: Pt) {
        if let Some(i) = self.char_index_at(p) {
            if let Some(t) = self.text.as_mut() {
                t.drag_to(i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::tools::{Styles, Tool};
    use cm_core::annotation::validate_list;

    const NONE: Mods = Mods {
        shift: false,
        alt: false,
        secondary: false,
    };
    const SHIFT: Mods = Mods {
        shift: true,
        alt: false,
        secondary: false,
    };

    fn st() -> EditorState {
        EditorState::new((1000, 600), Vec::new(), Styles::default())
    }

    fn arrow() -> OpKind {
        OpKind::Arrow {
            from: Pt::new(5.0, 5.0),
            to: Pt::new(60.0, 40.0),
            color: Color::BLACK,
            width: 4.0,
            style: Default::default(),
        }
    }

    fn drag(s: &mut EditorState, a: (f64, f64), b: (f64, f64)) -> Vec<Edit> {
        let mut e = s.pointer_down(Pt::new(a.0, a.1), NONE, 1, 6.0);
        s.pointer_move(Pt::new(b.0, b.1), NONE, 6.0);
        e.extend(s.pointer_up(Pt::new(b.0, b.1), NONE, 6.0));
        e
    }

    fn line(x: f64, y: f64, w: f64, text: &str) -> Line {
        Line {
            rect: Rc::new(x, y, w, 16.0),
            text: text.into(),
            confidence: 0.9,
            fill: Some(Fill::solid(Color::WHITE)),
            color: Some(Color::rgb(0x20, 0x20, 0x20)),
            busy: false,
        }
    }

    fn measured(busy: bool) -> Measured {
        Measured {
            fill: Fill::solid(Color::rgb(0xF0, 0xF0, 0xF0)),
            busy,
            color: Color::rgb(0x10, 0x10, 0x10),
            size: 14.0,
        }
    }

    fn base_names(s: &EditorState) -> Vec<&'static str> {
        s.ops.iter().map(|o| o.kind.name()).collect()
    }

    fn type_text(s: &mut EditorState, text: &str) {
        s.text_ime(None, text, false);
    }

    // ---- Zoom Lens ---------------------------------------------------------------------

    /// ANN-8.AC7: while the pointer is down the loupe and its connector are in the preview;
    /// on release one `magnify` op is made in one edit, and undo (one edit) removes it.
    #[test]
    fn ann_8_ac7_the_loupe_is_live_while_dragging_and_one_op_is_made() {
        let mut s = st();
        s.set_tool(Tool::Magnify);
        s.pointer_down(Pt::new(100.0, 100.0), NONE, 1, 6.0);
        s.pointer_move(Pt::new(200.0, 150.0), NONE, 6.0);
        let live = s.render_ops();
        assert_eq!(live.len(), 1);
        let OpKind::Magnify {
            source,
            at,
            zoom,
            connector,
            ..
        } = &live[0].kind
        else {
            panic!("the draft lens is rendered live")
        };
        assert_eq!(
            (source.w, source.h, *zoom, *connector),
            (100.0, 50.0, 3.0, true)
        );
        // The loupe is beside the source and inside the image.
        assert!(at.x - source.w * zoom / 2.0 >= source.x + source.w);
        assert!(s.ops.is_empty(), "nothing stored until release");
        let edits = s.pointer_up(Pt::new(200.0, 150.0), NONE, 6.0);
        assert_eq!(edits.len(), 1);
        let Edit::Append(ops) = &edits[0] else {
            panic!()
        };
        assert!(matches!(ops[0].kind, OpKind::Magnify { .. }));
        assert_eq!(s.ops.len(), 1);
        assert!(s.selected.is_some());
    }

    /// ANN-8.AC5: a fifth lens is refused with a message in the panel and nothing is stored;
    /// a source under 8 px is not a lens.
    #[test]
    fn ann_8_ac5_a_fifth_lens_is_refused() {
        let mut s = st();
        s.set_tool(Tool::Magnify);
        for i in 0..4 {
            let y = 20.0 + 100.0 * f64::from(i);
            assert_eq!(drag(&mut s, (20.0, y), (60.0, y + 30.0)).len(), 1);
        }
        assert_eq!(s.lens_count(), 4);
        let before = s.ops.clone();
        assert!(drag(&mut s, (500.0, 20.0), (560.0, 60.0)).is_empty());
        assert_eq!(s.ops, before);
        let info = s.lens_info().unwrap();
        assert_eq!((info.count, info.max), (4, 4));
        assert!(info.notice.is_some());
        // Too small.
        let mut s = st();
        s.set_tool(Tool::Magnify);
        assert!(drag(&mut s, (20.0, 20.0), (25.0, 25.0)).is_empty());
        assert!(s.ops.is_empty());
    }

    /// ANN-8.AC5: a zoom that makes the loupe too large is refused in the panel.
    #[test]
    fn ann_8_ac5_a_huge_loupe_is_refused() {
        let mut s = EditorState::new((20000, 20000), Vec::new(), Styles::default());
        s.set_tool(Tool::Magnify);
        // 3000 px source at zoom 3 = 9000 px side but 81 MP: over 32 MP.
        assert!(drag(&mut s, (100.0, 100.0), (3100.0, 3100.0)).is_empty());
        assert!(s.lens_info().unwrap().notice.is_some());
    }

    #[test]
    fn the_zoom_lens_panel_changes_the_selected_lens_in_one_edit_each() {
        let mut s = st();
        s.set_tool(Tool::Magnify);
        drag(&mut s, (100.0, 100.0), (200.0, 150.0));
        let e = s.set_param(ParamValue::Zoom(6.0)).unwrap();
        let Edit::Replace(ops) = e else { panic!() };
        assert!(matches!(ops[0].kind, OpKind::Magnify { zoom, .. } if zoom == 6.0));
        assert!(matches!(
            s.set_lens_shape(MagnifyShape::Circle),
            Some(Edit::Replace(_))
        ));
        assert!(
            s.set_lens_shape(MagnifyShape::Circle).is_none(),
            "unchanged: no edit"
        );
        assert!(matches!(
            s.set_lens_connector(false),
            Some(Edit::Replace(_))
        ));
        assert!(matches!(s.set_lens_badge(false), Some(Edit::Replace(_))));
        assert!(matches!(
            s.set_param(ParamValue::BorderWidth(12.0)),
            Some(Edit::Replace(_))
        ));
        assert!(matches!(
            s.set_param(ParamValue::Color(Color::rgb(0xFF, 0x3B, 0x3B))),
            Some(Edit::Replace(_))
        ));
        let OpKind::Magnify {
            shape,
            connector,
            badge,
            border_width,
            border,
            ..
        } = &s.ops[0].kind
        else {
            panic!()
        };
        assert_eq!(*shape, MagnifyShape::Circle);
        assert!(!*connector && !*badge);
        assert_eq!(*border_width, 12.0);
        assert_eq!(*border, Color::rgb(0xFF, 0x3B, 0x3B));
        // The choices are remembered for the next lens.
        let st = s.styles.get(Tool::Magnify);
        assert_eq!(
            (st.zoom, st.lens_shape, st.connector, st.badge),
            (6.0, MagnifyShape::Circle, false, false)
        );
        // A circle lens takes a square source.
        s.selected = None;
        drag(&mut s, (300.0, 400.0), (380.0, 430.0));
        let OpKind::Magnify { source, .. } = &s.ops[1].kind else {
            panic!()
        };
        assert_eq!(source.w, source.h);
    }

    #[test]
    fn dragging_the_loupe_moves_only_the_loupe_and_the_source_moves_both() {
        let mut s = st();
        s.set_tool(Tool::Magnify);
        drag(&mut s, (100.0, 100.0), (160.0, 140.0));
        let OpKind::Magnify {
            source: s0, at: a0, ..
        } = s.ops[0].kind.clone()
        else {
            panic!()
        };
        // Press on the loupe (far from the source).
        let on_loupe = Pt::new(a0.x, a0.y);
        s.pointer_down(on_loupe, NONE, 1, 6.0);
        s.pointer_move(Pt::new(on_loupe.x + 30.0, on_loupe.y + 10.0), NONE, 6.0);
        let e = s.pointer_up(Pt::new(on_loupe.x + 30.0, on_loupe.y + 10.0), NONE, 6.0);
        assert_eq!(e.len(), 1);
        let OpKind::Magnify {
            source: s1, at: a1, ..
        } = s.ops[0].kind.clone()
        else {
            panic!()
        };
        assert_eq!(s1, s0, "the source stayed");
        assert_eq!((a1.x - a0.x, a1.y - a0.y), (30.0, 10.0));
        // Press on the source: the whole lens moves.
        s.pointer_down(Pt::new(130.0, 120.0), NONE, 1, 6.0);
        s.pointer_move(Pt::new(140.0, 120.0), NONE, 6.0);
        s.pointer_up(Pt::new(140.0, 120.0), NONE, 6.0);
        let OpKind::Magnify {
            source: s2, at: a2, ..
        } = s.ops[0].kind.clone()
        else {
            panic!()
        };
        assert_eq!(s2.x, s1.x + 10.0);
        assert_eq!(a2.x, a1.x + 10.0);
    }

    #[test]
    fn the_loupe_goes_right_then_below_then_left() {
        let src = Rc::new(100.0, 100.0, 100.0, 50.0);
        let a = geom::default_loupe_at(src, 3.0, (1000, 600));
        assert!(a.x > 200.0);
        let wide = Rc::new(500.0, 100.0, 100.0, 50.0);
        let b = geom::default_loupe_at(wide, 3.0, (800, 600));
        assert!(b.y > 150.0, "below when the right side does not fit");
        let tall = Rc::new(550.0, 100.0, 100.0, 40.0);
        let c = geom::default_loupe_at(tall, 3.0, (700, 220));
        assert!(c.x < 550.0, "left when neither right nor below fits");
    }

    // ---- Retype ------------------------------------------------------------------------

    fn retype_ready(s: &mut EditorState, lines: Vec<Line>) {
        s.set_tool(Tool::Retype);
        assert!(s.lines_wanted());
        s.set_lines_loading();
        s.set_lines(Ok(lines));
        assert!(!s.lines_wanted(), "asked once");
    }

    /// ANN-9: hovering shows the recognized lines; a click on one asks for the measurement, the
    /// text opens filled in, and Enter stores the op before the first non-base op.
    #[test]
    fn ann_9_click_a_line_type_and_apply() {
        let mut s = st();
        s.ops.push(AnnotationOp::new("a1", arrow()));
        retype_ready(
            &mut s,
            vec![
                line(100.0, 100.0, 60.0, "Save"),
                line(100.0, 140.0, 80.0, "Cancel"),
            ],
        );
        s.pointer_move(Pt::new(110.0, 108.0), NONE, 6.0);
        let boxes = s.retype_boxes();
        assert_eq!(boxes.len(), 2);
        assert!(boxes[0].hovered && !boxes[1].hovered);
        assert!(drag(&mut s, (110.0, 108.0), (110.0, 108.0)).is_empty());
        let req = s
            .take_measure_request()
            .expect("the click asks for a measurement");
        assert_eq!(req.what, MeasureFor::Retype);
        assert_eq!(req.text, "Save");
        assert_eq!(req.rect, Rc::new(100.0, 100.0, 60.0, 16.0));
        assert!(s.touch.measuring && s.busy());
        s.touch_measured(req, Ok(measured(false)));
        let t = s
            .text
            .as_ref()
            .expect("the text opens with the recognized text");
        assert_eq!(t.text, "Save");
        assert!(t.sel_all, "typing replaces it");
        assert!(matches!(t.target, TextTarget::Retype { .. }));
        // The preview shows the filled box and the new text while typing.
        type_text(&mut s, "Save all");
        let live = s.render_ops();
        assert!(matches!(&live[0].kind, OpKind::Retype { text, .. } if text == "Save all"));
        assert!(
            matches!(live[1].kind, OpKind::Arrow { .. }),
            "the base edit sorts before the arrow"
        );
        let e = s.finish_text().expect("one edit");
        let Edit::Replace(ops) = e else {
            panic!("a base edit is sent as the whole list")
        };
        assert_eq!(base_names(&s), ["retype", "arrow"]);
        assert!(validate_list(&ops).is_ok(), "annotate accepts the order");
        let OpKind::Retype {
            text,
            size,
            color,
            fill,
            weight,
            ..
        } = &ops[0].kind
        else {
            panic!()
        };
        assert_eq!(text, "Save all");
        assert_eq!(*size, 14.0);
        assert_eq!(*color, Color::rgb(0x10, 0x10, 0x10));
        assert_eq!(fill.colors[0], Color::rgb(0xF0, 0xF0, 0xF0));
        assert_eq!(*weight, 400);
        assert!(s.selected.is_some());
    }

    /// ANN-9 (0.3.4 QA): while typing, a text wider than the recognized box is drawn whole (the
    /// box widens live as the commit does), and the caret position the overlay computes
    /// (`retype_edit_origin_x` + the editor's measure) lands on the last drawn pixel of the text,
    /// for left, centred and right alignment and for Vietnamese.
    #[test]
    fn ann_9_live_preview_is_not_cut_at_the_box_and_the_caret_is_at_the_drawn_end() {
        for text in [
            "Databricks Lakehouse Platform (sold by Databricks Inc.)",
            "Nền tảng Lakehouse Databricks (do Databricks Inc. bán) Ếộ",
        ] {
            for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
                let mut s = EditorState::new((1000, 100), Vec::new(), Styles::default());
                s.set_tool(Tool::Retype);
                let kind = OpKind::Retype {
                    rect: Rc::new(300.0, 40.0, 150.0, 18.0),
                    fill: Fill::solid(Color::WHITE),
                    text: String::new(),
                    size: 14.0,
                    color: Color::BLACK,
                    weight: 400,
                    align,
                };
                s.text = Some(EditorState::retype_edit(kind, None, Some("x".into())));
                type_text(&mut s, text);
                let ops = s.render_ops();
                let base =
                    image::RgbaImage::from_pixel(1000, 100, image::Rgba([255, 255, 255, 255]));
                let out = cm_render::render_annotations(&base, &ops).unwrap();
                let (mut x0, mut x1) = (u32::MAX, 0u32);
                for (x, _, p) in out.enumerate_pixels() {
                    if p.0[0] < 160 {
                        x0 = x0.min(x);
                        x1 = x1.max(x + 1);
                    }
                }
                assert!(x1 > x0, "text is drawn");
                let drawn = f64::from(x1 - x0);
                let mut m = crate::editor::TextMetrics::default();
                let b = m.measure(text, 14.0);
                assert!(
                    (drawn - b.w).abs() <= 6.0,
                    "{align:?}: whole text drawn ({drawn} vs {})",
                    b.w
                );
                assert!(drawn > 150.0, "wider than the old box");
                let origin = s.retype_edit_origin_x().unwrap();
                let caret = origin + b.dx + b.w;
                assert!(
                    (caret - f64::from(x1)).abs() <= 6.0,
                    "{align:?}: caret {caret} vs drawn end {x1}"
                );
            }
        }
    }

    /// QA E4: a click on the panel's Text field (a plain box) must give typing back to the
    /// canvas whether the text is open (focus only) or a stored retype is selected (open it).
    #[test]
    fn e4_the_panel_field_click_leads_to_typing() {
        let mut s = st();
        retype_ready(&mut s, vec![line(100.0, 100.0, 60.0, "Save")]);
        drag(&mut s, (110.0, 108.0), (110.0, 108.0));
        let req = s.take_measure_request().unwrap();
        s.touch_measured(req, Ok(measured(false)));
        // Open on the canvas, nothing stored yet: the click only refocuses.
        let info = s.retype_info().unwrap();
        assert!(info.editing && !info.has_op);
        assert_eq!(info.field_click(), FieldClick::Refocus);
        type_text(&mut s, "Save all");
        assert_eq!(s.retype_info().unwrap().text, "Save all");
        s.finish_text().unwrap();
        // Stored and selected: the click opens it, and typing then changes the panel text.
        let info = s.retype_info().unwrap();
        assert!(!info.editing && info.has_op);
        assert_eq!(info.field_click(), FieldClick::Edit);
        s.edit_selected_text();
        let info = s.retype_info().unwrap();
        assert!(info.editing);
        assert_eq!(info.field_click(), FieldClick::Refocus);
        type_text(&mut s, "Keep");
        assert_eq!(s.retype_info().unwrap().text, "Keep");
        // The tool's settings with nothing selected: nothing to type into.
        s.finish_text();
        s.selected = None;
        assert_eq!(s.retype_info().unwrap().field_click(), FieldClick::Nothing);
    }

    /// ANN-9: a retype goes after the earlier base edits and before everything else.
    #[test]
    fn ann_9_a_second_retype_goes_after_the_first_and_before_the_arrow() {
        let mut s = st();
        retype_ready(
            &mut s,
            vec![line(10.0, 10.0, 60.0, "One"), line(10.0, 50.0, 60.0, "Two")],
        );
        for (y, txt) in [(12.0, "1"), (52.0, "2")] {
            drag(&mut s, (12.0, y), (12.0, y));
            let req = s.take_measure_request().unwrap();
            s.touch_measured(req, Ok(measured(false)));
            type_text(&mut s, txt);
            s.finish_text().unwrap();
            if txt == "1" {
                // An annotation drawn in between.
                s.ops.push(AnnotationOp::new("ar", arrow()));
            }
        }
        assert_eq!(base_names(&s), ["retype", "retype", "arrow"]);
        let texts: Vec<_> = s
            .ops
            .iter()
            .filter_map(|o| match &o.kind {
                OpKind::Retype { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["1", "2"], "their own order is kept");
    }

    /// Retyping to the same text, or to nothing, stores nothing.
    #[test]
    fn ann_9_unchanged_or_empty_text_stores_nothing() {
        let mut s = st();
        retype_ready(&mut s, vec![line(10.0, 10.0, 60.0, "Save")]);
        drag(&mut s, (12.0, 12.0), (12.0, 12.0));
        let req = s.take_measure_request().unwrap();
        s.touch_measured(req, Ok(measured(false)));
        assert!(s.finish_text().is_none(), "Enter without a change");
        assert!(s.ops.is_empty());
        drag(&mut s, (12.0, 12.0), (12.0, 12.0));
        let req = s.take_measure_request().unwrap();
        s.touch_measured(req, Ok(measured(false)));
        type_text(&mut s, "");
        s.text.as_mut().unwrap().text.clear();
        assert!(s.finish_text().is_none());
        assert!(s.ops.is_empty());
    }

    /// A press and its release under the busy sheet never reach the canvas (QA pass 2, E3).
    #[test]
    fn busy_sheet_swallows_canvas_pointer() {
        let mut s = st();
        retype_ready(&mut s, vec![line(10.0, 10.0, 60.0, "Save")]);
        drag(&mut s, (12.0, 12.0), (12.0, 12.0));
        let req = s.take_measure_request().unwrap();
        s.touch_measured(req, Ok(measured(true)));
        assert!(s.touch.busy_prompt.is_some());
        let m = NONE;
        assert!(s.pointer_down(Pt::new(12.0, 12.0), m, 1, 4.0).is_empty());
        s.busy_choice(false);
        assert!(s.pointer_up(Pt::new(12.0, 12.0), m, 4.0).is_empty());
        assert!(s.text.is_none() && s.ops.is_empty() && !s.touch.sheet_press);
        // The next press works again.
        s.pointer_down(Pt::new(12.0, 12.0), m, 1, 4.0);
        assert!(!s.touch.sheet_press);
    }

    /// QA E4: the panel's Text field edits a stored retype: `edit_selected_text` opens its text.
    #[test]
    fn panel_text_field_edits_a_stored_retype() {
        let mut s = st();
        retype_ready(&mut s, vec![line(10.0, 10.0, 60.0, "Save")]);
        drag(&mut s, (12.0, 12.0), (12.0, 12.0));
        let req = s.take_measure_request().unwrap();
        s.touch_measured(req, Ok(measured(false)));
        type_text(&mut s, "Keep");
        assert!(s.finish_text().is_some());
        assert!(s.text.is_none());
        let id = s.ops.last().unwrap().id.clone();
        s.selected = Some(id);
        s.edit_selected_text();
        assert!(
            s.text.is_some(),
            "the panel click opens the text for typing"
        );
        type_text(&mut s, "!");
        assert!(s.text.as_ref().unwrap().text.ends_with('!'));
    }

    /// ANN-9.AC3: a busy background warns before the op is created; Cancel stores nothing and
    /// Use anyway goes on.
    #[test]
    fn ann_9_ac3_busy_background_warns_first() {
        let mut s = st();
        retype_ready(&mut s, vec![line(10.0, 10.0, 60.0, "Save")]);
        drag(&mut s, (12.0, 12.0), (12.0, 12.0));
        let req = s.take_measure_request().unwrap();
        s.touch_measured(req.clone(), Ok(measured(true)));
        assert!(s.touch.busy_prompt.is_some(), "the sheet is shown");
        assert!(s.text.is_none() && s.ops.is_empty(), "no op yet");
        s.busy_choice(false);
        assert!(s.touch.busy_prompt.is_none() && s.text.is_none() && s.ops.is_empty());
        // Again, Use anyway.
        s.touch_measured(req, Ok(measured(true)));
        s.busy_choice(true);
        assert!(s.text.is_some(), "the text opens");
        type_text(&mut s, "Keep");
        assert!(s.finish_text().is_some());
    }

    /// Esc closes the busy sheet first and stores nothing.
    #[test]
    fn esc_on_the_busy_sheet_is_cancel() {
        let mut s = st();
        retype_ready(&mut s, vec![line(10.0, 10.0, 60.0, "Save")]);
        drag(&mut s, (12.0, 12.0), (12.0, 12.0));
        let req = s.take_measure_request().unwrap();
        s.touch_measured(req, Ok(measured(true)));
        assert!(matches!(s.escape(), EscapeOutcome::Edits(e) if e.is_empty()));
        assert!(s.touch.busy_prompt.is_none() && s.ops.is_empty());
    }

    /// A box dragged over text the recognizer missed starts with empty text.
    #[test]
    fn ann_9_a_dragged_box_is_the_fallback() {
        let mut s = st();
        retype_ready(&mut s, vec![]);
        assert!(drag(&mut s, (200.0, 200.0), (300.0, 230.0)).is_empty());
        let req = s.take_measure_request().unwrap();
        assert_eq!(req.rect, Rc::new(200.0, 200.0, 100.0, 30.0));
        assert_eq!(req.text, "");
        // A click on nothing asks for nothing.
        let mut s = st();
        retype_ready(&mut s, vec![line(10.0, 10.0, 60.0, "x")]);
        drag(&mut s, (500.0, 500.0), (500.0, 500.0));
        assert!(s.take_measure_request().is_none());
    }

    /// A line the daemon cut at 60 characters is not kept as the retype's start text.
    #[test]
    fn a_cut_preview_starts_empty() {
        let mut s = st();
        retype_ready(
            &mut s,
            vec![line(
                10.0,
                10.0,
                600.0,
                "A very long sentence that goes on...",
            )],
        );
        drag(&mut s, (12.0, 12.0), (12.0, 12.0));
        let req = s.take_measure_request().unwrap();
        assert_eq!(req.text, "");
    }

    /// Editing a stored retype: double click opens it; a text wider than the box
    /// widens it (no warning, no cut); weight and align are one edit on a selected retype.
    #[test]
    fn ann_9_a_stored_retype_is_edited_and_styled() {
        let mut s = st();
        let kind = OpKind::Retype {
            rect: Rc::new(100.0, 100.0, 40.0, 16.0),
            fill: Fill::solid(Color::WHITE),
            text: "Hi".into(),
            size: 14.0,
            color: Color::BLACK,
            weight: 400,
            align: TextAlign::Left,
        };
        s.ops.push(AnnotationOp::new("r1", kind));
        s.set_tool(Tool::Retype);
        s.pointer_down(Pt::new(110.0, 108.0), NONE, 2, 6.0);
        s.pointer_up(Pt::new(110.0, 108.0), NONE, 6.0);
        assert!(s.text.is_some(), "a double click edits it");
        type_text(&mut s, "A much longer text than the box can hold");
        let info = s.retype_info().unwrap();
        assert!(info.editing && info.has_op);
        assert!(
            s.set_retype_weight(700).is_none(),
            "while typing it goes into the op committed with the text"
        );
        let e = s.finish_text().unwrap();
        let Edit::Replace(ops) = e else { panic!() };
        assert!(
            matches!(&ops[0].kind, OpKind::Retype { weight: 700, text, .. } if text.starts_with("A much"))
        );
        let OpKind::Retype { rect, .. } = &ops[0].kind else {
            panic!()
        };
        assert!(
            rect.normalized().w > 40.0 && rect.normalized().x == 100.0,
            "the box grew to the right to hold the text instead of cutting it"
        );
        let e = s.set_retype_align(TextAlign::Center);
        assert!(matches!(e, Some(Edit::Replace(_))));
        assert!(s.set_retype_align(TextAlign::Center).is_none());
        // Size and colour are measured, not remembered.
        s.set_param(ParamValue::TextSize(20.0)).unwrap();
        assert_eq!(
            s.styles.get(Tool::Retype).size,
            crate::editor::tools::ToolStyle::defaults(Tool::Retype).size
        );
    }

    /// ANN-9.AC6 (editor side): Revert to original removes every retype.
    #[test]
    fn ann_9_ac6_revert_removes_base_edits() {
        let mut s = st();
        retype_ready(&mut s, vec![line(10.0, 10.0, 60.0, "Save")]);
        drag(&mut s, (12.0, 12.0), (12.0, 12.0));
        let req = s.take_measure_request().unwrap();
        s.touch_measured(req, Ok(measured(false)));
        type_text(&mut s, "Saved");
        s.finish_text().unwrap();
        assert!(matches!(s.revert(), Some(Edit::Replace(v)) if v.is_empty()));
        assert!(s.ops.is_empty());
    }

    /// Problem 12: a file written by 0.3.3 holds a base edit after an arrow; the editor puts it
    /// first when it loads (and again when the daemon's list is adopted).
    #[test]
    fn a_loaded_list_has_its_base_edits_first() {
        let rt = AnnotationOp::new(
            "r",
            OpKind::MoveArea {
                source: Rc::new(0.0, 0.0, 10.0, 10.0),
                to: None,
                fill: Fill::solid(Color::WHITE),
            },
        );
        let ar = AnnotationOp::new("a", arrow());
        let s = EditorState::new((100, 100), vec![ar.clone(), rt.clone()], Styles::default());
        assert_eq!(s.ops[0].id, "r");
        let mut s = st();
        s.adopt_ops(vec![ar, rt]);
        assert_eq!(s.ops[0].id, "r");
        assert!(validate_list(&s.ops).is_ok());
    }

    /// ANN-7.AC4 still holds with base edits in the list: a new spotlight goes after them.
    #[test]
    fn a_spotlight_is_inserted_after_the_base_edits() {
        let mut s = st();
        s.ops.push(AnnotationOp::new(
            "m",
            OpKind::MoveArea {
                source: Rc::new(0.0, 0.0, 10.0, 10.0),
                to: None,
                fill: Fill::solid(Color::WHITE),
            },
        ));
        s.ops.push(AnnotationOp::new("a", arrow()));
        s.add_default_hole(cm_core::annotation::SpotShapeKind::Rect)
            .unwrap();
        assert_eq!(base_names(&s), ["move_area", "spotlight", "arrow"]);
    }

    /// A base edit is not copied or duplicated (a copy at the end of the list would be invalid).
    #[test]
    fn base_edits_are_not_duplicated() {
        let mut s = st();
        s.ops.push(AnnotationOp::new(
            "m",
            OpKind::MoveArea {
                source: Rc::new(0.0, 0.0, 10.0, 10.0),
                to: Some(Pt::new(30.0, 0.0)),
                fill: Fill::solid(Color::WHITE),
            },
        ));
        s.selected = Some("m".into());
        assert!(s.duplicate().is_none());
        assert_eq!(s.ops.len(), 1);
    }

    // ---- Move area ----------------------------------------------------------------------

    fn move_ready(s: &mut EditorState) {
        s.set_tool(Tool::MoveArea);
        drag(s, (100.0, 100.0), (140.0, 120.0));
        let req = s
            .take_measure_request()
            .expect("the drag asks for a measurement");
        assert_eq!(req.what, MeasureFor::MoveSource);
        assert_eq!(req.rect, Rc::new(100.0, 100.0, 40.0, 20.0));
        s.touch_measured(req, Ok(measured(false)));
        assert!(s.touch.mv.is_some());
    }

    /// ANN-10.AC1 (editor side): drag a source, drag its contents 120 px right: one op with
    /// `to` 120 px to the right, snapped to the row, with the pink guide while dragging.
    #[test]
    fn ann_10_ac1_drag_the_contents_120_px_right() {
        let mut s = st();
        s.ops.push(AnnotationOp::new("a", arrow()));
        move_ready(&mut s);
        let info = s.move_info().unwrap();
        assert!(info.pending && info.snap);
        let ov = s.move_overlay().unwrap();
        assert_eq!(ov.source, Rc::new(100.0, 100.0, 40.0, 20.0));
        assert_eq!(ov.tag, "filled \u{B7} #F0F0F0");
        s.pointer_down(Pt::new(110.0, 110.0), NONE, 1, 6.0);
        s.pointer_move(Pt::new(230.0, 113.0), NONE, 6.0);
        let g = s.touch.guide.clone().expect("a pink guide while snapped");
        assert_eq!(g.axis, touchup::Axis::Row);
        let live = s.render_ops();
        assert!(
            matches!(&live[0].kind, OpKind::MoveArea { to: Some(t), .. } if t.x == 220.0 && t.y == 100.0)
        );
        let e = s.pointer_up(Pt::new(230.0, 113.0), NONE, 6.0);
        assert_eq!(e.len(), 1);
        let Edit::Replace(ops) = &e[0] else { panic!() };
        assert!(validate_list(ops).is_ok());
        assert_eq!(base_names(&s), ["move_area", "arrow"]);
        let OpKind::MoveArea { source, to, fill } = &ops[0].kind else {
            panic!()
        };
        assert_eq!(*source, Rc::new(100.0, 100.0, 40.0, 20.0));
        assert_eq!(*to, Some(Pt::new(220.0, 100.0)));
        assert_eq!(fill.colors[0], Color::rgb(0xF0, 0xF0, 0xF0));
        assert!(s.touch.mv.is_none() && s.touch.guide.is_none());
        assert!(s.selected.is_some());
    }

    /// Shift locks the axis; the snap switch off leaves the drag free.
    #[test]
    fn ann_10_shift_locks_the_axis_and_snap_can_be_off() {
        let mut s = st();
        move_ready(&mut s);
        s.pointer_down(Pt::new(110.0, 110.0), NONE, 1, 6.0);
        s.pointer_move(Pt::new(150.0, 190.0), SHIFT, 6.0);
        let d = s.touch.mv.as_ref().unwrap().dest.unwrap();
        assert_eq!(d.x, 100.0, "the dominant (vertical) axis is kept");
        assert_eq!(s.touch.guide.as_ref().unwrap().axis, touchup::Axis::Column);
        s.pointer_move(Pt::new(150.0, 190.0), NONE, 6.0);
        let d = s.touch.mv.as_ref().unwrap().dest.unwrap();
        assert_eq!((d.x, d.y), (140.0, 180.0));
        assert!(s.touch.guide.is_none());
        s.pointer_up(Pt::new(150.0, 190.0), NONE, 6.0);
        let mut s = st();
        move_ready(&mut s);
        s.set_snap_on(false);
        s.pointer_down(Pt::new(110.0, 110.0), NONE, 1, 6.0);
        s.pointer_move(Pt::new(230.0, 113.0), NONE, 6.0);
        assert_eq!(s.touch.mv.as_ref().unwrap().dest.unwrap().y, 103.0);
        assert!(s.touch.guide.is_none());
    }

    /// ANN-10.AC2: Delete on a chosen source removes the contents (`to: null`).
    #[test]
    fn ann_10_ac2_delete_removes_the_contents() {
        let mut s = st();
        move_ready(&mut s);
        let e = s.delete_selected().expect("an edit");
        let Edit::Replace(ops) = e else { panic!() };
        assert!(matches!(&ops[0].kind, OpKind::MoveArea { to: None, .. }));
        assert!(s.touch.mv.is_none());
        assert!(validate_list(&ops).is_ok());
        // A removed area has nothing to drag; "Remove contents" on a moved area sets null.
        let mut s = st();
        move_ready(&mut s);
        s.pointer_down(Pt::new(110.0, 110.0), NONE, 1, 6.0);
        s.pointer_move(Pt::new(210.0, 110.0), NONE, 6.0);
        s.pointer_up(Pt::new(210.0, 110.0), NONE, 6.0);
        assert!(matches!(s.remove_contents(), Some(Edit::Replace(_))));
        assert!(matches!(&s.ops[0].kind, OpKind::MoveArea { to: None, .. }));
        assert!(s.remove_contents().is_none());
    }

    /// A press outside the chosen source starts a new source; a click that did not move the
    /// contents keeps the source; Esc drops it.
    #[test]
    fn ann_10_pending_source_rules() {
        let mut s = st();
        move_ready(&mut s);
        // A click inside without moving: still pending, no op.
        s.pointer_down(Pt::new(110.0, 110.0), NONE, 1, 6.0);
        assert!(s.pointer_up(Pt::new(110.0, 110.0), NONE, 6.0).is_empty());
        assert!(s.touch.mv.is_some() && s.ops.is_empty());
        assert!(s.busy());
        assert!(matches!(s.escape(), EscapeOutcome::Edits(e) if e.is_empty()));
        assert!(s.touch.mv.is_none());
        // Dragging an existing moved area moves it again with the snap rule.
        let mut s = st();
        move_ready(&mut s);
        s.pointer_down(Pt::new(110.0, 110.0), NONE, 1, 6.0);
        s.pointer_move(Pt::new(210.0, 140.0), NONE, 6.0);
        s.pointer_up(Pt::new(210.0, 140.0), NONE, 6.0);
        s.pointer_down(Pt::new(210.0, 140.0), NONE, 1, 6.0);
        // 25 px up: within the snap distance of the source row.
        s.pointer_move(Pt::new(210.0, 115.0), NONE, 6.0);
        let e = s.pointer_up(Pt::new(210.0, 115.0), NONE, 6.0);
        assert_eq!(e.len(), 1);
        let OpKind::MoveArea { to: Some(t), .. } = &s.ops[0].kind else {
            panic!()
        };
        assert_eq!((t.x, t.y), (200.0, 100.0));
    }

    // ---- Blueprint ----------------------------------------------------------------------

    fn blueprint_ready(s: &mut EditorState, n: usize) {
        let lines: Vec<Line> = (0..n)
            .map(|i| line(20.0, 20.0 + 30.0 * i as f64, 200.0, &format!("line {i}")))
            .collect();
        s.blueprint_open(&lines);
        assert!(s.blueprint_active());
    }

    /// ANN-11.AC1/AC3 (editor side): 20 lines, two kept by clicking them; Apply is one op,
    /// inserted before the arrow, with 18 boxes; nothing is stored before Apply.
    #[test]
    fn ann_11_apply_is_one_op_with_the_kept_lines_left_out() {
        let mut s = st();
        s.ops.push(AnnotationOp::new("a", arrow()));
        blueprint_ready(&mut s, 20);
        let live = s.render_ops();
        assert!(matches!(&live[0].kind, OpKind::Blueprint { boxes, .. } if boxes.len() == 20));
        assert!(s.pointer_down(Pt::new(30.0, 28.0), NONE, 1, 6.0).is_empty());
        s.pointer_up(Pt::new(30.0, 28.0), NONE, 6.0);
        s.pointer_down(Pt::new(30.0, 88.0), NONE, 1, 6.0);
        let info = s.blueprint_info().unwrap();
        assert_eq!((info.found, info.bars, info.kept), (20, 18, 2));
        assert_eq!(info.kept_rows.len(), 2);
        assert_eq!(s.ops.len(), 1, "nothing stored before Apply");
        s.pointer_move(Pt::new(30.0, 150.0), NONE, 6.0);
        assert!(s.blueprint_info().unwrap().hover.is_some());
        let e = s.blueprint_apply().expect("one edit");
        let Edit::Replace(ops) = e else { panic!() };
        assert!(validate_list(&ops).is_ok());
        assert_eq!(base_names(&s), ["blueprint", "arrow"]);
        let OpKind::Blueprint {
            boxes,
            colors,
            fills,
            ..
        } = &ops[0].kind
        else {
            panic!()
        };
        assert_eq!((boxes.len(), colors.len(), fills.len()), (18, 18, 18));
        assert!(!s.blueprint_active());
    }

    /// ANN-11.AC3: inside a draft each keep-list change is one step of the panel's own undo;
    /// on a stored Blueprint each change is one op update (one edit).
    #[test]
    fn ann_11_ac3_a_keep_list_change_is_one_step() {
        let mut s = st();
        blueprint_ready(&mut s, 5);
        assert!(s.blueprint_toggle(1).is_none(), "a draft stores nothing");
        assert!(s.blueprint_toggle(2).is_none());
        assert!(s.blueprint_undo_keep());
        assert_eq!(s.blueprint_info().unwrap().kept, 1);
        assert!(s.blueprint_undo_keep());
        assert!(!s.blueprint_undo_keep());
        s.blueprint_toggle(0);
        let Edit::Replace(_) = s.blueprint_apply().unwrap() else {
            panic!()
        };
        // The stored op: reopen, change the keep-list.
        s.set_tool(Tool::Select);
        assert!(s.blueprint_open_existing());
        let info = s.blueprint_info().unwrap();
        assert!(info.existing);
        assert_eq!((info.found, info.bars), (4, 4));
        let e = s.blueprint_toggle(0).expect("one op update");
        let Edit::Replace(ops) = e else { panic!() };
        assert!(matches!(&ops[0].kind, OpKind::Blueprint { boxes, .. } if boxes.len() == 3));
        assert!(
            !s.blueprint_undo_keep(),
            "the daemon's undo takes a stored change back"
        );
        // Style, colour and radius of a stored Blueprint are op updates too.
        assert!(matches!(
            s.set_blueprint_style(BlueprintStyle::Box),
            Some(Edit::Replace(_))
        ));
        assert!(s.set_blueprint_style(BlueprintStyle::Box).is_none());
        assert!(matches!(
            s.set_blueprint_color(BlueprintColor::Fixed(Color::rgb(0xCC, 0xCC, 0xCC))),
            Some(Edit::Replace(_))
        ));
        assert!(matches!(
            s.set_blueprint_radius(8.0),
            Some(Edit::Replace(_))
        ));
        // Keeping every line removes the op.
        for i in 1..4 {
            s.blueprint_toggle(i);
        }
        assert!(!s
            .ops
            .iter()
            .any(|o| matches!(o.kind, OpKind::Blueprint { .. })));
        assert!(!s.blueprint_active());
    }

    #[test]
    fn ann_11_cancel_and_esc_store_nothing_and_no_text_means_a_note() {
        let mut s = st();
        blueprint_ready(&mut s, 3);
        assert!(matches!(s.escape(), EscapeOutcome::Edits(e) if e.is_empty()));
        assert!(!s.blueprint_active() && s.ops.is_empty());
        blueprint_ready(&mut s, 3);
        s.blueprint_cancel();
        assert!(!s.blueprint_active() && s.ops.is_empty());
        s.blueprint_open(&[]);
        assert!(!s.blueprint_active());
        assert!(s.touch.notice.is_some());
    }

    /// A Blueprint op is not selectable by `[` / `]` (its panel edits it).
    #[test]
    fn a_blueprint_op_is_not_a_selection_target() {
        let mut s = st();
        blueprint_ready(&mut s, 2);
        s.blueprint_apply().unwrap();
        s.select_step(true);
        assert!(s.selected.is_none());
    }

    // ---- text selection (ANN-1.AC9 to AC13) -----------------------------------------------

    fn text_edit(text: &str) -> TextEdit {
        TextEdit {
            target: TextTarget::Text,
            existing: None,
            at: Pt::new(0.0, 0.0),
            text: text.into(),
            cursor: text.chars().count(),
            size: 24.0,
            color: Color::WHITE,
            outline: None,
            bg: None,
            marked: None,
            sel_all: false,
            anchor: None,
        }
    }

    /// ANN-1.AC9: a double click selects the word; typing replaces it and nothing else changes.
    #[test]
    fn ann_1_ac9_double_click_selects_a_word_and_typing_replaces_it() {
        let mut t = text_edit("Check the billing");
        t.select_word_at(12);
        assert_eq!(t.selected_text().as_deref(), Some("billing"));
        assert_eq!(t.selection(), Some(10..17));
        t.ime_replace(None, "invoice", false);
        assert_eq!(t.text, "Check the invoice");
        assert_eq!(t.selection(), None);
        // A double click on a space selects the space run; punctuation is its own word.
        let mut t = text_edit("a  b,c");
        t.select_word_at(2);
        assert_eq!(t.selected_text().as_deref(), Some("  "));
        t.select_word_at(4);
        assert_eq!(t.selected_text().as_deref(), Some(","));
    }

    /// QA E1: a triple click selects the whole line, not past a line break.
    #[test]
    fn triple_click_selects_the_line() {
        let mut t = text_edit("one two\nthree four");
        t.select_line_at(2);
        assert_eq!(t.selected_text().as_deref(), Some("one two"));
        t.select_line_at(10);
        assert_eq!(t.selected_text().as_deref(), Some("three four"));
        let mut t = text_edit("single line");
        t.select_line_at(4);
        assert_eq!(t.selected_text().as_deref(), Some("single line"));
    }

    /// ANN-1.AC10: Shift+Right extends by one character; Cmd/Ctrl+A selects all; copy gives the
    /// plain text, cut removes it, paste replaces the selection and is cut at 2000 characters.
    #[test]
    fn ann_1_ac10_keys_copy_cut_paste() {
        let mut s = st();
        s.set_tool(Tool::Text);
        s.text = Some(text_edit("Hello world"));
        s.text.as_mut().unwrap().cursor = 0;
        assert!(s.text_key("right", None, SHIFT));
        assert!(s.text_key("right", None, SHIFT));
        assert_eq!(s.text_copy().as_deref(), Some("He"));
        assert!(s.text_key("left", None, SHIFT));
        assert_eq!(s.text_copy().as_deref(), Some("H"));
        let all = Mods {
            secondary: true,
            ..NONE
        };
        assert!(s.text_key("a", None, all));
        assert_eq!(s.text_copy().as_deref(), Some("Hello world"));
        assert_eq!(s.text_cut().as_deref(), Some("Hello world"));
        assert_eq!(s.text.as_ref().unwrap().text, "");
        s.text_insert("pasted");
        s.text.as_mut().unwrap().select_all();
        s.text_insert("replaced");
        assert_eq!(s.text.as_ref().unwrap().text, "replaced");
        // The limit: a paste that would exceed 2000 characters is cut there.
        s.text.as_mut().unwrap().select_all();
        s.text_insert(&"x".repeat(2500));
        assert_eq!(s.text.as_ref().unwrap().text.chars().count(), 2000);
        s.text_insert("more");
        assert_eq!(s.text.as_ref().unwrap().text.chars().count(), 2000);
        // Nothing selected: copy is None (the clipboard is left alone).
        let n = s.text.as_ref().unwrap().text.chars().count();
        s.text.as_mut().unwrap().cursor = n;
        s.text.as_mut().unwrap().anchor = None;
        assert!(s.text_copy().is_none());
        // Shift+Home/End and plain arrows collapse.
        s.text.as_mut().unwrap().text = "ab\ncd".into();
        s.text.as_mut().unwrap().cursor = 4;
        s.text_key("home", None, SHIFT);
        assert_eq!(s.text_copy().as_deref(), Some("c"));
        s.text_key("left", None, NONE);
        assert!(s.text_copy().is_none());
        assert_eq!(s.text.as_ref().unwrap().cursor, 3);
    }

    /// ANN-1.AC11: a press inside the box and a drag that leaves the box keeps selecting until
    /// release (the view keeps feeding the pointer).
    #[test]
    fn ann_1_ac11_a_drag_selects_and_follows_outside_the_box() {
        let mut s = st();
        s.set_tool(Tool::Text);
        s.pointer_down(Pt::new(100.0, 100.0), NONE, 1, 6.0);
        type_text(&mut s, "Check the billing");
        let at = s.text.as_ref().unwrap().at;
        let frame = s.text_frame().unwrap();
        // Press near the start, drag far to the right and below, outside the box.
        s.pointer_down(Pt::new(at.x + 1.0, at.y + 4.0), NONE, 1, 6.0);
        assert!(s.text_drag, "the pointer is captured");
        s.pointer_move(
            Pt::new(frame.x + frame.w + 400.0, frame.y + 300.0),
            NONE,
            6.0,
        );
        assert_eq!(s.text_copy().as_deref(), Some("Check the billing"));
        s.pointer_move(Pt::new(at.x + 1.0, at.y + 4.0), NONE, 6.0);
        assert!(
            s.text_copy().is_none(),
            "back at the start: nothing selected"
        );
        s.pointer_move(Pt::new(frame.x + frame.w + 400.0, frame.y + 4.0), NONE, 6.0);
        assert_eq!(s.text_copy().as_deref(), Some("Check the billing"));
        s.pointer_up(Pt::new(frame.x + frame.w + 400.0, frame.y + 4.0), NONE, 6.0);
        assert!(!s.text_drag, "ended on release");
        assert!(s.text.is_some(), "the text is still being edited");
        // A double click on a word selects it (AC9, through the pointer).
        let mid = Pt::new(at.x + frame.w * 0.45, at.y + 8.0);
        s.pointer_down(mid, NONE, 2, 6.0);
        let w = s.text_copy().unwrap();
        assert!(!w.contains(' ') && !w.is_empty(), "a word: {w:?}");
        // A press outside the box commits the text (as before).
        let e = s.pointer_down(Pt::new(800.0, 500.0), NONE, 1, 6.0);
        assert!(
            s.text.as_ref().is_some_and(|t| t.text.is_empty()),
            "a new box opens there"
        );
        assert!(e.iter().any(|x| matches!(x, Edit::Append(_))));
    }

    /// ANN-1.AC13: composing replaces the selection; the composing range is marked.
    #[test]
    fn ann_1_ac13_composition_replaces_the_selection() {
        let mut t = text_edit("Tieng viet");
        t.select_word_at(7);
        assert_eq!(t.selected_text().as_deref(), Some("viet"));
        // Telex: the first draft replaces the selected word and is marked.
        t.ime_replace(None, "v", true);
        assert_eq!(t.text, "Tieng v");
        assert_eq!(t.marked, Some(6..7));
        t.ime_replace(None, "vi\u{1EC7}", true);
        assert_eq!(t.text, "Tieng vi\u{1EC7}");
        t.ime_replace(None, "vi\u{1EC7}t", false);
        assert_eq!(t.text, "Tieng vi\u{1EC7}t");
        assert_eq!(t.marked, None);
        // The platform sees the selection as a UTF-16 range.
        let mut t = text_edit("a\u{1F600}b");
        t.select_all();
        assert_eq!(t.selection_utf16(), 0..4);
    }

    /// The step label that opens with its whole text selected still works with the new model.
    #[test]
    fn a_whole_label_selection_collapses_with_the_arrows() {
        let mut t = text_edit("Step");
        t.sel_all = true;
        assert_eq!(t.selection(), Some(0..4));
        t.left();
        assert_eq!((t.cursor, t.selection()), (0, None));
        t.sel_all = true;
        t.right();
        assert_eq!(t.cursor, 4);
        t.sel_all = true;
        t.extend(false);
        assert_eq!(t.selection(), Some(0..3));
    }
}
