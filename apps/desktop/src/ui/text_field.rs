//! A text field: one line (the library search box, the setup fields) or, with
//! [`TextField::multiline`], several (the YouTube description). GPUI ships no text input widget,
//! so this handles the editing keys directly (caret, backspace/delete, paste, select all, Enter,
//! Escape) and takes all typed text from the platform through GPUI's input-handler interface
//! ([`EntityInputHandler`]), which is what makes input methods work: Vietnamese Telex and VNI,
//! Japanese, Chinese and Korean compose text in a marked (underlined) range that the OS
//! replaces key by key and finally commits. A printable key is therefore not handled in
//! `on_key` (it is left to propagate to the platform, which calls `replace_text_in_range` or
//! `replace_and_mark_text_in_range`); the editing logic is `crate::text_buffer`.
//!
//! The multi-line field wraps to its width, starts `min_rows` tall, grows to `max_rows` and then
//! scrolls (with the shared scrollbar); Enter inserts a line break (Cmd/Ctrl+Enter submits),
//! Up and Down move by displayed line, Home and End go to the start and end of the line (up to
//! its line break), and a click puts the caret where it landed. The pure text logic is
//! `crate::text_buffer`; what is drawn here is not seen by eye yet.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use std::ops::Range;

use cm_i18n::tr;
use gpui::{
    canvas, div, point, prelude::*, px, quad, size, BorderStyle, Bounds, BoxShadow, Context,
    ElementInputHandler, EntityInputHandler, EventEmitter, FocusHandle, Focusable, HighlightStyle,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, Render, Role,
    ScrollHandle, SharedString, Stateful, StyledText, TextLayout, UTF16Selection, UnderlineStyle,
    Window,
};

use crate::text_buffer::TextBuffer;
use crate::theme::Color;
use crate::ui::components::icon;
use crate::ui::field_style::{
    FieldTokens, CARET_BLINK_MS, CARET_SOLID_MS, CARET_W, FIELD_H, FIELD_PAD_X, FIELD_RADIUS,
    FIELD_TEXT, FOCUS_BLUE, FOCUS_RING, MULTI_LINE_HEIGHT, MULTI_MAX_H, MULTI_MIN_H, RING_W,
};
use crate::ui::paint::{hsla_of, rgba_of};
use crate::ui::scroll::nested_scroll;
use crate::ui::studio::ThemeGlobal;

/// Line height of the multi-line field (13.5 px text at 1.5).
const LINE_H: f32 = FIELD_TEXT * MULTI_LINE_HEIGHT;
/// Padding (px) of the multi-line field above and below its text.
const MULTI_PAD_Y: f32 = 10.;
/// How often a focused field repaints for the caret blink (ms); the phase itself is
/// [`caret_visible`].
const BLINK_TICK_MS: u64 = 130;
/// The size of the clear button of a search field (px).
const CLEAR_D: f32 = 20.;
/// One bullet of a secret field, and its length in bytes.
const BULLET: &str = "\u{2022}";

/// Whether the caret is drawn `since_input_ms` after the last key, click or edit: solid while
/// typing and for [`CARET_SOLID_MS`] after, then on and off every [`CARET_BLINK_MS`].
pub fn caret_visible(since_input_ms: u64) -> bool {
    if since_input_ms < CARET_SOLID_MS {
        return true;
    }
    (since_input_ms / CARET_BLINK_MS).is_multiple_of(2)
}

/// What a click does at `click_count` clicks in a row: the caret, a word, everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickKind {
    Caret,
    Word,
    All,
}

pub fn click_kind(click_count: usize) -> ClickKind {
    match click_count {
        0 | 1 => ClickKind::Caret,
        2 => ClickKind::Word,
        _ => ClickKind::All,
    }
}

/// What the Escape key does, in order: a draft the input method left alone is dropped; a
/// clearable (search) field with text is cleared; anything else leaves the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscStep {
    DropDraft,
    Clear,
    Leave,
}

pub fn escape_step(composing: bool, clearable: bool, empty: bool) -> EscStep {
    if composing {
        EscStep::DropDraft
    } else if clearable && !empty {
        EscStep::Clear
    } else {
        EscStep::Leave
    }
}

/// The clear (x) button of a search field shows only when there is something to clear.
pub fn clear_visible(clearable: bool, empty: bool, disabled: bool, read_only: bool) -> bool {
    clearable && !empty && !disabled && !read_only
}

/// How the frame of a field looks (board T0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameState {
    Rest,
    /// Focused: blue border and the 3 px ring.
    Focused,
    /// Focused in a window that is not the active one: the ring is removed.
    FocusedInactive,
    Error,
    ReadOnly,
    Disabled,
}

pub fn frame_state(
    disabled: bool,
    read_only: bool,
    error: bool,
    focused: bool,
    window_active: bool,
) -> FrameState {
    if disabled {
        FrameState::Disabled
    } else if error {
        FrameState::Error
    } else if read_only {
        FrameState::ReadOnly
    } else if focused && window_active {
        FrameState::Focused
    } else if focused {
        FrameState::FocusedInactive
    } else {
        FrameState::Rest
    }
}

/// The byte offset in the shown text of byte `byte` of the real text: a secret field shows one
/// 3-byte bullet per character.
fn shown_index(text: &str, byte: usize, secret: bool) -> usize {
    if secret {
        text[..byte.min(text.len())].chars().count() * BULLET.len()
    } else {
        byte.min(text.len())
    }
}

/// The inverse of [`shown_index`].
fn byte_from_shown(text: &str, shown: usize, secret: bool) -> usize {
    if secret {
        text.char_indices()
            .nth(shown / BULLET.len())
            .map_or(text.len(), |(i, _)| i)
    } else {
        shown.min(text.len())
    }
}

/// The ring outside a field's border: a shadow with no blur and a 3 px spread, so nothing moves.
fn ring(color: Color) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: hsla_of(color),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(RING_W),
        inset: false,
    }]
}

