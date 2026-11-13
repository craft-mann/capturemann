//! The editor's tool state machine: drawing, selecting, moving, resizing, text entry, crop,
//! keyboard edits and the `Edit`s that go to the daemon (one `Edit` = one undo step,
//! ANN-3). No GPUI types here, so every interaction is unit tested.
//!
//! The local op list is optimistic: it is updated at once and mirrored to the daemon by the
//! session, which later reconciles ids (new ops get daemon ids) and history flags.

use cm_core::annotation::StampKind;
use cm_core::annotation::{
    step_label, CalloutStyle, SpotMode, SpotShape, SpotShapeKind, StepStyle, MAX_LABEL_CHARS,
    MAX_SPOTLIGHT_SHAPES,
};
use cm_core::{visible_rect, AnnotationOp, Color, OpKind, Pt, Rc};
use cm_i18n::{tr, tr_ctx, trn};
use std::collections::HashMap;

use super::geom::{self, Handle, TextMetrics};
use super::keys::Mods;
use super::palette::Faces;
use super::tools::{Param, ParamValue, StampWord, Styles, Tool, ToolStyle};

/// One user-visible change, sent to the daemon as a single `annotate` call.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Add ops to the end (`annotate`, `replace: false`). Ids are minted by the daemon.
    Append(Vec<AnnotationOp>),
    /// Replace the whole op list (`replace: true`): move, resize, recolor, delete, crop. An
    /// empty list is "Revert to original".
    Replace(Vec<AnnotationOp>),
}

/// What the text being typed belongs to. Callouts and step labels use the same input method
/// as the Text tool (ANN-4.AC5, ANN-6), so Telex, VNI and Pinyin compose in them too.
#[derive(Debug, Clone, PartialEq)]
pub enum TextTarget {
    /// A text annotation.
    Text,
    /// A callout; the op is the base its text, size and colour are written onto.
    Callout(OpKind),
    /// The label of a step badge (ANN-6); one line.
    Step(OpKind),
}

/// Text being typed on the canvas (ANN-1: click to place, type, Enter for a new line,
/// Esc to finish).
#[derive(Debug, Clone, PartialEq)]
pub struct TextEdit {
    pub target: TextTarget,
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

    /// The op this edit would store. A callout keeps its stored box here: the grown box is
    /// written by [`EditorState::finish_text`] (ANN-4.AC5).
    fn kind(&self) -> OpKind {
        match &self.target {
            TextTarget::Callout(OpKind::Callout {
                rect,
                tail,
                bg,
                radius,
                style,
                ..
            }) => OpKind::Callout {
                rect: *rect,
                tail: *tail,
                text: self.text.clone(),
                size: self.size,
                color: self.color,
                bg: *bg,
                radius: *radius,
                style: *style,
            },
            TextTarget::Step(OpKind::Step {
                at,
                n,
                size,
                color,
                style,
                start,
                ..
            }) => {
                let label: String = self.text.trim().chars().take(MAX_LABEL_CHARS).collect();
                OpKind::Step {
                    at: *at,
                    n: *n,
                    size: *size,
                    color: *color,
                    style: *style,
                    start: *start,
                    text: (!label.is_empty()).then_some(label),
                }
            }
            _ => OpKind::Text {
                at: self.at,
                text: self.text.clone(),
                size: self.size,
                color: self.color,
                outline: self.outline,
                bg: self.bg,
            },
        }
    }

    /// A step label is one line: Enter commits it.
    pub fn single_line(&self) -> bool {
        matches!(self.target, TextTarget::Step(_))
    }

