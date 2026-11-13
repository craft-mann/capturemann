//! Inspector panels of the 0.3.3 tools (design spec section 5): Callout, Spotlight, Steps and
//! Stamp. The rows common to every tool (colour, sizes) are `param_group` in `inspector.rs`; the
//! panels add what is specific: the text field, the style switch, the hole list, the order list.
//! All state changes go through `EditorDoc` and the pure `EditorState`.

use super::*;
use crate::editor::tools::StampWord;
use crate::ui::components::ellipsis_text;
use crate::ui::field_style::{
    row, FieldTokens, CARET_W, FIELD_PAD_X, FOCUS_BLUE, FOCUS_RING, RING_W,
};
use cm_core::annotation::StampKind;
use cm_core::annotation::{CalloutStyle, SpotMode, SpotShapeKind, StepStyle, MAX_SPOTLIGHT_SHAPES};

/// The "New in 0.3.3" pill in a panel header (info blue). Shown for the whole release; remove
/// with the palette dot in 0.3.4.
pub(super) fn new_pill(t: &Theme) -> Div {
    let _ = t;
    div()
        .flex_none()
        .px(px(8.))
        .py(px(2.))
        .rounded_full()
        .bg(gpui::rgba(0x5B8C_FF24))
        .text_size(px(11.))
        .text_color(gpui::rgb(0x8FB0FF))
        .child(SharedString::from(tr_ctx!("panel badge", "New in 0.3.3")))
}

/// A blue-tinted note under a panel (the design's info line).
pub(super) fn info_note(t: &Theme, icon_name: Option<&'static str>, text: String) -> Div {
    let _ = t;
    div()
        .flex()
        .items_start()
        .gap(px(8.))
        .p(px(10.))
        .rounded(px(scale::RADIUS_SM))
        .bg(gpui::rgba(0x5B8C_FF14))
        .text_size(px(12.))
        .text_color(gpui::rgb(0x8FB0FF))
        .children(icon_name.map(|n| icon(n, 14., Color(0x8FB0_FFFF))))
        .child(div().flex_1().min_w(px(0.)).child(SharedString::from(text)))
}

thread_local! {
    /// The row being dragged (the drop line's style callback sees only the style).
    static STEP_DRAG_FROM: std::cell::Cell<usize> = const { std::cell::Cell::new(usize::MAX) };
}

/// A step row being dragged in the order list.
struct StepDrag {
    from: usize,
    badge: String,
    text: String,
}

/// The lifted row that follows the pointer while a step is dragged (board T2, 7).
struct StepGhost {
    badge: SharedString,
    text: SharedString,
}

impl Render for StepGhost {
    fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        let (bg, line) = step_row_colors(&t);
        div()
            .h(px(row::STEP_H))
            .w(px(240.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(row::STEP_GAP))
            .rounded(px(row::STEP_RADIUS))
            .border_1()
            .border_color(rgba_of(line))
            .bg(rgba_of(bg))
            .shadow_lg()
            .text_size(px(row::LABEL_TEXT))
            .text_color(rgba_of(t.text))
            .child(step_badge(&self.badge))
            .child(ellipsis_text(self.text.clone()))
    }
}

/// Background and border of a selected row (the spec's dark values; light follows the theme).
fn step_row_colors(t: &Theme) -> (Color, Color) {
    if t.mode == Mode::Dark {
        (row::STEP_SELECTED_BG, row::STEP_SELECTED_BORDER)
    } else {
        (t.raised, t.line)
    }
}

/// The red 22 px badge (red marks the step itself, never focus).
fn step_badge(label: &str) -> Div {
    div()
        .size(px(row::BADGE_D))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(rgba_of(row::BADGE))
        .text_color(gpui::white())
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .tnum()
        .child(SharedString::from(label.to_owned()))
}

