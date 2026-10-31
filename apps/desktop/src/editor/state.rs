//! The editor's tool state machine: drawing, selecting, moving, resizing, text entry, crop,
//! keyboard edits and the `Edit`s that go to the daemon (one `Edit` = one undo step,
//! ANN-3). No GPUI types here, so every interaction is unit tested.
//!
//! The local op list is optimistic: it is updated at once and mirrored to the daemon by the
//! session, which later reconciles ids (new ops get daemon ids) and history flags.

use cm_core::{visible_rect, AnnotationOp, Color, OpKind, Pt, Rc};
use cm_i18n::{tr, tr_ctx, trn};
use std::collections::HashMap;

use super::geom::{self, Handle, TextMetrics};
use super::keys::Mods;
use super::tools::{Param, ParamValue, Styles, Tool, ToolStyle};

/// One user-visible change, sent to the daemon as a single `annotate` call.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Add ops to the end (`annotate`, `replace: false`). Ids are minted by the daemon.
    Append(Vec<AnnotationOp>),
    /// Replace the whole op list (`replace: true`): move, resize, recolor, delete, crop. An
    /// empty list is "Revert to original".
    Replace(Vec<AnnotationOp>),
}

/// Text being typed on the canvas (ANN-1: click to place, type, Enter for a new line,
/// Esc to finish).
#[derive(Debug, Clone, PartialEq)]
pub struct TextEdit {
    /// The op being re-edited, or `None` for a new one.
    pub existing: Option<String>,
    pub at: Pt,
    pub text: String,
    /// Caret as a char index.
    pub cursor: usize,
    pub size: f64,
    pub color: Color,
    pub outline: Option<Color>,
    pub bg: Option<Color>,
    /// The draft an input method is composing (Telex, Pinyin, ...), as char indexes into `text`.
    pub marked: Option<std::ops::Range<usize>>,
}

impl TextEdit {
    fn byte(&self, char_idx: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_idx)
            .map_or(self.text.len(), |(b, _)| b)
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    /// UTF-16 length of the first `chars` characters (the platform addresses text in UTF-16).
    fn utf16_of(&self, chars: usize) -> usize {
        self.text.chars().take(chars).map(char::len_utf16).sum()
    }

    /// The char index at a UTF-16 offset (rounded up when it falls inside a surrogate pair).
    fn chars_of(&self, utf16: usize) -> usize {
        let mut units = 0;
        for (i, c) in self.text.chars().enumerate() {
            if units >= utf16 {
                return i;
            }
            units += c.len_utf16();
        }
        self.len()
    }

    /// The draft as a UTF-16 range, for the platform.
    pub fn marked_utf16(&self) -> Option<std::ops::Range<usize>> {
        let m = self.marked.as_ref()?;
        Some(self.utf16_of(m.start)..self.utf16_of(m.end))
    }

    /// The caret as a UTF-16 range (collapsed), for the platform.
    pub fn selection_utf16(&self) -> std::ops::Range<usize> {
        let c = self.utf16_of(self.cursor);
        c..c
    }

    /// The text in a UTF-16 range, and the range actually used.
    pub fn text_utf16(&self, range: std::ops::Range<usize>) -> (String, std::ops::Range<usize>) {
        let a = self.chars_of(range.start);
        let b = self.chars_of(range.end).max(a);
        let text: String = self.text.chars().skip(a).take(b - a).collect();
        (text, self.utf16_of(a)..self.utf16_of(b))
    }

    /// An input method put `new_text` in place of `range` (UTF-16; default: the draft, else the
    /// caret). With `mark` the new text is a draft the method will replace again; without, it is
    /// final. Returns whether the text changed.
    pub fn ime_replace(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        new_text: &str,
        mark: bool,
    ) -> bool {
        let clean: String = new_text
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .collect();
        let (a, b) = match range {
            Some(r) => (self.chars_of(r.start), self.chars_of(r.end)),
            None => match &self.marked {
                Some(m) => (m.start.min(self.len()), m.end.min(self.len())),
                None => (self.cursor, self.cursor),
            },
        };
        let b = b.max(a);
        let (ba, bb) = (self.byte(a), self.byte(b));
        let changed = self.text[ba..bb] != clean;
        self.text.replace_range(ba..bb, &clean);
        let n = clean.chars().count();
        self.cursor = a + n;
        self.marked = (mark && n > 0).then_some(a..a + n);
        changed
    }

    pub fn insert(&mut self, s: &str) {
        self.marked = None;
        let clean: String = s
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .collect();
        if clean.is_empty() {
            return;
        }
        let b = self.byte(self.cursor);
        self.text.insert_str(b, &clean);
        self.cursor += clean.chars().count();
    }

    pub fn backspace(&mut self) {
        self.marked = None;
        if self.cursor > 0 {
            self.cursor -= 1;
            let b = self.byte(self.cursor);
            self.text.remove(b);
        }
    }

    pub fn delete(&mut self) {
        self.marked = None;
        if self.cursor < self.len() {
            let b = self.byte(self.cursor);
            self.text.remove(b);
        }
    }

    pub fn left(&mut self) {
        self.marked = None;
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.marked = None;
        self.cursor = (self.cursor + 1).min(self.len());
    }

    pub fn home(&mut self) {
        self.marked = None;
        while self.cursor > 0 && self.text.chars().nth(self.cursor - 1) != Some('\n') {
            self.cursor -= 1;
        }
    }

    pub fn end(&mut self) {
        self.marked = None;
        while self.cursor < self.len() && self.text.chars().nth(self.cursor) != Some('\n') {
            self.cursor += 1;
        }
    }