/// The frame of a field in `state`.
fn framed(d: Stateful<gpui::Div>, ft: &FieldTokens, state: FrameState) -> Stateful<gpui::Div> {
    let d = d.border_1();
    match state {
        FrameState::Rest => d
            .border_color(rgba_of(ft.border))
            .bg(rgba_of(ft.bg))
            .hover({
                let (b, bg) = (ft.border_hover, ft.bg_hover);
                move |s| s.border_color(rgba_of(b)).bg(rgba_of(bg))
            }),
        FrameState::Focused => d
            .border_color(rgba_of(FOCUS_BLUE))
            .bg(rgba_of(ft.bg))
            .shadow(ring(FOCUS_RING)),
        FrameState::FocusedInactive => d.border_color(rgba_of(FOCUS_BLUE)).bg(rgba_of(ft.bg)),
        FrameState::Error => d
            .border_color(rgba_of(ft.error))
            .bg(rgba_of(ft.bg))
            .shadow(ring(ft.error_ring)),
        FrameState::ReadOnly => d.border_dashed().border_color(rgba_of(ft.readonly_border)),
        FrameState::Disabled => d
            .border_color(rgba_of(ft.border))
            .bg(rgba_of(ft.bg))
            .opacity(ft.disabled_opacity as f32 / 100.),
    }
}

/// The highlight of selected text: the strong blue (grey when the window is inactive).
fn selection_style(ft: &FieldTokens, window_active: bool) -> HighlightStyle {
    HighlightStyle {
        background_color: Some(hsla_of(if window_active {
            ft.selection
        } else {
            ft.selection_inactive
        })),
        color: window_active.then(|| hsla_of(ft.selection_text)),
        ..Default::default()
    }
}

/// The multi-line mode's state.
struct Multi {
    min_rows: u32,
    max_rows: u32,
    /// Font size and line height of the text, px.
    text_px: f32,
    line_h: f32,
    /// The in-place caption style (R3c): Enter commits, the arrow keys can leave the field. See
    /// [`TextField::in_place`].
    in_place: bool,
    scroll: ScrollHandle,
    /// The layout of the last painted frame; only set once it has been measured.
    layout: Rc<RefCell<Option<TextLayout>>>,
    /// The caret moved: scroll it into view at the next paint.
    reveal: Rc<Cell<bool>>,
    /// The x the caret keeps while moving up and down.
    goal_x: Option<Pixels>,
}

/// Where the one-line field drew its caret and its composing text in the last frame, for the
/// input method's candidate window (the multi-line field asks its text layout instead).
#[derive(Clone, Copy, Default)]
struct LineGeometry {
    caret: Option<Bounds<Pixels>>,
    marked: Option<Bounds<Pixels>>,
    /// The visible text area (the field inside its padding and icon).
    view: Option<Bounds<Pixels>>,
    /// The whole line of text, which can be wider than `view` and is then shifted left.
    row: Option<Bounds<Pixels>>,
}

/// How far the one-line text has to move so the caret stays inside the visible area: `offset` is
/// how far it is shifted left now. Moves right when the caret is past the right edge, back when it
/// is before the left edge, and back when the text ends before the right edge (it was deleted).
/// Never negative.
fn follow_caret(offset: f32, view: (f32, f32), row_right: f32, caret: Option<(f32, f32)>) -> f32 {
    const MARGIN: f32 = 12.;
    let mut next = offset;
    if let Some((left, right)) = caret {
        if right > view.1 - 2. {
            next += right - view.1 + MARGIN;
        } else if left < view.0 {
            next -= view.0 - left + MARGIN;
        }
    }
    if row_right < view.1 && next > 0. {
        next -= view.1 - row_right;
    }
    next.max(0.)
}

pub enum TextFieldEvent {
    Changed(String),
    Submitted,
    Escaped,
}

/// Keys of an [`in_place`](TextField::in_place) field that mean something to the list around it.
/// A separate event type, so the screens that match `TextFieldEvent` are not affected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptionKey {
    /// Cmd/Ctrl+Enter: split the line at the text cursor.
    SplitAtCursor,
    /// Up on the first row (`up`) or Down on the last row of the text. `column` is the caret's
    /// column in chars from the start of its row, so the next field can keep the position.
    Edge { up: bool, column: usize },
    /// Cmd/Ctrl+Alt+Down (`forward`) or Up: the next or previous doubtful word (AI-6a).
    Doubtful { forward: bool },
}

/// Where the caret goes when text is loaded with [`TextField::load`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaretAt {
    Start,
    End,
    /// A byte offset (a click on the row's text).
    Byte(usize),
    /// `column` chars into the first (`last_line == false`) or last line of the text.
    Column {
        column: usize,
        last_line: bool,
    },
    /// Everything selected, so typing replaces it (a name opened for renaming).
    SelectAll,
    /// The part before the extension selected (a file name: `report` in `report.pdf`); the whole
    /// text when there is no extension.
    SelectStem,
}

pub struct TextField {
    pub focus: FocusHandle,
    buffer: TextBuffer,
    placeholder: SharedString,
    label: SharedString,
    icon: Option<&'static str>,
    /// Shows bullets instead of the text and keeps the text out of the accessibility value.
    secret: bool,
    multi: Option<Multi>,
    geometry: Rc<Cell<LineGeometry>>,
    /// The one-line text is shifted left by this much (px) so a long text follows the caret.
    h_off: Rc<Cell<f32>>,
    /// Height of a one-line field, px.
    height: f32,
    /// Byte ranges drawn with a wavy warning underline (the multi-line field only): the words
    /// Whisper was unsure about. Set by the caption editor, never part of the text.
    doubtful: Vec<Range<usize>>,
    /// A search field: shows a clear button when there is text, and Escape clears first.
    clearable: bool,
    /// The message under the field, which also gives it the error frame.
    error: Option<SharedString>,
    disabled: bool,
    read_only: bool,
    /// The text the field was loaded with (`load`, `set_text`): Escape puts it back.
    baseline: Option<String>,
    /// The layout of the last painted one-line text, for clicks and drags.
    line_layout: Rc<RefCell<Option<TextLayout>>>,
    /// The time of the last key, click or edit: the caret stays solid for a moment after it.
    last_input: Instant,
    /// Set by every paint of a focused field; the blink timer repaints only while it is set.
    blink_wanted: bool,
    /// The byte where a drag started.
    drag_from: Option<usize>,
}

impl EventEmitter<TextFieldEvent> for TextField {}
impl EventEmitter<CaptionKey> for TextField {}

