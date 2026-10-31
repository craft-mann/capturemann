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

use std::ops::Range;

use gpui::{
    canvas, div, point, prelude::*, px, quad, size, BorderStyle, Bounds, Context,
    ElementInputHandler, EntityInputHandler, EventEmitter, FocusHandle, Focusable, HighlightStyle,
    KeyDownEvent, MouseButton, MouseDownEvent, Pixels, Point, Render, Role, ScrollHandle,
    SharedString, Stateful, StyledText, TextLayout, UTF16Selection, UnderlineStyle, Window,
};

use crate::text_buffer::TextBuffer;
use crate::theme::{scale, type_scale};
use crate::ui::components::icon;
use crate::ui::paint::{hsla_of, rgba_of};
use crate::ui::scroll::nested_scroll;
use crate::ui::studio::ThemeGlobal;

/// Line height of the multi-line field.
const LINE_H: f32 = 20.;

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
            height: scale::CONTROL_H,
            doubtful: Vec::new(),
        }
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
            text_px: type_scale::UI,
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
        match at {
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
        cx.emit(TextFieldEvent::Changed(self.buffer.text().to_owned()));
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if self.buffer.clear() {
            cx.emit(TextFieldEvent::Changed(String::new()));
        }
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        let m = k.modifiers;
        let cmd = m.platform || m.control;
        let multiline = self.multi.is_some();
        let mut changed = false;
        let mut vertical = false;
        match k.key.as_str() {
            "backspace" => changed = self.buffer.backspace(),
            "delete" => changed = self.buffer.delete(),
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
                // A draft the input method left alone is dropped first; the next Escape leaves.
                if self.buffer.cancel_composition() {
                    cx.emit(TextFieldEvent::Changed(self.buffer.text().to_owned()));
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                cx.emit(TextFieldEvent::Escaped);
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
            "enter" if multiline && !cmd => changed = self.put("\n"),
            "enter" => {
                self.buffer.unmark();
                cx.emit(TextFieldEvent::Submitted);
                cx.stop_propagation();
                return;
            }
            "tab" => return,
            "a" if cmd => self.buffer.select_all(),
            "c" if cmd => {
                if let Some(text) = self.buffer.selected_text() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_owned()));
                }
            }
            "x" if cmd => {
                if let Some(text) = self.buffer.selected_text() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_owned()));
                    changed = self.buffer.clear();
                }
            }
            "v" if cmd => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    changed = self.put(&text);
                }
            }
            // Typed text is not handled here: the event propagates to the platform, which feeds
            // it through the input method into `replace_text_in_range`.
            _ => return,
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

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        let focused = self.focus.is_focused(window);
        if !focused {
            // Focus left in the middle of a composition: the draft stays as typed.
            self.buffer.unmark();
        }
        if self.multi.is_some() {
            return self.render_multi(window, cx);
        }
        self.geometry.set(LineGeometry::default());
        if !focused {
            // A field that is not being typed in shows the start of its text.
            self.h_off.set(0.);
        }
        let focus = self.focus.clone();
        let empty = self.buffer.is_empty();
        let selected = self.buffer.all_selected();
        let body = if empty {
            div()
                .text_color(rgba_of(t.text_3))
                .child(self.placeholder.clone())
                .into_any_element()
        } else {
            self.line_body(&t, focused, selected)
        };
        // The text sits in a clipped area and is shifted left when it is longer than the area,
        // so the caret (and the end of the text) can be reached with the keys.
        let (view_geometry, row_geometry) = (self.geometry.clone(), self.geometry.clone());
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
                    .child(body)
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |b: Bounds<Pixels>, _, _, _| {
                                let mut g = row_geometry.get();
                                g.row = Some(b);
                                row_geometry.set(g);
                            },
                        )
                        .absolute()
                        .size_full(),
                    ),
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
        div()
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
            .track_focus(&self.focus)
            .tab_stop(true)
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx);
            })
            .relative()
            .h(px(self.height))
            .px(px(10.))
            .flex()
            .items_center()
            .overflow_hidden()
            .gap(px(8.))
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface))
            .text_color(rgba_of(t.text))
            .text_size(px(type_scale::UI))
            .whitespace_nowrap()
            .cursor_text()
            .hover(|s| s.border_color(rgba_of(t.mist)))
            .focus(|s| s.border_color(rgba_of(t.text)))
            .children(self.icon.map(|name| icon(name, 16., t.mist)))
            .child(body)
            .child(
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
                .size_full(),
            )
            .into_any_element()
    }
}