impl EditorView {
    /// A labelled group of controls.
    pub(super) fn panel_group(&self, t: &Theme, label: &str, body: impl IntoElement) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(section_label(t, label))
            .child(body)
            .into_any_element()
    }

    /// The rows above the shared colour and size rows of the tool being shown.
    pub(super) fn panel_top(&self, t: &Theme, s: &Snap, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        if let Some(info) = &s.callout {
            rows.push(self.callout_text_field(t, info));
            rows.push(self.callout_style_row(t, info.style));
        }
        if let Some(info) = &s.spot {
            rows.push(self.spot_holes_row(t, info));
        }
        if let Some(info) = &s.steps {
            rows.extend(self.steps_top(t, info));
        }
        if let Some(info) = &s.stamp {
            rows.push(self.stamp_picker(t, info));
        }
        let _ = cx;
        rows
    }

    /// The rows below the shared rows (info lines, warnings).
    pub(super) fn panel_bottom(&self, t: &Theme, s: &Snap) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        if s.callout.is_some() {
            rows.push(
                info_note(
                    t,
                    None,
                    tr!(
                        "Drag the blue dot to aim the tail. Moving the box keeps the tail tip \
                         where it is. The box grows to fit the text."
                    ),
                )
                .into_any_element(),
            );
        }
        if let Some(info) = &s.spot {
            rows.extend(self.spot_bottom(t, info));
        }
        if let Some(info) = &s.steps {
            rows.extend(self.steps_bottom(t, info));
        }
        if s.stamp.is_some() {
            rows.push(
                info_note(
                    t,
                    None,
                    tr!(
                        "Click the image to place a stamp. Symbols are drawn as shapes, so they \
                         look the same on every system."
                    ),
                )
                .into_any_element(),
            );
        }
        rows
    }

    /// The stamp picker: a 4-column grid of symbols and the BEFORE / AFTER words (written in
    /// the app language when placed).
    fn stamp_picker(&self, t: &Theme, info: &crate::editor::state::StampInfo) -> AnyElement {
        let symbols: [(StampKind, &'static str, String); 4] = [
            (StampKind::Check, "check", tr_ctx!("stamp", "Check mark")),
            (StampKind::Cross, "x", tr_ctx!("stamp", "Cross")),
            (
                StampKind::Exclaim,
                "circle-alert",
                tr_ctx!("stamp", "Exclamation mark"),
            ),
            (StampKind::Question, "", tr_ctx!("stamp", "Question mark")),
        ];
        let tile = |id: ElementId, label: String, on: bool, content: AnyElement| {
            let hover = rgba_of(t.raised);
            div()
                .id(id)
                .role(Role::RadioButton)
                .aria_label(SharedString::from(label))
                .aria_toggled(toggled(on))
                .focusable()
                .tab_stop(true)
                .h(px(46.))
                .flex_1()
                .min_w(px(0.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(scale::RADIUS_MD))
                .border_1()
                .border_color(rgba_of(if on { accent(t) } else { t.line }))
                .cursor_pointer()
                .when(on, |d| d.bg(rgba_of(t.raised)))
                .when(!on, |d| d.hover(move |s| s.bg(hover)))
                .focus_visible(|s| s.border_color(rgba_of(t.text)))
                .child(content)
        };
        let grid = div()
            .flex()
            .gap(px(6.))
            .children(
                symbols
                    .into_iter()
                    .enumerate()
                    .map(|(i, (kind, ic, label))| {
                        let content: AnyElement = if ic.is_empty() {
                            div()
                                .text_size(px(20.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgba_of(t.text))
                                .child(SharedString::from(tr_ctx!("stamp glyph", "?")))
                                .into_any_element()
                        } else {
                            icon(ic, 20., t.text).into_any_element()
                        };
                        tile(
                            ElementId::from(("stamp-kind", i)),
                            label,
                            info.kind == kind,
                            content,
                        )
                        .on_click(
                            self.doc_action(move |d, cx| d.set_stamp(kind, StampWord::Before, cx)),
                        )
                    }),
            );
        let words = div()
            .flex()
            .gap(px(6.))
            .children(StampWord::ALL.into_iter().enumerate().map(|(i, word)| {
                let on = info.kind == StampKind::Label && info.word == Some(word);
                tile(
                    ElementId::from(("stamp-word", i)),
                    word.text(),
                    on,
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgba_of(t.text))
                        .child(SharedString::from(word.text().to_uppercase()))
                        .into_any_element(),
                )
                .on_click(self.doc_action(move |d, cx| d.set_stamp(StampKind::Label, word, cx)))
            }));
        self.panel_group(
            t,
            &tr_ctx!("annotation setting", "Kind"),
            div().flex().flex_col().gap(px(6.)).child(grid).child(words),
        )
    }

    /// Numbering (1 2 3 / A B C / a b c) and the start number; both apply to all steps.
    fn steps_top(&self, t: &Theme, info: &crate::editor::state::StepsInfo) -> Vec<AnyElement> {
        let options = [
            // i18n-ignore (the sample of each numbering style, not words)
            (StepStyle::Number, "1 2 3"),
            // i18n-ignore
            (StepStyle::Upper, "A B C"),
            // i18n-ignore
            (StepStyle::Lower, "a b c"),
        ];
        let numbering = div()
            .flex()
            .gap(px(6.))
            .children(options.into_iter().enumerate().map(|(i, (style, text))| {
                self.seg_button(
                    t,
                    ElementId::from(("step-style", i)),
                    text.to_owned(),
                    tr!("Numbering {text}", text = text),
                    info.style == style,
                    self.doc_action(move |d, cx| d.set_step_numbering(Some(style), None, cx)),
                )
            }));
        let start = info.start;
        let dec = start.saturating_sub(1).max(1);
        let inc = (start + 1).min(9999);
        let stepper = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                icon_button(
                    t,
                    ("step-start", 0usize),
                    "minus",
                    SharedString::from(tr!("Start at one less")),
                    false,
                    None,
                )
                .border_color(rgba_of(t.line))
                .bg(rgba_of(t.surface_2))
                .on_click(self.doc_action(move |d, cx| d.set_step_numbering(None, Some(dec), cx))),
            )
            .child(
                div()
                    .id("step-start-value")
                    .role(Role::SpinButton)
                    .aria_label(tr!("Start at"))
                    .aria_numeric_value(f64::from(start))
                    .aria_min_numeric_value(1.0)
                    .aria_max_numeric_value(9999.0)
                    .flex_1()
                    .min_w(px(48.))
                    .h(px(scale::CONTROL_H))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(scale::RADIUS_MD))
                    .bg(rgba_of(t.raised))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .text_size(px(type_scale::UI))
                    .mono()
                    .tnum()
                    .child(SharedString::from(info.start_label.clone())),
            )
            .child(
                icon_button(
                    t,
                    ("step-start", 2usize),
                    "plus",
                    SharedString::from(tr!("Start at one more")),
                    false,
                    None,
                )
                .border_color(rgba_of(t.line))
                .bg(rgba_of(t.surface_2))
                .on_click(self.doc_action(move |d, cx| d.set_step_numbering(None, Some(inc), cx))),
            );
        vec![
            self.panel_group(t, &tr_ctx!("annotation setting", "Numbering"), numbering),
            self.panel_group(t, &tr_ctx!("annotation setting", "Start at"), stepper),
        ]
    }

    /// The order list (drag a row to reorder; the numbers follow), the label of the selected
    /// step and the info line.
    fn steps_bottom(&self, t: &Theme, info: &crate::editor::state::StepsInfo) -> Vec<AnyElement> {
        let ft = FieldTokens::of(t);
        let dark = t.mode == Mode::Dark;
        let (sel_bg, sel_line) = step_row_colors(t);
        let (hover_bg, label_hover_bg, label_hover_line) = if dark {
            (
                row::STEP_HOVER_BG,
                row::LABEL_HOVER_BG,
                row::LABEL_HOVER_BORDER,
            )
        } else {
            (t.raised, ft.bg, ft.border)
        };
        let rows = info.rows.iter().enumerate().map(|(i, srow)| {
            let on = info.selected == Some(i);
            let typing = on && info.editing;
            let group = SharedString::from(format!("step-row-g-{i}"));
            let name = if srow.text.is_empty() {
                tr!("Step {badge}", badge = srow.badge)
            } else {
                tr!("Step {badge}: {text}", badge = srow.badge, text = srow.text)
            };
            let badge = srow.badge.clone();
            let doc = self.doc.clone();
            let key_doc = self.doc.clone();
            let canvas_focus = self.canvas_focus.clone();
            // The drop line sits in the gap above the row when a step comes from below, and
            // below it when it comes from above (the dragged step takes this row's place).
            let line = |above: bool| {
                let g = group.clone();
                div()
                    .absolute()
                    .left(px(4.))
                    .right(px(4.))
                    .when(above, |d| d.top(px(-row::STEP_GAP / 2. - 1.)))
                    .when(!above, |d| d.bottom(px(-row::STEP_GAP / 2. - 1.)))
                    .h(px(row::DROP_LINE_W))
                    .rounded_full()
                    .bg(rgba_of(FOCUS_BLUE))
                    .shadow(vec![gpui::BoxShadow {
                        color: gpui::Hsla::from(rgba_of(FOCUS_RING)),
                        offset: gpui::point(px(0.), px(0.)),
                        blur_radius: px(0.),
                        inset: false,
                        spread_radius: px(row::DROP_RING_W / 2.),
                    }])
                    .opacity(0.)
                    .group_drag_over::<StepDrag>(g, move |s| {
                        let from = STEP_DRAG_FROM.with(std::cell::Cell::get);
                        if (from > i) == above && from != i {
                            s.opacity(1.)
                        } else {
                            s
                        }
                    })
            };
            let empty = srow.text.is_empty();
            let shown = if empty {
                tr!("Add a label")
            } else {
                srow.text.clone()
            };
            let caret = || {
                div()
                    .w(px(CARET_W))
                    .h(px(15.))
                    .flex_none()
                    .bg(rgba_of(ft.caret))
            };
            let label = div()
                .flex_1()
                .min_w(px(0.))
                .h(px(row::LABEL_H))
                .px(px(FIELD_PAD_X - 3.))
                .flex()
                .items_center()
                .rounded(px(row::LABEL_RADIUS))
                .border_1()
                .border_color(gpui::transparent_black())
                .text_size(px(row::LABEL_TEXT))
                .text_color(rgba_of(if empty { ft.placeholder } else { ft.text }))
                .overflow_hidden()
                .cursor_text()
                .when(!typing, |d| {
                    d.group_hover(group.clone(), move |s| {
                        s.bg(rgba_of(label_hover_bg))
                            .border_color(rgba_of(label_hover_line))
                    })
                })
                .when(typing, |d| {
                    d.bg(rgba_of(ft.bg))
                        .border_color(rgba_of(FOCUS_BLUE))
                        .shadow(vec![gpui::BoxShadow {
                            color: gpui::Hsla::from(rgba_of(FOCUS_RING)),
                            offset: gpui::point(px(0.), px(0.)),
                            blur_radius: px(0.),
                            inset: false,
                            spread_radius: px(RING_W),
                        }])
                })
                .when(typing && empty, |d| d.child(caret()))
                .child(if typing && info.label_selected && !empty {
                    // The whole label is selected: blue behind the text.
                    div()
                        .min_w(px(0.))
                        .bg(rgba_of(ft.selection))
                        .text_color(rgba_of(ft.selection_text))
                        .child(SharedString::from(shown))
                } else {
                    ellipsis_text(shown)
                })
                .when(typing && !empty && !info.label_selected, |d| {
                    d.child(caret())
                });
            let del_group = SharedString::from(format!("step-del-g-{i}"));
            let delete = div()
                .id(ElementId::from(("step-row-delete", i)))
                .role(Role::Button)
                .aria_label(SharedString::from(tr!(
                    "Delete step {badge}",
                    badge = srow.badge
                )))
                .focusable()
                .tab_stop(true)
                .group(del_group.clone())
                .relative()
                .size(px(row::DELETE_SIZE))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(scale::RADIUS_MD))
                .cursor_pointer()
                .opacity(if on { 0.7 } else { 0. })
                .group_hover(group.clone(), |s| s.opacity(1.))
                .focus_visible(|s| s.opacity(1.))
                .on_click(self.doc_action(move |d, cx| d.delete_step_row(i, cx)))
                .child(icon("x", 14., t.text_2))
                .child(
                    div()
                        .absolute()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .opacity(0.)
                        .group_hover(del_group, |s| s.opacity(1.))
                        .child(icon("x", 14., row::DELETE_HOVER)),
                );
            div()
                .id(ElementId::from(("step-row", i)))
                .group(group.clone())
                .relative()
                .role(Role::Button)
                .aria_label(SharedString::from(name))
                .aria_toggled(toggled(on))
                .focusable()
                .tab_stop(true)
                .h(px(row::STEP_H))
                .pl(px(4.))
                .pr(px(6.))
                .flex()
                .items_center()
                .gap(px(row::STEP_GAP))
                .rounded(px(row::STEP_RADIUS))
                .border_1()
                .border_color(if on { rgba_of(sel_line) } else { gpui::rgba(0) })
                .cursor_pointer()
                .when(on, |d| d.bg(rgba_of(sel_bg)))
                .when(!on, |d| d.hover(move |s| s.bg(rgba_of(hover_bg))))
                .focus_visible(|s| s.border_color(rgba_of(t.text)))
                .on_key_down(move |ev, window, cx| {
                    // Keys on the focused row while no label is open (Enter, Alt+arrows,
                    // Delete); an open label gets its keys through the canvas.
                    let Some(doc) = &key_doc else { return };
                    if doc.read(cx).state.text.is_some() {
                        return;
                    }
                    let k = &ev.keystroke;
                    let used =
                        doc.update(cx, |d, cx| d.step_row_key(i, &k.key, k.modifiers.alt, cx));
                    if used {
                        if doc.read(cx).state.text.is_some() {
                            window.focus(&canvas_focus, cx);
                        }
                        cx.stop_propagation();
                    }
                })
                .on_drag(
                    StepDrag {
                        from: i,
                        badge: badge.clone(),
                        text: srow.text.clone(),
                    },
                    |d, _, _, cx| {
                        STEP_DRAG_FROM.with(|c| c.set(d.from));
                        cx.new(|_| StepGhost {
                            badge: SharedString::from(d.badge.clone()),
                            text: SharedString::from(d.text.clone()),
                        })
                    },
                )
                .on_drop::<StepDrag>(move |d, _, cx| {
                    if let Some(doc) = &doc {
                        let from = d.from;
                        doc.update(cx, |doc, cx| doc.reorder_steps(from, i, cx));
                    }
                })
                .on_click(self.doc_action(move |d, cx| d.select_step_row(i, cx)))
                .child(line(true))
                .child(line(false))
                .child(
                    div()
                        .flex_none()
                        .opacity(row::GRIP_REST_OPACITY as f32 / 100.)
                        .group_hover(group.clone(), |s| s.opacity(1.))
                        .child(icon("grip-vertical", 16., t.mist)),
                )
                .child(
                    // The selected step's badge gets the canvas ring: 1.5 px blue, 2 px gap.
                    div()
                        .flex_none()
                        .p(px(row::BADGE_RING_GAP))
                        .rounded_full()
                        .border(px(row::BADGE_RING_W))
                        .border_color(if on {
                            rgba_of(FOCUS_BLUE)
                        } else {
                            gpui::rgba(0)
                        })
                        .child(step_badge(&badge)),
                )
                .child(label)
                .child(delete)
                .into_any_element()
        });
        let list = div()
            .flex()
            .flex_col()
            .gap(px(row::STEP_GAP))
            .children(rows);
        let mut out = Vec::new();
        // The how-to sits in the tooltip of the (i) next to the heading, not in a note below.
        let hint = tr!(
            "Drag to reorder: the numbers follow. Deleting a step renumbers the rest. \
             Labels sit right of the badge and flip left near the image edge. After Z \
             come AA, AB."
        );
        if !info.rows.is_empty() {
            let heading = div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(section_label(t, &tr_ctx!("annotation setting", "Order")))
                .child(
                    div()
                        .id("step-order-info")
                        .role(Role::Image)
                        .aria_label(SharedString::from(hint.clone()))
                        .flex_none()
                        .flex()
                        .items_center()
                        .tooltip(crate::ui::tooltip::tip(hint, Vec::new()))
                        .child(icon("info", 14., t.mist)),
                );
            out.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(heading)
                    .child(list)
                    .into_any_element(),
            );
        }
        out
    }

    /// Holes "2 of 8", the list with a delete button per hole, and "+ Rectangle" / "+ Ellipse".
    fn spot_holes_row(&self, t: &Theme, info: &crate::editor::state::SpotInfo) -> AnyElement {
        let n = info.holes.len();
        let title = tr!("Holes {n} of {max}", n = n, max = MAX_SPOTLIGHT_SHAPES);
        let list = div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .children(info.holes.iter().enumerate().map(|(i, kind)| {
                let on = i == info.selected;
                let (name, ic) = match kind {
                    SpotShapeKind::Rect => (tr!("Rectangle {n}", n = i + 1), "square"),
                    SpotShapeKind::Ellipse => (tr!("Ellipse {n}", n = i + 1), "circle"),
                };
                let hover = rgba_of(t.raised);
                div()
                    .id(ElementId::from(("spot-hole", i)))
                    .role(Role::Button)
                    .aria_label(SharedString::from(name.clone()))
                    .aria_toggled(toggled(on))
                    .focusable()
                    .tab_stop(true)
                    .h(px(34.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(scale::RADIUS_SM))
                    .border_1()
                    .border_color(rgba_of(if on { accent(t) } else { t.line }))
                    .cursor_pointer()
                    .when(on, |d| d.bg(rgba_of(t.raised)))
                    .when(!on, |d| d.hover(move |s| s.bg(hover)))
                    .focus_visible(|s| s.border_color(rgba_of(t.text)))
                    .on_click(self.doc_action(move |d, cx| d.select_hole(i, cx)))
                    .child(icon(ic, 16., t.text_2))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(13.))
                            .child(shrink_text(name.clone())),
                    )
                    .child(
                        icon_button(
                            t,
                            ("spot-hole-delete", i),
                            "x",
                            SharedString::from(tr!("Remove {name}", name = name)),
                            false,
                            None,
                        )
                        .on_click(self.doc_action(move |d, cx| d.remove_hole(i, cx))),
                    )
                    .into_any_element()
            }));
        let add = |id: &'static str, label: String, kind: SpotShapeKind, ic: &'static str| {
            let b = button_with_icon(t, id, label, ButtonKind::Secondary, Some(ic))
                .on_click(self.doc_action(move |d, cx| d.add_default_hole(kind, cx)));
            if info.at_limit {
                b.opacity(0.45)
            } else {
                b
            }
        };
        let buttons = div()
            .flex()
            .flex_wrap()
            .gap(px(8.))
            .child(add(
                "spot-add-rect",
                tr!("+ Rectangle"),
                SpotShapeKind::Rect,
                "square",
            ))
            .child(add(
                "spot-add-ellipse",
                tr!("+ Ellipse"),
                SpotShapeKind::Ellipse,
                "circle",
            ));
        self.panel_group(
            t,
            &title,
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(list)
                .child(buttons),
        )
    }

    /// Outside the holes (Dim or Blur), the blur strength, the info line and the warning.
    fn spot_bottom(&self, t: &Theme, info: &crate::editor::state::SpotInfo) -> Vec<AnyElement> {
        let options = [
            (SpotMode::Dim, tr_ctx!("spotlight outside", "Dim")),
            (SpotMode::Blur, tr_ctx!("spotlight outside", "Blur")),
        ];
        let mode_row = div()
            .flex()
            .gap(px(6.))
            .children(options.into_iter().enumerate().map(|(i, (mode, label))| {
                self.seg_button(
                    t,
                    ElementId::from(("spot-mode", i)),
                    label.clone(),
                    label,
                    info.mode == mode,
                    self.doc_action(move |d, cx| d.set_spot_mode(mode, cx)),
                )
            }));
        let mut rows = vec![self.panel_group(
            t,
            &tr_ctx!("annotation setting", "Outside the holes"),
            mode_row,
        )];
        rows.push(
            info_note(
                t,
                Some("layers"),
                tr!(
                    "One spotlight per image: drawing another shape adds a hole to it. It goes \
                     under your arrows and callouts, so they stay bright."
                ),
            )
            .into_any_element(),
        );
        if info.mode == SpotMode::Blur {
            rows.push(
                div()
                    .p(px(10.))
                    .rounded(px(scale::RADIUS_SM))
                    .bg(gpui::rgba(0xF5B8_4A14))
                    .text_size(px(12.))
                    .text_color(gpui::rgb(0xF5B84A))
                    .child(SharedString::from(tr!(
                        "Blur here is only a look. To hide information, use Blur (B) or \
                         Pixelate (X)."
                    )))
                    .into_any_element(),
            );
        }
        rows
    }

    /// The callout text: a field that shows the text; clicking it types on the image, where the
    /// platform input method (Telex, VNI) works as for the Text tool (ANN-4.AC5).
    fn callout_text_field(
        &self,
        t: &Theme,
        info: &crate::editor::state::CalloutInfo,
    ) -> AnyElement {
        let placeholder = !info.has_op && !info.editing;
        let shown = if info.text.is_empty() {
            if placeholder {
                tr!("Drag on the image to place a callout, then type its text.")
            } else {
                tr!("Type the callout text")
            }
        } else {
            info.text.clone()
        };
        let editing = info.editing;
        let can_edit = info.has_op && !editing;
        let hover = rgba_of(t.raised);
        let field = div()
            .id("callout-text")
            .role(Role::MultilineTextInput)
            .aria_label(tr!("Callout text"))
            .focusable()
            .tab_stop(true)
            .w_full()
            .min_h(px(64.))
            .p(px(10.))
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(if editing { accent(t) } else { t.line }))
            .bg(rgba_of(t.field))
            .text_size(px(type_scale::UI))
            .text_color(rgba_of(if info.text.is_empty() { t.mist } else { t.text }))
            .when(can_edit, |d| {
                d.cursor_text()
                    .hover(move |s| s.bg(hover))
                    .on_click(self.doc_action(|d, cx| d.edit_selected_text(cx)))
            })
            .focus_visible(|s| s.border_color(rgba_of(t.text)))
            .child(SharedString::from(shown));
        self.panel_group(t, &tr_ctx!("annotation setting", "Text"), field)
    }

    /// Bubble or Box.
    fn callout_style_row(&self, t: &Theme, current: CalloutStyle) -> AnyElement {
        let options = [
            (CalloutStyle::Bubble, tr_ctx!("callout style", "Bubble")),
            (CalloutStyle::Box, tr_ctx!("callout style", "Box")),
        ];
        let row = div()
            .flex()
            .gap(px(6.))
            .children(options.into_iter().enumerate().map(|(i, (style, label))| {
                self.seg_button(
                    t,
                    ElementId::from(("callout-style", i)),
                    label.clone(),
                    label,
                    current == style,
                    self.doc_action(move |d, cx| d.set_callout_style(style, cx)),
                )
            }));
        self.panel_group(t, &tr_ctx!("annotation setting", "Style"), row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_pill_text_is_translatable() {
        assert_eq!(tr_ctx!("panel badge", "New in 0.3.3"), "New in 0.3.3");
    }
}