impl Focusable for TextField {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TextField {
    pub fn new(
        label: impl Into<SharedString>,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        TextField {
            focus: cx.focus_handle(),
            buffer: TextBuffer::default(),
            placeholder: placeholder.into(),
            label: label.into(),
            icon: Some("search"),
            secret: false,
            multi: None,
            geometry: Rc::default(),
            h_off: Rc::default(),
            height: FIELD_H,
            doubtful: Vec::new(),
            clearable: false,
            error: None,
            disabled: false,
            read_only: false,
            baseline: None,
            line_layout: Rc::default(),
            last_input: Instant::now(),
            blink_wanted: false,
            drag_from: None,
        }
        .with_blink_timer(cx)
    }

    /// Repaints a focused field every [`BLINK_TICK_MS`] so the caret blinks; idle fields do not
    /// repaint.
    fn with_blink_timer(self, cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(BLINK_TICK_MS))
                .await;
            let alive = this.update(cx, |this, cx| {
                if std::mem::take(&mut this.blink_wanted) {
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
        })
        .detach();
        self
    }

    /// A search field: a clear (x) button shows when there is text, the first Escape clears the
    /// text and the second leaves the field.
    pub fn clearable(mut self) -> Self {
        self.clearable = true;
        self
    }

    /// A field that cannot be typed in or focused (drawn at 45 % opacity).
    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    pub fn set_disabled(&mut self, on: bool, cx: &mut Context<Self>) {
        self.disabled = on;
        cx.notify();
    }

    /// A field whose text can be selected and copied but not changed (dashed frame, no fill).
    pub fn read_only(mut self, on: bool) -> Self {
        self.read_only = on;
        self
    }

    pub fn set_read_only(&mut self, on: bool, cx: &mut Context<Self>) {
        self.read_only = on;
        cx.notify();
    }

    /// The field starts in the error state with `msg` under it.
    pub fn error(mut self, msg: impl Into<SharedString>) -> Self {
        self.error = Some(msg.into());
        self
    }

    /// Shows (`Some`) or clears (`None`) the error frame and the message under the field.
    pub fn set_error(&mut self, msg: Option<String>, cx: &mut Context<Self>) {
        self.error = msg.map(SharedString::from);
        cx.notify();
    }

    /// Puts the text the field was loaded with back, without announcing a change (Escape does
    /// this before it emits [`TextFieldEvent::Escaped`]). False when no text was loaded.
    pub fn restore(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(old) = self.baseline.clone() else {
            return false;
        };
        self.buffer.clear();
        self.put(&old);
        self.buffer.set_cursor(old.len());
        cx.notify();
        true
    }

    /// The caret stays solid from now for a moment (a key, a click, an edit).
    fn touch(&mut self) {
        self.last_input = Instant::now();
    }

    fn can_type(&self) -> bool {
        !self.disabled && !self.read_only
    }

    /// The ranges (byte offsets into [`text`](Self::text)) to underline as doubtful. Ranges that
    /// do not fall on character boundaries are ignored when drawing. No repaint is requested: the
    /// caller sets them while it renders.
    pub fn set_doubtful(&mut self, ranges: Vec<Range<usize>>) {
        self.doubtful = ranges;
    }

    /// A one-line field of another height (the find bar uses a compact one).
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    /// A field of several lines: `min_rows` tall when empty, growing to `max_rows`, then
    /// scrolling.
    pub fn multiline(mut self, min_rows: u32, max_rows: u32) -> Self {
        self.multi = Some(Multi {
            min_rows: min_rows.max(1),
            max_rows: max_rows.max(min_rows).max(1),
            text_px: FIELD_TEXT,
            line_h: LINE_H,
            in_place: false,
            scroll: ScrollHandle::new(),
            layout: Rc::default(),
            reveal: Rc::default(),
            goal_x: None,
        });
        self.icon = None;
        self
    }

    /// The caption editor's field (board R3c): the text of a row, edited where it is. 15 px text
    /// on a 23.25 px line, a 56 px minimum height, a 1.5 px frame; Enter submits (the list moves
    /// to the next line), Shift+Enter inserts a line break, Cmd/Ctrl+Enter and the arrow keys at
    /// the first and last row send a [`CaptionKey`]. Call after [`multiline`](Self::multiline).
    pub fn in_place(mut self) -> Self {
        if let Some(m) = self.multi.as_mut() {
            m.text_px = 15.;
            m.line_h = 23.25;
            m.in_place = true;
        }
        self
    }

    fn is_in_place(&self) -> bool {
        self.multi.as_ref().is_some_and(|m| m.in_place)
    }

    /// The caret's byte offset.
    pub fn cursor_byte(&self) -> usize {
        self.buffer.cursor()
    }

    /// Replaces the text without selecting it and without announcing a change (the caller put it
    /// there), with the caret at `at`. The in-place field starts like this: typing adds to the
    /// caption instead of replacing it.
    pub fn load(&mut self, text: &str, at: CaretAt, cx: &mut Context<Self>) {
        self.buffer.clear();
        self.put(text);
        self.baseline = Some(self.buffer.text().to_owned());
        self.touch();
        match at {
            CaretAt::SelectAll => self.buffer.select_all(),
            CaretAt::SelectStem => self.buffer.select_stem(),
            CaretAt::Start => self.buffer.set_cursor(0),
            CaretAt::End => {}
            CaretAt::Byte(b) => self.buffer.set_cursor(b),
            CaretAt::Column { column, last_line } => {
                self.buffer.set_cursor_column(last_line, column)
            }
        }
        if let Some(m) = self.multi.as_mut() {
            m.goal_x = None;
            m.reveal.set(true);
        }
        cx.notify();
    }

    /// The caret's column (chars from the start of its displayed row) when it is on the first
    /// (`up`) or last displayed row, `None` when it is on another row. Uses the painted layout;
    /// before the first paint, the text's own lines.
    fn at_edge(&self, up: bool) -> Option<usize> {
        let m = self.multi.as_ref()?;
        let text = self.buffer.text();
        let layout = m.layout.borrow().clone().filter(|l| l.len() == text.len());
        if let Some(l) = layout {
            let here = l.position_for_index(self.buffer.cursor())?;
            let edge = l.position_for_index(if up { 0 } else { text.len() })?;
            if here.y != edge.y {
                return None;
            }
            let probe = point(px(-1e4), here.y + l.line_height() * 0.5);
            let row_start = match l.index_for_position(probe) {
                Ok(i) | Err(i) => i.min(self.buffer.cursor()),
            };
            return Some(text[row_start..self.buffer.cursor()].chars().count());
        }
        let on_edge = if up {
            self.buffer.line_start() == 0
        } else {
            self.buffer.line_end() >= text.len()
        };
        on_edge.then(|| self.buffer.column())
    }

