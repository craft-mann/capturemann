//! The Captions panel and the captions lane (R3c, `specs/caption-editor-plan.md`): one row per
//! line with its times, the text edited in place, hover actions, find and replace, the footer with
//! the display switches and the exports, and the lane under the timeline. Drawing and events
//! only: the rules are in `crate::ai::captions` (split at the caret, issues), the search in
//! `crate::ai::caption_find`, the follow, width and lane decisions in `crate::ai::caption_view`
//! (all unit tested); everything saved goes through `ai_save` to the daemon, so undo and redo
//! work as before. Not seen by eye yet (`tests/manual/CAP-EDIT.md`).

use std::ops::Range;
use std::time::Instant;

use cm_ai::caption::CaptionLine;
use cm_ai::doubtful::spans_in_text;
use cm_i18n::{tr, tr_ctx, trn};
use gpui::{
    div, prelude::*, px, AnyElement, Context, FontWeight, HighlightStyle, MouseButton,
    MouseDownEvent, Role, SharedString, Stateful, StyledText, UnderlineStyle, Window,
};

use super::ai_cards::{stamp, AiNote, Pending};
use super::tabs::{hint, panel, split_at_key, Inspector, KEY_MARK};
use super::{RecordingEditor, Session};
use crate::ai::caption_find::{line_ranges, line_text, search, Match};
use crate::ai::caption_view::{
    doubtful_jump, follow_allowed, follow_target, lane_blocks, lane_label, LinePlay,
};
use crate::ai::captions::{caret_char_index, editable_text, neighbor, LineIssue};
use crate::ai::client::CaptionFormat;
use crate::recedit::geom::LABEL_W;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::ai;
use crate::ui::components::{ellipsis_text, icon, keycap, shrink_text, switch};
use crate::ui::paint::{hsla_of, rgba_of, TextExt};
use crate::ui::scroll::fill_scroll;
use crate::ui::settings::widgets::small_button_with_icon;
use crate::ui::text_field::{CaptionKey, CaretAt, TextFieldEvent};

/// Height of the captions lane, px (the same as the zoom lane).
const LANE_H: f32 = 26.0;
/// Width of the time column of a row, px.
const TIME_W: f32 = 72.0;

/// What a row's own Merge and Remove buttons ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RowOp {
    Merge,
    Remove,
}

impl RecordingEditor {
    // ---- editing -------------------------------------------------------------------------

    pub(super) fn ai_field_event(&mut self, ev: &TextFieldEvent, cx: &mut Context<Self>) {
        match ev {
            TextFieldEvent::Changed(text) => {
                if let Some(i) = self.ai.field_for {
                    self.ai.captions.set_text(i, text);
                    cx.notify();
                }
            }
            // Enter: the line is done, the next one opens (the last one just closes).
            TextFieldEvent::Submitted => {
                let Some(i) = self.ai.field_for else { return };
                let count = self.ai.captions.lines().len();
                match neighbor(i, count, true) {
                    Some(next) => {
                        self.ai.pending = Some(Pending::Edit {
                            line: next,
                            at: CaretAt::Start,
                        })
                    }
                    None => self.ai_finish_edit(cx),
                }
                cx.notify();
            }
            // Esc: done, the edit is kept (and saved).
            TextFieldEvent::Escaped => {
                self.ai.line_play = None;
                self.ai_finish_edit(cx);
                cx.notify();
            }
        }
    }

    /// Leaves the field: saves what was typed and gives the keyboard back to the editor.
    fn ai_finish_edit(&mut self, cx: &mut Context<Self>) {
        self.ai_save(cx);
        self.ai.field_for = None;
        self.ai.pending = Some(Pending::Defocus);
    }

    pub(super) fn ai_caption_key(&mut self, key: CaptionKey, cx: &mut Context<Self>) {
        let Some(i) = self.ai.field_for else { return };
        match key {
            CaptionKey::SplitAtCursor => self.ai_split(i, cx),
            CaptionKey::Doubtful { forward } => {
                if let Some(j) = self.ai_doubtful_jump(forward, cx) {
                    self.ai.pending = Some(Pending::Word {
                        line: j.line,
                        byte: j.byte,
                        start: j.start,
                    });
                    cx.notify();
                }
            }
            CaptionKey::Edge { up, column } => {
                let count = self.ai.captions.lines().len();
                if let Some(line) = neighbor(i, count, !up) {
                    self.ai.pending = Some(Pending::Edit {
                        line,
                        at: CaretAt::Column {
                            column,
                            last_line: up,
                        },
                    });
                    cx.notify();
                }
            }
        }
    }

