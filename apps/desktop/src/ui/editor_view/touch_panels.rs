//! The inspector panels of the 0.3.4 tools: Zoom Lens (ANN-8), Retype (ANN-9), Move area
//! (ANN-10), the Blueprint panel that takes over the inspector column (ANN-11), and the busy
//! background sheet. Layout and wording follow the 0.3.4 design spec (boards S4 to S7); the
//! behaviour is in `crate::editor::state::touch`. Not looked at on a screen yet.

use super::panels::{info_note, new_pill};
use super::*;
use crate::editor::state::touch::{BlueprintInfo, LensInfo, MoveInfo, RetypeInfo};
use crate::editor::tools::WEIGHTS;
use crate::editor::touchup::BusyPrompt;
use cm_core::annotation::{BlueprintColor, BlueprintStyle, FillKind, MagnifyShape, TextAlign};

/// Neutral bar colours offered for "One colour" (Blueprint).
const BAR_COLORS: [(u32, &str); 4] = [
    (0xC7C7CC, "Light grey"), // i18n-ignore (id, see bar_color_name)
    (0x9A9AA1, "Grey"),       // i18n-ignore (id, see bar_color_name)
    (0x6B6B73, "Dark grey"),  // i18n-ignore (id, see bar_color_name)
    (0x3A3A40, "Charcoal"),   // i18n-ignore (id, see bar_color_name)
];

fn bar_color_name(id: &str) -> String {
    match id {
        "Light grey" => tr_ctx!("colour", "Light grey"),
        "Grey" => tr_ctx!("colour", "Grey"),
        "Dark grey" => tr_ctx!("colour", "Dark grey"),
        "Charcoal" => tr_ctx!("colour", "Charcoal"),
        other => other.to_owned(),
    }
}

fn warning_note(text: String) -> AnyElement {
    div()
        .p(px(10.))
        .rounded(px(scale::RADIUS_SM))
        .bg(gpui::rgba(0xF5B8_4A14))
        .text_size(px(12.))
        .text_color(gpui::rgb(0xF5B84A))
        .child(SharedString::from(text))
        .into_any_element()
}

/// A label and a mono value on one row (Source, Moved to, ...).
fn kv(t: &Theme, label: String, value: String) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .text_size(px(13.))
        .child(
            div()
                .min_w(px(0.))
                .text_color(rgba_of(t.text_3))
                .child(SharedString::from(label)),
        )
        .child(
            div()
                .text_color(rgba_of(t.text))
                .mono()
                .tnum()
                .child(SharedString::from(value)),
        )
        .into_any_element()
}

fn rect_text(r: Rc) -> String {
    format!("{:.0}, {:.0}  {:.0} \u{D7} {:.0}", r.x, r.y, r.w, r.h)
}

/// A swatch of a measured colour with its hex value and the kind of fill.
fn fill_row(t: &Theme, color: OpColor, kind: FillKind) -> AnyElement {
    let kind_label = match kind {
        FillKind::Solid => tr_ctx!("fill kind", "Solid"),
        FillKind::Linear => tr_ctx!("fill kind", "Gradient"),
    };
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(
            div()
                .size(px(24.))
                .flex_none()
                .rounded(px(6.))
                .border_1()
                .border_color(rgba_of(t.line))
                .bg(gpui_rgb(color)),
        )
        .child(
            div()
                .text_size(px(13.))
                .text_color(rgba_of(t.text))
                .mono()
                .child(SharedString::from(color.to_hex())),
        )
        .child(
            div()
                .text_size(px(12.))
                .text_color(rgba_of(t.text_3))
                .child(SharedString::from(kind_label)),
        )
        .into_any_element()
}

impl EditorView {
    // ---- Zoom Lens ---------------------------------------------------------------------------