    fn put(&mut self, text: &str) -> bool {
        if self.multi.is_some() {
            self.buffer.insert_multiline(text)
        } else {
            self.buffer.insert(text)
        }
    }

    /// Up (`-1`) or down (`1`) by displayed line, keeping the x position; the text's own lines
    /// when no frame has been painted for the current text yet.
    fn move_vertical(&mut self, dir: i32) {
        let Some(m) = self.multi.as_mut() else {
            return;
        };
        let layout = m.layout.borrow().clone();
        if let Some(layout) = layout.filter(|l| l.len() == self.buffer.text().len()) {
            if let Some(pos) = layout.position_for_index(self.buffer.cursor()) {
                let lh = layout.line_height();
                let x = m.goal_x.unwrap_or(pos.x);
                let y = pos.y + lh * (dir as f32) + lh * 0.5;
                let at = match layout.index_for_position(point(x, y)) {
                    Ok(i) | Err(i) => i,
                };
                self.buffer.set_cursor(at);
                m.goal_x = Some(x);
                return;
            }
        }
        if dir < 0 {
            self.buffer.up();
        } else {
            self.buffer.down();
        }
    }

    /// Changes the label and the hint of a field that is reused for another provider.
    pub fn set_labels(
        &mut self,
        label: impl Into<SharedString>,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.label = label.into();
        self.placeholder = placeholder.into();
        cx.notify();
    }

    /// A field for a secret (a client secret): the text is shown as bullets.
    pub fn secret(mut self) -> Self {
        self.secret = true;
        self
    }

    /// A plain field without the leading search glyph.
    pub fn without_icon(mut self) -> Self {
        self.icon = None;
        self
    }

    pub fn text(&self) -> &str {
        self.buffer.text()
    }