    fn kind(&self) -> OpKind {
        OpKind::Text {
            at: self.at,
            text: self.text.clone(),
            size: self.size,
            color: self.color,
            outline: self.outline,
            bg: self.bg,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Interaction {
    Idle,
    Drawing {
        start: Pt,
        cur: Pt,
        shift: bool,
    },
    Dragging {
        id: String,
        start: Pt,
        orig: OpKind,
        moved: bool,
    },
    Resizing {
        id: String,
        handle: Handle,
        orig: OpKind,
        moved: bool,
    },
}

/// What Esc did.
#[derive(Debug, Clone, PartialEq)]
pub enum EscapeOutcome {
    Edits(Vec<Edit>),
    /// Nothing left to cancel: leave the editor.
    Leave,
}

/// Selection box and handles to paint.
#[derive(Debug, Clone, PartialEq)]
pub struct Chrome {
    pub bounds: Rc,
    pub handles: Vec<(Handle, Pt)>,
}

pub struct EditorState {
    pub image: (u32, u32),
    pub ops: Vec<AnnotationOp>,
    pub tool: Tool,
    pub styles: Styles,
    pub selected: Option<String>,
    pub interaction: Interaction,
    pub text: Option<TextEdit>,
    /// Auto-redact suggestions on show (ANN-2.AC4); not ops until accepted.
    pub proposals: Vec<super::redact::Proposal>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub hover: Option<Pt>,
    /// The style table changed since the last `take_styles_dirty` (persist it).
    styles_dirty: bool,
    clip: Vec<AnnotationOp>,
    paste_count: u32,
    tmp_seq: u64,
    pub metrics: TextMetrics,
}

/// The lowercase name of an annotation kind in the screen-reader summary ("rect 1 of 2").
fn op_name(kind: &OpKind) -> String {
    match kind {
        OpKind::Arrow { .. } => tr_ctx!("annotation kind", "arrow"),
        OpKind::Line { .. } => tr_ctx!("annotation kind", "line"),
        OpKind::Rect { .. } => tr_ctx!("annotation kind", "rect"),
        OpKind::Ellipse { .. } => tr_ctx!("annotation kind", "ellipse"),
        OpKind::Text { .. } => tr_ctx!("annotation kind", "text"),
        OpKind::Highlight { .. } => tr_ctx!("annotation kind", "highlight"),
        OpKind::Step { .. } => tr_ctx!("annotation kind", "step"),
        OpKind::Blur { .. } => tr_ctx!("annotation kind", "blur"),
        OpKind::Pixelate { .. } => tr_ctx!("annotation kind", "pixelate"),
        OpKind::Crop { .. } => tr_ctx!("annotation kind", "crop"),
    }
}

impl EditorState {
    pub fn new(image: (u32, u32), ops: Vec<AnnotationOp>, styles: Styles) -> Self {
        EditorState {
            image,
            ops,
            tool: Tool::Select,
            styles,
            selected: None,
            interaction: Interaction::Idle,
            text: None,
            proposals: Vec::new(),
            can_undo: false,
            can_redo: false,
            hover: None,
            styles_dirty: false,
            clip: Vec::new(),
            paste_count: 0,
            tmp_seq: 0,
            metrics: TextMetrics::default(),
        }
    }

    pub(super) fn tmp_id(&mut self) -> String {
        self.tmp_seq += 1;
        format!("~{}", self.tmp_seq)
    }

    pub fn is_tmp_id(id: &str) -> bool {
        id.starts_with('~')
    }

    pub fn take_styles_dirty(&mut self) -> bool {
        std::mem::take(&mut self.styles_dirty)
    }

    fn index_of(&self, id: &str) -> Option<usize> {
        self.ops.iter().position(|o| o.id == id)
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected.as_deref().and_then(|id| self.index_of(id))
    }

    /// The op whose handles are live: the selection, or the current crop in the Crop tool.
    fn active_index(&self) -> Option<usize> {
        if self.tool == Tool::Crop {
            return self
                .ops
                .iter()
                .rposition(|o| matches!(o.kind, OpKind::Crop { .. }));
        }
        self.selected_index()
    }

    pub fn image_rect(&self) -> Rc {
        Rc::new(0.0, 0.0, f64::from(self.image.0), f64::from(self.image.1))
    }

    /// The rect the canvas shows: the crop (clipped to the image), or the whole image. The
    /// Crop tool shows everything so the crop can be adjusted.
    pub fn content_rect(&self) -> Rc {
        let full = self.image_rect();
        if self.tool == Tool::Crop {
            return full;
        }
        match visible_rect(&self.ops) {
            Some(c) => {
                let x0 = c.x.max(0.0);
                let y0 = c.y.max(0.0);
                let x1 = (c.x + c.w).min(full.w);
                let y1 = (c.y + c.h).min(full.h);
                if x1 - x0 >= 1.0 && y1 - y0 >= 1.0 {
                    Rc::new(x0, y0, x1 - x0, y1 - y0)
                } else {
                    full
                }
            }
            None => full,
        }
    }

    pub fn crop_rect(&self) -> Option<Rc> {
        visible_rect(&self.ops)
    }

    /// Ops for the preview renderer: crops removed (the canvas applies them as a view),
    /// text being edited shown as typed. Blur and pixelate stay in the list, so previews are
    /// produced by the real renderer and never reveal pixels (ANN-2).
    pub fn render_ops(&self) -> Vec<AnnotationOp> {
        let editing = self.text.as_ref();
        let mut out = Vec::with_capacity(self.ops.len() + 1);
        let mut replaced = false;
        for o in &self.ops {
            if matches!(o.kind, OpKind::Crop { .. }) {
                continue;
            }
            match editing {
                Some(t) if t.existing.as_deref() == Some(o.id.as_str()) => {
                    replaced = true;
                    if !t.text.is_empty() {
                        out.push(AnnotationOp::new(o.id.clone(), t.kind()));
                    }
                }
                _ => out.push(o.clone()),
            }
        }
        if let Some(t) = editing {
            if !replaced && !t.text.is_empty() {
                out.push(AnnotationOp::new("~draft", t.kind()));
            }
        }
        out
    }

    pub fn step_count(&self) -> usize {
        self.ops
            .iter()
            .filter(|o| matches!(o.kind, OpKind::Step { .. }))
            .count()
    }

    /// ANN-1: the next step number, shown on the cursor.
    pub fn next_step_number(&self) -> u32 {
        self.step_count() as u32 + 1
    }

    /// The shape being dragged out, for the overlay.
    pub fn draft(&self) -> Option<OpKind> {
        match &self.interaction {
            Interaction::Drawing { start, cur, shift } => geom::shape_from_drag(
                self.tool,
                &self.styles.get(self.tool),
                *start,
                *cur,
                *shift,
                self.image,
            ),
            _ => None,
        }
    }

    pub fn chrome(&mut self) -> Option<Chrome> {
        let i = self.active_index()?;
        if self.text.is_some() {
            return None;
        }
        let kind = self.ops[i].kind.clone();
        let bounds = geom::bounds(&kind, &mut self.metrics);
        Some(Chrome {
            bounds,
            handles: geom::handles(&kind, &mut self.metrics),
        })
    }

    pub fn selected_kind(&self) -> Option<&OpKind> {
        self.selected_index().map(|i| &self.ops[i].kind)
    }

    /// Short description for assistive tech: "Arrow 2 of 5, selected".
    pub fn selection_summary(&self) -> String {
        let shown: Vec<usize> = (0..self.ops.len())
            .filter(|&i| !matches!(self.ops[i].kind, OpKind::Crop { .. }))
            .collect();
        match self.selected_index() {
            Some(i) => {
                let pos = shown.iter().position(|&x| x == i).map_or(0, |p| p + 1);
                tr!(
                    "{name} {pos} of {total}, selected",
                    name = op_name(&self.ops[i].kind),
                    pos = pos,
                    total = shown.len()
                )
            }
            None => trn!(
                "{n} annotation, none selected",
                "{n} annotations, none selected",
                shown.len()
            ),
        }
    }

    // ---- tool and selection -------------------------------------------------------------

    pub fn set_tool(&mut self, tool: Tool) -> Vec<Edit> {
        let edits = self.finish_text().into_iter().collect();
        self.cancel_interaction();
        if self.tool != tool {
            self.tool = tool;
            if let Some(k) = self.selected_kind() {
                if tool != Tool::Select && Tool::for_kind(k) != tool {
                    self.selected = None;
                }
            }
        }
        edits
    }

    pub fn select(&mut self, id: Option<String>) {
        self.selected = id.filter(|i| self.index_of(i).is_some());
    }

    fn selectable(&self) -> Vec<usize> {
        (0..self.ops.len())
            .filter(|&i| !matches!(self.ops[i].kind, OpKind::Crop { .. }))
            .collect()
    }

    /// `[` / `]`: cycle the selection through the ops (the keyboard way to pick an op).
    pub fn select_step(&mut self, forward: bool) {
        let list = self.selectable();
        if list.is_empty() {
            self.selected = None;
            return;
        }
        let cur = self
            .selected_index()
            .and_then(|i| list.iter().position(|&x| x == i));
        let next = match (cur, forward) {
            (None, true) => 0,
            (None, false) => list.len() - 1,
            (Some(c), true) => (c + 1) % list.len(),
            (Some(c), false) => (c + list.len() - 1) % list.len(),
        };
        self.selected = Some(self.ops[list[next]].id.clone());
        if self.tool != Tool::Select {
            self.tool = Tool::Select;
        }
    }

    // ---- pointer ------------------------------------------------------------------------

    /// `tol` is the pointer tolerance in image px (about 6 logical px).
    pub fn pointer_down(&mut self, p: Pt, mods: Mods, clicks: usize, tol: f64) -> Vec<Edit> {
        let mut edits: Vec<Edit> = self.finish_text().into_iter().collect();
        self.hover = Some(p);
        if let Some(i) = self.active_index() {
            let kind = self.ops[i].kind.clone();
            if let Some(h) = geom::hit_handle(&kind, p, tol * 1.6, &mut self.metrics) {
                self.interaction = Interaction::Resizing {
                    id: self.ops[i].id.clone(),
                    handle: h,
                    orig: kind,
                    moved: false,
                };
                return edits;
            }
        }
        match self.tool {
            Tool::Select => match geom::topmost_hit(&self.ops, p, tol, &mut self.metrics) {
                Some(i) => {
                    self.selected = Some(self.ops[i].id.clone());
                    if clicks >= 2 && matches!(self.ops[i].kind, OpKind::Text { .. }) {
                        self.begin_text_edit(i);
                    } else {
                        self.interaction = Interaction::Dragging {
                            id: self.ops[i].id.clone(),
                            start: p,
                            orig: self.ops[i].kind.clone(),
                            moved: false,
                        };
                    }
                }
                None => self.selected = None,
            },
            Tool::Text => {
                match geom::topmost_hit(&self.ops, p, tol, &mut self.metrics)
                    .filter(|&i| matches!(self.ops[i].kind, OpKind::Text { .. }))
                {
                    Some(i) => self.begin_text_edit(i),
                    None => self.begin_text_new(p),
                }
            }
            Tool::Step => edits.push(self.place_step(p)),
            _ => {
                self.selected = None;
                self.interaction = Interaction::Drawing {
                    start: p,
                    cur: p,
                    shift: mods.shift,
                };
            }
        }
        edits
    }

    pub fn pointer_move(&mut self, p: Pt, mods: Mods, tol: f64) {
        self.hover = Some(p);
        let image = self.image;
        let tm = &mut self.metrics;
        match &mut self.interaction {
            Interaction::Idle => {}
            Interaction::Drawing { cur, shift, .. } => {
                *cur = p;
                *shift = mods.shift;
            }
            Interaction::Dragging {
                id,
                start,
                orig,
                moved,
            } => {
                let (dx, dy) = (p.x - start.x, p.y - start.y);
                if !*moved && dx.hypot(dy) < tol / 2.0 {
                    return;
                }
                *moved = true;
                let kind = geom::translate(orig, dx, dy);
                if let Some(o) = self.ops.iter_mut().find(|o| o.id == *id) {
                    o.kind = kind;
                }
            }
            Interaction::Resizing {
                id,
                handle,
                orig,
                moved,
            } => {
                *moved = true;
                let kind = geom::resize(orig, *handle, p, mods.shift, tm);
                let kind = Self::clamp_crop(image, kind);
                if let Some(o) = self.ops.iter_mut().find(|o| o.id == *id) {
                    o.kind = kind;
                }
            }
        }
    }

    fn clamp_crop(image: (u32, u32), kind: OpKind) -> OpKind {
        match kind {
            OpKind::Crop { rect } => {
                let r = rect.normalized();
                let (w, h) = (f64::from(image.0), f64::from(image.1));
                let x0 = r.x.clamp(0.0, w);
                let y0 = r.y.clamp(0.0, h);
                let x1 = (r.x + r.w).clamp(0.0, w);
                let y1 = (r.y + r.h).clamp(0.0, h);
                OpKind::Crop {
                    rect: Rc::new(x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0)),
                }
            }
            other => other,
        }
    }

    pub fn pointer_up(&mut self, p: Pt, mods: Mods, tol: f64) -> Vec<Edit> {
        self.pointer_move(p, mods, tol);
        let inter = std::mem::replace(&mut self.interaction, Interaction::Idle);
        match inter {
            Interaction::Idle => Vec::new(),
            Interaction::Drawing { start, cur, shift } => {
                let style = self.styles.get(self.tool);
                let Some(kind) =
                    geom::shape_from_drag(self.tool, &style, start, cur, shift, self.image)
                else {
                    return Vec::new();
                };
                if matches!(kind, OpKind::Crop { .. }) {
                    return vec![self.set_crop(kind)];
                }
                let id = self.tmp_id();
                let op = AnnotationOp::new(id.clone(), kind);
                self.ops.push(op.clone());
                self.selected = Some(id);
                vec![Edit::Append(vec![op])]
            }
            Interaction::Dragging { moved, .. } | Interaction::Resizing { moved, .. } => {
                if moved {
                    vec![Edit::Replace(self.ops.clone())]
                } else {
                    Vec::new()
                }
            }
        }
    }

    /// Esc during a drag, or a lost mouse-up: put the op back.
    pub fn cancel_interaction(&mut self) {
        match std::mem::replace(&mut self.interaction, Interaction::Idle) {
            Interaction::Dragging { id, orig, .. } | Interaction::Resizing { id, orig, .. } => {
                if let Some(o) = self.ops.iter_mut().find(|o| o.id == id) {
                    o.kind = orig;
                }
            }
            _ => {}
        }
    }

    /// Sets the one crop op (the last crop wins, so older ones are dropped).
    fn set_crop(&mut self, kind: OpKind) -> Edit {
        let existing = self
            .ops
            .iter()
            .rev()
            .find(|o| matches!(o.kind, OpKind::Crop { .. }))
            .map(|o| o.id.clone());
        self.ops.retain(|o| !matches!(o.kind, OpKind::Crop { .. }));
        let id = existing.unwrap_or_else(|| self.tmp_id());
        self.ops.push(AnnotationOp::new(id, kind));
        Edit::Replace(self.ops.clone())
    }

    /// "Reset crop": drops the crop op.
    pub fn clear_crop(&mut self) -> Option<Edit> {
        self.crop_rect()?;
        self.ops.retain(|o| !matches!(o.kind, OpKind::Crop { .. }));
        Some(Edit::Replace(self.ops.clone()))
    }

    fn place_step(&mut self, p: Pt) -> Edit {
        let style = self.styles.get(Tool::Step);
        let id = self.tmp_id();
        let op = AnnotationOp::new(
            id.clone(),
            OpKind::Step {
                at: p,
                n: self.next_step_number(),
                size: style.size,
                color: style.color,
            },
        );
        self.ops.push(op.clone());
        self.selected = Some(id);
        Edit::Append(vec![op])
    }

    // ---- text ---------------------------------------------------------------------------

    fn begin_text_new(&mut self, p: Pt) {
        let s = self.styles.get(Tool::Text);
        self.selected = None;
        self.text = Some(TextEdit {
            existing: None,
            at: p,
            text: String::new(),
            cursor: 0,
            size: s.size,
            color: s.color,
            outline: Some(Color::BLACK),
            bg: None,
            marked: None,
        });
    }

    fn begin_text_edit(&mut self, i: usize) {
        if let OpKind::Text {
            at,
            text,
            size,
            color,
            outline,
            bg,
        } = self.ops[i].kind.clone()
        {
            self.interaction = Interaction::Idle;
            self.text = Some(TextEdit {
                existing: Some(self.ops[i].id.clone()),
                at,
                cursor: text.chars().count(),
                text,
                size,
                color,
                outline,
                bg,
                marked: None,
            });
        }
    }

    /// Commits the text being typed (Esc, click elsewhere, tool change). Empty text creates
    /// nothing, and removes an existing text op.
    pub fn finish_text(&mut self) -> Option<Edit> {
        let t = self.text.take()?;
        match t.existing.clone() {
            Some(id) => {
                let i = self.index_of(&id)?;
                if t.text.is_empty() {
                    self.ops.remove(i);
                    self.selected = None;
                    return Some(Edit::Replace(self.ops.clone()));
                }
                let kind = t.kind();
                if self.ops[i].kind == kind {
                    self.selected = Some(id);
                    return None;
                }
                self.ops[i].kind = kind;
                self.selected = Some(id);
                Some(Edit::Replace(self.ops.clone()))
            }
            None => {
                if t.text.is_empty() {
                    return None;
                }
                let id = self.tmp_id();
                let op = AnnotationOp::new(id.clone(), t.kind());
                self.ops.push(op.clone());
                self.selected = Some(id);
                Some(Edit::Append(vec![op]))
            }
        }
    }

    // ---- keyboard edits -----------------------------------------------------------------

    /// Delete/Backspace (ANN-1.AC2: deleting a step renumbers the rest, in the same step).
    pub fn delete_selected(&mut self) -> Option<Edit> {
        let i = self.selected_index()?;
        self.ops.remove(i);
        self.selected = None;
        geom::renumber_steps(&mut self.ops);
        Some(Edit::Replace(self.ops.clone()))
    }

    /// "Revert to original": clears every op as one undo step (ANN-3).
    pub fn revert(&mut self) -> Option<Edit> {
        if self.ops.is_empty() {
            return None;
        }
        self.ops.clear();
        self.selected = None;
        self.text = None;
        self.interaction = Interaction::Idle;
        Some(Edit::Replace(Vec::new()))
    }

    pub fn copy_selection(&mut self) -> bool {
        match self.selected_index() {
            Some(i) => {
                self.clip = vec![self.ops[i].clone()];
                self.paste_count = 0;
                true
            }
            None => false,
        }
    }

    /// Paste duplicates the copied ops at +10 px per paste (ANN-1).
    pub fn paste(&mut self) -> Option<Edit> {
        if self.clip.is_empty() {
            return None;
        }
        self.paste_count += 1;
        let off = 10.0 * f64::from(self.paste_count);
        let mut added = Vec::new();
        let mut last = None;
        for src in self.clip.clone() {
            let id = self.tmp_id();
            let mut kind = geom::translate(&src.kind, off, off);
            if let OpKind::Step { n, .. } = &mut kind {
                *n = self.next_step_number();
            }
            let op = AnnotationOp::new(id.clone(), kind);
            self.ops.push(op.clone());
            added.push(op);
            last = Some(id);
        }
        self.selected = last;
        Some(Edit::Append(added))
    }

    pub fn duplicate(&mut self) -> Option<Edit> {
        if self.copy_selection() {
            self.paste()
        } else {
            None
        }
    }

    /// Arrow keys: move by 1 px (10 with Shift); with Alt, resize.
    pub fn nudge(&mut self, dx: i32, dy: i32, resize: bool, big: bool) -> Option<Edit> {
        let i = self.selected_index()?;
        let step = if big { 10.0 } else { 1.0 };
        let (dx, dy) = (f64::from(dx) * step, f64::from(dy) * step);
        let kind = &self.ops[i].kind;
        let new = if resize {
            geom::resize_by(kind, dx, dy, &mut self.metrics)
        } else {
            geom::translate(kind, dx, dy)
        };
        self.ops[i].kind = new;
        Some(Edit::Replace(self.ops.clone()))
    }

    /// Enter on the canvas: a default-sized op of the active tool centered at `center`, so the
    /// whole editor works without a pointer.
    pub fn place_default(&mut self, center: Pt) -> Vec<Edit> {
        let s = (f64::from(self.image.0.min(self.image.1)) / 6.0).max(40.0);
        let style = self.styles.get(self.tool);
        let rect = |w: f64, h: f64| Rc::new(center.x - w / 2.0, center.y - h / 2.0, w, h);
        let kind = match self.tool {
            Tool::Arrow => OpKind::Arrow {
                from: Pt::new(center.x - s, center.y + s / 2.0),
                to: Pt::new(center.x + s, center.y - s / 2.0),
                color: style.color,
                width: style.width,
            },
            Tool::Line => OpKind::Line {
                from: Pt::new(center.x - s, center.y),
                to: Pt::new(center.x + s, center.y),
                color: style.color,
                width: style.width,
            },
            Tool::Rect => OpKind::Rect {
                rect: rect(s * 1.6, s),
                color: style.color,
                width: style.width,
                fill: None,
                radius: 0.0,
            },
            Tool::Ellipse => OpKind::Ellipse {
                rect: rect(s * 1.6, s),
                color: style.color,
                width: style.width,
            },
            Tool::Highlight => OpKind::Highlight {
                rect: rect(s * 2.0, s / 2.0),
                color: style.color,
                opacity: style.opacity,
            },
            Tool::Blur => OpKind::Blur {
                rect: rect(s * 2.0, s / 2.0),
                strength: style.strength,
            },
            Tool::Pixelate => OpKind::Pixelate {
                rect: rect(s * 2.0, s / 2.0),
                block: style.block,
            },
            Tool::Step => return vec![self.place_step(center)],
            Tool::Text => {
                self.finish_text();
                self.begin_text_new(center);
                return Vec::new();
            }
            Tool::Crop => {
                let w = f64::from(self.image.0);
                let h = f64::from(self.image.1);
                let kind = OpKind::Crop {
                    rect: Rc::new(w * 0.1, h * 0.1, w * 0.8, h * 0.8),
                };
                return vec![self.set_crop(kind)];
            }
            Tool::Select => return Vec::new(),
        };
        let id = self.tmp_id();
        let op = AnnotationOp::new(id.clone(), kind);
        self.ops.push(op.clone());
        self.selected = Some(id);
        vec![Edit::Append(vec![op])]
    }

    pub fn escape(&mut self) -> EscapeOutcome {
        if self.text.is_some() {
            return EscapeOutcome::Edits(self.finish_text().into_iter().collect());
        }
        if self.interaction != Interaction::Idle {
            self.cancel_interaction();
            return EscapeOutcome::Edits(Vec::new());
        }
        if self.selected.is_some() {
            self.selected = None;
            return EscapeOutcome::Edits(Vec::new());
        }
        if self.tool != Tool::Select {
            self.tool = Tool::Select;
            return EscapeOutcome::Edits(Vec::new());
        }
        EscapeOutcome::Leave
    }

    // ---- text keys ----------------------------------------------------------------------

    /// Routes a key to the text editor. Returns `true` if it was consumed (while text is
    /// open every key is, apart from Cmd-chords that are handled by the caller).
    pub fn text_key(&mut self, key: &str, key_char: Option<&str>, mods: Mods) -> bool {
        let Some(t) = self.text.as_mut() else {
            return false;
        };
        match key {
            "backspace" => t.backspace(),
            "delete" => t.delete(),
            "left" => t.left(),
            "right" => t.right(),
            "home" => t.home(),
            "end" => t.end(),
            "enter" => t.insert("\n"),
            "escape" => return false,
            _ => {
                if mods.secondary {
                    return false;
                }
                match key_char {
                    Some(c) => t.insert(c),
                    None => return false,
                }
            }
        }
        true
    }

    /// An input method committed or is composing text in the text being typed.
    pub fn text_ime(&mut self, range: Option<std::ops::Range<usize>>, s: &str, mark: bool) -> bool {
        self.text
            .as_mut()
            .is_some_and(|t| t.ime_replace(range, s, mark))
    }

    pub fn text_insert(&mut self, s: &str) {
        if let Some(t) = self.text.as_mut() {
            t.insert(s);
        }
    }

    // ---- inspector ----------------------------------------------------------------------

    /// Controls the inspector shows: those of the op being edited or selected, else the
    /// active tool's.
    pub fn inspector_params(&self) -> &'static [Param] {
        if self.text.is_some() {
            return Tool::Text.params();
        }
        match self.selected_kind() {
            Some(k) => geom::params_of(k),
            None => self.tool.params(),
        }
    }

    pub fn inspector_value(&self, p: Param) -> Option<ParamValue> {
        if let Some(t) = &self.text {
            return match p {
                Param::Color => Some(ParamValue::Color(t.color)),
                Param::TextSize => Some(ParamValue::TextSize(t.size)),
                _ => None,
            };
        }
        match self.selected_kind() {
            Some(k) => geom::param_value(k, p),
            None => self
                .tool
                .params()
                .contains(&p)
                .then(|| self.styles.get(self.tool).value(p)),
        }
    }

    /// What the inspector heading names.
    pub fn inspector_title(&self) -> String {
        if self.text.is_some() {
            return Tool::Text.label();
        }
        match self.selected_kind() {
            Some(k) => Tool::for_kind(k).label(),
            None => self.tool.label(),
        }
    }

    /// Applies a parameter change (ANN-1.AC5: one undo step, only the target op changes).
    pub fn set_param(&mut self, v: ParamValue) -> Option<Edit> {
        if let Some(t) = self.text.as_mut() {
            match v {
                ParamValue::Color(c) => t.color = c,
                ParamValue::TextSize(s) => t.size = s.clamp(8.0, 400.0),
                _ => return None,
            }
            self.styles.apply(Tool::Text, v);
            self.styles_dirty = true;
            return None;
        }
        if let Some(i) = self.selected_index() {
            let new = geom::with_param(&self.ops[i].kind, v)?;
            let tool = Tool::for_kind(&new);
            self.styles.apply(tool, v);
            self.styles_dirty = true;
            self.ops[i].kind = new;
            return Some(Edit::Replace(self.ops.clone()));
        }
        if self.tool.params().contains(&v.param()) {
            let tool = self.tool;
            self.styles.apply(tool, v);
            self.styles_dirty = true;
        }
        None
    }

    pub fn tool_style(&self) -> ToolStyle {
        self.styles.get(self.tool)
    }

    // ---- reconciliation with the daemon ---------------------------------------------------

    /// The daemon minted real ids for ops we created with temporary ones.
    pub fn apply_id_map(&mut self, map: &HashMap<String, String>) {
        for o in &mut self.ops {
            if let Some(n) = map.get(&o.id) {
                o.id = n.clone();
            }
        }
        if let Some(s) = &self.selected {
            if let Some(n) = map.get(s) {
                self.selected = Some(n.clone());
            }
        }
        if let Some(t) = &mut self.text {
            if let Some(e) = &t.existing {
                if let Some(n) = map.get(e) {
                    t.existing = Some(n.clone());
                }
            }
        }
        match &mut self.interaction {
            Interaction::Dragging { id, .. } | Interaction::Resizing { id, .. } => {
                if let Some(n) = map.get(id) {
                    *id = n.clone();
                }
            }
            _ => {}
        }
    }

    /// Adopts the daemon's op list (after undo, redo, or when all edits are confirmed).
    pub fn adopt_ops(&mut self, ops: Vec<AnnotationOp>) {
        self.ops = ops;
        if self
            .selected
            .as_deref()
            .is_some_and(|s| self.index_of(s).is_none())
        {
            self.selected = None;
        }
        if let Some(t) = &self.text {
            if t.existing
                .as_deref()
                .is_some_and(|e| self.index_of(e).is_none())
            {
                self.text = None;
            }
        }
        self.interaction = Interaction::Idle;
    }

    /// True while a pointer gesture or text entry is in progress (the session must not
    /// overwrite the local ops then).
    pub fn busy(&self) -> bool {
        self.text.is_some() || self.interaction != Interaction::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    const TOL: f64 = 4.0;

    fn st() -> EditorState {
        EditorState::new((1200, 800), Vec::new(), Styles::default())
    }

    fn drag(s: &mut EditorState, a: (f64, f64), b: (f64, f64), mods: Mods) -> Vec<Edit> {
        let mut edits = s.pointer_down(Pt::new(a.0, a.1), mods, 1, TOL);
        s.pointer_move(Pt::new((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), mods, TOL);
        edits.extend(s.pointer_up(Pt::new(b.0, b.1), mods, TOL));
        edits
    }

    fn only_append(edits: &[Edit]) -> &AnnotationOp {
        match edits {
            [Edit::Append(v)] if v.len() == 1 => &v[0],
            other => panic!("expected one append, got {other:?}"),
        }
    }

    /// ANN-1.AC1: an arrow dragged from (120, 300) to (380, 210) is one op with those
    /// coordinates (one edit = one undo step).
    #[test]
    fn drawing_an_arrow_makes_one_append_with_exact_coordinates() {
        let mut s = st();
        s.set_tool(Tool::Arrow);
        let edits = drag(&mut s, (120.0, 300.0), (380.0, 210.0), NONE);
        let op = only_append(&edits);
        assert_eq!(
            op.kind,
            OpKind::Arrow {
                from: Pt::new(120.0, 300.0),
                to: Pt::new(380.0, 210.0),
                color: Color::rgb(0xFF, 0x3B, 0x3B),
                width: 4.0
            }
        );
        assert_eq!(s.ops.len(), 1);
        assert_eq!(
            s.selected.as_deref(),
            Some(op.id.as_str()),
            "new op is selected"
        );
        assert!(EditorState::is_tmp_id(&op.id));
    }

    /// ANN-1.AC3: Shift while drawing a rectangle gives width == height.
    #[test]
    fn shift_drag_makes_a_square() {
        let mut s = st();
        s.set_tool(Tool::Rect);
        let edits = drag(&mut s, (100.0, 100.0), (300.0, 190.0), SHIFT);
        match &only_append(&edits).kind {
            OpKind::Rect { rect, .. } => assert_eq!(rect.w, rect.h),
            k => panic!("{k:?}"),
        }
    }

    /// ANN-1: a click without a drag draws nothing.
    #[test]
    fn click_without_drag_draws_nothing() {
        let mut s = st();
        s.set_tool(Tool::Ellipse);
        assert!(drag(&mut s, (50.0, 50.0), (51.0, 51.0), NONE).is_empty());
        assert!(s.ops.is_empty());
    }

    /// ANN-1.AC2: three step badges; deleting badge 2 renumbers to 1 and 2 in one edit.
    #[test]
    fn deleting_a_step_renumbers_in_one_edit() {
        let mut s = st();
        s.set_tool(Tool::Step);
        assert_eq!(s.next_step_number(), 1);
        for (i, x) in [100.0, 200.0, 300.0].into_iter().enumerate() {
            let edits = s.pointer_down(Pt::new(x, 100.0), NONE, 1, TOL);
            let op = only_append(&edits);
            assert!(matches!(op.kind, OpKind::Step { n, .. } if n == i as u32 + 1));
            s.pointer_up(Pt::new(x, 100.0), NONE, TOL);
        }
        assert_eq!(s.next_step_number(), 4);
        s.set_tool(Tool::Select);
        s.pointer_down(Pt::new(200.0, 100.0), NONE, 1, TOL);
        s.pointer_up(Pt::new(200.0, 100.0), NONE, TOL);
        let edit = s.delete_selected().unwrap();
        let Edit::Replace(ops) = edit else { panic!() };
        let ns: Vec<u32> = ops
            .iter()
            .filter_map(|o| match o.kind {
                OpKind::Step { n, .. } => Some(n),
                _ => None,
            })
            .collect();
        assert_eq!(ns, vec![1, 2]);
        assert_eq!(s.next_step_number(), 3);
    }

    /// ANN-1: select, move (one edit on release, not per mouse move) and delete.
    #[test]
    fn moving_an_op_is_one_replace_edit() {
        let mut s = st();
        s.set_tool(Tool::Rect);
        drag(&mut s, (100.0, 100.0), (200.0, 160.0), NONE);
        s.set_tool(Tool::Select);
        s.selected = None;
        let down = s.pointer_down(Pt::new(100.0, 130.0), NONE, 1, TOL);
        assert!(down.is_empty(), "grabbing sends nothing yet");
        s.pointer_move(Pt::new(130.0, 150.0), NONE, TOL);
        s.pointer_move(Pt::new(150.0, 180.0), NONE, TOL);
        let up = s.pointer_up(Pt::new(150.0, 180.0), NONE, TOL);
        let [Edit::Replace(ops)] = up.as_slice() else {
            panic!("{up:?}")
        };
        match &ops[0].kind {
            OpKind::Rect { rect, .. } => assert_eq!(*rect, Rc::new(150.0, 150.0, 100.0, 60.0)),
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn a_click_on_an_op_selects_without_an_edit_and_a_click_on_nothing_deselects() {
        let mut s = st();
        s.set_tool(Tool::Highlight);
        drag(&mut s, (10.0, 10.0), (110.0, 40.0), NONE);
        s.set_tool(Tool::Select);
        s.selected = None;
        assert!(s.pointer_down(Pt::new(50.0, 20.0), NONE, 1, TOL).is_empty());
        assert!(s.pointer_up(Pt::new(50.0, 20.0), NONE, TOL).is_empty());
        assert!(s.selected.is_some());
        s.pointer_down(Pt::new(900.0, 700.0), NONE, 1, TOL);
        s.pointer_up(Pt::new(900.0, 700.0), NONE, TOL);
        assert!(s.selected.is_none());
    }

    /// ANN-1: handles resize; a resize is also one edit.
    #[test]
    fn dragging_a_handle_resizes_in_one_edit() {
        let mut s = st();
        s.set_tool(Tool::Rect);
        drag(&mut s, (100.0, 100.0), (200.0, 160.0), NONE);
        s.set_tool(Tool::Select); // keeps the selection (same kind not required for Select)
        assert!(s.selected.is_some());
        s.pointer_down(Pt::new(200.0, 160.0), NONE, 1, TOL);
        s.pointer_move(Pt::new(260.0, 200.0), NONE, TOL);
        let up = s.pointer_up(Pt::new(260.0, 200.0), NONE, TOL);
        let [Edit::Replace(ops)] = up.as_slice() else {
            panic!()
        };
        match &ops[0].kind {
            OpKind::Rect { rect, .. } => assert_eq!(*rect, Rc::new(100.0, 100.0, 160.0, 100.0)),
            k => panic!("{k:?}"),
        }
    }

    /// ANN-1.AC5: changing the color of the selected op changes only that op, in one edit,
    /// and the tool remembers the color for the next shape.
    #[test]
    fn recolor_changes_only_the_selected_op_in_one_edit() {
        let mut s = st();
        s.set_tool(Tool::Rect);
        drag(&mut s, (10.0, 10.0), (60.0, 60.0), NONE);
        drag(&mut s, (100.0, 10.0), (160.0, 60.0), NONE);
        let first = s.ops[0].clone();
        let blue = Color::rgb(0x2E, 0x8B, 0xFF);
        let edit = s.set_param(ParamValue::Color(blue)).unwrap();
        let Edit::Replace(ops) = edit else { panic!() };
        assert_eq!(ops[0], first, "other op untouched");
        assert!(matches!(&ops[1].kind, OpKind::Rect { color, .. } if *color == blue));
        assert!(s.take_styles_dirty());
        assert_eq!(s.styles.get(Tool::Rect).color, blue);
        s.selected = None;
        let edits = drag(&mut s, (300.0, 300.0), (360.0, 360.0), NONE);
        assert!(matches!(&only_append(&edits).kind, OpKind::Rect { color, .. } if *color == blue));
    }

    #[test]
    fn param_with_nothing_selected_sets_the_tool_style_without_an_edit() {
        let mut s = st();
        s.set_tool(Tool::Blur);
        assert!(s.set_param(ParamValue::Strength(20)).is_none());
        assert_eq!(s.styles.get(Tool::Blur).strength, 20);
        assert!(
            s.set_param(ParamValue::Color(Color::WHITE)).is_none(),
            "no color on blur"
        );
        assert_eq!(s.inspector_params(), &[Param::Strength]);
        assert_eq!(
            s.inspector_value(Param::Strength),
            Some(ParamValue::Strength(20))
        );
    }

    /// ANN-2: blur and pixelate are ops over the original; the preview ops keep them so the
    /// real renderer produces the redaction (no pixel is ever shown unredacted by an overlay).
    #[test]
    fn redaction_ops_stay_in_the_render_list_and_drafts_are_outline_only() {
        let mut s = st();
        s.set_tool(Tool::Blur);
        s.pointer_down(Pt::new(10.0, 10.0), NONE, 1, TOL);
        s.pointer_move(Pt::new(90.0, 40.0), NONE, TOL);
        let draft = s.draft().unwrap();
        assert!(draft.is_redaction());
        assert!(
            s.render_ops().is_empty(),
            "the draft is not rendered, only outlined"
        );
        let up = s.pointer_up(Pt::new(90.0, 40.0), NONE, TOL);
        assert_eq!(up.len(), 1);
        assert!(matches!(
            s.render_ops()[0].kind,
            OpKind::Blur { strength: 12, .. }
        ));
        s.set_tool(Tool::Pixelate);
        drag(&mut s, (200.0, 10.0), (260.0, 40.0), NONE);
        assert!(matches!(
            s.render_ops()[1].kind,
            OpKind::Pixelate { block: 12, .. }
        ));
    }

    /// ANN-1.AC4 / non-destructive: crop sets the visible rect, keeps other ops, and only
    /// one crop op ever exists (the last wins).
    #[test]
    fn crop_is_one_replace_edit_and_replaces_the_previous_crop() {
        let mut s = st();
        s.set_tool(Tool::Rect);
        drag(&mut s, (900.0, 600.0), (1000.0, 700.0), NONE);
        s.set_tool(Tool::Crop);
        let edits = drag(&mut s, (0.0, 0.0), (600.0, 400.0), NONE);
        let [Edit::Replace(ops)] = edits.as_slice() else {
            panic!("{edits:?}")
        };
        assert_eq!(ops.len(), 2, "the rect outside the crop is kept");
        assert_eq!(s.crop_rect(), Some(Rc::new(0.0, 0.0, 600.0, 400.0)));
        // Outside the Crop tool the canvas shows only the crop; in it, everything.
        s.set_tool(Tool::Select);
        assert_eq!(s.content_rect(), Rc::new(0.0, 0.0, 600.0, 400.0));
        s.set_tool(Tool::Crop);
        assert_eq!(s.content_rect(), s.image_rect());
        // Cropping again does not stack crop ops.
        drag(&mut s, (100.0, 100.0), (500.0, 300.0), NONE);
        assert_eq!(
            s.ops
                .iter()
                .filter(|o| matches!(o.kind, OpKind::Crop { .. }))
                .count(),
            1
        );
        // The preview list never contains the crop (it is a view, the renderer would cut).
        assert!(s
            .render_ops()
            .iter()
            .all(|o| !matches!(o.kind, OpKind::Crop { .. })));
        assert!(s.clear_crop().is_some());
        assert_eq!(s.crop_rect(), None);
    }

    #[test]
    fn crop_handles_resize_the_existing_crop_and_stay_inside_the_image() {
        let mut s = st();
        s.set_tool(Tool::Crop);
        drag(&mut s, (100.0, 100.0), (500.0, 400.0), NONE);
        s.pointer_down(Pt::new(500.0, 400.0), NONE, 1, TOL); // SE handle
        s.pointer_move(Pt::new(5000.0, 5000.0), NONE, TOL);
        let up = s.pointer_up(Pt::new(5000.0, 5000.0), NONE, TOL);
        assert_eq!(up.len(), 1);
        assert_eq!(s.crop_rect(), Some(Rc::new(100.0, 100.0, 1100.0, 700.0)));
    }

    /// ANN-1: text — click to place, type, Enter for a new line, Esc to finish; defaults 24 px
    /// white with a black outline.
    #[test]
    fn text_flow_click_type_enter_escape() {
        let mut s = st();
        s.set_tool(Tool::Text);
        assert!(s
            .pointer_down(Pt::new(130.0, 320.0), NONE, 1, TOL)
            .is_empty());
        for c in "Click".chars() {
            assert!(s.text_key(&c.to_string(), Some(&c.to_string()), NONE));
        }
        assert!(s.text_key("enter", None, NONE));
        for c in "Save".chars() {
            s.text_key(&c.to_string(), Some(&c.to_string()), NONE);
        }
        // Live: the draft shows in the preview list before it is committed.
        assert!(
            matches!(&s.render_ops()[0].kind, OpKind::Text { text, .. } if text == "Click\nSave")
        );
        assert!(s.ops.is_empty());
        let EscapeOutcome::Edits(edits) = s.escape() else {
            panic!()
        };
        let op = only_append(&edits);
        assert_eq!(
            op.kind,
            OpKind::Text {
                at: Pt::new(130.0, 320.0),
                text: "Click\nSave".into(),
                size: 24.0,
                color: Color::WHITE,
                outline: Some(Color::BLACK),
                bg: None
            }
        );
        assert!(s.text.is_none());
    }

    #[test]
    fn empty_text_creates_nothing_and_clearing_existing_text_deletes_it() {
        let mut s = st();
        s.set_tool(Tool::Text);
        s.pointer_down(Pt::new(10.0, 10.0), NONE, 1, TOL);
        assert!(s.finish_text().is_none());
        assert!(s.ops.is_empty());
        // Create one, then re-edit it (double click with Select) and wipe it.
        s.pointer_down(Pt::new(10.0, 10.0), NONE, 1, TOL);
        s.text_key("h", Some("h"), NONE);
        s.finish_text();
        s.set_tool(Tool::Select);
        let b = geom::bounds(&s.ops[0].kind.clone(), &mut s.metrics);
        let c = Pt::new(b.x + b.w / 2.0, b.y + b.h / 2.0);
        s.pointer_down(c, NONE, 2, TOL);
        assert_eq!(s.text.as_ref().map(|t| t.text.as_str()), Some("h"));
        s.text_key("backspace", None, NONE);
        let edit = s.finish_text().unwrap();
        assert_eq!(edit, Edit::Replace(Vec::new()));
    }

    #[test]
    fn text_editing_caret_and_unicode() {
        let mut t = TextEdit {
            existing: None,
            at: Pt::new(0.0, 0.0),
            text: String::new(),
            cursor: 0,
            size: 24.0,
            color: Color::WHITE,
            outline: None,
            bg: None,
            marked: None,
        };
        t.insert("a\u{4F60}\u{597D}");
        t.left();
        t.insert("X");
        assert_eq!(t.text, "a\u{4F60}X\u{597D}");
        t.backspace();
        t.delete();
        assert_eq!(t.text, "a\u{4F60}");
        t.home();
        assert_eq!(t.cursor, 0);
        t.end();
        assert_eq!(t.cursor, 2);
        t.insert("\u{1F600}\n");
        assert_eq!(t.text.chars().count(), 4);
    }

    /// ANN-1: copy/paste duplicates with a +10 px offset, cumulative per paste; pasted steps
    /// take the next number.
    #[test]
    fn paste_offsets_by_10px_and_renumbers_steps() {
        let mut s = st();
        s.set_tool(Tool::Rect);
        drag(&mut s, (100.0, 100.0), (150.0, 140.0), NONE);
        assert!(s.copy_selection());
        let Edit::Append(a) = s.paste().unwrap() else {
            panic!()
        };
        assert!(
            matches!(&a[0].kind, OpKind::Rect { rect, .. } if *rect == Rc::new(110.0, 110.0, 50.0, 40.0))
        );
        let Edit::Append(b) = s.paste().unwrap() else {
            panic!()
        };
        assert!(matches!(&b[0].kind, OpKind::Rect { rect, .. } if rect.x == 120.0));
        assert_eq!(s.ops.len(), 3);
        s.set_tool(Tool::Step);
        s.pointer_down(Pt::new(5.0, 5.0), NONE, 1, TOL);
        s.pointer_up(Pt::new(5.0, 5.0), NONE, TOL);
        assert!(s.copy_selection());
        let Edit::Append(c) = s.paste().unwrap() else {
            panic!()
        };
        assert!(matches!(c[0].kind, OpKind::Step { n: 2, .. }));
    }

    #[test]
    fn nudge_moves_by_one_or_ten_and_alt_resizes() {
        let mut s = st();
        s.set_tool(Tool::Rect);
        drag(&mut s, (100.0, 100.0), (150.0, 140.0), NONE);
        s.nudge(1, 0, false, false).unwrap();
        s.nudge(0, 1, false, true).unwrap();
        assert!(
            matches!(&s.ops[0].kind, OpKind::Rect { rect, .. } if *rect == Rc::new(101.0, 110.0, 50.0, 40.0))
        );
        s.nudge(1, 1, true, false).unwrap();
        assert!(
            matches!(&s.ops[0].kind, OpKind::Rect { rect, .. } if rect.w == 51.0 && rect.h == 41.0)
        );
        s.selected = None;
        assert!(s.nudge(1, 0, false, false).is_none());
    }

    /// Keyboard-only drawing: Enter places a default op of the active tool and selects it.
    #[test]
    fn place_default_creates_a_selected_op_for_each_tool() {
        for tool in [
            Tool::Arrow,
            Tool::Line,
            Tool::Rect,
            Tool::Ellipse,
            Tool::Highlight,
            Tool::Blur,
            Tool::Pixelate,
            Tool::Step,
        ] {
            let mut s = st();
            s.set_tool(tool);
            let edits = s.place_default(Pt::new(600.0, 400.0));
            assert_eq!(edits.len(), 1, "{tool:?}");
            assert!(s.selected.is_some());
            assert!(cm_core::validate_op(0, &s.ops[0]).is_ok(), "{tool:?}");
        }
        let mut s = st();
        s.set_tool(Tool::Text);
        assert!(s.place_default(Pt::new(10.0, 10.0)).is_empty());
        assert!(s.text.is_some(), "text starts editing");
    }

    /// Esc peels back one layer at a time and finally leaves.
    #[test]
    fn escape_cancels_then_deselects_then_selects_the_pointer_then_leaves() {
        let mut s = st();
        s.set_tool(Tool::Rect);
        s.pointer_down(Pt::new(10.0, 10.0), NONE, 1, TOL);
        s.pointer_move(Pt::new(80.0, 80.0), NONE, TOL);
        assert_eq!(s.escape(), EscapeOutcome::Edits(Vec::new()));
        assert!(s.ops.is_empty() && s.interaction == Interaction::Idle);
        drag(&mut s, (10.0, 10.0), (80.0, 80.0), NONE);
        assert!(s.selected.is_some());
        s.escape();
        assert!(s.selected.is_none());
        s.escape();
        assert_eq!(s.tool, Tool::Select);
        assert_eq!(s.escape(), EscapeOutcome::Leave);
    }

    #[test]
    fn cancelling_a_move_restores_the_op() {
        let mut s = st();
        s.set_tool(Tool::Rect);
        drag(&mut s, (100.0, 100.0), (200.0, 160.0), NONE);
        let before = s.ops[0].clone();
        s.set_tool(Tool::Select);
        s.pointer_down(Pt::new(100.0, 130.0), NONE, 1, TOL);
        s.pointer_move(Pt::new(300.0, 300.0), NONE, TOL);
        assert_ne!(s.ops[0], before);
        s.escape();
        assert_eq!(s.ops[0], before);
    }

    /// ANN-3: "Revert to original" clears every op as one edit, once.
    #[test]
    fn revert_clears_everything_in_one_replace() {
        let mut s = st();
        assert!(s.revert().is_none(), "nothing to revert");
        s.set_tool(Tool::Rect);
        drag(&mut s, (10.0, 10.0), (60.0, 60.0), NONE);
        drag(&mut s, (100.0, 10.0), (160.0, 60.0), NONE);
        assert_eq!(s.revert(), Some(Edit::Replace(Vec::new())));
        assert!(s.ops.is_empty() && s.selected.is_none());
    }

    #[test]
    fn id_map_rewrites_ops_and_selection() {
        let mut s = st();
        s.set_tool(Tool::Rect);
        drag(&mut s, (10.0, 10.0), (60.0, 60.0), NONE);
        let tmp = s.ops[0].id.clone();
        let map = HashMap::from([(tmp, "op_9".to_owned())]);
        s.apply_id_map(&map);
        assert_eq!(s.ops[0].id, "op_9");
        assert_eq!(s.selected.as_deref(), Some("op_9"));
    }

    /// I18N P2: one annotation reads "1 annotation".
    #[test]
    fn one_annotation_is_singular() {
        let mut s = st();
        s.set_tool(Tool::Rect);
        drag(&mut s, (10.0, 10.0), (60.0, 60.0), NONE);
        s.select(None);
        assert_eq!(s.selection_summary(), "1 annotation, none selected");
    }

    #[test]
    fn selection_cycles_with_brackets_and_is_described_for_screen_readers() {
        let mut s = st();
        s.set_tool(Tool::Rect);
        drag(&mut s, (10.0, 10.0), (60.0, 60.0), NONE);
        drag(&mut s, (100.0, 10.0), (160.0, 60.0), NONE);
        s.select(None);
        assert_eq!(s.selection_summary(), "2 annotations, none selected");
        // I18N P2: the count is a plural message and the kind name a placeholder.
        cm_i18n::with_language(cm_i18n::Lang::Vi, || {
            assert!(s.selection_summary().contains('2'));
        });
        s.select_step(true);
        assert_eq!(s.selection_summary(), "rect 1 of 2, selected");
        s.select_step(true);
        s.select_step(true);
        assert_eq!(s.selection_summary(), "rect 1 of 2, selected");
        s.select_step(false);
        assert_eq!(s.selection_summary(), "rect 2 of 2, selected");
    }

    #[test]
    fn switching_tool_deselects_a_mismatched_op_but_not_for_select() {
        let mut s = st();
        s.set_tool(Tool::Rect);
        drag(&mut s, (10.0, 10.0), (60.0, 60.0), NONE);
        s.set_tool(Tool::Arrow);
        assert!(s.selected.is_none());
        s.set_tool(Tool::Rect);
        drag(&mut s, (10.0, 10.0), (60.0, 60.0), NONE);
        s.set_tool(Tool::Select);
        assert!(s.selected.is_some());
    }

    #[test]
    fn adopt_ops_drops_a_selection_that_no_longer_exists() {
        let mut s = st();
        s.set_tool(Tool::Rect);
        drag(&mut s, (10.0, 10.0), (60.0, 60.0), NONE);
        s.adopt_ops(Vec::new());
        assert!(s.selected.is_none());
    }

    fn blank_text() -> TextEdit {
        TextEdit {
            existing: None,
            at: Pt::new(0.0, 0.0),
            text: String::new(),
            cursor: 0,
            size: 24.0,
            color: Color::WHITE,
            outline: None,
            bg: None,
            marked: None,
        }
    }

    #[test]
    fn a_telex_draft_is_replaced_until_it_is_committed() {
        // "Vieejt": the engine commits "Vi", then drafts "e", "ê", "ệ" and commits "ệt".
        let mut t = blank_text();
        t.ime_replace(None, "Vi", false);
        t.ime_replace(None, "e", true);
        assert_eq!((t.text.as_str(), t.marked_utf16()), ("Vie", Some(2..3)));
        t.ime_replace(None, "\u{ea}", true);
        t.ime_replace(None, "\u{1ec7}", true);
        assert_eq!(t.text, "Vi\u{1ec7}");
        t.ime_replace(None, "\u{1ec7}t", false);
        assert_eq!(t.text, "Vi\u{1ec7}t");
        assert_eq!((t.cursor, t.marked_utf16()), (4, None));
    }

    #[test]
    fn a_draft_goes_where_the_caret_is_and_utf16_offsets_follow_emoji() {
        let mut t = blank_text();
        t.insert("a\u{1F600}b");
        t.left();
        // Caret between the emoji (2 UTF-16 units) and b.
        assert_eq!(t.selection_utf16(), 3..3);
        t.ime_replace(None, "x", true);
        assert_eq!(t.text, "a\u{1F600}xb");
        assert_eq!(t.marked_utf16(), Some(3..4));
        assert_eq!(t.text_utf16(1..3), ("\u{1F600}".to_owned(), 1..3));
        // An explicit range replaces what is there.
        t.ime_replace(Some(3..4), "yz", false);
        assert_eq!(t.text, "a\u{1F600}yzb");
        assert_eq!(t.marked_utf16(), None);
    }

    #[test]
    fn typing_keys_or_moving_ends_a_draft() {
        let mut t = blank_text();
        t.ime_replace(None, "ab", true);
        t.left();
        assert_eq!(t.marked_utf16(), None);
        t.ime_replace(None, "c", true);
        t.insert("d");
        assert_eq!(t.marked_utf16(), None);
        // An empty draft is no draft; control characters are dropped.
        t.ime_replace(None, "", true);
        assert_eq!(t.marked_utf16(), None);
        t.ime_replace(None, "\u{1b}q", false);
        assert!(t.text.contains('q') && !t.text.contains('\u{1b}'));
    }
}