    pub(super) fn lens_bottom(&self, t: &Theme, info: &LensInfo) -> Vec<AnyElement> {
        let shapes = [
            (MagnifyShape::Circle, tr_ctx!("lens shape", "Circle")),
            (MagnifyShape::Rounded, tr_ctx!("lens shape", "Rounded")),
            (MagnifyShape::Rect, tr_ctx!("lens shape", "Rect")),
        ];
        let shape_row = div()
            .flex()
            .gap(px(6.))
            .children(shapes.into_iter().enumerate().map(|(i, (shape, label))| {
                self.seg_button(
                    t,
                    ElementId::from(("lens-shape", i)),
                    label.clone(),
                    label,
                    info.shape == shape,
                    self.doc_action(move |d, cx| d.set_lens_shape(shape, cx)),
                )
            }));
        let (connector, badge) = (info.connector, info.badge);
        let toggles = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(13.))
                    .child(tr_ctx!("lens setting", "Connector"))
                    .child(
                        switch(
                            t,
                            "lens-connector",
                            tr_ctx!("lens setting", "Connector"),
                            connector,
                            true,
                        )
                        .on_click(
                            self.doc_action(move |d, cx| d.set_lens_connector(!connector, cx)),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(13.))
                    .child(tr_ctx!("lens setting", "Zoom badge"))
                    .child(
                        switch(
                            t,
                            "lens-badge",
                            tr_ctx!("lens setting", "Zoom badge"),
                            badge,
                            true,
                        )
                        .on_click(self.doc_action(move |d, cx| d.set_lens_badge(!badge, cx))),
                    ),
            );
        let mut rows = vec![
            self.panel_group(t, &tr_ctx!("annotation setting", "Shape"), shape_row),
            toggles.into_any_element(),
        ];
        if let Some(src) = info.source {
            rows.push(kv(t, tr_ctx!("lens setting", "Source"), rect_text(src)));
        }
        rows.push(kv(
            t,
            tr_ctx!("lens setting", "Lenses on this image"),
            tr!("{n} of {max}", n = info.count, max = info.max),
        ));
        if let Some(n) = &info.notice {
            rows.push(warning_note(n.clone()));
        } else if info.count >= info.max {
            rows.push(warning_note(tr!(
                "This image has 4 zoom lenses, the most it can have. Delete one to add another."
            )));
        }
        rows.push(
            info_note(
                t,
                None,
                tr!(
                    "Drawn from the original, so it stays sharp and shows what you drew before \
                     it. Blur and Pixelate always apply inside the lens."
                ),
            )
            .into_any_element(),
        );
        rows
    }

    // ---- Retype ------------------------------------------------------------------------------

    pub(super) fn retype_top(&self, t: &Theme, info: &RetypeInfo) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        let shown = if info.text.is_empty() {
            tr!("Click a line on the image, or drag a box over text that was not found.")
        } else {
            info.text.clone()
        };
        rows.push(
            self.panel_group(
                t,
                &tr_ctx!("annotation setting", "Text"),
                div()
                    .id("retype-text")
                    .role(Role::Group)
                    .w_full()
                    .min_h(px(40.))
                    .p(px(10.))
                    .rounded(px(scale::RADIUS_MD))
                    .border_1()
                    .border_color(rgba_of(if info.editing { accent(t) } else { t.line }))
                    .bg(rgba_of(t.field))
                    .text_size(px(type_scale::UI))
                    .text_color(rgba_of(if info.text.is_empty() { t.mist } else { t.text }))
                    .child(SharedString::from(shown)),
            ),
        );
        if info.has_op || info.editing {
            let weights = [
                (WEIGHTS[0], tr_ctx!("font weight", "Regular")),
                (WEIGHTS[1], tr_ctx!("font weight", "Medium")),
                (WEIGHTS[2], tr_ctx!("font weight", "Bold")),
            ];
            let weight = info.weight;
            rows.push(
                self.panel_group(
                    t,
                    &tr_ctx!("annotation setting", "Weight"),
                    div()
                        .flex()
                        .gap(px(6.))
                        .children(weights.into_iter().enumerate().map(|(i, (w, label))| {
                            self.seg_button(
                                t,
                                ElementId::from(("retype-weight", i)),
                                label.clone(),
                                label,
                                weight == w || (w == 700 && weight >= 600 && weight != 500),
                                self.doc_action(move |d, cx| d.set_retype_weight(w, cx)),
                            )
                        })),
                ),
            );
            let aligns = [
                (TextAlign::Left, tr_ctx!("text alignment", "Left")),
                (TextAlign::Center, tr_ctx!("text alignment", "Center")),
                (TextAlign::Right, tr_ctx!("text alignment", "Right")),
            ];
            let align = info.align;
            rows.push(
                self.panel_group(
                    t,
                    &tr_ctx!("annotation setting", "Align"),
                    div()
                        .flex()
                        .gap(px(6.))
                        .children(aligns.into_iter().enumerate().map(|(i, (a, label))| {
                            self.seg_button(
                                t,
                                ElementId::from(("retype-align", i)),
                                label.clone(),
                                label,
                                align == a,
                                self.doc_action(move |d, cx| d.set_retype_align(a, cx)),
                            )
                        })),
                ),
            );
        }
        rows
    }

    pub(super) fn retype_bottom(&self, t: &Theme, info: &RetypeInfo) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        if info.has_op || info.editing {
            rows.push(self.panel_group(
                t,
                &tr_ctx!("annotation setting", "Background"),
                fill_row(t, info.fill_color, info.fill_kind),
            ));
        }
        if info.too_wide {
            rows.push(warning_note(tr!(
                "The new text is wider than the box and will be cut. Widen the box or shorten the text."
            )));
        }
        if let Some(n) = &info.note {
            rows.push(warning_note(n.clone()));
        }
        rows.push(
            info_note(
                t,
                None,
                tr!(
                    "Drawn in the system font, with the size and colour measured from the box. \
                     Custom fonts are not matched. It is a base edit: it sits under your other \
                     annotations."
                ),
            )
            .into_any_element(),
        );
        rows.push(warning_note(tr!(
            "Not a redaction. To hide secrets, use Blur (B) or Pixelate (X)."
        )));
        rows
    }

    // ---- Move area ---------------------------------------------------------------------------

    pub(super) fn move_top(&self, t: &Theme, info: &MoveInfo) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        match info.source {
            Some(src) => {
                rows.push(kv(t, tr_ctx!("move area", "Source"), rect_text(src)));
                rows.push(kv(
                    t,
                    tr_ctx!("move area", "Moved to"),
                    match (info.to, info.removes) {
                        (Some(to), _) => format!("{:.0}, {:.0}", to.x, to.y),
                        (None, true) => tr_ctx!("move area", "Removed"),
                        (None, false) => "\u{2013}".to_owned(),
                    },
                ));
                rows.push(self.panel_group(
                    t,
                    &tr_ctx!("annotation setting", "Fill behind"),
                    fill_row(t, info.fill_color, info.fill_kind),
                ));
            }
            None => rows.push(
                div()
                    .text_size(px(13.))
                    .text_color(rgba_of(t.text_3))
                    .child(SharedString::from(tr!(
                        "Drag a box over what to move, then drag its contents to the new place."
                    )))
                    .into_any_element(),
            ),
        }
        let snap = info.snap;
        rows.push(
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(13.))
                .child(shrink_text(tr!("Snap to the source row and column")))
                .child(
                    switch(
                        t,
                        "move-snap",
                        tr!("Snap to the source row and column"),
                        snap,
                        true,
                    )
                    .on_click(self.doc_action(move |d, cx| d.set_snap_on(!snap, cx))),
                )
                .into_any_element(),
        );
        if info.pending || (info.source.is_some() && !info.removes) {
            rows.push(
                button_with_icon(
                    t,
                    "move-remove",
                    tr!("Remove contents"),
                    ButtonKind::Secondary,
                    Some("trash-2"),
                )
                .on_click(self.doc_action(|d, cx| d.remove_contents(cx)))
                .child(keycaps(t, &["Del".to_owned()]))
                .into_any_element(),
            );
        }
        rows
    }

    pub(super) fn move_bottom(&self, t: &Theme) -> Vec<AnyElement> {
        vec![
            info_note(
                t,
                None,
                tr!(
                    "Hold Shift to keep the move on one row or column. It is a base edit: it \
                     sits under your other annotations, and the hole is filled with the \
                     background colour."
                ),
            )
            .into_any_element(),
            warning_note(tr!(
                "Not a redaction. To hide secrets, use Blur (B) or Pixelate (X)."
            )),
        ]
    }

    // ---- Blueprint ---------------------------------------------------------------------------

    /// The Blueprint panel: it takes the place of the inspector column while it is open, so the
    /// image stays fully visible (design decision 3).
    pub(super) fn blueprint_panel(&self, t: &Theme, info: &BlueprintInfo) -> AnyElement {
        let col = || card(t).flex().flex_col().gap(px(12.)).flex_none();
        let count = tr!(
            "{found} text lines found \u{B7} {bars} become bars \u{B7} {kept} kept",
            found = info.found,
            bars = info.bars,
            kept = info.kept
        );
        let styles = [
            (BlueprintStyle::Bar, tr_ctx!("blueprint style", "Bars")),
            (BlueprintStyle::Box, tr_ctx!("blueprint style", "Boxes")),
        ];
        let style = info.style;
        let style_row = div()
            .flex()
            .gap(px(6.))
            .children(styles.into_iter().enumerate().map(|(i, (s, label))| {
                self.seg_button(
                    t,
                    ElementId::from(("bp-style", i)),
                    label.clone(),
                    label,
                    style == s,
                    self.doc_action(move |d, cx| d.set_blueprint_style(s, cx)),
                )
            }));
        let auto = info.color == BlueprintColor::Auto;
        let color_modes = div()
            .flex()
            .gap(px(6.))
            .child(self.seg_button(
                t,
                ElementId::from(("bp-color", 0usize)),
                tr_ctx!("blueprint colour", "Auto"),
                tr_ctx!("blueprint colour", "Auto"),
                auto,
                self.doc_action(|d, cx| d.set_blueprint_color(BlueprintColor::Auto, cx)),
            ))
            .child(self.seg_button(
                t,
                ElementId::from(("bp-color", 1usize)),
                tr_ctx!("blueprint colour", "One colour"),
                tr_ctx!("blueprint colour", "One colour"),
                !auto,
                self.doc_action(|d, cx| {
                    d.set_blueprint_color(BlueprintColor::Fixed(OpColor::rgb(0x9A, 0x9A, 0xA1)), cx)
                }),
            ));
        let swatches = (!auto).then(|| {
            let cur = match info.color {
                BlueprintColor::Fixed(c) => Some(c),
                BlueprintColor::Auto => None,
            };
            div()
                .flex()
                .gap(px(8.))
                .children(BAR_COLORS.iter().enumerate().map(|(i, (hex, name))| {
                    let c = OpColor::rgb((hex >> 16) as u8, (hex >> 8) as u8, *hex as u8);
                    let on = cur == Some(c);
                    div()
                        .id(ElementId::from(("bp-swatch", i)))
                        .role(Role::Button)
                        .aria_label(SharedString::from(bar_color_name(name)))
                        .aria_toggled(toggled(on))
                        .focusable()
                        .tab_stop(true)
                        .size(px(24.))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(rgba_of(if on { accent(t) } else { t.line }))
                        .bg(gpui_rgb(c))
                        .cursor_pointer()
                        .on_click(self.doc_action(move |d, cx| {
                            d.set_blueprint_color(BlueprintColor::Fixed(c), cx)
                        }))
                }))
        });
        let radius = info.radius;
        let radius_row =
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    icon_button(
                        t,
                        "bp-radius-dec",
                        "minus",
                        tr!("Smaller corners"),
                        false,
                        None,
                    )
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.surface_2))
                    .on_click(self.doc_action(move |d, cx| {
                        d.set_blueprint_radius((radius - 1.0).max(0.0), cx)
                    })),
                )
                .child(
                    div()
                        .flex_1()
                        .h(px(scale::CONTROL_H))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(scale::RADIUS_MD))
                        .bg(rgba_of(t.raised))
                        .border_1()
                        .border_color(rgba_of(t.line))
                        .mono()
                        .tnum()
                        .text_size(px(type_scale::UI))
                        .child(SharedString::from(format!("{radius:.0}"))),
                )
                .child(
                    icon_button(
                        t,
                        "bp-radius-inc",
                        "plus",
                        tr!("Larger corners"),
                        false,
                        None,
                    )
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.surface_2))
                    .on_click(self.doc_action(move |d, cx| {
                        d.set_blueprint_radius((radius + 1.0).min(40.0), cx)
                    })),
                );
        let kept_rows: Vec<AnyElement> = info
            .kept_rows
            .iter()
            .map(|(i, text)| {
                let i = *i;
                div()
                    .id(ElementId::from(("bp-kept", i)))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .h(px(32.))
                    .px(px(8.))
                    .rounded(px(scale::RADIUS_SM))
                    .bg(rgba_of(t.raised))
                    .child(icon("check", 14., t.success))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .text_size(px(13.))
                            .whitespace_nowrap()
                            .child(SharedString::from(if text.is_empty() {
                                tr!("A line you kept")
                            } else {
                                text.clone()
                            })),
                    )
                    .child(
                        icon_button(
                            t,
                            ElementId::from(("bp-unkeep", i)),
                            "x",
                            tr!("Turn this line back into a bar"),
                            false,
                            None,
                        )
                        .on_click(self.doc_action(move |d, cx| d.blueprint_toggle(i, cx))),
                    )
                    .into_any_element()
            })
            .collect();
        let hover = info.hover.clone().map(|(text, bar)| {
            let quoted = if text.is_empty() {
                tr!("This line")
            } else {
                format!("\u{201C}{text}\u{201D}")
            };
            if bar {
                tr!("{line} \u{B7} click to keep the real text", line = quoted)
            } else {
                tr!("{line} \u{B7} click to make it a bar again", line = quoted)
            }
        });
        let existing = info.existing;
        let actions = div()
            .flex()
            .gap(px(8.))
            .child(
                button(
                    t,
                    "bp-cancel",
                    if existing {
                        tr_ctx!("button", "Done")
                    } else {
                        tr_ctx!("button", "Cancel")
                    },
                    ButtonKind::Secondary,
                )
                .on_click(self.doc_action(|d, cx| d.blueprint_cancel(cx))),
            )
            .when(!existing, |d| {
                d.child(
                    button(t, "bp-apply", tr!("Apply Blueprint"), ButtonKind::Primary)
                        .on_click(self.doc_action(|d, cx| d.blueprint_apply(cx))),
                )
            });
        div()
            .id("editor-inspector")
            .role(Role::Group)
            .aria_label(tr!("Blueprint"))
            .w(px(INSPECTOR_W))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .id("editor-blueprint-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .overflow_y_scroll()
                    .child(
                        col()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(icon("cm-blueprint", 18., t.text))
                                    .child(
                                        div()
                                            .id("blueprint-title")
                                            .role(Role::Heading)
                                            .aria_level(2)
                                            .aria_label(SharedString::from(tr!("Blueprint")))
                                            .text_size(px(type_scale::BODY))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(SharedString::from(tr!("Blueprint"))),
                                    )
                                    .child(new_pill(t)),
                            )
                            .child(
                                div()
                                    .text_size(px(type_scale::CAPTION))
                                    .text_color(rgba_of(t.mist))
                                    .child(SharedString::from(count)),
                            )
                            .children(hover.map(|h| {
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgba_of(t.text_2))
                                    .child(SharedString::from(h))
                            }))
                            .child(self.panel_group(
                                t,
                                &tr_ctx!("annotation setting", "Style"),
                                style_row,
                            ))
                            .child(
                                self.panel_group(
                                    t,
                                    &tr_ctx!("annotation setting", "Colour"),
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(8.))
                                        .child(color_modes)
                                        .children(swatches),
                                ),
                            )
                            .child(self.panel_group(
                                t,
                                &tr_ctx!("annotation setting", "Corner radius"),
                                radius_row,
                            ))
                            .child(self.panel_group(
                                t,
                                &tr!("Kept lines"),
                                if kept_rows.is_empty() {
                                    div()
                                        .text_size(px(13.))
                                        .text_color(rgba_of(t.text_3))
                                        .child(SharedString::from(tr!(
                                            "Click a line on the image to keep its real text."
                                        )))
                                } else {
                                    div().flex().flex_col().gap(px(6.)).children(kept_rows)
                                },
                            ))
                            .child(info_note(
                                t,
                                None,
                                if existing {
                                    tr!("Each change is one undo step. Copy text will read only \
                                         the kept lines.")
                                } else {
                                    tr!("Applied as one edit and one undo step. Copy text will \
                                         read only the kept lines.")
                                },
                            ))
                            .child(info_note(
                                t,
                                None,
                                tr!("Icons and pictures stay as they are in 0.3.4."),
                            ))
                            .child(actions),
                    ),
            )
            .into_any_element()
    }

    // ---- the busy background sheet ------------------------------------------------------------

    /// "The background is too busy to fill cleanly": shown before the op is created. Cancel has
    /// the default focus and stores nothing; Use anyway goes on (ANN-9.AC3).
    pub(super) fn busy_sheet(
        &self,
        t: &Theme,
        prompt: &BusyPrompt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let _ = (window, cx);
        let what = match prompt.request.what {
            crate::editor::touchup::MeasureFor::Retype => tr!(
                "The new text will sit on a patch that does not match what is around it. You can still go on, or cancel and nothing is stored."
            ),
            crate::editor::touchup::MeasureFor::MoveSource => tr!(
                "The hole left behind will not match what is around it. You can still go on, or cancel and nothing is stored."
            ),
        };
        div()
            .id("editor-busy-scrim")
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba_of(t.scrim))
            .child(
                div()
                    .id("editor-busy-sheet")
                    .role(Role::Dialog)
                    .aria_label(SharedString::from(tr!(
                        "The background is too busy to fill cleanly"
                    )))
                    .w(px(380.))
                    .p(px(20.))
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .rounded(px(scale::RADIUS_DIALOG))
                    .border_1()
                    .border_color(rgba_of(t.border))
                    .bg(rgba_of(t.surface))
                    .child(
                        div()
                            .text_size(px(type_scale::BODY))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(tr!(
                                "The background is too busy to fill cleanly"
                            ))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                div()
                                    .size(px(40.))
                                    .flex_none()
                                    .rounded(px(8.))
                                    .border_1()
                                    .border_color(rgba_of(t.line))
                                    .bg(gpui_rgb(
                                        prompt
                                            .measured
                                            .fill
                                            .colors
                                            .first()
                                            .copied()
                                            .unwrap_or(OpColor::WHITE),
                                    )),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_size(px(13.))
                                    .text_color(rgba_of(t.text_2))
                                    .child(SharedString::from(what)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                button(
                                    t,
                                    "busy-cancel",
                                    tr_ctx!("button", "Cancel"),
                                    ButtonKind::Primary,
                                )
                                .on_click(self.doc_action(|d, cx| d.busy_choice(false, cx))),
                            )
                            .child(
                                button(t, "busy-anyway", tr!("Use anyway"), ButtonKind::Secondary)
                                    .on_click(self.doc_action(|d, cx| d.busy_choice(true, cx))),
                            ),
                    ),
            )
            .into_any_element()
    }
}