    /// Replaces the text (the rename box starts with the current title selected for typing over).
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.buffer.clear();
        self.put(text);
        self.buffer.select_all();
        self.baseline = Some(self.buffer.text().to_owned());
        self.touch();
        cx.emit(TextFieldEvent::Changed(self.buffer.text().to_owned()));
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if self.buffer.clear() {
            cx.emit(TextFieldEvent::Changed(String::new()));
        }
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        let m = k.modifiers;
        let cmd = m.platform || m.control;
        let multiline = self.multi.is_some();
        let edit = self.can_type();
        let extend = m.shift && !cmd;
        let anchor = self.buffer.anchor_or_cursor();
        let mut changed = false;
        let mut vertical = false;
        self.touch();
        match k.key.as_str() {
            "backspace" if edit => changed = self.buffer.backspace(),
            "delete" if edit => changed = self.buffer.delete(),
            "left" if extend => self.buffer.extend_left(),
            "right" if extend => self.buffer.extend_right(),
            "left" => self.buffer.left(),
            "right" => self.buffer.right(),
            "home" if multiline => self.buffer.home_line(),
            "end" if multiline => self.buffer.end_line(),
            "home" => self.buffer.home(),
            "end" => self.buffer.end(),
            "up" | "down" if multiline && self.is_in_place() && cmd && m.alt => {
                self.buffer.unmark();
                cx.emit(CaptionKey::Doubtful {
                    forward: k.key == "down",
                });
                cx.stop_propagation();
                return;
            }
            "up" | "down" if multiline && self.is_in_place() => {
                let up = k.key == "up";
                if let Some(column) = self.at_edge(up) {
                    self.buffer.unmark();
                    cx.emit(CaptionKey::Edge { up, column });
                    cx.stop_propagation();
                    return;
                }
                self.move_vertical(if up { -1 } else { 1 });
                vertical = true;
            }
            "up" if multiline => {
                self.move_vertical(-1);
                vertical = true;
            }
            "down" if multiline => {
                self.move_vertical(1);
                vertical = true;
            }
            "escape" => {
                // The ladder (see `escape_step`): a draft the input method left alone is
                // dropped, then a search field's text is cleared, then the field is left: the
                // text it was loaded with comes back and focus goes.
                match escape_step(
                    self.buffer.is_composing(),
                    self.clearable,
                    self.buffer.is_empty(),
                ) {
                    EscStep::DropDraft => {
                        self.buffer.cancel_composition();
                        cx.emit(TextFieldEvent::Changed(self.buffer.text().to_owned()));
                    }
                    EscStep::Clear => {
                        self.clear(cx);
                    }
                    EscStep::Leave => {
                        if edit {
                            self.restore(cx);
                        }
                        if self.clearable {
                            window.blur(cx);
                        }
                        cx.emit(TextFieldEvent::Escaped);
                        return;
                    }
                }
                cx.stop_propagation();
                cx.notify();
                return;
            }
            "enter" if multiline && self.is_in_place() => {
                self.buffer.unmark();
                if m.shift && !cmd {
                    changed = self.put("\n");
                } else if cmd {
                    cx.emit(CaptionKey::SplitAtCursor);
                    cx.stop_propagation();
                    return;
                } else {
                    cx.emit(TextFieldEvent::Submitted);
                    cx.stop_propagation();
                    return;
                }
            }
            "enter" if multiline && !cmd && edit => changed = self.put("\n"),
            "enter" => {
                self.buffer.unmark();
                cx.emit(TextFieldEvent::Submitted);
                cx.stop_propagation();
                return;
            }
            "tab" => return,
            "a" if cmd => self.buffer.select_all(),
            "c" if cmd && !self.secret => {
                if let Some(text) = self.buffer.selected_text() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_owned()));
                }
            }
            "x" if cmd && edit && !self.secret => {
                if let Some(text) = self.buffer.selected_text() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_owned()));
                    changed = self.buffer.backspace();
                }
            }
            "v" if cmd && edit => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    changed = self.put(&text);
                }
            }
            // Typed text is not handled here: the event propagates to the platform, which feeds
            // it through the input method into `replace_text_in_range`.
            _ => return,
        }
        if extend && matches!(k.key.as_str(), "home" | "end" | "up" | "down") {
            // Shift+Home/End/Up/Down: the caret moved, the selection stays anchored.
            self.buffer.set_anchor(anchor);
        }
        if let Some(multi) = self.multi.as_mut() {
            if !vertical {
                multi.goal_x = None;
            }
            multi.reveal.set(true);
        }
        if changed {
            cx.emit(TextFieldEvent::Changed(self.buffer.text().to_owned()));
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl TextField {
    /// After the input method changed the text or the draft.
    fn edited(&mut self, changed: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.touch();
        if let Some(multi) = self.multi.as_mut() {
            multi.goal_x = None;
            multi.reveal.set(true);
        }
        if changed {
            cx.emit(TextFieldEvent::Changed(self.buffer.text().to_owned()));
        }
        // The candidate window follows the draft.
        window.invalidate_character_coordinates();
        cx.notify();
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let (text, used) = self.buffer.text_for_range_utf16(&range);
        *adjusted_range = Some(used);
        Some(text)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.buffer.selection_utf16(),
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.buffer.marked_range_utf16()
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.buffer.unmark();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let multiline = self.multi.is_some();
        let changed = self.buffer.replace_range_utf16(range, text, multiline);
        self.edited(changed, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let multiline = self.multi.is_some();
        let changed = self
            .buffer
            .mark_range_utf16(range, new_text, new_selected_range, multiline);
        self.edited(changed, window, cx);
    }

    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.set_selection_utf16(&range);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let fallback = Bounds::new(element_bounds.origin, size(px(1.), px(LINE_H)));
        if let Some(m) = self.multi.as_ref() {
            let layout = m.layout.borrow().clone();
            let bytes = self.buffer.range_from_utf16(&range);
            let found = layout
                .filter(|l| l.len() == self.buffer.text().len())
                .and_then(|l| {
                    let a = l.position_for_index(bytes.start)?;
                    let b = l.position_for_index(bytes.end).filter(|b| b.y == a.y);
                    let w = b.map_or(px(1.), |b| (b.x - a.x).max(px(1.)));
                    Some(Bounds::new(a, size(w, l.line_height())))
                });
            return Some(found.unwrap_or(fallback));
        }
        let g = self.geometry.get();
        Some(g.marked.or(g.caret).unwrap_or(fallback))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let m = self.multi.as_ref()?;
        let layout = m.layout.borrow().clone();
        let layout = layout.filter(|l| l.len() == self.buffer.text().len())?;
        let (Ok(i) | Err(i)) = layout.index_for_position(point);
        Some(self.buffer.byte_to_utf16(i))
    }
}

impl TextField {
    /// Whether the caret is drawn in this frame: focused, window active, editable, and in the
    /// visible half of the blink.
    fn caret_on(&self, focused: bool, active: bool) -> bool {
        focused
            && active
            && self.can_type()
            && caret_visible(self.last_input.elapsed().as_millis() as u64)
    }

    /// The byte offset of the text under `pos` (a window position), from the last painted layout.
    fn hit(&self, pos: Point<Pixels>) -> Option<usize> {
        let text = self.buffer.text();
        if let Some(m) = self.multi.as_ref() {
            let layout = m.layout.borrow().clone();
            let l = layout.filter(|l| l.len() == text.len())?;
            let (Ok(i) | Err(i)) = l.index_for_position(pos);
            return Some(i);
        }
        let shown_len = if self.secret {
            text.chars().count() * BULLET.len()
        } else {
            text.len()
        };
        let layout = self.line_layout.borrow().clone();
        let l = layout.filter(|l| l.len() == shown_len)?;
        let (Ok(i) | Err(i)) = l.index_for_position(pos);
        Some(byte_from_shown(text, i, self.secret))
    }

    /// A left click: focus, then the caret (one click), a word (two), everything (three); Shift
    /// extends the selection from where it was. The first click in a field that opened fully
    /// selected therefore places the caret.
    fn press(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        window.focus(&self.focus, cx);
        self.touch();
        self.drag_from = None;
        if let Some(m) = self.multi.as_mut() {
            m.goal_x = None;
            m.reveal.set(true);
        }
        let at = self.hit(ev.position);
        match click_kind(ev.click_count) {
            ClickKind::Caret => {
                if let Some(at) = at {
                    if ev.modifiers.shift {
                        let from = self.buffer.anchor_or_cursor();
                        self.buffer.set_cursor(at);
                        self.buffer.set_anchor(from);
                        self.drag_from = Some(from);
                    } else {
                        self.buffer.set_cursor(at);
                        self.drag_from = Some(self.buffer.cursor());
                    }
                }
            }
            ClickKind::Word => match at {
                Some(at) if !self.secret => self.buffer.select_word_at(at),
                _ => self.buffer.select_all(),
            },
            ClickKind::All => self.buffer.select_all(),
        }
        cx.notify();
    }

    /// Dragging with the left button held extends the selection from where the press started.
    fn drag(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !ev.dragging() {
            self.drag_from = None;
            return;
        }
        let Some(from) = self.drag_from else {
            return;
        };
        if let Some(at) = self.hit(ev.position) {
            self.buffer.set_cursor(at);
            self.buffer.set_anchor(from);
            self.touch();
            if let Some(m) = self.multi.as_mut() {
                m.reveal.set(true);
            }
            cx.notify();
        }
    }
}

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        let ft = FieldTokens::of(&t);
        let focused = !self.disabled && self.focus.is_focused(window);
        let active = window.is_window_active();
        if !focused {
            // Focus left in the middle of a composition: the draft stays as typed.
            self.buffer.unmark();
            self.drag_from = None;
        } else {
            self.blink_wanted = true;
        }
        let state = frame_state(
            self.disabled,
            self.read_only,
            self.error.is_some(),
            focused,
            active,
        );
        let field = if self.multi.is_some() {
            self.render_multi(window, cx, &ft, state, focused, active)
        } else {
            self.render_line(cx, &ft, state, focused, active)
        };
        match self.error.clone() {
            Some(msg) => div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(field)
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgba_of(ft.error_text))
                        .child(msg),
                )
                .into_any_element(),
            None => field,
        }
    }
}