impl TextField {
    /// The one-line text: runs split at the caret and at the ends of the draft being composed,
    /// the draft underlined, the caret between them.
    fn line_body(
        &self,
        t: &crate::theme::Theme,
        focused: bool,
        selected: bool,
    ) -> gpui::AnyElement {
        let text = self.buffer.text();
        let cursor = self.buffer.cursor();
        let marked = self.buffer.marked_range();
        let mut cuts = vec![0, cursor, text.len()];
        if let Some(m) = &marked {
            cuts.extend([m.start, m.end]);
        }
        cuts.sort_unstable();
        cuts.dedup();
        let show_caret = focused && !selected;
        let mut row = div().flex().items_center();
        let mut caret_done = false;
        let caret = || {
            let geometry = self.geometry.clone();
            div()
                .relative()
                .w(px(1.))
                .h(px(14.))
                .bg(rgba_of(t.text))
                .child(
                    canvas(
                        |_, _, _| (),
                        move |b: Bounds<Pixels>, _, _, _| {
                            let mut g = geometry.get();
                            g.caret = Some(b);
                            geometry.set(g);
                        },
                    )
                    .absolute()
                    .size_full(),
                )
        };
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a == cursor && show_caret && !caret_done {
                caret_done = true;
                row = row.child(caret());
            }
            let shown = if self.secret {
                "\u{2022}".repeat(text[a..b].chars().count())
            } else {
                text[a..b].to_owned()
            };
            let composing = marked.as_ref().is_some_and(|m| a >= m.start && b <= m.end);
            let mut run = div().relative().child(SharedString::from(shown));
            if selected && b <= cursor {
                run = run.bg(rgba_of(t.line));
            }
            if composing {
                let geometry = self.geometry.clone();
                run = run
                    .underline()
                    .text_decoration_color(hsla_of(t.text))
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |b: Bounds<Pixels>, _, _, _| {
                                let mut g = geometry.get();
                                g.marked = Some(g.marked.map_or(b, |m| m.union(&b)));
                                geometry.set(g);
                            },
                        )
                        .absolute()
                        .size_full(),
                    );
            }
            row = row.child(run);
        }
        if show_caret && !caret_done {
            row = row.child(caret());
        }
        row.into_any_element()
    }

    fn render_multi(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = cx.global::<ThemeGlobal>().0;
        let Some(m) = self.multi.as_ref() else {
            return div().into_any_element();
        };
        let focused = self.focus.is_focused(window);
        let focus = self.focus.clone();
        let text = self.buffer.text().to_owned();
        let empty = text.is_empty();
        let selected = self.buffer.all_selected();
        let cursor = self.buffer.cursor();
        let (min_rows, max_rows) = (m.min_rows, m.max_rows);
        let (text_px, line_h, in_place) = (m.text_px, m.line_h, m.in_place);
        let scroll = m.scroll.clone();
        let store = m.layout.clone();
        let reveal = m.reveal.clone();
        let caret_color = hsla_of(t.text);
        let warn_color = hsla_of(t.warning);
        let doubtful = self.doubtful.clone();
        let marked = self.buffer.marked_range();
        let composing = marked.is_some();
        let input_focus = self.focus.clone();
        let entity = cx.entity();

        let content = if empty {
            div()
                .relative()
                .text_color(rgba_of(t.text_3))
                .child(self.placeholder.clone())
                .children(focused.then(|| {
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .w(px(1.))
                        .h(px(line_h))
                        .bg(rgba_of(t.text))
                }))
                .into_any_element()
        } else {
            let mut styled = StyledText::new(SharedString::from(text.clone()));
            if selected {
                styled = styled.with_highlights([(
                    0..text.len(),
                    HighlightStyle {
                        background_color: Some(hsla_of(t.line)),
                        ..Default::default()
                    },
                )]);
            } else if let Some(m) = marked {
                // The draft being composed is underlined.
                styled = styled.with_highlights([(
                    m,
                    HighlightStyle {
                        underline: Some(UnderlineStyle {
                            thickness: px(1.),
                            color: Some(caret_color),
                            wavy: false,
                        }),
                        ..Default::default()
                    },
                )]);
            }
            if !selected && !composing && !doubtful.is_empty() {
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
                            if focused && !selected {
                                window.paint_quad(quad(
                                    Bounds::new(origin, size(px(1.), lh)),
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
        let body = nested_scroll(&t, &scroll, line_h * max_rows as f32, list);
        div()
            .id("text-field")
            .debug_selector(|| format!("text-field:{}", self.label))
            .relative()
            .role(Role::MultilineTextInput)
            .aria_label(self.label.clone())
            .aria_placeholder(self.placeholder.clone())
            .aria_value(SharedString::from(text))
            .track_focus(&self.focus)
            .tab_stop(true)
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                    window.focus(&focus, cx);
                    if let Some(m) = this.multi.as_mut() {
                        m.goal_x = None;
                        let layout = m.layout.borrow().clone();
                        if let Some(l) = layout.filter(|l| l.len() == this.buffer.text().len()) {
                            let (Ok(i) | Err(i)) = l.index_for_position(ev.position);
                            this.buffer.set_cursor(i);
                        }
                    }
                    cx.notify();
                }),
            )
            .w_full()
            .when(in_place, |d| {
                // Board R3c: a 1.5 px frame in the text colour, the field fill, 9/12 padding.
                d.min_h(px(56.))
                    .px(px(12.))
                    .py(px(9.))
                    .border(px(1.5))
                    .border_color(rgba_of(t.text))
                    .bg(rgba_of(t.field))
            })
            .when(!in_place, |d| {
                d.px(px(10.))
                    .py(px(10.))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.surface))
                    .hover(|s| s.border_color(rgba_of(t.mist)))
                    .focus(|s| s.border_color(rgba_of(t.text)))
            })
            .rounded(px(if in_place { 10. } else { scale::RADIUS_MD }))
            .text_color(rgba_of(t.text))
            .text_size(px(text_px))
            .cursor_text()
            .child(body)
            .child(
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
                .size_full(),
            )
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