    /// Another tab was opened while a caption or the find bar was open: save what was typed and
    /// give the keyboard back, or the single-letter shortcuts would stay off with no field on
    /// screen.
    pub(super) fn ai_leave_captions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai.field_for.is_none() && !self.ai.find_open {
            return;
        }
        self.ai_save(cx);
        self.ai.field_for = None;
        self.ai.find_open = false;
        self.ai.pending = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Runs the move that waited for a window: the key and click handlers cannot focus anything.
    pub(super) fn ai_apply_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.ai.pending.take() else {
            return;
        };
        match p {
            Pending::Edit { line, at } => self.ai_edit(line, at, window, cx),
            Pending::Defocus => window.focus(&self.focus, cx),
            Pending::Word { line, byte, start } => self.ai_open_word(line, byte, start, window, cx),
        }
    }

    /// Where the next (or previous) doubtful word is, from the caret of the field being edited,
    /// else from the selected line, else from the line under the playhead.
    fn ai_doubtful_jump(
        &self,
        forward: bool,
        cx: &mut Context<Self>,
    ) -> Option<crate::ai::caption_view::DoubtfulJump> {
        let ai = &self.ai;
        let spans = ai.doubtful();
        let (line, caret) = match ai.field_for {
            Some(i) => {
                let f = ai.field.read(cx);
                (i, Some((f.text().to_owned(), f.cursor_byte())))
            }
            None => {
                let at = match &self.load {
                    super::Load::Ready(s) => ai.captions.line_at(s.model.playhead),
                    _ => None,
                };
                (ai.captions.selected.or(at).unwrap_or(0), None)
            }
        };
        doubtful_jump(
            &spans,
            ai.captions.lines(),
            line,
            caret.as_ref().map(|(t, b)| (t.as_str(), *b)),
            forward,
        )
    }

    /// The Previous and Next doubtful word buttons.
    pub(super) fn ai_goto_doubtful(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(j) = self.ai_doubtful_jump(forward, cx) {
            self.ai_open_word(j.line, j.byte, j.start, window, cx);
        }
    }

    /// Selects the line of a doubtful word, puts the caret at the word, scrolls it into view and
    /// moves the playhead to where the word is spoken.
    fn ai_open_word(
        &mut self,
        line: usize,
        byte: usize,
        start: cm_core::MediaTime,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai_edit(line, CaretAt::Byte(byte), window, cx);
        if let Some(s) = self.session() {
            s.model.seek(start);
        }
        cx.notify();
    }

    /// Opens line `i` for editing in its row, the caret at `at`, and moves the playhead to its
    /// start (the preview shows it). A line with unsaved edits that is left is saved first.
    pub(super) fn ai_edit(
        &mut self,
        i: usize,
        at: CaretAt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ai.captions.is_dirty() && self.ai.field_for != Some(i) {
            self.ai_save(cx);
        }
        let Some(line) = self.ai.captions.lines().get(i) else {
            return;
        };
        let (text, start) = (line_text(line), line.start);
        self.ai.captions.select(Some(i));
        self.ai.field_for = Some(i);
        self.ai.line_play = None;
        let field = self.ai.field.clone();
        field.update(cx, |f, cx| f.load(&text, at, cx));
        window.focus(&field.read(cx).focus.clone(), cx);
        if let Some(s) = self.session() {
            // Typing while the picture runs is no use: stop where the line starts.
            s.model.playing = false;
            s.pacer.stop();
            s.model.seek(start);
            s.geom.reveal(start);
        }
        self.ai.view.scroll_to.set(Some(i));
        cx.notify();
    }

    /// Selects line `i` without editing it (a click on its time or its block on the lane): the
    /// list scrolls to its row and the playhead moves to its start.
    pub(super) fn ai_pick(&mut self, i: usize, cx: &mut Context<Self>) {
        if self.ai.captions.is_dirty() && self.ai.field_for != Some(i) {
            self.ai_save(cx);
        }
        let Some(start) = self.ai.captions.seek_target(i) else {
            return;
        };
        if self.ai.field_for != Some(i) {
            self.ai.field_for = None;
        }
        self.ai.captions.select(Some(i));
        self.ai.line_play = None;
        if let Some(s) = self.session() {
            s.model.playing = false;
            s.pacer.stop();
            s.model.seek(start);
            s.geom.reveal(start);
        }
        self.ai.view.scroll_to.set(Some(i));
        cx.notify();
    }

    pub(super) fn ai_add_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let at = match &self.load {
            super::Load::Ready(s) => s.model.playhead,
            _ => return,
        };
        match self.ai.captions.add_at(at) {
            Ok(i) => {
                self.ai.note = None;
                self.ai_edit(i, CaretAt::End, window, cx);
            }
            Err(why) => {
                self.ai.note = Some(AiNote::AddAt(why));
                cx.notify();
            }
        }
    }

    /// Splits line `i` at the text cursor when it is being edited (the middle word otherwise, or
    /// when the cursor is at either end). Editing goes on in the second half after a split at the
    /// cursor, in the first after the fallback.
    pub(super) fn ai_split(&mut self, i: usize, cx: &mut Context<Self>) {
        let editing = self.ai.field_for == Some(i);
        let caret = editing.then(|| {
            let f = self.ai.field.read(cx);
            let stored = self.ai.captions.lines().get(i).map(|l| l.text.clone());
            stored.map(|s| caret_char_index(f.text(), f.cursor_byte(), &editable_text(&s)))
        });
        let caret = caret.flatten();
        let at_caret = caret.is_some_and(|c| self.ai.captions.splits_at_caret(i, c));
        let ok = match caret {
            Some(c) => self.ai.captions.split_at(i, c),
            None => self.ai.captions.split(i),
        };
        if !ok {
            return;
        }
        if editing {
            self.ai.pending = Some(if at_caret {
                Pending::Edit {
                    line: i + 1,
                    at: CaretAt::Start,
                }
            } else {
                Pending::Edit {
                    line: i,
                    at: CaretAt::End,
                }
            });
        }
        cx.notify();
    }

    /// Merge with next or Remove on row `i` (from its hover icons or its own action row).
    pub(super) fn ai_row_op(&mut self, i: usize, op: RowOp, cx: &mut Context<Self>) {
        // An edit field on another row would point at a shifted line: close it first.
        let own = self.ai.field_for == Some(i);
        if !own {
            self.ai.field_for = None;
        }
        let ok = match op {
            RowOp::Merge => self.ai.captions.merge_with_next(i),
            RowOp::Remove => self.ai.captions.remove(i),
        };
        if !ok {
            return;
        }
        self.ai.line_play = None;
        match (op, own) {
            (RowOp::Merge, true) => {
                self.ai.pending = Some(Pending::Edit {
                    line: i,
                    at: CaretAt::End,
                })
            }
            (RowOp::Remove, true) => {
                self.ai.field_for = None;
                self.ai.pending = Some(Pending::Defocus);
            }
            _ => {}
        }
        cx.notify();
    }

    /// Plays line `i` alone: from its start to its end, then pauses there.
    pub(super) fn ai_play_line(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(l) = self.ai.captions.lines().get(i) else {
            return;
        };
        let play = LinePlay {
            line: i,
            end: l.end,
        };
        let start = l.start;
        self.ai.captions.select(Some(i));
        self.ai.view.scroll_to.set(Some(i));
        if let Some(s) = self.session() {
            s.model.seek(start);
            s.model.playing = true;
            s.pacer.stop();
        }
        self.ai.line_play = Some(play);
        cx.notify();
    }

    /// Stops "play this line" (Esc, or the button again): playback pauses where it is.
    pub(super) fn ai_stop_line_play(&mut self, cx: &mut Context<Self>) {
        if self.ai.line_play.take().is_some() {
            if let Some(s) = self.session() {
                s.model.playing = false;
                s.pacer.stop();
            }
            cx.notify();
        }
    }

    /// The field shows the text of the line it edits again (after a replace changed it).
    fn ai_reload_field(&mut self, cx: &mut Context<Self>) {
        let Some(j) = self.ai.field_for else { return };
        match self.ai.captions.lines().get(j) {
            Some(l) => {
                let text = line_text(l);
                self.ai
                    .field
                    .update(cx, |f, cx| f.load(&text, CaretAt::End, cx));
            }
            None => self.ai.field_for = None,
        }
    }

    // ---- find and replace ----------------------------------------------------------------

    pub(super) fn ai_matches(&self) -> Vec<Match> {
        if !self.ai.find_open {
            return Vec::new();
        }
        search(
            self.ai.captions.lines(),
            &self.ai.find.query,
            self.ai.find.match_case,
        )
    }

    pub(super) fn ai_open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ai.find_open = true;
        let f = self.ai.find_field.read(cx).focus.clone();
        window.focus(&f, cx);
        cx.notify();
    }

    pub(super) fn ai_close_find(&mut self, cx: &mut Context<Self>) {
        self.ai.find_open = false;
        self.ai.pending = Some(Pending::Defocus);
        cx.notify();
    }

    pub(super) fn ai_find_field_event(
        &mut self,
        ev: &TextFieldEvent,
        is_find: bool,
        cx: &mut Context<Self>,
    ) {
        match ev {
            TextFieldEvent::Changed(text) => {
                if is_find {
                    self.ai.find.query = text.clone();
                    let from = self.ai.captions.selected.unwrap_or(0);
                    let matches = self.ai_matches();
                    self.ai.find.jump_to_line(&matches, from);
                    self.ai_show_match(&matches);
                } else {
                    self.ai.find.replacement = text.clone();
                }
                cx.notify();
            }
            TextFieldEvent::Submitted if is_find => self.ai_find_step(true, cx),
            TextFieldEvent::Submitted => self.ai_replace_current(cx),
            TextFieldEvent::Escaped => self.ai_close_find(cx),
        }
    }

    /// Scrolls the list to the line of the current match.
    fn ai_show_match(&self, matches: &[Match]) {
        if let Some(m) = matches.get(self.ai.find.current) {
            self.ai.view.scroll_to.set(Some(m.line));
        }
    }

    pub(super) fn ai_find_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let matches = self.ai_matches();
        self.ai.find.step(matches.len(), forward);
        self.ai_show_match(&matches);
        cx.notify();
    }

    fn ai_toggle_match_case(&mut self, cx: &mut Context<Self>) {
        self.ai.find.match_case = !self.ai.find.match_case;
        let n = self.ai_matches().len();
        self.ai.find.clamp(n);
        cx.notify();
    }

    /// Replaces the current match and saves, so it is its own undo step.
    pub(super) fn ai_replace_current(&mut self, cx: &mut Context<Self>) {
        let matches = self.ai_matches();
        let Some(m) = matches.get(self.ai.find.current) else {
            return;
        };
        let with = self.ai.find.replacement.clone();
        if self.ai.captions.replace_match(m, &with) {
            self.ai_reload_field(cx);
            let n = self.ai_matches().len();
            self.ai.find.clamp(n);
            self.ai_save(cx);
            cx.notify();
        }
    }

    /// Replaces every match and saves once: one undo step for the whole replacement.
    pub(super) fn ai_replace_all(&mut self, cx: &mut Context<Self>) {
        if self.ai.find.query.is_empty() {
            return;
        }
        let (query, with, case) = (
            self.ai.find.query.clone(),
            self.ai.find.replacement.clone(),
            self.ai.find.match_case,
        );
        let n = self.ai.captions.replace_all(&query, &with, case);
        if n == 0 {
            return;
        }
        self.ai.note = Some(AiNote::Replaced(n));
        self.ai.find.current = 0;
        self.ai_reload_field(cx);
        self.ai_save(cx);
        cx.notify();
    }

    // ---- the panel -----------------------------------------------------------------------

    /// A 28 px icon button of a row: invisible until the row is hovered or the button has the
    /// keyboard focus (`always` for the selected row), with its name as the tooltip.
    #[allow(clippy::too_many_arguments)]
    fn row_icon_button(
        t: &Theme,
        id: SharedString,
        glyph: &'static str,
        aria: String,
        group: SharedString,
        always: bool,
        tab: bool,
        color: crate::theme::Color,
    ) -> Stateful<gpui::Div> {
        let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(SharedString::from(aria.clone()))
            .focusable()
            .tab_stop(tab)
            .size(px(28.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .border_1()
            .border_color(gpui::transparent_black())
            .cursor_pointer()
            .opacity(if always { 1.0 } else { 0.0 })
            .group_hover(group, |s| s.opacity(1.0))
            .hover(move |s| s.bg(hover))
            .focus_visible(move |s| s.opacity(1.0).border_color(focus))
            .tooltip(crate::ui::tooltip::tip(aria, Vec::new()))
            .child(icon(glyph, 14., color))
    }

    /// One action of the slim row under the field being edited.
    fn edit_action(
        t: &Theme,
        id: &'static str,
        label: &str,
        glyph: &'static str,
        color: crate::theme::Color,
    ) -> Stateful<gpui::Div> {
        let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(SharedString::from(label.to_owned()))
            .focusable()
            .tab_stop(true)
            .h(px(30.))
            .px(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .rounded(px(scale::RADIUS_SM))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface_2))
            .text_size(px(12.5))
            .text_color(rgba_of(color))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .focus_visible(move |s| s.border_color(focus))
            .child(icon(glyph, 13., color))
            .child(SharedString::from(label.to_owned()))
    }

    /// The text of an idle row: the caption at 15 px, find matches marked. Remembers its layout
    /// so a click can put the caret where it landed.
    #[allow(clippy::too_many_arguments)]
    fn row_text(
        &self,
        t: &Theme,
        i: usize,
        shown: &str,
        hits: &[(Range<usize>, bool)],
        doubtful: &[Range<usize>],
        bright: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = rgba_of(if bright { t.text } else { t.text_3 });
        let empty = shown.trim().is_empty();
        let body: AnyElement = if empty {
            div()
                .text_color(rgba_of(t.faint))
                .child(SharedString::from(tr_ctx!("caption line", "(empty)")))
                .into_any_element()
        } else {
            let mut styled = StyledText::new(SharedString::from(shown.to_owned()));
            // Doubtful words: a wavy underline in the warning tone, under any find highlight.
            let doubts = doubtful.iter().map(|r| {
                (
                    r.clone(),
                    HighlightStyle {
                        underline: Some(UnderlineStyle {
                            thickness: px(1.5),
                            color: Some(hsla_of(t.warning)),
                            wavy: true,
                        }),
                        ..Default::default()
                    },
                )
            });
            if hits.is_empty() && !doubtful.is_empty() {
                styled = styled.with_highlights(doubts.collect::<Vec<_>>());
            } else if !hits.is_empty() {
                let marks = hits.iter().map(|(r, current)| {
                    let style = if *current {
                        HighlightStyle {
                            background_color: Some(hsla_of(t.text)),
                            color: Some(hsla_of(t.primary_fg)),
                            ..Default::default()
                        }
                    } else {
                        HighlightStyle {
                            background_color: Some(hsla_of(t.mist.with_alpha(35))),
                            ..Default::default()
                        }
                    };
                    (r.clone(), style)
                });
                styled = styled.with_highlights(
                    gpui::combine_highlights(marks.collect::<Vec<_>>(), doubts.collect::<Vec<_>>())
                        .collect::<Vec<_>>(),
                );
            }
            self.ai
                .view
                .layouts
                .borrow_mut()
                .insert(i, styled.layout().clone());
            div().child(styled).into_any_element()
        };
        let view = self.ai.view.clone();
        let len = shown.len();
        div()
            .id(SharedString::from(format!("re-cap-text-{i}")))
            .flex_1()
            .min_w(px(0.))
            .text_size(px(15.))
            .line_height(px(23.25))
            .text_color(color)
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                    let layout = view.layouts.borrow().get(&i).cloned();
                    let at = layout
                        .filter(|l| l.len() == len)
                        .map(|l| match l.index_for_position(ev.position) {
                            Ok(b) | Err(b) => CaretAt::Byte(b),
                        })
                        .unwrap_or(CaretAt::End);
                    this.ai_edit(i, at, window, cx);
                }),
            )
            .child(body)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn caption_row(
        &self,
        t: &Theme,
        i: usize,
        l: &CaptionLine,
        count: usize,
        flags: (bool, bool),
        hits: &[(Range<usize>, bool)],
        doubtful: &[Range<usize>],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (is_sel, is_active) = flags;
        let editing = self.ai.field_for == Some(i);
        let issues = self.ai.captions.issues_of(i);
        let group = SharedString::from(format!("re-cap-row-{i}"));
        let shown = line_text(l);
        let playing_this = self.ai.line_play.is_some_and(|p| p.line == i);
        let keys_here = is_sel || is_active;

        let time_color = if is_active || is_sel {
            t.text
        } else {
            t.text_3
        };
        let jump = tr!("Jump to {time}", time = stamp(l.start));
        let time_btn = div()
            .id(SharedString::from(format!("re-cap-time-{i}")))
            .role(Role::Button)
            .aria_label(SharedString::from(jump))
            .focusable()
            .tab_stop(keys_here)
            .flex()
            .flex_col()
            .items_start()
            .gap(px(2.))
            .px(px(6.))
            .py(px(3.))
            .ml(px(-6.))
            .mt(px(-3.))
            .rounded(px(6.))
            .cursor_pointer()
            .when(is_sel, |d| d.bg(rgba_of(t.raised)))
            .hover({
                let h = rgba_of(t.raised);
                move |s| s.bg(h)
            })
            .focus_visible({
                let f = rgba_of(t.text);
                move |s| s.border_1().border_color(f)
            })
            .on_click(cx.listener(move |this, _, _, cx| this.ai_pick(i, cx)))
            .child(
                div()
                    .mono()
                    .text_size(px(12.5))
                    .text_color(rgba_of(time_color))
                    .child(SharedString::from(stamp(l.start))),
            )
            .child(
                div()
                    .mono()
                    .text_size(px(11.))
                    .text_color(rgba_of(t.faint))
                    .child(SharedString::from(format!("\u{2192} {}", stamp(l.end)))),
            );
        let play_aria = if playing_this {
            tr!("Stop playing this line")
        } else {
            tr!("Play this line")
        };
        let play_btn = Self::row_icon_button(
            t,
            SharedString::from(format!("re-cap-play-{i}")),
            if playing_this { "rec-pause" } else { "play" },
            play_aria,
            group.clone(),
            is_sel || playing_this,
            keys_here,
            t.text_2,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            if this.ai.line_play.is_some_and(|p| p.line == i) {
                this.ai_stop_line_play(cx);
            } else {
                this.ai_play_line(i, cx);
            }
        }));
        let time_col = div()
            .relative()
            .flex_none()
            .w(px(TIME_W))
            .child(time_btn)
            .child(div().absolute().top(px(-4.)).right(px(-6.)).child(play_btn));

        let body: AnyElement = if editing {
            let mut col = div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(self.ai.field.clone());
            for issue in &issues {
                col = col.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgba_of(t.warning))
                        .child(SharedString::from(LineIssue::message(issue))),
                );
            }
            // TRANSLATORS: {key} is the Esc keycap, drawn in the sentence; keep it where it reads
            // naturally. The hint sits right of the Split, Merge and Remove buttons.
            let (before, after) = split_at_key(&tr!(
                "Split at the text cursor \u{00B7} {key} done",
                key = KEY_MARK
            ));
            let mut bar = div().flex().flex_wrap().items_center().gap(px(6.)).child(
                Self::edit_action(
                    t,
                    "re-cap-split",
                    &tr_ctx!("caption line", "Split"),
                    "cm-split",
                    t.text,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.ai_split(i, cx))),
            );
            if i + 1 < count {
                bar = bar.child(
                    Self::edit_action(t, "re-cap-merge", &tr!("Merge with next"), "merge", t.text)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.ai_row_op(i, RowOp::Merge, cx)),
                        ),
                );
            }
            bar = bar
                .child(
                    Self::edit_action(
                        t,
                        "re-cap-remove",
                        &tr_ctx!("button", "Remove"),
                        "trash-2",
                        t.rec_soft,
                    )
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.ai_row_op(i, RowOp::Remove, cx)),
                    ),
                )
                .child(
                    div()
                        .ml_auto()
                        .min_w(px(0.))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .text_size(px(11.5))
                        .text_color(rgba_of(t.faint))
                        .child(shrink_text(before))
                        // i18n-ignore: a key name stays in English on its keycap
                        .child(keycap(t, "Esc"))
                        .child(shrink_text(after)),
                );
            col.child(bar).into_any_element()
        } else {
            let text = self.row_text(t, i, &shown, hits, doubtful, is_active || is_sel, cx);
            let mut r = div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .gap(px(8.))
                .items_start()
                .child(text);
            if !issues.is_empty() {
                r = r.child(div().flex_none().pt(px(4.)).child(icon(
                    "triangle-alert",
                    14.,
                    t.warning,
                )));
            }
            r.into_any_element()
        };

        let actions = (!editing).then(|| {
            let mut a = div().flex().flex_none().gap(px(2.)).items_start();
            a = a.child(
                Self::row_icon_button(
                    t,
                    SharedString::from(format!("re-cap-row-split-{i}")),
                    "cm-split",
                    tr_ctx!("caption line", "Split"),
                    group.clone(),
                    is_sel,
                    keys_here,
                    t.text_2,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.ai_split(i, cx))),
            );
            if i + 1 < count {
                a = a.child(
                    Self::row_icon_button(
                        t,
                        SharedString::from(format!("re-cap-row-merge-{i}")),
                        "merge",
                        tr!("Merge with next"),
                        group.clone(),
                        is_sel,
                        keys_here,
                        t.text_2,
                    )
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.ai_row_op(i, RowOp::Merge, cx)),
                    ),
                );
            }
            a.child(
                Self::row_icon_button(
                    t,
                    SharedString::from(format!("re-cap-row-remove-{i}")),
                    "trash-2",
                    tr_ctx!("button", "Remove"),
                    group.clone(),
                    is_sel,
                    keys_here,
                    t.text_2,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.ai_row_op(i, RowOp::Remove, cx))),
            )
        });

        let aria = if editing {
            tr!(
                "Caption at {time}, editing: {text}",
                time = stamp(l.start),
                text = shown.replace('\n', " ")
            )
        } else {
            tr!(
                "Caption at {time}: {text}",
                time = stamp(l.start),
                text = shown.replace('\n', " ")
            )
        };
        let hover_bg = rgba_of(t.surface_2);
        let bar_color = if is_active {
            Some(t.text)
        } else if is_sel {
            Some(t.mist)
        } else {
            None
        };
        div()
            .id(SharedString::from(format!("re-cap-{i}")))
            .group(group)
            .role(Role::ListItem)
            .aria_label(SharedString::from(aria))
            .focusable()
            .tab_stop(true)
            .on_key_down(
                cx.listener(move |this, ev: &gpui::KeyDownEvent, window, cx| {
                    // Enter on a focused row opens it for editing (the keyboard way in).
                    if ev.keystroke.key == "enter" && this.ai.field_for != Some(i) {
                        this.ai_edit(i, CaretAt::End, window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .relative()
            .flex()
            .gap(px(12.))
            .pl(px(18.))
            .pr(px(16.))
            .py(px(12.))
            .border_t_1()
            .border_color(if i == 0 {
                gpui::transparent_black()
            } else {
                rgba_of(t.row_divider()).into()
            })
            .when(is_sel || is_active, |d| d.bg(rgba_of(t.surface_2)))
            .hover(move |s| s.bg(hover_bg))
            .children(bar_color.map(|c| {
                div()
                    .absolute()
                    .left(px(0.))
                    .top(px(0.))
                    .bottom(px(0.))
                    .w(px(3.))
                    .bg(rgba_of(c))
            }))
            .child(time_col)
            .child(div().flex_1().min_w(px(0.)).child(body))
            .children(actions)
            .into_any_element()
    }

    /// The find and replace bar under the slim bar.
    fn find_bar(&self, t: &Theme, total: usize, cx: &mut Context<Self>) -> AnyElement {
        let f = &self.ai.find;
        let (n, m) = f.position(total);
        let count = if f.query.is_empty() {
            String::new()
        } else if total == 0 {
            tr!("No matches")
        } else {
            // TRANSLATORS: the find bar's match counter: the current match and how many there are.
            tr!("{n} of {m}", n = n, m = m)
        };
        let case_btn = {
            let on = f.match_case;
            let label = tr!("Match case");
            let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
            div()
                .id("re-cap-find-case")
                .role(Role::Button)
                .aria_label(SharedString::from(label.clone()))
                .aria_toggled(if on {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .focusable()
                .tab_stop(true)
                .h(px(32.))
                .px(px(10.))
                .flex()
                .flex_none()
                .items_center()
                .rounded(px(scale::RADIUS_SM))
                .border_1()
                .border_color(rgba_of(if on { t.text } else { t.line }))
                .bg(rgba_of(if on { t.raised } else { t.surface }))
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(if on { t.text } else { t.text_2 }))
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .focus_visible(move |s| s.border_color(focus))
                .child(ellipsis_text(label))
                .on_click(cx.listener(|this, _, _, cx| this.ai_toggle_match_case(cx)))
        };
        let step = |id: &'static str, glyph: &'static str, aria: String, forward: bool| {
            super::tabs::icon_only(t, id, glyph, aria)
                .on_click(cx.listener(move |this, _, _, cx| this.ai_find_step(forward, cx)))
        };
        let can = total > 0;
        let action = |id: &'static str, label: String| {
            let b = small_button_with_icon(t, id, &label, None);
            if can {
                b
            } else {
                b.opacity(0.5).cursor_default()
            }
        };
        let replace = action("re-cap-replace", tr_ctx!("find bar", "Replace"))
            .on_click(cx.listener(|this, _, _, cx| this.ai_replace_current(cx)));
        let replace_all = action("re-cap-replace-all", tr!("Replace all"))
            .on_click(cx.listener(|this, _, _, cx| this.ai_replace_all(cx)));
        div()
            .id("re-cap-find")
            .role(Role::Group)
            .aria_label(SharedString::from(tr!("Find and replace")))
            .flex()
            .flex_col()
            .gap(px(6.))
            .px(px(16.))
            .py(px(10.))
            .border_b_1()
            .border_color(rgba_of(t.row_divider()))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(140.))
                            .child(self.ai.find_field.clone()),
                    )
                    .child(
                        div()
                            .mono()
                            .flex_none()
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.mist))
                            .child(SharedString::from(count)),
                    )
                    .child(step(
                        "re-cap-find-prev",
                        "arrow-up",
                        tr!("Previous match"),
                        false,
                    ))
                    .child(step(
                        "re-cap-find-next",
                        "arrow-down",
                        tr!("Next match"),
                        true,
                    ))
                    .child(case_btn)
                    .child(
                        super::tabs::icon_only(t, "re-cap-find-close", "x", tr!("Close find"))
                            .on_click(cx.listener(|this, _, _, cx| this.ai_close_find(cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(140.))
                            .child(self.ai.replace_field.clone()),
                    )
                    .child(replace)
                    .child(replace_all),
            )
            .into_any_element()
    }

    pub(super) fn captions_panel(
        &self,
        t: &Theme,
        s: &Session,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ai = &self.ai;
        let id = self.id.clone().unwrap_or_default();
        let scope = tr!("Whole recording");
        if ai.loading && !ai.has_transcript {
            return panel(
                t,
                Inspector::Captions,
                scope,
                vec![hint(t, "captions", tr!("Loading\u{2026}")).into_any_element()],
                Vec::new(),
            );
        }
        if !ai.has_transcript {
            return panel(
                t,
                Inspector::Captions,
                scope,
                vec![
                    hint(
                        t,
                        "captions",
                        tr!(
                            "No transcript yet. Transcribe the microphone to get captions; it runs \
                             on this Mac."
                        ),
                    )
                    .into_any_element(),
                    crate::ui::ai_view::transcribe_row(t, &self.app, &id, "re", cx),
                ],
                Vec::new(),
            );
        }
        let lines = ai.captions.lines();
        let count = lines.len();
        let problems = ai.captions.problem_count();
        let dirty = ai.captions.is_dirty();
        let playhead = s.model.playhead;
        let active = ai.captions.line_at(playhead);
        let selected = ai.captions.selected;
        let matches = self.ai_matches();

        // ---- following the playhead (never while typing, never right after a manual scroll)
        let view = &ai.view;
        let since = view.last_manual.get().map(|at: Instant| at.elapsed());
        if let Some(i) = view.scroll_to.take() {
            self.captions_scroll.scroll_to_item(i);
            view.last_followed.set(Some(i));
            view.last_manual.set(None);
        } else if follow_allowed(since, view.typing.get()) {
            if let Some(i) = follow_target(active, view.last_followed.get()) {
                self.captions_scroll.scroll_to_item(i);
                view.last_followed.set(Some(i));
            }
        }
        if active.is_none() {
            view.last_followed.set(None);
        }

        // ---- header
        let shown_line = selected.or(active);
        // The title and the hint share one line, so the rows get the height the second line took;
        // the hint is cut with an ellipsis when the panel is narrow.
        let mut head_left = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(10.))
                    .min_w(px(0.))
                    .child(
                        div()
                            .id("re-panel-title-re-tab-captions")
                            .role(Role::Heading)
                            .aria_level(2)
                            .aria_label(Inspector::Captions.label())
                            .flex_none()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(Inspector::Captions.label()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(12.))
                            .text_color(rgba_of(t.mist))
                            .child(ellipsis_text(SharedString::from(trn!(
                                "{n} line \u{00B7} click text to edit, a time to jump",
                                "{n} lines \u{00B7} click text to edit, a time to jump",
                                count
                            )))),
                    ),
            );
        if problems > 0 {
            head_left = head_left.child(
                div()
                    .text_size(px(12.))
                    .text_color(rgba_of(t.warning))
                    .child(SharedString::from(trn!(
                        "{n} caption breaks the rules (42 characters a row, 2 rows, 1 to 7 s).",
                        "{n} captions break the rules (42 characters a row, 2 rows, 1 to 7 s).",
                        problems
                    ))),
            );
        }
        let find_open = ai.find_open;
        let header = div()
            .flex_none()
            .flex()
            .items_start()
            .gap(px(12.))
            .px(px(16.))
            .py(px(14.))
            .border_b_1()
            .border_color(rgba_of(t.row_divider()))
            .child(head_left)
            .children(shown_line.map(|i| {
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .mono()
                    .text_size(px(12.))
                    .text_color(rgba_of(t.text_3))
                    .child(crate::ui::components::dot(t.text, 7.))
                    // TRANSLATORS: the line that is selected or under the playhead, and how many
                    // lines there are; short, in the panel header.
                    .child(SharedString::from(tr!(
                        "Line {i} of {n}",
                        i = i + 1,
                        n = count
                    )))
            }))
            .child(
                super::tabs::icon_only(t, "re-cap-find-open", "search", tr!("Find and replace"))
                    .when(find_open, |d| d.bg(rgba_of(t.raised)))
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.ai.find_open {
                            this.ai_close_find(cx);
                        } else {
                            this.ai_open_find(window, cx);
                        }
                    })),
            );

        // ---- the slim bar: Add at playhead, Save changes, the note
        let mut slim = div()
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .px(px(16.))
            .py(px(8.))
            .border_b_1()
            .border_color(rgba_of(t.row_divider()))
            .child(
                small_button_with_icon(t, "re-cap-add", &tr!("Add at playhead"), Some("plus"))
                    .on_click(cx.listener(|this, _, window, cx| this.ai_add_line(window, cx))),
            );
        if dirty {
            slim = slim.child(
                small_button_with_icon(
                    t,
                    "re-cap-save",
                    &if ai.saving {
                        tr!("Saving\u{2026}")
                    } else {
                        tr!("Save changes")
                    },
                    Some("save"),
                )
                .on_click(cx.listener(|this, _, _, cx| this.ai_save(cx))),
            );
        }
        let doubt_count = ai.doubtful().len();
        if doubt_count > 0 {
            // Previous / next doubtful word and how many there are (Cmd/Ctrl+Alt+Up/Down from
            // the caption field), on the same row as Add and Save.
            slim = slim.child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        super::tabs::icon_only(
                            t,
                            "re-cap-doubt-prev",
                            "arrow-up",
                            tr!("Previous doubtful word"),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.ai_goto_doubtful(false, window, cx)
                        })),
                    )
                    .child(
                        super::tabs::icon_only(
                            t,
                            "re-cap-doubt-next",
                            "arrow-down",
                            tr!("Next doubtful word"),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.ai_goto_doubtful(true, window, cx)
                        })),
                    )
                    .child(
                        div()
                            .ml(px(4.))
                            .text_size(px(12.))
                            .text_color(rgba_of(t.warning))
                            .child(shrink_text(tr!("{n} doubtful", n = doubt_count))),
                    ),
            );
        }
        if let Some(n) = ai.note.as_ref() {
            slim = slim.child(
                div()
                    .flex_1()
                    .min_w(px(120.))
                    .text_size(px(12.))
                    .text_color(rgba_of(t.text_3))
                    .child(SharedString::from(n.text())),
            );
        }

        // ---- the rows
        ai.view.layouts.borrow_mut().clear();
        let doubtful = ai.doubtful();
        let mut doubt_of: std::collections::HashMap<usize, Vec<Range<usize>>> = Default::default();
        for d in doubtful.iter() {
            doubt_of.entry(d.line).or_default().push(d.range.clone());
        }
        // The field being edited underlines against its own, live text.
        if let Some(l) = ai.field_for.and_then(|i| lines.get(i)) {
            let text = ai.field.read(cx).text().to_owned();
            let ranges = spans_in_text(&text, l.start, l.end, &ai.words)
                .into_iter()
                .map(|s| s.range)
                .collect();
            ai.field.update(cx, |f, _| f.set_doubtful(ranges));
        }
        let rows: Vec<AnyElement> = lines
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let hits = line_ranges(&matches, i, ai.find.current);
                self.caption_row(
                    t,
                    i,
                    l,
                    count,
                    (selected == Some(i), active == Some(i)),
                    &hits,
                    doubt_of.get(&i).map_or(&[][..], Vec::as_slice),
                    cx,
                )
            })
            .collect();
        let view = ai.view.clone();
        let list = div()
            .id("re-cap-list")
            .role(Role::List)
            .aria_label(SharedString::from(tr!("Caption lines")))
            .flex()
            .flex_col()
            .children(rows);
        let list = if count == 0 {
            list.child(div().p(px(16.)).child(hint(
                t,
                "captions",
                tr!("There are no caption lines yet. Add one at the playhead."),
            )))
        } else {
            list
        };
        let list = fill_scroll(t, &self.captions_scroll, list, move || {
            view.last_manual.set(Some(Instant::now()))
        });

        // ---- the footer: display switches, exports
        let options = ai.options;
        let stamps = self.app.read(cx).transcribe.txt_timestamps;
        let app = self.app.clone();
        let toggle = |glyph: &'static str, label: String, sw: Stateful<gpui::Div>| {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .min_w(px(0.))
                .text_size(px(12.5))
                .text_color(rgba_of(t.text_2))
                .child(icon(glyph, 13., t.mist))
                .child(shrink_text(label))
                .child(sw)
        };
        // One line, never wrapped: the two switches share the width, their short labels are cut with an
        // ellipsis when needed, and the switches' own labels (for screen readers) say it in full.
        let display_toggle = |glyph: &'static str, label: String, sw: Stateful<gpui::Div>| {
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(12.5))
                .text_color(rgba_of(t.text_2))
                .child(icon(glyph, 13., t.mist))
                .child(div().flex_1().min_w(px(0.)).child(ellipsis_text(label)))
                .child(sw)
        };
        let display = div()
            .flex()
            .items_center()
            .gap_x(px(16.))
            .child(display_toggle(
                "eye",
                tr!("Show in preview"),
                switch(
                    t,
                    "re-cap-show",
                    tr!("Show captions in the preview"),
                    options.enabled,
                    !ai.options_busy,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    let now = this.ai.options.enabled;
                    this.ai_set_option(Some(!now), None, cx)
                })),
            ))
            .child(display_toggle(
                "file-video",
                tr!("Include in export"),
                switch(
                    t,
                    "re-cap-burn",
                    tr!("Include captions in the exported video"),
                    options.burn_in,
                    !ai.options_busy,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    let now = this.ai.options.burn_in;
                    this.ai_set_option(None, Some(!now), cx)
                })),
            ));
        let export = |id: &'static str, aria: String, fmt: CaptionFormat| {
            small_button_with_icon(t, id, fmt.label(), Some("download"))
                .aria_label(SharedString::from(aria))
                .mono()
                .on_click(cx.listener(move |this, _, _, cx| this.ai_export(fmt, cx)))
        };
        let saves = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(rgba_of(t.mist))
                    .mr(px(4.))
                    .child(tr!("Save a copy")),
            )
            .child(export("re-cap-srt", tr!("Save SRT"), CaptionFormat::Srt))
            .child(export("re-cap-vtt", tr!("Save VTT"), CaptionFormat::Vtt))
            .child(export("re-cap-txt", tr!("Save TXT"), CaptionFormat::Txt))
            .child(div().flex_1())
            .child(toggle(
                "clock",
                tr!("Timestamps in TXT"),
                switch(
                    t,
                    "re-cap-txt-times",
                    tr!("Timestamps in the TXT file"),
                    stamps,
                    true,
                )
                .on_click(move |_, _, cx| {
                    let app = app.clone();
                    cx.defer(move |cx| ai::set_txt_timestamps(&app, !stamps, cx));
                }),
            ));
        let footer = div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(16.))
            .py(px(8.))
            .border_t_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.sidebar))
            .child(display)
            .child(saves);

        div()
            .id("re-panel-re-tab-captions")
            .role(Role::Group)
            .aria_label(SharedString::from(tr!(
                "{title} panel",
                title = Inspector::Captions.label()
            )))
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .rounded(px(scale::RADIUS_XL))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .overflow_hidden()
            .child(header)
            .child(slim)
            .children((find_open).then(|| self.find_bar(t, matches.len(), cx)))
            .child(list)
            .child(footer)
            .into_any_element()
    }

    // ---- the lane ------------------------------------------------------------------------

    /// One block per caption line on the timeline's time axis, under the tracks: the text cut to
    /// the block, the line under the playhead bright, the selected one framed, a line that breaks
    /// a rule outlined in amber. A click selects the line, scrolls the list to it and moves the
    /// playhead there. The lane shares the timeline's zoomed range (`recedit::range`), so the
    /// toolbar's "Timeline zoomed to" label describes it too.
    pub(super) fn captions_lane(
        &self,
        t: &Theme,
        s: &Session,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.ai.has_transcript() || self.ai.lines().is_empty() {
            return None;
        }
        let g = s.geom;
        let lines = self.ai.lines();
        let blocks = lane_blocks(
            lines,
            |m| g.x_of(m),
            g.width,
            s.model.playhead,
            self.ai.selected_line(),
        );
        let els = blocks.into_iter().map(|b| {
            let i = b.index;
            let text = lane_label(&lines[i]);
            let (fill, ink, border) = if b.active {
                (t.text, t.primary_fg, t.text)
            } else {
                (t.raised, t.text_2, t.line)
            };
            let border = if b.problem { t.warning } else { border };
            div()
                .id(SharedString::from(format!("re-cap-block-{i}")))
                .role(Role::Button)
                .aria_label(SharedString::from(tr!(
                    "Caption at {time}: {text}",
                    time = stamp(lines[i].start),
                    text = text.clone()
                )))
                .absolute()
                .left(px(b.x))
                .top(px(2.))
                .w(px(b.w))
                .h(px(LANE_H - 4.))
                .px(px(6.))
                .flex()
                .items_center()
                .overflow_hidden()
                .rounded(px(6.))
                .border_1()
                .border_color(rgba_of(border))
                .when(b.selected, |d| d.border_2().border_color(rgba_of(t.text)))
                .when(b.problem && b.selected, |d| {
                    d.border_color(rgba_of(t.warning))
                })
                .bg(rgba_of(fill))
                .text_size(px(11.))
                .text_color(rgba_of(ink))
                .cursor_pointer()
                .child(ellipsis_text(text))
                .on_click(cx.listener(move |this, _, _, cx| this.ai_pick(i, cx)))
        });
        let playhead = div()
            .absolute()
            .left(px(g.x_of(s.model.playhead) - 0.5))
            .top(px(0.))
            .w(px(1.))
            .h(px(LANE_H))
            .bg(rgba_of(t.text.with_alpha(110)));
        Some(
            div()
                .id("re-captions-lane")
                .role(Role::Group)
                .aria_label(SharedString::from(tr!("Captions lane")))
                .flex()
                .border_t_1()
                .border_color(rgba_of(t.row_divider()))
                .pt(px(8.))
                .gap(px(super::timeline::BODY_GAP))
                .child(
                    div()
                        .w(px(LABEL_W))
                        .h(px(LANE_H))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.text_2))
                        .child(icon("captions", 14., t.mist))
                        .child(ellipsis_text(tr_ctx!("timeline lane", "Captions"))),
                )
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_w(px(0.))
                        .h(px(LANE_H))
                        .overflow_hidden()
                        .children(els)
                        .child(playhead),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use cm_i18n::{tr, tr_ctx, trn, with_language, Lang};

    /// Every sentence this panel words has a Vietnamese text (none falls back to English by a
    /// typo in the key) and keeps its placeholders.
    #[test]
    fn the_panel_wording_is_translated_and_keeps_its_placeholders() {
        let both = |f: &dyn Fn() -> String| (f(), with_language(Lang::Vi, f));
        let pairs = [
            both(&|| tr!("Play this line")),
            both(&|| tr!("Stop playing this line")),
            both(&|| tr!("Find and replace")),
            both(&|| tr!("Match case")),
            both(&|| tr!("Replace all")),
            both(&|| tr_ctx!("find bar", "Replace")),
            both(&|| tr_ctx!("timeline lane", "Captions")),
            both(&|| tr!("Jump to {time}", time = "0:01.0")),
            both(&|| tr!("Line {i} of {n}", i = 4, n = 14)),
            both(&|| tr!("{n} of {m}", n = 2, m = 9)),
            both(&|| {
                tr!(
                    "Caption at {time}, editing: {text}",
                    time = "0:01.0",
                    text = "Hi"
                )
            }),
            both(&|| {
                trn!(
                    "Replaced {n} place in the captions.",
                    "Replaced {n} places in the captions.",
                    3
                )
            }),
            both(&|| {
                trn!(
                    "{n} line \u{00B7} click text to edit, a time to jump",
                    "{n} lines \u{00B7} click text to edit, a time to jump",
                    14
                )
            }),
        ];
        for (en, vi) in &pairs {
            assert_ne!(en, vi, "not translated: {en}");
        }
        let (_, vi) = both(&|| tr!("Line {i} of {n}", i = 4, n = 14));
        assert!(vi.contains('4') && vi.contains("14"), "{vi}");
        let (_, vi) = both(&|| {
            tr!(
                "Caption at {time}, editing: {text}",
                time = "0:01.0",
                text = "Hi"
            )
        });
        assert!(vi.contains("0:01.0") && vi.contains("Hi"), "{vi}");
    }

    /// AI-6a: the doubtful-word controls and the kept-edits wording have Vietnamese texts and keep
    /// their numbers.
    #[test]
    fn the_doubtful_word_wording_is_translated_and_keeps_its_numbers() {
        let both = |f: &dyn Fn() -> String| (f(), with_language(Lang::Vi, f));
        for (en, vi) in [
            both(&|| tr!("Previous doubtful word")),
            both(&|| tr!("Next doubtful word")),
            both(&|| tr!("{n} doubtful", n = 7)),
        ] {
            assert_ne!(en, vi, "not translated: {en}");
        }
        let (_, vi) = both(&|| tr!("{n} doubtful", n = 7));
        assert!(vi.contains('7'), "{vi}");
        let (_, vi) = both(&|| crate::ai::summary::kept_edits_note(2, 1).unwrap());
        assert!(vi.contains('2') && vi.contains('3'), "{vi}");
        let (_, vi) = both(&|| crate::ai::summary::kept_edits_note(5, 0).unwrap());
        assert!(vi.contains('5'), "{vi}");
        let (_, vi) = both(&|| crate::ai::summary::kept_edits_note(0, 4).unwrap());
        assert!(vi.contains('4'), "{vi}");
    }
}