impl TextField {
    fn render_line(
        &mut self,
        cx: &mut Context<Self>,
        ft: &FieldTokens,
        state: FrameState,
        focused: bool,
        active: bool,
    ) -> gpui::AnyElement {
        let t = cx.global::<ThemeGlobal>().0;
        self.geometry.set(LineGeometry::default());
        if !focused {
            // A field that is not being typed in shows the start of its text.
            self.h_off.set(0.);
        }
        let caret_on = self.caret_on(focused, active);
        let body = self.line_body(ft, caret_on, active);
        // The text sits in a clipped area and is shifted left when it is longer than the area,
        // so the caret (and the end of the text) can be reached with the keys.
        let (view_geometry, row_geometry) = (self.geometry.clone(), self.geometry.clone());
        let _ = row_geometry;
        let body = div()
            .relative()
            .flex_1()
            .min_w(px(0.))
            .overflow_hidden()
            .flex()
            .items_center()
            .child(
                div()
                    .relative()
                    .flex_none()
                    .ml(px(-self.h_off.get()))
                    .child(body),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |b: Bounds<Pixels>, _, _, _| {
                        let mut g = view_geometry.get();
                        g.view = Some(b);
                        view_geometry.set(g);
                    },
                )
                .absolute()
                .size_full(),
            );
        let (input_focus, geometry, h_off) = (
            self.focus.clone(),
            self.geometry.clone(),
            self.h_off.clone(),
        );
        let entity = cx.entity();
        let can_type = self.can_type();
        let disabled = self.disabled;
        let show_clear = clear_visible(
            self.clearable,
            self.buffer.is_empty(),
            self.disabled,
            self.read_only,
        );
        let clear_button = show_clear.then(|| {
            div()
                .id("text-field-clear")
                .role(Role::Button)
                .aria_label(SharedString::from(tr!("Clear search")))
                .flex_none()
                .size(px(CLEAR_D))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .cursor_pointer()
                .hover(|s| s.bg(rgba_of(t.raised)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.clear(cx);
                        this.touch();
                        window.focus(&this.focus, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(icon("x", 14., t.mist))
        });
        let field = div()
            .id("text-field")
            .debug_selector(|| format!("text-field:{}", self.label))
            .role(Role::TextInput)
            .aria_label(self.label.clone())
            .aria_placeholder(self.placeholder.clone())
            .aria_value(SharedString::from(if self.secret {
                String::new()
            } else {
                self.buffer.text().to_owned()
            }))
            .when(!disabled, |d| d.track_focus(&self.focus).tab_stop(true))
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| this.press(ev, window, cx)),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| this.drag(ev, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &gpui::MouseUpEvent, _, _| this.drag_from = None),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &gpui::MouseUpEvent, _, _| this.drag_from = None),
            )
            .relative()
            .h(px(self.height))
            .px(px(FIELD_PAD_X))
            .flex()
            .items_center()
            .overflow_hidden()
            .gap(px(8.))
            .rounded(px(FIELD_RADIUS))
            .text_color(rgba_of(ft.text))
            .text_size(px(FIELD_TEXT))
            .whitespace_nowrap();
        framed(field, ft, state)
            .when(disabled, |d| d.cursor_default())
            .when(!disabled, |d| d.cursor_text())
            .children(self.icon.map(|name| icon(name, 16., t.mist)))
            .child(body)
            .children(clear_button)
            .children(can_type.then(|| {
                // Hands the platform this field as the text input (typing, composing), and, once
                // the text and the caret are painted, moves the text if the caret left the view.
                canvas(
                    |_, _, _| (),
                    move |bounds: Bounds<Pixels>, _, window, cx| {
                        window.handle_input(
                            &input_focus,
                            ElementInputHandler::new(bounds, entity),
                            cx,
                        );
                        let g = geometry.get();
                        if let (Some(view), Some(row)) = (g.view, g.row) {
                            let now = h_off.get();
                            let next = follow_caret(
                                now,
                                (f32::from(view.left()), f32::from(view.right())),
                                f32::from(row.right()),
                                g.caret.map(|c| (f32::from(c.left()), f32::from(c.right()))),
                            );
                            if (next - now).abs() > 0.5 {
                                h_off.set(next);
                                window.refresh();
                            }
                        }
                    },
                )
                .absolute()
                .size_full()
            }))
            .into_any_element()
    }

    /// The one-line text as one laid-out run: the selection highlighted, the draft being composed
    /// underlined in the focus blue, the caret painted over it once the layout is known.
    fn line_body(&self, ft: &FieldTokens, caret_on: bool, active: bool) -> gpui::AnyElement {
        let text = self.buffer.text();
        let secret = self.secret;
        let map =
            |r: Range<usize>| shown_index(text, r.start, secret)..shown_index(text, r.end, secret);
        let cursor = shown_index(text, self.buffer.cursor(), secret);
        let selection = self.buffer.selection().map(map);
        let marked = self.buffer.marked_range().map(map);
        let geometry = self.geometry.clone();
        let store = self.line_layout.clone();
        let caret_color = hsla_of(ft.caret);
        let mut row = div().relative();
        let layout = if text.is_empty() {
            row = row
                .text_color(rgba_of(ft.placeholder))
                .child(self.placeholder.clone());
            None
        } else {
            let shown = if secret {
                BULLET.repeat(text.chars().count())
            } else {
                text.to_owned()
            };
            let mut styled = StyledText::new(SharedString::from(shown));
            if let Some(r) = selection {
                styled = styled.with_highlights([(r, selection_style(ft, active))]);
            } else if let Some(m) = marked.clone() {
                styled = styled.with_highlights([(
                    m,
                    HighlightStyle {
                        underline: Some(UnderlineStyle {
                            thickness: px(1.),
                            color: Some(hsla_of(FOCUS_BLUE)),
                            wavy: false,
                        }),
                        ..Default::default()
                    },
                )]);
            }
            let layout = styled.layout().clone();
            row = row.child(styled);
            Some(layout)
        };
        let has_selection = self.buffer.selection().is_some();
        row.child(
            canvas(
                |_, _, _| (),
                move |b: Bounds<Pixels>, _, window, _| {
                    store.replace(layout.clone());
                    let (origin, lh) = match &layout {
                        Some(l) => (
                            l.position_for_index(cursor).unwrap_or(b.origin),
                            l.line_height(),
                        ),
                        None => (b.origin, px(FIELD_TEXT * 1.35)),
                    };
                    let caret = Bounds::new(origin, size(px(CARET_W), lh));
                    let mut g = geometry.get();
                    g.row = Some(b);
                    g.caret = Some(caret);
                    g.marked = layout.as_ref().zip(marked.as_ref()).and_then(|(l, m)| {
                        let a = l.position_for_index(m.start)?;
                        let z = l.position_for_index(m.end)?;
                        Some(Bounds::new(a, size((z.x - a.x).max(px(1.)), lh)))
                    });
                    geometry.set(g);
                    if caret_on && !has_selection {
                        window.paint_quad(quad(
                            caret,
                            px(0.),
                            caret_color,
                            px(0.),
                            gpui::transparent_black(),
                            BorderStyle::default(),
                        ));
                    }
                },
            )
            .absolute()
            .size_full(),
        )
        .into_any_element()
    }

    fn render_multi(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
        ft: &FieldTokens,
        state: FrameState,
        focused: bool,
        active: bool,
    ) -> gpui::AnyElement {
        let t = cx.global::<ThemeGlobal>().0;
        let Some(m) = self.multi.as_ref() else {
            return div().into_any_element();
        };
        let text = self.buffer.text().to_owned();
        let empty = text.is_empty();
        let selection = self.buffer.selection();
        let cursor = self.buffer.cursor();
        let (min_rows, max_rows) = (m.min_rows, m.max_rows);
        let (text_px, line_h, in_place) = (m.text_px, m.line_h, m.in_place);
        let scroll = m.scroll.clone();
        let store = m.layout.clone();
        let reveal = m.reveal.clone();
        let caret_color = hsla_of(ft.caret);
        let caret_on = self.caret_on(focused, active);
        let warn_color = hsla_of(t.warning);
        let doubtful = self.doubtful.clone();
        let marked = self.buffer.marked_range();
        let composing = marked.is_some();
        let input_focus = self.focus.clone();
        let entity = cx.entity();
        let can_type = self.can_type();
        let disabled = self.disabled;

        let content = if empty {
            div()
                .relative()
                .text_color(rgba_of(ft.placeholder))
                .child(self.placeholder.clone())
                .children((caret_on).then(|| {
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .w(px(CARET_W))
                        .h(px(line_h))
                        .bg(caret_color)
                }))
                .into_any_element()
        } else {
            let mut styled = StyledText::new(SharedString::from(text.clone()));
            if let Some(r) = selection.clone() {
                styled = styled.with_highlights([(r, selection_style(ft, active))]);
            } else if let Some(m) = marked {
                // The draft being composed is underlined in the focus blue.
                styled = styled.with_highlights([(
                    m,
                    HighlightStyle {
                        underline: Some(UnderlineStyle {
                            thickness: px(1.),
                            color: Some(hsla_of(FOCUS_BLUE)),
                            wavy: false,
                        }),
                        ..Default::default()
                    },
                )]);
            }
            if selection.is_none() && !composing && !doubtful.is_empty() {
                // Doubtful words: a wavy underline in the warning tone (not the error red).
                let mut ranges: Vec<Range<usize>> = doubtful
                    .iter()
                    .filter(|r| {
                        r.start < r.end
                            && r.end <= text.len()
                            && text.is_char_boundary(r.start)
                            && text.is_char_boundary(r.end)
                    })
                    .cloned()
                    .collect();
                ranges.sort_by_key(|r| r.start);
                let mut last_end = 0;
                ranges.retain(|r| {
                    let keep = r.start >= last_end;
                    if keep {
                        last_end = r.end;
                    }
                    keep
                });
                styled = styled.with_highlights(ranges.into_iter().map(|r| {
                    (
                        r,
                        HighlightStyle {
                            underline: Some(UnderlineStyle {
                                thickness: px(1.5),
                                color: Some(warn_color),
                                wavy: true,
                            }),
                            ..Default::default()
                        },
                    )
                }));
            }
            let layout = styled.layout().clone();
            let has_selection = selection.is_some();
            let (scroll, store, reveal) = (scroll.clone(), store.clone(), reveal.clone());
            div()
                .child(styled)
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds: Bounds<Pixels>, _, window, _| {
                            // Painting measured the text: key and mouse handlers may use it now.
                            store.replace(Some(layout.clone()));
                            let lh = layout.line_height();
                            let origin = layout.position_for_index(cursor).unwrap_or(bounds.origin);
                            if caret_on && !has_selection {
                                window.paint_quad(quad(
                                    Bounds::new(origin, size(px(CARET_W), lh)),
                                    px(0.),
                                    caret_color,
                                    px(0.),
                                    gpui::transparent_black(),
                                    BorderStyle::default(),
                                ));
                            }
                            if reveal.replace(false) {
                                // Keep the caret inside the visible rows.
                                let view = scroll.bounds();
                                let mut y = scroll.offset().y;
                                if origin.y + lh > view.bottom() {
                                    y -= origin.y + lh - view.bottom();
                                } else if origin.y < view.top() {
                                    y += view.top() - origin.y;
                                }
                                let y = y.clamp(-scroll.max_offset().y, px(0.));
                                if y != scroll.offset().y {
                                    scroll.set_offset(point(scroll.offset().x, y));
                                    window.refresh();
                                }
                            }
                        },
                    )
                    .absolute()
                    .size_full(),
                )
                .into_any_element()
        };
        let list: Stateful<gpui::Div> = div()
            .id("text-field-lines")
            .relative()
            .w_full()
            .min_h(px(line_h * min_rows as f32))
            .line_height(px(line_h))
            .child(
                // The caret needs the text's origin: the text is the one in-flow child.
                div().relative().w_full().child(content),
            );
        // The box grows to MULTI_MAX_H (padding and border included), then scrolls.
        let inner_max = if in_place {
            line_h * max_rows as f32
        } else {
            (line_h * max_rows as f32).min(MULTI_MAX_H - 2. * MULTI_PAD_Y - 2.)
        };
        let body = nested_scroll(&t, &scroll, inner_max, list);
        let field = div()
            .id("text-field")
            .debug_selector(|| format!("text-field:{}", self.label))
            .relative()
            .role(Role::MultilineTextInput)
            .aria_label(self.label.clone())
            .aria_placeholder(self.placeholder.clone())
            .aria_value(SharedString::from(text))
            .when(!disabled, |d| d.track_focus(&self.focus).tab_stop(true))
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| this.press(ev, window, cx)),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| this.drag(ev, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &gpui::MouseUpEvent, _, _| this.drag_from = None),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &gpui::MouseUpEvent, _, _| this.drag_from = None),
            )
            .w_full()
            .when(in_place, |d| {
                // Board R3c: the field's focus frame, 9/12 padding, 56 px at least.
                d.min_h(px(56.)).px(px(12.)).py(px(9.))
            })
            .when(!in_place, |d| {
                d.min_h(px(MULTI_MIN_H))
                    .px(px(FIELD_PAD_X))
                    .py(px(MULTI_PAD_Y))
            })
            .rounded(px(if in_place { 10. } else { FIELD_RADIUS }))
            .text_color(rgba_of(ft.text))
            .text_size(px(if in_place { text_px } else { FIELD_TEXT }));
        framed(field, ft, state)
            .when(disabled, |d| d.cursor_default())
            .when(!disabled, |d| d.cursor_text())
            .child(body)
            .children(can_type.then(|| {
                canvas(
                    |_, _, _| (),
                    move |bounds: Bounds<Pixels>, _, window, cx| {
                        window.handle_input(
                            &input_focus,
                            ElementInputHandler::new(bounds, entity),
                            cx,
                        );
                    },
                )
                .absolute()
                .size_full()
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod follow_tests {
    use super::follow_caret;

    /// The visible area is x 100..300.
    const VIEW: (f32, f32) = (100., 300.);

    /// Seen on Oct 28, 2026: in the rename box a long title was cut at the right edge and the right
    /// arrow could not reach the rest of it.
    #[test]
    fn a_caret_past_the_right_edge_moves_the_text_left() {
        // The caret (1 px wide) ends at x 341, 41 px past the edge: the text moves 41 + 12 px.
        let next = follow_caret(0., VIEW, 340., Some((340., 341.)));
        assert_eq!(next, 53.);
        // Moving the text left by that puts the caret 12 px inside the edge.
        assert_eq!(341. - next, 288.);
    }

    #[test]
    fn a_caret_inside_the_view_changes_nothing() {
        assert_eq!(follow_caret(30., VIEW, 400., Some((150., 151.))), 30.);
        // Right at the edge still counts as inside (2 px of room).
        assert_eq!(follow_caret(30., VIEW, 400., Some((297., 298.))), 30.);
    }

    #[test]
    fn a_caret_before_the_left_edge_moves_the_text_back_but_never_below_zero() {
        // The text is shifted 80 px and the caret is 30 px left of the view.
        assert_eq!(follow_caret(80., VIEW, 500., Some((70., 71.))), 38.);
        assert_eq!(follow_caret(10., VIEW, 500., Some((70., 71.))), 0.);
    }

    /// After deleting text, the end of it comes back from under the edge.
    #[test]
    fn a_text_that_now_ends_before_the_edge_comes_back() {
        // Shifted 60 px, the text ends at x 250: 50 px short of the edge.
        assert_eq!(follow_caret(60., VIEW, 250., Some((250., 251.))), 10.);
        // Short enough to fit: back to the start.
        assert_eq!(follow_caret(20., VIEW, 150., Some((150., 151.))), 0.);
        // Nothing is shifted and nothing is wrong: it stays.
        assert_eq!(follow_caret(0., VIEW, 150., None), 0.);
    }

    #[test]
    fn without_a_caret_only_the_end_of_the_text_matters() {
        assert_eq!(follow_caret(40., VIEW, 500., None), 40.);
    }
}

#[cfg(test)]
mod behaviour_tests {
    use super::*;

    #[test]
    fn the_caret_is_solid_after_a_key_then_blinks_every_530_ms() {
        assert!(caret_visible(0));
        assert!(caret_visible(499));
        // After the solid half second: on for the rest of the first period, then 530 off, 530 on.
        assert!(caret_visible(500));
        assert!(caret_visible(529));
        assert!(!caret_visible(530));
        assert!(!caret_visible(1059));
        assert!(caret_visible(1060));
        assert!(!caret_visible(1590));
    }

    #[test]
    fn clicks_in_a_row_place_the_caret_then_a_word_then_everything() {
        assert_eq!(click_kind(1), ClickKind::Caret);
        assert_eq!(click_kind(2), ClickKind::Word);
        assert_eq!(click_kind(3), ClickKind::All);
        assert_eq!(click_kind(5), ClickKind::All);
    }

    #[test]
    fn the_clear_button_shows_only_with_text_in_an_editable_search_field() {
        assert!(clear_visible(true, false, false, false));
        assert!(!clear_visible(true, true, false, false), "no text");
        assert!(
            !clear_visible(false, false, false, false),
            "not a search field"
        );
        assert!(!clear_visible(true, false, true, false), "disabled");
        assert!(!clear_visible(true, false, false, true), "read-only");
    }

    #[test]
    fn escape_goes_draft_then_clear_then_leave() {
        assert_eq!(escape_step(true, true, false), EscStep::DropDraft);
        assert_eq!(escape_step(false, true, false), EscStep::Clear);
        assert_eq!(escape_step(false, true, true), EscStep::Leave);
        // A field that is not a search field leaves at once, with text or without.
        assert_eq!(escape_step(false, false, false), EscStep::Leave);
        assert_eq!(escape_step(false, false, true), EscStep::Leave);
    }

    #[test]
    fn the_frame_follows_the_state_with_the_spec_priorities() {
        use FrameState::*;
        assert_eq!(frame_state(false, false, false, false, true), Rest);
        assert_eq!(frame_state(false, false, false, true, true), Focused);
        assert_eq!(
            frame_state(false, false, false, true, false),
            FocusedInactive,
            "the ring goes when the window is inactive"
        );
        assert_eq!(frame_state(false, false, true, true, true), Error);
        assert_eq!(frame_state(false, true, false, true, true), ReadOnly);
        assert_eq!(frame_state(true, true, true, true, true), Disabled);
    }

    #[test]
    fn a_secret_field_maps_offsets_between_the_text_and_its_bullets() {
        let t = "aé\u{1F600}b";
        // a=1 byte, é=2, emoji=4: bullets are 3 bytes per char.
        assert_eq!(shown_index(t, 0, true), 0);
        assert_eq!(shown_index(t, 1, true), 3);
        assert_eq!(shown_index(t, 3, true), 6);
        assert_eq!(shown_index(t, t.len(), true), 12);
        assert_eq!(byte_from_shown(t, 6, true), 3);
        assert_eq!(byte_from_shown(t, 99, true), t.len());
        assert_eq!(byte_from_shown(t, 3, false), 3);
    }
}