    /// An empty callout is still a box (ANN-4 edge cases); empty text is not an annotation.
    fn keeps_when_empty(&self) -> bool {
        matches!(self.target, TextTarget::Callout(_) | TextTarget::Step(_))
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

/// What the stamp picker shows (see [`EditorState::stamp_info`]).
#[derive(Debug, Clone, PartialEq)]
pub struct StampInfo {
    pub kind: StampKind,
    /// The word of a label stamp.
    pub word: Option<StampWord>,
    pub has_op: bool,
}

/// One row of the Steps panel.
#[derive(Debug, Clone, PartialEq)]
pub struct StepRow {
    pub id: String,
    /// What the badge reads (`3`, `C`, `c`).
    pub badge: String,
    /// The label, empty when there is none.
    pub text: String,
}

/// What the Steps panel shows (see [`EditorState::steps_info`]).
#[derive(Debug, Clone, PartialEq)]
pub struct StepsInfo {
    pub style: StepStyle,
    pub start: u32,
    /// The first number written in the current style (`3`, `C`, `c`), as the "Start at" field shows it.
    pub start_label: String,
    pub rows: Vec<StepRow>,
    /// The row of the selected step.
    pub selected: Option<usize>,
    /// A label is being typed on the canvas.
    pub editing: bool,
}

/// What the Spotlight panel shows (see [`EditorState::spot_info`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SpotInfo {
    /// The shape of each hole, in order.
    pub holes: Vec<SpotShapeKind>,
    /// The hole the handles act on.
    pub selected: usize,
    pub mode: SpotMode,
    pub dim: f64,
    /// The spotlight exists as an op (not just the settings of the next one).
    pub has_op: bool,
    /// Eight holes: no more can be added.
    pub at_limit: bool,
    /// The shape the next drag makes.
    pub next_shape: SpotShapeKind,
}

/// What the Callout panel shows (see [`EditorState::callout_info`]).
#[derive(Debug, Clone, PartialEq)]
pub struct CalloutInfo {
    pub text: String,
    pub style: CalloutStyle,
    /// The text is being typed on the canvas.
    pub editing: bool,
    /// The callout exists as an op (not just the settings of the next one).
    pub has_op: bool,
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
    /// The hole of the selected spotlight that handles, moves and Alt+arrows act on (ANN-7).
    pub spot_sel: usize,
    /// The tool each palette group's button shows (remembered per user).
    pub faces: Faces,
    faces_dirty: bool,
    clip: Vec<AnnotationOp>,
    paste_count: u32,
    tmp_seq: u64,
    pub metrics: TextMetrics,
    /// ANN-5: a rectangle the Select text tool just finished dragging; the view takes it with
    /// [`EditorState::take_text_region`] and asks the daemon for its text. Never an op.
    text_region: Option<Rc>,
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
        OpKind::Callout { .. } => tr_ctx!("annotation kind", "callout"),
        OpKind::Spotlight { .. } => tr_ctx!("annotation kind", "spotlight"),
        OpKind::Stamp { .. } => tr_ctx!("annotation kind", "stamp"),
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
            spot_sel: 0,
            faces: Faces::default(),
            faces_dirty: false,
            clip: Vec::new(),
            paste_count: 0,
            tmp_seq: 0,
            metrics: TextMetrics::default(),
            text_region: None,
        }
    }

    pub(super) fn tmp_id(&mut self) -> String {
        self.tmp_seq += 1;
        format!("~{}", self.tmp_seq)
    }

    pub fn is_tmp_id(id: &str) -> bool {
        id.starts_with('~')
    }

    /// The rectangle (image px) the Select text tool finished dragging, once.
    pub fn take_text_region(&mut self) -> Option<Rc> {
        self.text_region.take()
    }

    pub fn take_styles_dirty(&mut self) -> bool {
        std::mem::take(&mut self.styles_dirty)
    }

    /// The palette faces changed since the last call (persist them).
    pub fn take_faces_dirty(&mut self) -> bool {
        std::mem::take(&mut self.faces_dirty)
    }

    /// Makes `tool` the active tool and the face of its palette group. Every path that changes
    /// the tool goes through here, so the keys, the buttons and the group menu agree.
    fn activate(&mut self, tool: Tool) {
        self.tool = tool;
        if tool.in_palette() && self.faces.note(tool) {
            self.faces_dirty = true;
        }
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
                    if !t.text.is_empty() || t.keeps_when_empty() {
                        out.push(AnnotationOp::new(o.id.clone(), t.kind()));
                    }
                }
                _ => out.push(o.clone()),
            }
        }
        if let Some(t) = editing {
            if !replaced && (!t.text.is_empty() || t.keeps_when_empty()) {
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
            Interaction::Drawing { start, cur, .. } if self.tool == Tool::SelectText => {
                // A translucent selection box; it is only drawn, never stored.
                super::copytext::selection_rect(*start, *cur, self.image).map(|rect| {
                    OpKind::Highlight {
                        rect,
                        color: Color::rgb(0x2E, 0x8B, 0xFF),
                        opacity: 0.25,
                    }
                })
            }
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
        if let OpKind::Spotlight { shapes, .. } = &kind {
            // A spotlight shows the handles of one hole at a time (the selected one).
            let h = self.spot_sel.min(shapes.len().saturating_sub(1));
            return Some(Chrome {
                bounds: shapes.get(h)?.rect.normalized(),
                handles: geom::hole_handles(&kind, h),
            });
        }
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
            self.activate(tool);
            if let Some(k) = self.selected_kind() {
                if tool != Tool::Select && Tool::for_kind(k) != tool {
                    self.selected = None;
                }
            }
            // There is one spotlight per image: choosing the tool shows it.
            if tool == Tool::Spotlight && self.selected.is_none() {
                if let Some(i) = self.spotlight_index() {
                    self.selected = Some(self.ops[i].id.clone());
                    self.spot_sel = 0;
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
            self.activate(Tool::Select);
        }
    }

    // ---- pointer ------------------------------------------------------------------------

    /// `tol` is the pointer tolerance in image px (about 6 logical px).
    pub fn pointer_down(&mut self, p: Pt, mods: Mods, clicks: usize, tol: f64) -> Vec<Edit> {
        let mut edits: Vec<Edit> = self.finish_text().into_iter().collect();
        self.hover = Some(p);
        if let Some(i) = self.active_index() {
            let kind = self.ops[i].kind.clone();
            let found = if matches!(kind, OpKind::Spotlight { .. }) {
                geom::pick_handle(&geom::hole_handles(&kind, self.spot_sel), p, tol * 1.6)
            } else {
                geom::hit_handle(&kind, p, tol * 1.6, &mut self.metrics)
            };
            if let Some(h) = found {
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
                Some(i) => self.grab(i, p, clicks),
                None => self.selected = None,
            },
            // Inside an existing hole the spotlight is grabbed; anywhere else a new hole is
            // dragged out (ANN-7: a second shape becomes a hole of the one spotlight).
            Tool::Spotlight => match geom::topmost_hit(&self.ops, p, tol, &mut self.metrics)
                .filter(|&i| matches!(self.ops[i].kind, OpKind::Spotlight { .. }))
            {
                Some(i) => self.grab(i, p, clicks),
                None => {
                    self.interaction = Interaction::Drawing {
                        start: p,
                        cur: p,
                        shift: mods.shift,
                    };
                }
            },
            // Clicking a callout selects it (double click edits its text); anywhere else
            // drags out a new box.
            Tool::Callout => match geom::topmost_hit(&self.ops, p, tol, &mut self.metrics)
                .filter(|&i| matches!(self.ops[i].kind, OpKind::Callout { .. }))
            {
                Some(i) => self.grab(i, p, clicks),
                None => {
                    self.selected = None;
                    self.interaction = Interaction::Drawing {
                        start: p,
                        cur: p,
                        shift: mods.shift,
                    };
                }
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
            Tool::Stamp => edits.push(self.place_stamp(p)),
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

    /// Selects op `i` and starts dragging it; a double click on text or a callout edits it.
    fn grab(&mut self, i: usize, p: Pt, clicks: usize) {
        self.selected = Some(self.ops[i].id.clone());
        if matches!(self.ops[i].kind, OpKind::Spotlight { .. }) {
            self.spot_sel = geom::hit_hole(&self.ops[i].kind, p, 0.0).unwrap_or(self.spot_sel);
        }
        if clicks >= 2
            && matches!(
                self.ops[i].kind,
                OpKind::Text { .. } | OpKind::Callout { .. } | OpKind::Step { .. }
            )
        {
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

    pub fn pointer_move(&mut self, p: Pt, mods: Mods, tol: f64) {
        self.hover = Some(p);
        let image = self.image;
        let hole = self.spot_sel;
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
                let kind = geom::translate_op(orig, dx, dy, hole);
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
                let kind = if matches!(orig, OpKind::Spotlight { .. }) {
                    geom::resize_hole(orig, hole, *handle, p, mods.shift)
                } else {
                    geom::resize(orig, *handle, p, mods.shift, tm)
                };
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
            Interaction::Drawing { start, cur, .. } if self.tool == Tool::SelectText => {
                self.text_region = super::copytext::selection_rect(start, cur, self.image);
                Vec::new()
            }
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
                if let OpKind::Spotlight { shapes, .. } = &kind {
                    return self.add_spot_shape(shapes[0]).into_iter().collect();
                }
                if matches!(kind, OpKind::Callout { .. }) {
                    // The box is not stored yet: the text goes in first and one commit stores
                    // box and text together (one undo step).
                    self.begin_callout_text(kind);
                    return Vec::new();
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

    /// The numbering new steps get: that of the existing steps (the panel sets it for all of
    /// them together), else the remembered one.
    fn step_numbering(&self) -> (StepStyle, u32) {
        let first = self.ops.iter().find_map(|o| match &o.kind {
            OpKind::Step { style, start, .. } => Some((*style, *start)),
            _ => None,
        });
        first.unwrap_or_else(|| {
            let s = self.styles.get(Tool::Step);
            (s.step_style, s.step_start)
        })
    }

    /// What the next step badge reads (shown on the cursor): `A` in letter style, and so on.
    pub fn next_step_label(&self) -> String {
        let (style, start) = self.step_numbering();
        step_label(style, start, self.next_step_number())
    }

    fn place_step(&mut self, p: Pt) -> Edit {
        let style = self.styles.get(Tool::Step);
        let (numbering, start) = self.step_numbering();
        let id = self.tmp_id();
        let op = AnnotationOp::new(
            id.clone(),
            OpKind::Step {
                at: p,
                n: self.next_step_number(),
                size: style.size,
                color: style.color,
                style: numbering,
                start,
                text: None,
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
            target: TextTarget::Text,
            existing: None,
            at: p,
            text: String::new(),
            cursor: 0,
            size: s.size,
            color: s.color,
            // New text has no automatic outline or shadow (ANN-1.AC8): the user changes the
            // colour if the screenshot is light.
            outline: None,
            bg: None,
            marked: None,
        });
    }

    fn begin_text_edit(&mut self, i: usize) {
        match self.ops[i].kind.clone() {
            OpKind::Text {
                at,
                text,
                size,
                color,
                outline,
                bg,
            } => {
                self.interaction = Interaction::Idle;
                self.text = Some(TextEdit {
                    target: TextTarget::Text,
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
            kind @ OpKind::Callout { .. } => {
                self.interaction = Interaction::Idle;
                self.text = Some(Self::callout_edit(kind, Some(self.ops[i].id.clone())));
            }
            kind @ OpKind::Step { .. } => {
                self.interaction = Interaction::Idle;
                let (at, size, color, text) = match &kind {
                    OpKind::Step {
                        at,
                        size,
                        color,
                        text,
                        ..
                    } => (*at, *size, *color, text.clone().unwrap_or_default()),
                    _ => unreachable!(),
                };
                self.text = Some(TextEdit {
                    target: TextTarget::Step(kind),
                    existing: Some(self.ops[i].id.clone()),
                    at,
                    cursor: text.chars().count(),
                    text,
                    size,
                    color,
                    outline: None,
                    bg: None,
                    marked: None,
                });
            }
            _ => {}
        }
    }

    /// Typing into a callout: the text, size and colour come from the op.
    fn callout_edit(kind: OpKind, existing: Option<String>) -> TextEdit {
        let (at, text, size, color) = match &kind {
            OpKind::Callout {
                rect,
                text,
                size,
                color,
                ..
            } => {
                let r = rect.normalized();
                let pad = size * 0.5;
                (Pt::new(r.x + pad, r.y + pad), text.clone(), *size, *color)
            }
            _ => (Pt::new(0.0, 0.0), String::new(), 24.0, Color::WHITE),
        };
        TextEdit {
            target: TextTarget::Callout(kind),
            existing,
            at,
            cursor: text.chars().count(),
            text,
            size,
            color,
            outline: None,
            bg: None,
            marked: None,
        }
    }

    /// A new callout whose box was just dragged (or placed with Enter): typing starts at once.
    fn begin_callout_text(&mut self, kind: OpKind) {
        self.selected = None;
        self.text = Some(Self::callout_edit(kind, None));
    }

    /// The inspector's text field: edits the text of the selected callout on the canvas.
    pub fn edit_selected_text(&mut self) {
        if let Some(i) = self.selected_index() {
            self.begin_text_edit(i);
        }
    }

    /// Commits the text being typed (Esc, click elsewhere, tool change). Empty text creates
    /// nothing, and removes an existing text op.
    pub fn finish_text(&mut self) -> Option<Edit> {
        let t = self.text.take()?;
        // What gets stored: a callout is written with the box the renderer draws (ANN-4.AC5).
        let kind = geom::with_drawn_box(&t.kind());
        match t.existing.clone() {
            Some(id) => {
                let i = self.index_of(&id)?;
                if t.text.is_empty() && !t.keeps_when_empty() {
                    self.ops.remove(i);
                    self.selected = None;
                    return Some(Edit::Replace(self.ops.clone()));
                }
                if self.ops[i].kind == kind {
                    self.selected = Some(id);
                    return None;
                }
                self.ops[i].kind = kind;
                self.selected = Some(id);
                Some(Edit::Replace(self.ops.clone()))
            }
            None => {
                if t.text.is_empty() && !t.keeps_when_empty() {
                    return None;
                }
                let id = self.tmp_id();
                let op = AnnotationOp::new(id.clone(), kind);
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
            // One spotlight per image (ANN-7): it is never duplicated.
            Some(i) if matches!(self.ops[i].kind, OpKind::Spotlight { .. }) => false,
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
        let new = if matches!(kind, OpKind::Spotlight { .. }) {
            if resize {
                geom::resize_hole_by(kind, self.spot_sel, dx, dy)
            } else {
                geom::translate_hole(kind, self.spot_sel, dx, dy)
            }
        } else if resize {
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
            Tool::Callout => {
                self.finish_text();
                let r = rect(s * 1.6, s * 0.6);
                let kind = OpKind::Callout {
                    rect: r,
                    tail: geom::default_tail(r, self.image),
                    text: String::new(),
                    size: style.size,
                    color: geom::text_on(style.color),
                    bg: style.color,
                    radius: style.radius,
                    style: style.callout_style,
                };
                self.begin_callout_text(kind);
                return Vec::new();
            }
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
            Tool::Spotlight => {
                let shape = geom::default_hole(style.spot_shape, center, self.image);
                return self.add_spot_shape(shape).into_iter().collect();
            }
            Tool::Stamp => return vec![self.place_stamp(center)],
            Tool::Select | Tool::SelectText => return Vec::new(),
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
            self.activate(Tool::Select);
            return EscapeOutcome::Edits(Vec::new());
        }
        EscapeOutcome::Leave
    }

    // ---- stamps (ANN-4) ---------------------------------------------------------------------

    /// Click to place the stamp the picker has chosen. A label stamp carries its word in the UI
    /// language, written into the op.
    fn place_stamp(&mut self, p: Pt) -> Edit {
        let st = self.styles.get(Tool::Stamp);
        let id = self.tmp_id();
        let op = AnnotationOp::new(
            id.clone(),
            OpKind::Stamp {
                at: p,
                kind: st.stamp_kind,
                size: st.size,
                color: st.color,
                text: (st.stamp_kind == StampKind::Label).then(|| st.stamp_word.text()),
            },
        );
        self.ops.push(op.clone());
        self.selected = Some(id);
        Edit::Append(vec![op])
    }

    /// The picker: a symbol or a word. Applies to the selected stamp (one revision) and is
    /// what the next click places; each kind brings its usual colour.
    pub fn set_stamp(&mut self, kind: StampKind, word: StampWord) -> Option<Edit> {
        self.styles.set_stamp_kind(kind, word);
        self.styles_dirty = true;
        let color = self.styles.get(Tool::Stamp).color;
        let i = self.selected_index()?;
        match &mut self.ops[i].kind {
            OpKind::Stamp {
                kind: k,
                color: c,
                text,
                ..
            } => {
                *k = kind;
                *c = color;
                *text = (kind == StampKind::Label).then(|| word.text());
            }
            _ => return None,
        }
        Some(Edit::Replace(self.ops.clone()))
    }

    /// What the picker shows as chosen: the selected stamp, else the next one's settings.
    pub fn stamp_info(&self) -> Option<StampInfo> {
        if self.inspector_tool() != Tool::Stamp || self.text.is_some() {
            return None;
        }
        let st = self.styles.get(Tool::Stamp);
        match self.selected_kind() {
            Some(OpKind::Stamp { kind, text, .. }) => Some(StampInfo {
                kind: *kind,
                word: StampWord::ALL
                    .into_iter()
                    .find(|w| text.as_deref() == Some(w.text().as_str())),
                has_op: true,
            }),
            _ => Some(StampInfo {
                kind: st.stamp_kind,
                word: (st.stamp_kind == StampKind::Label).then_some(st.stamp_word),
                has_op: false,
            }),
        }
    }

    // ---- steps (ANN-6) ---------------------------------------------------------------------

    /// Indices of the step ops in the order of their numbers.
    fn steps_in_order(&self) -> Vec<usize> {
        let mut v: Vec<(u32, usize)> = self
            .ops
            .iter()
            .enumerate()
            .filter_map(|(i, o)| match &o.kind {
                OpKind::Step { n, .. } => Some((*n, i)),
                _ => None,
            })
            .collect();
        v.sort_unstable();
        v.into_iter().map(|(_, i)| i).collect()
    }

    /// What the Steps panel shows: the numbering and the steps in order.
    pub fn steps_info(&self) -> Option<StepsInfo> {
        if self.inspector_tool() != Tool::Step {
            return None;
        }
        let (style, start) = self.step_numbering();
        let order = self.steps_in_order();
        let sel = self.selected_index();
        let rows = order
            .iter()
            .map(|&i| match &self.ops[i].kind {
                OpKind::Step { n, text, .. } => StepRow {
                    id: self.ops[i].id.clone(),
                    badge: step_label(style, start, *n),
                    // The label being typed shows in its row as it is typed.
                    text: match &self.text {
                        Some(t) if t.existing.as_deref() == Some(self.ops[i].id.as_str()) => {
                            t.text.clone()
                        }
                        _ => text.clone().unwrap_or_default(),
                    },
                },
                _ => unreachable!(),
            })
            .collect();
        Some(StepsInfo {
            style,
            start,
            start_label: step_label(style, start, 1),
            rows,
            selected: sel.and_then(|s| order.iter().position(|&i| i == s)),
            editing: self.text.is_some(),
        })
    }

    /// Numbering style and first number for **all** steps together (one revision), remembered
    /// for new ones. `None` changes nothing.
    pub fn set_step_numbering(
        &mut self,
        style: Option<StepStyle>,
        start: Option<u32>,
    ) -> Option<Edit> {
        let (cur_style, cur_start) = self.step_numbering();
        let style = style.unwrap_or(cur_style);
        let start = start.unwrap_or(cur_start).clamp(1, 9999);
        self.styles.set_step_numbering(style, start);
        self.styles_dirty = true;
        let mut changed = false;
        for o in &mut self.ops {
            if let OpKind::Step {
                style: st,
                start: s0,
                ..
            } = &mut o.kind
            {
                if *st != style || *s0 != start {
                    *st = style;
                    *s0 = start;
                    changed = true;
                }
            }
        }
        changed.then(|| Edit::Replace(self.ops.clone()))
    }

    /// Moves the step at row `from` of the panel list to row `to`: the numbers follow, in one
    /// revision (ANN-6.AC4). The ops keep their place in the list, so nothing jumps in front of
    /// something else.
    pub fn reorder_steps(&mut self, from: usize, to: usize) -> Option<Edit> {
        let mut order = self.steps_in_order();
        if from >= order.len() || to >= order.len() || from == to {
            return None;
        }
        let moved = order.remove(from);
        order.insert(to, moved);
        for (rank, i) in order.into_iter().enumerate() {
            if let OpKind::Step { n, .. } = &mut self.ops[i].kind {
                *n = rank as u32 + 1;
            }
        }
        Some(Edit::Replace(self.ops.clone()))
    }

    /// Deletes the step at row `row` of the panel list; the rest renumber.
    pub fn delete_step_row(&mut self, row: usize) -> Option<Edit> {
        let i = *self.steps_in_order().get(row)?;
        self.ops.remove(i);
        if self.selected_index().is_none() {
            self.selected = None;
        }
        geom::renumber_steps(&mut self.ops);
        Some(Edit::Replace(self.ops.clone()))
    }

    /// Selects the step at row `row` of the panel list.
    pub fn select_step_row(&mut self, row: usize) {
        if let Some(&i) = self.steps_in_order().get(row) {
            self.selected = Some(self.ops[i].id.clone());
        }
    }

    // ---- spotlight (ANN-7) ----------------------------------------------------------------

    /// The index of the image's one spotlight.
    pub fn spotlight_index(&self) -> Option<usize> {
        self.ops
            .iter()
            .position(|o| matches!(o.kind, OpKind::Spotlight { .. }))
    }

    /// Where a new spotlight goes: before the first op that is not a redaction (or crop), so
    /// arrows, steps and callouts added later stay bright and blurs stay below it (ANN-7.AC4).
    fn spotlight_insert_at(&self) -> usize {
        self.ops
            .iter()
            .position(|o| {
                !matches!(
                    o.kind,
                    OpKind::Blur { .. } | OpKind::Pixelate { .. } | OpKind::Crop { .. }
                )
            })
            .unwrap_or(self.ops.len())
    }

    /// Adds a hole: to the existing spotlight (one revision, up to 8, ANN-7.AC3) or as a new
    /// spotlight at the right place in the list. `None` at the limit.
    pub fn add_spot_shape(&mut self, shape: SpotShape) -> Option<Edit> {
        if let Some(i) = self.spotlight_index() {
            let new = geom::add_hole(&self.ops[i].kind, shape)?;
            self.ops[i].kind = new;
            self.spot_sel = geom::spot_shapes(&self.ops[i].kind).len() - 1;
            self.selected = Some(self.ops[i].id.clone());
            return Some(Edit::Replace(self.ops.clone()));
        }
        let style = self.styles.get(Tool::Spotlight);
        let id = self.tmp_id();
        let op = AnnotationOp::new(
            id.clone(),
            OpKind::Spotlight {
                shapes: vec![shape],
                dim: style.dim,
                mode: style.spot_mode,
                strength: style.strength,
            },
        );
        let at = self.spotlight_insert_at();
        self.ops.insert(at, op);
        self.spot_sel = 0;
        self.selected = Some(id);
        Some(Edit::Replace(self.ops.clone()))
    }

    /// The panel's "+ Rectangle" / "+ Ellipse": a default hole in the middle of what is shown.
    /// The shape is also what the next drag makes.
    pub fn add_default_hole(&mut self, kind: SpotShapeKind) -> Option<Edit> {
        self.styles.set_spotlight(None, Some(kind));
        self.styles_dirty = true;
        let r = self.content_rect();
        let c = Pt::new(r.x + r.w / 2.0, r.y + r.h / 2.0);
        let shape = geom::default_hole(kind, c, self.image);
        self.add_spot_shape(shape)
    }

    /// Makes hole `i` of the selected spotlight the one the handles act on.
    pub fn select_hole(&mut self, i: usize) {
        if let Some(k) = self.selected_kind() {
            if i < geom::spot_shapes(k).len() {
                self.spot_sel = i;
            }
        }
    }

    /// Deletes hole `i`; the last hole takes the spotlight with it (one revision either way).
    pub fn remove_hole(&mut self, i: usize) -> Option<Edit> {
        let idx = self.selected_index()?;
        if !matches!(self.ops[idx].kind, OpKind::Spotlight { .. }) {
            return None;
        }
        match geom::remove_hole(&self.ops[idx].kind, i) {
            Some(new) => {
                self.ops[idx].kind = new;
                let n = geom::spot_shapes(&self.ops[idx].kind).len();
                self.spot_sel = self.spot_sel.min(n.saturating_sub(1));
            }
            None => {
                self.ops.remove(idx);
                self.selected = None;
                self.spot_sel = 0;
            }
        }
        Some(Edit::Replace(self.ops.clone()))
    }

    /// Dim or Blur for the outside: the selected spotlight (one revision) and new ones.
    pub fn set_spot_mode(&mut self, mode: SpotMode) -> Option<Edit> {
        self.styles.set_spotlight(Some(mode), None);
        self.styles_dirty = true;
        let i = self.selected_index()?;
        match &mut self.ops[i].kind {
            OpKind::Spotlight { mode: m, .. } if *m != mode => *m = mode,
            _ => return None,
        }
        Some(Edit::Replace(self.ops.clone()))
    }

    /// What the Spotlight panel shows, when the inspector is about a spotlight.
    pub fn spot_info(&self) -> Option<SpotInfo> {
        if self.inspector_tool() != Tool::Spotlight || self.text.is_some() {
            return None;
        }
        let style = self.styles.get(Tool::Spotlight);
        match self.selected_kind() {
            Some(OpKind::Spotlight {
                shapes, mode, dim, ..
            }) => Some(SpotInfo {
                holes: shapes.iter().map(|s| s.kind).collect(),
                selected: self.spot_sel.min(shapes.len().saturating_sub(1)),
                mode: *mode,
                dim: *dim,
                has_op: true,
                at_limit: shapes.len() >= MAX_SPOTLIGHT_SHAPES,
                next_shape: style.spot_shape,
            }),
            _ => Some(SpotInfo {
                holes: Vec::new(),
                selected: 0,
                mode: style.spot_mode,
                dim: style.dim,
                has_op: false,
                at_limit: false,
                next_shape: style.spot_shape,
            }),
        }
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
                // Typed text is not handled here (ANN-1.AC7): the platform feeds it through the
                // input method into `text_ime`, so Telex and VNI can compose. Inserting
                // `key_char` here would stop the event and leave the raw Latin letters.
                let _ = (key_char, mods);
                return false;
            }
        }
        true
    }

    /// Whether a key press in the text tool is typed text that belongs to the platform input
    /// method (not a shortcut and not an editing key).
    pub fn is_typed_text(key_char: Option<&str>, mods: Mods) -> bool {
        !mods.secondary
            && key_char.is_some_and(|c| !c.is_empty() && c.chars().all(|ch| !ch.is_control()))
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
        if let Some(t) = &self.text {
            return match t.target {
                TextTarget::Callout(_) => Tool::Callout.params(),
                TextTarget::Step(_) => Tool::Step.params(),
                TextTarget::Text => Tool::Text.params(),
            };
        }
        match self.selected_kind() {
            Some(k) => geom::params_of(k),
            None => self.tool.params(),
        }
    }

    pub fn inspector_value(&self, p: Param) -> Option<ParamValue> {
        if let Some(t) = &self.text {
            if matches!(t.target, TextTarget::Callout(_) | TextTarget::Step(_)) {
                return geom::param_value(&t.kind(), p);
            }
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

    /// The tool the inspector is about: the text being typed, the selected op, else the
    /// active tool.
    pub fn inspector_tool(&self) -> Tool {
        if let Some(t) = &self.text {
            return match t.target {
                TextTarget::Callout(_) => Tool::Callout,
                TextTarget::Step(_) => Tool::Step,
                TextTarget::Text => Tool::Text,
            };
        }
        match self.selected_kind() {
            Some(k) => Tool::for_kind(k),
            None => self.tool,
        }
    }

    /// What the inspector heading names.
    pub fn inspector_title(&self) -> String {
        self.inspector_tool().label()
    }

    /// Applies a parameter change (ANN-1.AC5: one undo step, only the target op changes).
    pub fn set_param(&mut self, v: ParamValue) -> Option<Edit> {
        if let Some(t) = self.text.as_mut() {
            if let TextTarget::Callout(base) | TextTarget::Step(base) = &mut t.target {
                // A callout or step label being typed: the change goes into the op that is
                // committed with the text.
                let new = geom::with_param(base, v)?;
                if let OpKind::Callout { size, color, .. } = &new {
                    t.size = *size;
                    t.color = *color;
                }
                let tool = Tool::for_kind(&new);
                *base = new;
                self.styles.apply(tool, v);
                self.styles_dirty = true;
                return None;
            }
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

    /// What the Callout panel shows: the callout being typed, the selected one, or (with the
    /// Callout tool and nothing selected) the settings a new one gets.
    pub fn callout_info(&self) -> Option<CalloutInfo> {
        if let Some(t) = &self.text {
            return match &t.target {
                TextTarget::Callout(OpKind::Callout { style, .. }) => Some(CalloutInfo {
                    text: t.text.clone(),
                    style: *style,
                    editing: true,
                    has_op: t.existing.is_some(),
                }),
                _ => None,
            };
        }
        match self.selected_kind() {
            Some(OpKind::Callout { text, style, .. }) => Some(CalloutInfo {
                text: text.clone(),
                style: *style,
                editing: false,
                has_op: true,
            }),
            Some(_) => None,
            None if self.tool == Tool::Callout => Some(CalloutInfo {
                text: String::new(),
                style: self.styles.get(Tool::Callout).callout_style,
                editing: false,
                has_op: false,
            }),
            None => None,
        }
    }

    /// Bubble or Box: applies to the callout being typed or selected (one undo step), and is
    /// remembered for new ones.
    pub fn set_callout_style(&mut self, style: CalloutStyle) -> Option<Edit> {
        self.styles.set_callout_style(style);
        self.styles_dirty = true;
        if let Some(t) = self.text.as_mut() {
            if let TextTarget::Callout(OpKind::Callout { style: st, .. }) = &mut t.target {
                *st = style;
            }
            return None;
        }
        let i = self.selected_index()?;
        match &mut self.ops[i].kind {
            OpKind::Callout { style: st, .. } if *st != style => *st = style,
            _ => return None,
        }
        Some(Edit::Replace(self.ops.clone()))
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
    /// ANN-5: the Select text tool drags a rectangle that is handed to the view once, draws
    /// nothing into the capture and is clamped to the image; a click selects nothing.
    #[test]
    fn select_text_drag_gives_a_region_and_never_an_op() {
        let mut s = st();
        s.set_tool(Tool::SelectText);
        let edits = drag(&mut s, (100.0, 50.0), (400.0, 90.0), NONE);
        assert!(edits.is_empty(), "no annotate call");
        assert!(s.ops.is_empty() && s.selected.is_none());
        let r = s.take_text_region().unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (100.0, 50.0, 300.0, 40.0));
        assert!(s.take_text_region().is_none(), "handed over once");

        // Past the edges of the image: clamped.
        drag(&mut s, (-30.0, 700.0), (1300.0, 900.0), NONE);
        let r = s.take_text_region().unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (0.0, 700.0, 1200.0, 100.0));

        // A click or a tiny drag is not a selection.
        drag(&mut s, (10.0, 10.0), (12.0, 12.0), NONE);
        assert!(s.take_text_region().is_none());
    }

    /// ANN-5: while dragging, the overlay is a translucent box (never stored).
    #[test]
    fn select_text_draft_is_a_translucent_box() {
        let mut s = st();
        s.set_tool(Tool::SelectText);
        s.pointer_down(Pt::new(10.0, 10.0), NONE, 1, TOL);
        s.pointer_move(Pt::new(200.0, 100.0), NONE, TOL);
        assert!(matches!(s.draft(), Some(OpKind::Highlight { .. })));
        s.cancel_interaction();
        assert!(s.draft().is_none());
        assert!(s.ops.is_empty());
    }

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
    /// white, no outline (ANN-1.AC8).
    #[test]
    fn text_flow_click_type_enter_escape() {
        let mut s = st();
        s.set_tool(Tool::Text);
        assert!(s
            .pointer_down(Pt::new(130.0, 320.0), NONE, 1, TOL)
            .is_empty());
        for c in "Click".chars() {
            // Typed text goes through the input method, not through the key handler.
            assert!(!s.text_key(&c.to_string(), Some(&c.to_string()), NONE));
            s.text_ime(None, &c.to_string(), false);
        }
        assert!(s.text_key("enter", None, NONE));
        for c in "Save".chars() {
            s.text_ime(None, &c.to_string(), false);
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
                outline: None,
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
        s.text_ime(None, "h", false);
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
            target: TextTarget::Text,
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
            target: TextTarget::Text,
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

    /// ANN-1.AC7: Telex composition reaches the text; "Tiếng Việt có dấu" is typed through the
    /// input-method path (draft, then commit), not through the key handler.
    #[test]
    fn telex_composition_builds_vietnamese_text() {
        let mut s = st();
        s.set_tool(Tool::Text);
        s.pointer_down(Pt::new(10.0, 10.0), NONE, 1, TOL);
        // A raw key press with a printable character is left to the input method.
        assert!(EditorState::is_typed_text(Some("T"), NONE));
        assert!(!EditorState::is_typed_text(
            Some("v"),
            Mods {
                secondary: true,
                ..NONE
            }
        ));
        assert!(!s.text_key("t", Some("t"), NONE));
        // Telex: each step replaces the draft (the whole marked range) with a longer one.
        let steps = ["T", "Ti", "Tie", "Tiê", "Tiêm", "Tiêng", "Tiếng"];
        for st_ in steps {
            s.text_ime(None, st_, true);
        }
        s.text_ime(None, "Tiếng", false);
        s.text_ime(None, " ", false);
        for st_ in ["V", "Vi", "Viê", "Việ", "Việt"] {
            s.text_ime(None, st_, true);
        }
        s.text_ime(None, "Việt ", false);
        s.text_ime(None, "có dấu", false);
        let t = s.text.as_ref().unwrap();
        assert_eq!(t.text, "Tiếng Việt có dấu");
        assert!(t.marked.is_none());
    }

    // ---- ANN-4: callouts ------------------------------------------------------------------

    fn callout_op(s: &EditorState) -> (&AnnotationOp, &OpKind) {
        let o = s
            .ops
            .iter()
            .find(|o| matches!(o.kind, OpKind::Callout { .. }))
            .expect("a callout");
        (o, &o.kind)
    }

    /// ANN-4: dragging with the Callout tool does not store anything yet; typing starts at once
    /// and one commit stores box and text together as one Append (one undo step).
    #[test]
    fn ann_4_callout_drag_then_type_is_one_append() {
        let mut s = st();
        s.set_tool(Tool::Callout);
        let edits = drag(&mut s, (100.0, 100.0), (320.0, 170.0), NONE);
        assert!(edits.is_empty(), "nothing stored while typing");
        assert!(s.ops.is_empty());
        let t = s.text.as_ref().expect("typing started");
        assert!(matches!(t.target, TextTarget::Callout(_)));
        s.text_ime(None, "Click here", false);
        let e = s.finish_text().expect("one edit");
        let op = only_append(&[e]).clone();
        match op.kind {
            OpKind::Callout {
                rect,
                tail,
                text,
                style,
                bg,
                ..
            } => {
                assert_eq!(text, "Click here");
                assert_eq!(rect, Rc::new(100.0, 100.0, 220.0, 70.0));
                assert_eq!(style, CalloutStyle::Bubble);
                assert_eq!(bg, cm_core::annotation::DEFAULT_COLOR);
                // The default tail sits below the box, inside the image.
                assert!(tail.y > 170.0 && tail.y <= 800.0, "{tail:?}");
            }
            other => panic!("{other:?}"),
        }
        assert!(s.selected.is_some());
    }

    /// ANN-4.AC5: the stored rect is the box the renderer draws after the commit (the editor
    /// writes the grown rectangle back), and the handles sit on that box.
    #[test]
    fn ann_4_ac5_commit_writes_the_drawn_box_and_handles_use_it() {
        let mut s = st();
        s.set_tool(Tool::Callout);
        drag(&mut s, (100.0, 100.0), (180.0, 140.0), NONE);
        s.text_ime(None, "a long line of callout text", false);
        s.finish_text();
        let k = &callout_op(&s).1.clone();
        let drawn = cm_render::callout_box(k).unwrap();
        match k {
            OpKind::Callout { rect, .. } => assert_eq!(rect.normalized(), drawn),
            _ => unreachable!(),
        }
        let hs = geom::handles(k, &mut s.metrics);
        let corners: Vec<Pt> = hs
            .iter()
            .filter(|(h, _)| *h != Handle::Tail)
            .map(|(_, p)| *p)
            .collect();
        assert_eq!(corners.len(), 4);
        assert!(corners.contains(&Pt::new(drawn.x, drawn.y)));
        assert!(corners.contains(&Pt::new(drawn.x + drawn.w, drawn.y + drawn.h)));
        assert!(hs.iter().any(|(h, _)| *h == Handle::Tail));
    }

    /// ANN-4.AC5: Telex typing composes in a callout like in the Text tool.
    #[test]
    fn ann_4_ac5_callout_text_composes_telex() {
        let mut s = st();
        s.set_tool(Tool::Callout);
        drag(&mut s, (50.0, 50.0), (300.0, 120.0), NONE);
        assert!(
            !s.text_key("t", Some("t"), NONE),
            "typed keys go to the input method"
        );
        for st_ in ["T", "Ti", "Tie", "Tiê", "Tiêng", "Tiếng"] {
            s.text_ime(None, st_, true);
        }
        s.text_ime(None, "Tiếng", false);
        s.text_ime(None, " ", false);
        for st_ in ["V", "Vi", "Viê", "Việ", "Việt"] {
            s.text_ime(None, st_, true);
        }
        s.text_ime(None, "Việt", false);
        s.finish_text();
        match &callout_op(&s).1 {
            OpKind::Callout { text, .. } => assert_eq!(text, "Tiếng Việt"),
            _ => unreachable!(),
        }
    }

    /// ANN-4.AC2: moving the box keeps the tail tip where it is; dragging the tail handle moves
    /// only the tip. Each is one Replace edit.
    #[test]
    fn ann_4_ac2_moving_the_box_keeps_the_tail_tip() {
        let mut s = st();
        s.set_tool(Tool::Callout);
        drag(&mut s, (100.0, 100.0), (300.0, 160.0), NONE);
        s.text_ime(None, "Hi", false);
        s.finish_text();
        let tail0 = match &callout_op(&s).1 {
            OpKind::Callout { tail, .. } => *tail,
            _ => unreachable!(),
        };
        s.set_tool(Tool::Select);
        let e = drag(&mut s, (200.0, 130.0), (260.0, 190.0), NONE);
        assert_eq!(e.len(), 1);
        match &callout_op(&s).1 {
            OpKind::Callout { rect, tail, .. } => {
                assert_eq!(*tail, tail0, "the tip did not move");
                assert_eq!(rect.normalized().x, 160.0);
                assert_eq!(rect.normalized().y, 160.0);
            }
            _ => unreachable!(),
        }
        // The blue dot moves only the tip.
        let e = drag(&mut s, (tail0.x, tail0.y), (500.0, 400.0), NONE);
        assert_eq!(e.len(), 1);
        match &callout_op(&s).1 {
            OpKind::Callout { rect, tail, .. } => {
                assert_eq!(*tail, Pt::new(500.0, 400.0));
                assert_eq!(rect.normalized().x, 160.0);
            }
            _ => unreachable!(),
        }
    }

    /// ANN-4: double-clicking a callout types in it again; changing the text is one Replace.
    #[test]
    fn ann_4_double_click_edits_the_text_of_an_existing_callout() {
        let mut s = st();
        s.set_tool(Tool::Callout);
        drag(&mut s, (100.0, 100.0), (300.0, 160.0), NONE);
        s.text_ime(None, "One", false);
        s.finish_text();
        s.set_tool(Tool::Select);
        s.pointer_down(Pt::new(150.0, 120.0), NONE, 2, TOL);
        s.pointer_up(Pt::new(150.0, 120.0), NONE, TOL);
        assert_eq!(s.text.as_ref().map(|t| t.text.as_str()), Some("One"));
        s.text_ime(None, " two", false);
        let e = s.finish_text().expect("changed");
        assert!(matches!(e, Edit::Replace(_)));
        match &callout_op(&s).1 {
            OpKind::Callout { text, .. } => assert_eq!(text, "One two"),
            _ => unreachable!(),
        }
        // Unchanged text commits nothing.
        s.edit_selected_text();
        assert!(s.finish_text().is_none());
    }

    /// ANN-4 edge case: an empty callout is still a box (and not removed by empty text).
    #[test]
    fn ann_4_empty_callout_text_keeps_the_box() {
        let mut s = st();
        s.set_tool(Tool::Callout);
        drag(&mut s, (100.0, 100.0), (300.0, 160.0), NONE);
        let e = s.finish_text().expect("stored");
        assert!(matches!(e, Edit::Append(_)));
        assert_eq!(s.ops.len(), 1);
        // The preview shows the box while it is typed, with no text yet.
        s.edit_selected_text();
        assert_eq!(s.render_ops().len(), 1);
        assert!(s.finish_text().is_none());
        assert_eq!(s.ops.len(), 1);
    }

    /// ANN-4: Bubble or Box applies to the selected callout in one Replace and is remembered for
    /// new ones; the fill changes the text colour to stay readable.
    #[test]
    fn ann_4_style_and_fill_apply_to_the_selected_callout() {
        let mut s = st();
        s.set_tool(Tool::Callout);
        drag(&mut s, (100.0, 100.0), (300.0, 160.0), NONE);
        s.finish_text();
        let e = s.set_callout_style(CalloutStyle::Box).expect("one edit");
        assert!(matches!(e, Edit::Replace(_)));
        assert!(
            s.set_callout_style(CalloutStyle::Box).is_none(),
            "unchanged"
        );
        assert_eq!(s.callout_info().map(|i| i.style), Some(CalloutStyle::Box));
        let yellow = Color::rgb(0xFF, 0xD6, 0x0A);
        s.set_param(ParamValue::Color(yellow)).expect("edit");
        match &callout_op(&s).1 {
            OpKind::Callout { bg, color, .. } => {
                assert_eq!(*bg, yellow);
                assert_eq!(*color, Color::rgb(0x11, 0x11, 0x11), "dark text on yellow");
            }
            _ => unreachable!(),
        }
        s.set_param(ParamValue::Radius(0.0)).expect("edit");
        s.set_param(ParamValue::TextSize(32.0)).expect("edit");
        // A new callout starts with the remembered look.
        s.select(None);
        drag(&mut s, (400.0, 400.0), (600.0, 460.0), NONE);
        s.finish_text();
        match &s.ops[1].kind {
            OpKind::Callout {
                style,
                bg,
                radius,
                size,
                ..
            } => {
                assert_eq!(*style, CalloutStyle::Box);
                assert_eq!(*bg, yellow);
                assert_eq!((*radius, *size), (0.0, 32.0));
            }
            _ => panic!(),
        }
        // The remembered look survives the settings round trip.
        let back = Styles::from_json(&s.styles.to_json());
        assert_eq!(back.get(Tool::Callout).callout_style, CalloutStyle::Box);
    }

    /// ANN-4: with the Callout tool nothing is selected until a callout is clicked, so the panel
    /// shows the settings of the next one.
    #[test]
    fn ann_4_callout_panel_info() {
        let mut s = st();
        assert!(s.callout_info().is_none());
        s.set_tool(Tool::Callout);
        let i = s.callout_info().unwrap();
        assert!(!i.has_op && !i.editing);
        drag(&mut s, (100.0, 100.0), (300.0, 160.0), NONE);
        let i = s.callout_info().unwrap();
        assert!(i.editing && !i.has_op);
        s.finish_text();
        assert!(s.callout_info().unwrap().has_op);
        assert_eq!(s.inspector_tool(), Tool::Callout);
    }

    /// The Enter key places a default callout at the view centre and starts typing, so the tool
    /// works without a pointer.
    #[test]
    fn ann_4_enter_places_a_default_callout_and_types() {
        let mut s = st();
        s.set_tool(Tool::Callout);
        assert!(s.place_default(Pt::new(600.0, 400.0)).is_empty());
        assert!(matches!(
            s.text.as_ref().map(|t| &t.target),
            Some(TextTarget::Callout(_))
        ));
        s.text_ime(None, "Here", false);
        assert!(matches!(s.finish_text(), Some(Edit::Append(_))));
    }

    /// Callouts are tools of the Text group; the tool for a callout op is Callout.
    #[test]
    fn ann_4_callout_tool_for_kind_and_group() {
        let k = OpKind::Callout {
            rect: Rc::new(0.0, 0.0, 10.0, 10.0),
            tail: Pt::new(5.0, 20.0),
            text: String::new(),
            size: 24.0,
            color: Color::WHITE,
            bg: Color::BLACK,
            radius: 12.0,
            style: CalloutStyle::Bubble,
        };
        assert_eq!(Tool::for_kind(&k), Tool::Callout);
        assert_eq!(Tool::Callout.group(), crate::editor::Group::Text);
        assert_eq!(Tool::from_key('k'), Some(Tool::Callout));
    }

    // ---- ANN-7: spotlight -----------------------------------------------------------------

    fn spot_holes(s: &EditorState) -> usize {
        s.ops
            .iter()
            .find_map(|o| match &o.kind {
                OpKind::Spotlight { shapes, .. } => Some(shapes.len()),
                _ => None,
            })
            .unwrap_or(0)
    }

    fn spot_count(s: &EditorState) -> usize {
        s.ops
            .iter()
            .filter(|o| matches!(o.kind, OpKind::Spotlight { .. }))
            .count()
    }

    /// ANN-7.AC3: a second drawn shape is a hole of the existing spotlight, in one revision; the
    /// whole op list is one `Replace`, so one undo leaves one hole.
    #[test]
    fn ann_7_ac3_second_shape_is_a_hole_in_one_edit() {
        let mut s = st();
        s.set_tool(Tool::Spotlight);
        let e1 = drag(&mut s, (100.0, 100.0), (300.0, 200.0), NONE);
        assert_eq!(e1.len(), 1);
        assert_eq!(spot_holes(&s), 1);
        let before = s.ops.clone();
        let e2 = drag(&mut s, (400.0, 300.0), (500.0, 380.0), NONE);
        assert_eq!(e2.len(), 1, "one revision");
        assert_eq!((spot_count(&s), spot_holes(&s)), (1, 2));
        match &e2[0] {
            Edit::Replace(ops) => {
                assert_eq!(ops.len(), 1);
                assert_ne!(ops, &before);
            }
            other => panic!("{other:?}"),
        }
        // The new hole is the one the handles act on.
        assert_eq!(s.spot_sel, 1);
        assert_eq!(
            s.chrome().unwrap().bounds,
            Rc::new(400.0, 300.0, 100.0, 80.0)
        );
    }

    /// ANN-7: up to 8 holes; the ninth is refused and nothing changes.
    #[test]
    fn ann_7_eight_holes_at_most() {
        let mut s = st();
        s.set_tool(Tool::Spotlight);
        for i in 0..8 {
            let x = 20.0 + f64::from(i) * 120.0;
            assert_eq!(drag(&mut s, (x, 100.0), (x + 80.0, 160.0), NONE).len(), 1);
        }
        assert_eq!(spot_holes(&s), 8);
        assert!(s.spot_info().unwrap().at_limit);
        let before = s.ops.clone();
        assert!(drag(&mut s, (20.0, 300.0), (100.0, 360.0), NONE).is_empty());
        assert!(s.add_default_hole(SpotShapeKind::Ellipse).is_none());
        assert_eq!(s.ops, before);
    }

    /// ANN-7.AC4: a new spotlight goes before the first non-redaction op; a blur stays below it.
    #[test]
    fn ann_7_ac4_spotlight_goes_before_the_first_non_redaction_op() {
        let mut s = st();
        s.set_tool(Tool::Blur);
        drag(&mut s, (10.0, 10.0), (100.0, 60.0), NONE);
        s.set_tool(Tool::Arrow);
        drag(&mut s, (200.0, 200.0), (300.0, 260.0), NONE);
        s.set_tool(Tool::Spotlight);
        drag(&mut s, (150.0, 150.0), (350.0, 300.0), NONE);
        let names: Vec<&str> = s.ops.iter().map(|o| o.kind.name()).collect();
        assert_eq!(names, ["blur", "spotlight", "arrow"]);
        // Later ops are appended after it and stay bright.
        s.set_tool(Tool::Rect);
        drag(&mut s, (400.0, 400.0), (450.0, 450.0), NONE);
        let names: Vec<&str> = s.ops.iter().map(|o| o.kind.name()).collect();
        assert_eq!(names, ["blur", "spotlight", "arrow", "rect"]);
        // Only redactions: the spotlight goes last.
        let mut t = st();
        t.set_tool(Tool::Pixelate);
        drag(&mut t, (10.0, 10.0), (100.0, 60.0), NONE);
        t.set_tool(Tool::Spotlight);
        drag(&mut t, (150.0, 150.0), (350.0, 300.0), NONE);
        let names: Vec<&str> = t.ops.iter().map(|o| o.kind.name()).collect();
        assert_eq!(names, ["pixelate", "spotlight"]);
        // The list is valid for `annotate`: one spotlight.
        assert!(cm_core::annotation::validate_list(&s.ops).is_ok());
    }

    /// ANN-7: moving and resizing act on one hole; Delete on the op; the inspector deletes a
    /// hole, and the last hole takes the op with it, each as one Replace.
    #[test]
    fn ann_7_holes_move_resize_and_delete() {
        let mut s = st();
        s.set_tool(Tool::Spotlight);
        drag(&mut s, (100.0, 100.0), (200.0, 200.0), NONE);
        drag(&mut s, (400.0, 100.0), (500.0, 200.0), NONE);
        s.set_tool(Tool::Select);
        // Click inside the first hole picks it and drags only it.
        let e = drag(&mut s, (150.0, 150.0), (170.0, 180.0), NONE);
        assert_eq!(e.len(), 1);
        assert_eq!(s.spot_sel, 0);
        let shapes = geom::spot_shapes(&s.ops[0].kind).to_vec();
        assert_eq!(shapes[0].rect, Rc::new(120.0, 130.0, 100.0, 100.0));
        assert_eq!(
            shapes[1].rect,
            Rc::new(400.0, 100.0, 100.0, 100.0),
            "untouched"
        );
        // A corner handle of the selected hole resizes it.
        let e = drag(&mut s, (220.0, 230.0), (260.0, 280.0), NONE);
        assert_eq!(e.len(), 1);
        let shapes = geom::spot_shapes(&s.ops[0].kind).to_vec();
        assert_eq!(shapes[0].rect, Rc::new(120.0, 130.0, 140.0, 150.0));
        // Alt+arrows resize and arrows move the selected hole only.
        s.nudge(1, 0, false, true).unwrap();
        assert_eq!(geom::spot_shapes(&s.ops[0].kind)[0].rect.x, 130.0);
        assert_eq!(geom::spot_shapes(&s.ops[0].kind)[1].rect.x, 400.0);
        // Remove the first hole: one left. Remove that: the op goes.
        s.select_hole(0);
        assert!(matches!(s.remove_hole(0), Some(Edit::Replace(_))));
        assert_eq!(spot_holes(&s), 1);
        assert!(matches!(s.remove_hole(0), Some(Edit::Replace(ref o)) if o.is_empty()));
        assert_eq!(spot_count(&s), 0);
        assert!(s.selected.is_none());
    }

    /// ANN-7: one spotlight per image, so copy, paste and duplicate never copy it.
    #[test]
    fn ann_7_a_spotlight_is_never_duplicated() {
        let mut s = st();
        s.set_tool(Tool::Spotlight);
        drag(&mut s, (100.0, 100.0), (200.0, 200.0), NONE);
        assert!(s.selected.is_some());
        assert!(!s.copy_selection());
        assert!(s.duplicate().is_none());
        assert!(s.paste().is_none());
        assert_eq!(spot_count(&s), 1);
    }

    /// ANN-7: Dim and Blur apply to the selected spotlight in one revision; the dim level and
    /// the mode are remembered for new ones; the default dim is 60 %.
    #[test]
    fn ann_7_dim_mode_and_defaults() {
        let mut s = st();
        s.set_tool(Tool::Spotlight);
        drag(&mut s, (100.0, 100.0), (200.0, 200.0), NONE);
        match &s.ops[0].kind {
            OpKind::Spotlight {
                dim,
                mode,
                strength,
                ..
            } => {
                assert_eq!((*dim, *mode, *strength), (0.6, SpotMode::Dim, 12));
            }
            _ => unreachable!(),
        }
        assert!(matches!(
            s.set_param(ParamValue::Dim(0.8)),
            Some(Edit::Replace(_))
        ));
        assert!(matches!(
            s.set_spot_mode(SpotMode::Blur),
            Some(Edit::Replace(_))
        ));
        assert!(s.set_spot_mode(SpotMode::Blur).is_none(), "unchanged");
        assert!(matches!(
            s.set_param(ParamValue::Strength(20)),
            Some(Edit::Replace(_))
        ));
        match &s.ops[0].kind {
            OpKind::Spotlight {
                dim,
                mode,
                strength,
                ..
            } => {
                assert_eq!((*dim, *mode, *strength), (0.8, SpotMode::Blur, 20));
            }
            _ => unreachable!(),
        }
        let info = s.spot_info().unwrap();
        assert_eq!(
            (info.mode, info.dim, info.holes.len()),
            (SpotMode::Blur, 0.8, 1)
        );
        // Without a spotlight the panel shows the next one's settings.
        s.delete_selected();
        s.set_tool(Tool::Select);
        s.set_tool(Tool::Spotlight);
        let info = s.spot_info().unwrap();
        assert!(!info.has_op);
        assert_eq!((info.mode, info.dim), (SpotMode::Blur, 0.8));
        let back = Styles::from_json(&s.styles.to_json());
        assert_eq!(back.get(Tool::Spotlight).spot_mode, SpotMode::Blur);
        assert_eq!(back.get(Tool::Spotlight).dim, 0.8);
    }

    /// ANN-7: choosing the tool selects the image's spotlight; "+ Ellipse" adds an ellipse hole
    /// and makes ellipse the shape of the next drag.
    #[test]
    fn ann_7_buttons_add_default_holes() {
        let mut s = st();
        s.set_tool(Tool::Spotlight);
        let e = s.add_default_hole(SpotShapeKind::Ellipse).expect("an edit");
        assert!(matches!(e, Edit::Replace(_)));
        match &s.ops[0].kind {
            OpKind::Spotlight { shapes, .. } => {
                assert_eq!(shapes[0].kind, SpotShapeKind::Ellipse);
                let r = shapes[0].rect;
                assert!(r.x >= 0.0 && r.x + r.w <= 1200.0 && r.y >= 0.0 && r.y + r.h <= 800.0);
            }
            _ => unreachable!(),
        }
        drag(&mut s, (900.0, 100.0), (1000.0, 200.0), NONE);
        assert_eq!(
            geom::spot_shapes(&s.ops[0].kind)[1].kind,
            SpotShapeKind::Ellipse
        );
        s.set_tool(Tool::Select);
        s.select(None);
        s.set_tool(Tool::Spotlight);
        assert!(s.selected.is_some(), "the one spotlight is shown");
        assert_eq!(s.spotlight_index(), Some(0));
    }

    /// Emphasis group: the S key picks Spotlight and Enter places a default hole.
    #[test]
    fn ann_7_key_s_and_enter() {
        assert_eq!(Tool::from_key('s'), Some(Tool::Spotlight));
        assert_eq!(Tool::Spotlight.group(), crate::editor::Group::Emphasis);
        let mut s = st();
        s.set_tool(Tool::Spotlight);
        let e = s.place_default(Pt::new(600.0, 400.0));
        assert_eq!(e.len(), 1);
        assert_eq!(spot_holes(&s), 1);
        assert_eq!(Tool::for_kind(&s.ops[0].kind), Tool::Spotlight);
    }

    // ---- ANN-6: steps panel ---------------------------------------------------------------

    fn place_steps(s: &mut EditorState, n: usize) {
        s.set_tool(Tool::Step);
        for i in 0..n {
            let x = 50.0 + 60.0 * i as f64;
            s.pointer_down(Pt::new(x, 100.0), NONE, 1, TOL);
            s.pointer_up(Pt::new(x, 100.0), NONE, TOL);
        }
    }

    fn step_ns(s: &EditorState) -> Vec<u32> {
        s.ops
            .iter()
            .filter_map(|o| match &o.kind {
                OpKind::Step { n, .. } => Some(*n),
                _ => None,
            })
            .collect()
    }

    fn badges(s: &EditorState) -> Vec<String> {
        s.steps_info()
            .unwrap()
            .rows
            .into_iter()
            .map(|r| r.badge)
            .collect()
    }

    /// ANN-6.AC1: style `upper` and start 3 read C, D, E; after the second is deleted, C, D. The
    /// style applies to all steps in one edit and new steps follow it.
    #[test]
    fn ann_6_ac1_numbering_applies_to_all_steps() {
        let mut s = st();
        place_steps(&mut s, 3);
        assert_eq!(badges(&s), ["1", "2", "3"]);
        let e = s
            .set_step_numbering(Some(StepStyle::Upper), Some(3))
            .expect("one edit");
        assert!(matches!(e, Edit::Replace(_)));
        assert_eq!(badges(&s), ["C", "D", "E"]);
        assert!(s
            .set_step_numbering(Some(StepStyle::Upper), Some(3))
            .is_none());
        // The next badge reads F and is stored with the same numbering.
        assert_eq!(s.next_step_label(), "F");
        s.pointer_down(Pt::new(500.0, 100.0), NONE, 1, TOL);
        s.pointer_up(Pt::new(500.0, 100.0), NONE, TOL);
        match &s.ops[3].kind {
            OpKind::Step {
                n, style, start, ..
            } => {
                assert_eq!((*n, *style, *start), (4, StepStyle::Upper, 3));
            }
            _ => unreachable!(),
        }
        // Delete the second: C, D, E.
        s.select_step_row(1);
        s.delete_step_row(1).expect("edit");
        assert_eq!(badges(&s), ["C", "D", "E"]);
        // The remembered numbering reaches the settings and back.
        let back = Styles::from_json(&s.styles.to_json());
        assert_eq!(back.get(Tool::Step).step_style, StepStyle::Upper);
        assert_eq!(back.get(Tool::Step).step_start, 3);
    }

    /// ANN-6: the "Start at" field reads the first number in the current style, and a row shows
    /// the label typed into it before it is committed.
    #[test]
    fn ann_6_start_reads_in_the_style_and_rows_show_the_label_as_typed() {
        let mut s = st();
        place_steps(&mut s, 2);
        assert_eq!(s.steps_info().unwrap().start_label, "1");
        s.set_step_numbering(Some(StepStyle::Upper), Some(3));
        assert_eq!(s.steps_info().unwrap().start_label, "C");
        s.set_step_numbering(Some(StepStyle::Lower), None);
        assert_eq!(s.steps_info().unwrap().start_label, "c");
        s.set_step_numbering(Some(StepStyle::Number), None);
        assert_eq!(s.steps_info().unwrap().start_label, "3");
        // Selecting a row and typing: the row shows the text at once.
        s.select_step_row(1);
        s.edit_selected_text();
        s.text_ime(None, "Open", false);
        let i = s.steps_info().unwrap();
        assert!(i.editing);
        assert_eq!(i.rows[1].text, "Open");
        assert_eq!(i.rows[0].text, "");
        assert!(s.finish_text().is_some());
        assert_eq!(s.steps_info().unwrap().rows[1].text, "Open");
    }

    /// ANN-6.AC2: the 26th step reads Z and the 27th AA.
    #[test]
    fn ann_6_ac2_letters_continue_past_z() {
        let mut s = st();
        place_steps(&mut s, 27);
        s.set_step_numbering(Some(StepStyle::Upper), Some(1));
        let b = badges(&s);
        assert_eq!((b[25].as_str(), b[26].as_str()), ("Z", "AA"));
        s.set_step_numbering(Some(StepStyle::Lower), None);
        assert_eq!(badges(&s)[26], "aa");
    }

    /// ANN-6.AC4: reordering steps 1, 2, 3 to 3, 1, 2 rewrites `n` in ONE edit; the ops keep
    /// their place in the list; deleting afterwards keeps the new order.
    #[test]
    fn ann_6_ac4_reorder_is_one_edit_and_survives_a_delete() {
        let mut s = st();
        place_steps(&mut s, 3);
        let ids: Vec<String> = s.ops.iter().map(|o| o.id.clone()).collect();
        let e = s.reorder_steps(2, 0).expect("one edit");
        let Edit::Replace(ops) = e else { panic!() };
        assert_eq!(ops.iter().map(|o| o.id.clone()).collect::<Vec<_>>(), ids);
        assert_eq!(step_ns(&s), [2, 3, 1]);
        assert_eq!(s.steps_info().unwrap().rows[0].id, ids[2]);
        assert!(s.reorder_steps(1, 1).is_none());
        assert!(s.reorder_steps(0, 7).is_none());
        // Delete the step that is now first: the rest close up in their new order.
        s.delete_step_row(0).expect("edit");
        assert_eq!(step_ns(&s), [1, 2]);
        assert_eq!(s.steps_info().unwrap().rows[0].id, ids[0]);
    }

    /// ANN-1.AC2 still holds with the new fields: deleting renumbers and keeps style, start and
    /// label.
    #[test]
    fn ann_6_delete_keeps_style_start_and_text() {
        let mut s = st();
        place_steps(&mut s, 3);
        s.set_step_numbering(Some(StepStyle::Lower), Some(2));
        s.select_step_row(2);
        s.edit_selected_text();
        s.text_ime(None, "Open Settings", false);
        s.finish_text().expect("edit");
        s.select_step_row(1);
        s.delete_selected().expect("edit");
        assert_eq!(step_ns(&s), [1, 2]);
        match &s.ops[1].kind {
            OpKind::Step {
                style, start, text, ..
            } => {
                assert_eq!(*style, StepStyle::Lower);
                assert_eq!(*start, 2);
                assert_eq!(text.as_deref(), Some("Open Settings"));
            }
            _ => unreachable!(),
        }
    }

    /// ANN-6: the label goes through the Text tool's input method (Telex composes), is one
    /// line, is trimmed, and an empty label removes it without removing the step.
    #[test]
    fn ann_6_label_is_typed_on_the_image_and_can_be_cleared() {
        let mut s = st();
        place_steps(&mut s, 1);
        s.edit_selected_text();
        let t = s.text.as_ref().unwrap();
        assert!(t.single_line());
        assert!(matches!(t.target, TextTarget::Step(_)));
        for st_ in ["M", "Mơ", "Mở"] {
            s.text_ime(None, st_, true);
        }
        s.text_ime(None, "Mở  ", false);
        s.finish_text().expect("edit");
        match &s.ops[0].kind {
            OpKind::Step { text, .. } => assert_eq!(text.as_deref(), Some("Mở")),
            _ => unreachable!(),
        }
        assert_eq!(s.steps_info().unwrap().rows[0].text, "Mở");
        // Clearing it keeps the step.
        s.edit_selected_text();
        s.text.as_mut().unwrap().text.clear();
        s.finish_text().expect("edit");
        assert_eq!(s.ops.len(), 1);
        match &s.ops[0].kind {
            OpKind::Step { text, .. } => assert!(text.is_none()),
            _ => unreachable!(),
        }
        // Double click on the badge edits its label too.
        s.set_tool(Tool::Select);
        s.pointer_down(Pt::new(50.0, 100.0), NONE, 2, TOL);
        assert!(s.text.is_some());
    }

    /// ANN-6: the panel exists for the Step tool or a selected step, and nothing else.
    #[test]
    fn ann_6_steps_panel_visibility() {
        let mut s = st();
        assert!(s.steps_info().is_none());
        s.set_tool(Tool::Step);
        let i = s.steps_info().unwrap();
        assert!(i.rows.is_empty() && i.selected.is_none());
        place_steps(&mut s, 2);
        assert_eq!(s.steps_info().unwrap().selected, Some(1));
        s.set_tool(Tool::Rect);
        assert!(s.steps_info().is_none());
    }

    // ---- ANN-4: stamps --------------------------------------------------------------------

    fn stamp_of(s: &EditorState, i: usize) -> (StampKind, Option<String>, f64, Color) {
        match &s.ops[i].kind {
            OpKind::Stamp {
                kind,
                text,
                size,
                color,
                ..
            } => (*kind, text.clone(), *size, *color),
            other => panic!("{other:?}"),
        }
    }

    /// ANN-4: a click places a check mark (green, 34 px) in one Append.
    #[test]
    fn ann_4_click_places_a_stamp() {
        let mut s = st();
        s.set_tool(Tool::Stamp);
        let e = s.pointer_down(Pt::new(300.0, 200.0), NONE, 1, TOL);
        let op = only_append(&e).clone();
        match op.kind {
            OpKind::Stamp {
                at,
                kind,
                size,
                color,
                text,
            } => {
                assert_eq!(at, Pt::new(300.0, 200.0));
                assert_eq!((kind, size), (StampKind::Check, 34.0));
                assert_eq!(color, Color::rgb(0x2E, 0xB8, 0x72));
                assert!(text.is_none());
            }
            other => panic!("{other:?}"),
        }
        assert!(s.pointer_up(Pt::new(300.0, 200.0), NONE, TOL).is_empty());
    }

    /// ANN-4: BEFORE / AFTER write their word into the op in the UI language; the op passes the
    /// core validation (a label stamp needs text); symbols carry no text.
    #[test]
    fn ann_4_label_stamps_carry_their_word() {
        let mut s = st();
        s.set_tool(Tool::Stamp);
        assert!(s.set_stamp(StampKind::Label, StampWord::After).is_none());
        s.pointer_down(Pt::new(100.0, 100.0), NONE, 1, TOL);
        let (kind, text, _, color) = stamp_of(&s, 0);
        assert_eq!((kind, text.as_deref()), (StampKind::Label, Some("After")));
        assert_eq!(color, cm_core::annotation::DEFAULT_COLOR);
        assert!(cm_core::annotation::validate_op(0, &s.ops[0]).is_ok());
        // Changing the selected stamp in the picker is one Replace and fixes text and colour.
        let e = s
            .set_stamp(StampKind::Check, StampWord::Before)
            .expect("edit");
        assert!(matches!(e, Edit::Replace(_)));
        let (kind, text, _, color) = stamp_of(&s, 0);
        assert_eq!((kind, text), (StampKind::Check, None));
        assert_eq!(color, Color::rgb(0x2E, 0xB8, 0x72));
        let e = s
            .set_stamp(StampKind::Label, StampWord::Before)
            .expect("edit");
        assert!(matches!(e, Edit::Replace(_)));
        assert_eq!(stamp_of(&s, 0).1.as_deref(), Some("Before"));
        assert!(cm_core::annotation::validate_op(0, &s.ops[0]).is_ok());
        let info = s.stamp_info().unwrap();
        assert_eq!(
            (info.kind, info.word),
            (StampKind::Label, Some(StampWord::Before))
        );
    }

    /// ANN-4: size and colour apply to the selected stamp; move and scale work; Enter places one.
    #[test]
    fn ann_4_stamp_size_colour_move_and_enter() {
        let mut s = st();
        s.set_tool(Tool::Stamp);
        s.set_stamp(StampKind::Cross, StampWord::Before);
        s.pointer_down(Pt::new(300.0, 200.0), NONE, 1, TOL);
        assert_eq!(stamp_of(&s, 0).3, cm_core::annotation::DEFAULT_COLOR);
        assert!(matches!(
            s.set_param(ParamValue::StampSize(64.0)),
            Some(Edit::Replace(_))
        ));
        assert_eq!(stamp_of(&s, 0).2, 64.0);
        let blue = Color::rgb(0x2E, 0x8B, 0xFF);
        s.set_param(ParamValue::Color(blue)).expect("edit");
        assert_eq!(stamp_of(&s, 0).3, blue);
        // Move with the Select tool.
        s.set_tool(Tool::Select);
        let e = drag(&mut s, (300.0, 200.0), (340.0, 230.0), NONE);
        assert_eq!(e.len(), 1);
        match &s.ops[0].kind {
            OpKind::Stamp { at, .. } => assert_eq!(*at, Pt::new(340.0, 230.0)),
            _ => unreachable!(),
        }
        // The scale handle sits on the bottom-right of the stamp.
        let ch = s.chrome().unwrap();
        assert_eq!(ch.bounds, Rc::new(308.0, 198.0, 64.0, 64.0));
        assert_eq!(ch.handles, vec![(Handle::Scale, Pt::new(372.0, 262.0))]);
        // Enter places one at the centre.
        s.set_tool(Tool::Stamp);
        let e = s.place_default(Pt::new(600.0, 400.0));
        assert_eq!(e.len(), 1);
        assert_eq!(s.ops.len(), 2);
        assert_eq!(Tool::from_key('u'), Some(Tool::Stamp));
        assert_eq!(Tool::Stamp.group(), crate::editor::Group::Markers);
    }

    /// The remembered stamp kind and word survive the settings round trip.
    #[test]
    fn ann_4_stamp_choice_persists() {
        let mut s = st();
        s.set_stamp(StampKind::Label, StampWord::After);
        let back = Styles::from_json(&s.styles.to_json());
        assert_eq!(back.get(Tool::Stamp).stamp_kind, StampKind::Label);
        assert_eq!(back.get(Tool::Stamp).stamp_word, StampWord::After);
    }
}
