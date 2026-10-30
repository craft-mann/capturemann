use super::*;

impl EditorView {
    fn doc_action(
        &self,
        f: impl Fn(&mut EditorDoc, &mut Context<EditorDoc>) + 'static,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let doc = self.doc.clone();
        move |_, _, cx| {
            if let Some(d) = &doc {
                d.update(cx, |d, cx| f(d, cx));
            }
        }
    }

    fn set_param_action(
        &self,
        v: ParamValue,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        self.doc_action(move |d, cx| d.set_param(v, cx))
    }

    fn seg_button(
        &self,
        t: &Theme,
        id: ElementId,
        text: String,
        label: String,
        on: bool,
        action: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let hover = rgba_of(t.raised);
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(SharedString::from(label))
            .aria_toggled(toggled(on))
            .focusable()
            .tab_stop(true)
            .h(px(scale::CONTROL_H))
            .flex_1()
            .min_w(px(0.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(scale::RADIUS_SM))
            .border_1()
            .border_color(rgba_of(if on { accent(t) } else { t.line }))
            .cursor_pointer()
            .text_size(px(13.))
            .tnum()
            .when(on, |d| d.bg(rgba_of(t.raised)).text_color(rgba_of(t.text)))
            .when(!on, |d| {
                d.text_color(rgba_of(t.text_3)).hover(move |s| s.bg(hover))
            })
            .focus_visible(|s| s.border_color(rgba_of(t.text)))
            .on_click(action)
            .child(SharedString::from(text))
            .into_any_element()
    }

    /// One annotation-palette swatch. The palette colors are content colors (what gets drawn
    /// on the image), not UI colors.
    fn swatch(&self, t: &Theme, i: usize, name: String, c: OpColor, on: bool) -> AnyElement {
        let hover = rgba_of(t.raised);
        div()
            .id(ElementId::from(("swatch", i)))
            .role(Role::Button)
            .aria_label(name)
            .aria_toggled(toggled(on))
            .focusable()
            .tab_stop(true)
            .h(px(36.))
            .flex_1()
            .min_w(px(0.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(scale::RADIUS_SM))
            .border_1()
            .border_color(rgba_of(if on { accent(t) } else { Color(0) }))
            .when(on, |d| d.bg(rgba_of(t.raised)))
            .when(!on, |d| d.hover(move |s| s.bg(hover)))
            .cursor_pointer()
            .focus_visible(|s| s.border_color(rgba_of(t.text)))
            .on_click(self.set_param_action(ParamValue::Color(c)))
            .child(
                div()
                    .size(px(20.))
                    .rounded_full()
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(gpui_rgb(c)),
            )
            .into_any_element()
    }

    /// `- value +` stepper (blur strength, pixel size) exposed as a spin button.
    #[allow(clippy::too_many_arguments)]
    fn stepper(
        &self,
        t: &Theme,
        id: &'static str,
        label: String,
        (dec_label, inc_label): (String, String),
        value: u32,
        (lo, hi): (u32, u32),
        make: fn(u32) -> ParamValue,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let step = 2;
        let dec = make(value.saturating_sub(step).max(lo));
        let inc = make((value + step).min(hi));
        let doc = self.doc.clone();
        let doc2 = self.doc.clone();
        let _ = cx;
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                icon_button(
                    t,
                    (id, 0usize),
                    "minus",
                    SharedString::from(dec_label),
                    false,
                    None,
                )
                .border_color(rgba_of(t.line))
                .bg(rgba_of(t.surface_2))
                .on_click(self.set_param_action(dec)),
            )
            .child(
                div()
                    .id(ElementId::from((id, 1usize)))
                    .role(Role::SpinButton)
                    .aria_label(label)
                    .aria_numeric_value(f64::from(value))
                    .aria_min_numeric_value(f64::from(lo))
                    .aria_max_numeric_value(f64::from(hi))
                    .aria_numeric_value_step(f64::from(step))
                    .focusable()
                    .tab_stop(true)
                    .on_a11y_action(AccessibleAction::Increment, move |_, _, cx| {
                        if let Some(d) = &doc {
                            d.update(cx, |d, cx| d.set_param(inc, cx));
                        }
                    })
                    .on_a11y_action(AccessibleAction::Decrement, move |_, _, cx| {
                        if let Some(d) = &doc2 {
                            d.update(cx, |d, cx| d.set_param(dec, cx));
                        }
                    })
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
                    .focus_visible(|s| s.border_color(rgba_of(t.text)))
                    .text_size(px(type_scale::UI))
                    .mono()
                    .tnum()
                    .child(SharedString::from(value.to_string())),
            )
            .child(
                icon_button(
                    t,
                    (id, 2usize),
                    "plus",
                    SharedString::from(inc_label),
                    false,
                    None,
                )
                .border_color(rgba_of(t.line))
                .bg(rgba_of(t.surface_2))
                .on_click(self.set_param_action(inc)),
            )
            .into_any_element()
    }

    fn param_group(
        &self,
        t: &Theme,
        p: Param,
        cur: ParamValue,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let body: AnyElement =
            match (p, cur) {
                (Param::Color, ParamValue::Color(c)) => div()
                    .flex()
                    .gap(px(2.))
                    .children(PALETTE.iter().enumerate().map(|(i, (name, col))| {
                        self.swatch(t, i, palette_name(name), *col, *col == c)
                    }))
                    .into_any_element(),
                (Param::Width, ParamValue::Width(w)) => self.segments(
                    t,
                    "width",
                    WIDTHS
                        .iter()
                        .map(|v| (*v, format!("{v:.0} px"), pixels_label(*v))),
                    w,
                    ParamValue::Width,
                ),
                (Param::TextSize, ParamValue::TextSize(w)) => self.segments(
                    t,
                    "tsize",
                    TEXT_SIZES
                        .iter()
                        .map(|v| (*v, format!("{v:.0}"), pixels_label(*v))),
                    w,
                    ParamValue::TextSize,
                ),
                (Param::StepSize, ParamValue::StepSize(w)) => self.segments(
                    t,
                    "ssize",
                    STEP_SIZES
                        .iter()
                        .map(|v| (*v, format!("{v:.0}"), pixels_label(*v))),
                    w,
                    ParamValue::StepSize,
                ),
                (Param::Opacity, ParamValue::Opacity(w)) => self.segments(
                    t,
                    "opacity",
                    OPACITIES.iter().map(|v| {
                        (
                            *v,
                            format!("{:.0}%", v * 100.0),
                            tr!("{n} percent", n = format!("{:.0}", v * 100.0)),
                        )
                    }),
                    w,
                    ParamValue::Opacity,
                ),
                (Param::Strength, ParamValue::Strength(v)) => self.stepper(
                    t,
                    "strength",
                    tr_ctx!("annotation setting", "Blur strength"),
                    (tr!("Decrease blur strength"), tr!("Increase blur strength")),
                    v,
                    (4, 32),
                    ParamValue::Strength,
                    cx,
                ),
                (Param::Block, ParamValue::Block(v)) => self.stepper(
                    t,
                    "block",
                    tr_ctx!("annotation setting", "Pixel size"),
                    (tr!("Decrease pixel size"), tr!("Increase pixel size")),
                    v,
                    (6, 32),
                    ParamValue::Block,
                    cx,
                ),
                _ => return None,
            };
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(section_label(t, &p.label()))
                .child(body)
                .into_any_element(),
        )
    }

    fn segments(
        &self,
        t: &Theme,
        id: &'static str,
        options: impl Iterator<Item = (f64, String, String)>,
        current: f64,
        make: fn(f64) -> ParamValue,
    ) -> AnyElement {
        div()
            .flex()
            .gap(px(6.))
            .children(options.enumerate().map(|(i, (v, text, label))| {
                self.seg_button(
                    t,
                    ElementId::from((id, i)),
                    text,
                    label,
                    near(v, current),
                    self.set_param_action(make(v)),
                )
            }))
            .into_any_element()
    }

    pub(super) fn inspector(&self, t: &Theme, s: &Snap, cx: &mut Context<Self>) -> AnyElement {
        let title = s.inspector_title.clone();
        let tool_icon = s.tool.icon();
        let subtitle = if s.selected {
            tr!("Changes apply to the selected annotation.")
        } else if s.tool == Tool::Select {
            tr!("Pick a tool, or click an annotation to edit it.")
        } else {
            tr!("Settings for new annotations.")
        };
        let params: Vec<AnyElement> = s
            .params
            .iter()
            .filter_map(|(p, v)| self.param_group(t, *p, *v, cx))
            .collect();
        let selected = s.selected && !s.editing_text;
        let has_crop = s.has_crop;
        let (iw, ih) = s.image;
        let pointer = s
            .hover
            .map(|p| format!("{:.0}, {:.0}", p.x.round(), p.y.round()))
            .unwrap_or_else(|| "-".into());
        let m = mod_label().to_owned();

        let info_row = |label: String, value: String| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(13.))
                .child(
                    div()
                        .min_w(px(0.))
                        .text_color(rgba_of(t.text_3))
                        .child(label),
                )
                .child(
                    div()
                        .text_color(rgba_of(t.text))
                        .mono()
                        .tnum()
                        .child(SharedString::from(value)),
                )
        };
        let col = || card(t).flex().flex_col().gap(px(12.)).flex_none();

        let status_card = self.statusbar(t, s);
        div()
            .id("editor-inspector")
            .role(Role::Group)
            .aria_label(tr!("Inspector"))
            .w(px(INSPECTOR_W))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .id("editor-inspector-scroll")
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
                                    .flex_col()
                                    .gap(px(4.))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .child(icon(tool_icon, 18., t.text))
                                            .child(
                                                div()
                                                    .id("inspector-title")
                                                    .min_w(px(0.))
                                                    .role(Role::Heading)
                                                    .aria_level(2)
                                                    .aria_label(SharedString::from(title.clone()))
                                                    .text_size(px(type_scale::BODY))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(SharedString::from(title)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(type_scale::CAPTION))
                                            .text_color(rgba_of(t.mist))
                                            .child(subtitle),
                                    ),
                            )
                            .children(params),
                    )
                    .when(
                        s.redact_available
                            && (!s.proposals.is_empty() || s.redact != RedactPhase::Idle),
                        |d| d.child(self.redact_card(t, s)),
                    )
                    .when(s.tool == Tool::Crop, |d| {
                        d.child(
                            col()
                                .child(section_label(t, &tr_ctx!("tool", "Crop")))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .text_color(rgba_of(t.text_3))
                                        .child(tr!(
                                    "Drag on the image to set the visible area. The original and \
                                     the annotations outside it are kept."
                                )),
                                )
                                .when(has_crop, |d| {
                                    d.child(
                                        button(
                                            t,
                                            "crop-reset",
                                            tr!("Reset crop"),
                                            ButtonKind::Secondary,
                                        )
                                        .on_click(self.doc_action(|d, cx| d.clear_crop(cx))),
                                    )
                                }),
                        )
                    })
                    .when(selected, |d| {
                        d.child(
                            col().child(section_label(t, &tr!("Annotation"))).child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(8.))
                                    .child(
                                        button_with_icon(
                                            t,
                                            "ann-duplicate",
                                            tr!("Duplicate"),
                                            ButtonKind::Secondary,
                                            Some("copy"),
                                        )
                                        .on_click(self.doc_action(|d, cx| d.duplicate(cx)))
                                        .child(keycaps(t, &[m.clone(), "D".into()])),
                                    )
                                    .child(
                                        // An undoable immediate delete, not a confirmation: neutral.
                                        button_with_icon(
                                            t,
                                            "ann-delete",
                                            tr_ctx!("button", "Delete"),
                                            ButtonKind::Secondary,
                                            Some("trash-2"),
                                        )
                                        .on_click(self.doc_action(|d, cx| d.delete_selected(cx)))
                                        .child(keycaps(t, &["Del".to_owned()])),
                                    ),
                            ),
                        )
                    })
                    .child(
                        div()
                            .id("inspector-image-info")
                            .role(Role::Group)
                            .aria_label(SharedString::from(tr!(
                                "Image {iw} by {ih} pixels. Pointer {pointer}",
                                iw = iw,
                                ih = ih,
                                pointer = pointer
                            )))
                            .child(
                                col()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .child(icon("image", 16., t.mist))
                                            .child(section_label(
                                                t,
                                                &tr_ctx!("inspector", "Image"),
                                            )),
                                    )
                                    .child(info_row(
                                        tr_ctx!("image info", "Size"),
                                        format!("{iw} \u{D7} {ih} px"),
                                    ))
                                    .child(info_row(tr_ctx!("image info", "Pointer"), pointer)),
                            ),
                    ),
            )
            .child(status_card)
            .into_any_element()
    }

    /// Auto-redact proposals (ANN-2.AC4): what was found, Accept all / Accept one / Dismiss.
    /// Nothing is blurred until the user accepts.
    fn redact_card(&self, t: &Theme, s: &Snap) -> AnyElement {
        let note = match &s.redact {
            RedactPhase::Running => tr!("Looking for emails and keys\u{2026}"),
            RedactPhase::NothingFound => tr!("No emails or keys found in this image."),
            RedactPhase::Failed(e) => tr!("Couldn\u{2019}t look for text: {e}", e = e.text()),
            RedactPhase::Idle => trn!(
                "{n} suggested. Nothing is blurred until you accept. Click a dashed box on the \
                 image to accept just that one.",
                "{n} suggested. Nothing is blurred until you accept. Click a dashed box on the \
                 image to accept just that one.",
                s.proposals.len()
            ),
        };
        let rows: Vec<AnyElement> = s
            .proposals
            .iter()
            .enumerate()
            .map(|(i, (kind, preview, conf))| {
                let label = tr!(
                    "{kind} {preview}, confidence {percent} percent",
                    kind = kind,
                    preview = preview,
                    percent = format!("{:.0}", conf * 100.0)
                );
                div()
                    .id(("redact-row", i))
                    .role(Role::Group)
                    .aria_label(SharedString::from(label))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .child(
                        div()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(rgba_of(t.text))
                                    .child(kind.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(type_scale::CAPTION))
                                    .text_color(rgba_of(t.text_3))
                                    .mono()
                                    .child(SharedString::from(preview.clone())),
                            ),
                    )
                    .child(
                        button(
                            t,
                            ("redact-accept", i),
                            tr_ctx!("button", "Accept"),
                            ButtonKind::Secondary,
                        )
                        .on_click(self.doc_action(move |d, cx| d.accept_proposal(Some(i), cx))),
                    )
                    .into_any_element()
            })
            .collect();
        let has = !s.proposals.is_empty();
        card(t)
            .id("editor-redact")
            .role(Role::Group)
            .aria_label(tr!("Auto-redact suggestions"))
            .flex()
            .flex_col()
            .gap(px(12.))
            .flex_none()
            .child(section_label(t, &tr!("Auto-redact")))
            .child(
                div()
                    .id("redact-note")
                    .role(Role::Status)
                    .aria_label(SharedString::from(note.clone()))
                    .text_size(px(13.))
                    .text_color(rgba_of(t.text_3))
                    .child(SharedString::from(note)),
            )
            .children(rows)
            .when(s.kinds_open, |d| d.child(self.redact_kinds(t, s)))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .when(has, |d| {
                        d.child(
                            button(
                                t,
                                "redact-accept-all",
                                tr!("Accept all"),
                                ButtonKind::Primary,
                            )
                            .on_click(self.doc_action(|d, cx| d.accept_proposal(None, cx))),
                        )
                    })
                    .child(
                        button(
                            t,
                            "redact-kinds-toggle",
                            if s.kinds_open {
                                tr_ctx!("button", "Hide options")
                            } else {
                                tr_ctx!("button", "What to look for")
                            },
                            ButtonKind::Secondary,
                        )
                        .aria_expanded(s.kinds_open)
                        .on_click(self.doc_action(|d, cx| d.toggle_kinds_open(cx))),
                    )
                    .child(
                        button(
                            t,
                            "redact-dismiss",
                            if has {
                                tr_ctx!("button", "Dismiss")
                            } else {
                                tr_ctx!("button", "Close")
                            },
                            ButtonKind::Secondary,
                        )
                        .on_click(self.doc_action(|d, cx| d.dismiss_redactions(cx))),
                    ),
            )
            .into_any_element()
    }

    /// The kinds Auto-redact looks for, in groups: a switch per group and a chip per kind, a
    /// reset to the defaults and a new search. The choice is remembered for this session.
    fn redact_kinds(&self, t: &Theme, s: &Snap) -> AnyElement {
        use crate::editor::redact::{group_kinds, group_title, kind_label, GROUP_COUNT};
        let groups: Vec<AnyElement> = (0..GROUP_COUNT)
            .map(|g| {
                let title = group_title(g);
                let chips: Vec<AnyElement> = group_kinds(g)
                    .iter()
                    .map(|k| {
                        let k = *k;
                        let on = s.kinds.is_on(k);
                        let label = kind_label(k.as_str());
                        let hover = rgba_of(t.raised);
                        div()
                            .id(ElementId::from(("redact-kind", g * 16 + chips_index(g, k))))
                            .role(Role::Button)
                            .aria_label(SharedString::from(label.clone()))
                            .aria_toggled(toggled(on))
                            .focusable()
                            .tab_stop(true)
                            .min_w(px(0.))
                            .px(px(8.))
                            .py(px(4.))
                            .rounded(px(scale::RADIUS_SM))
                            .border_1()
                            .border_color(rgba_of(if on { accent(t) } else { t.line }))
                            .cursor_pointer()
                            .text_size(px(type_scale::CAPTION))
                            .when(on, |d| d.bg(rgba_of(t.raised)).text_color(rgba_of(t.text)))
                            .when(!on, |d| {
                                d.text_color(rgba_of(t.text_3)).hover(move |s| s.bg(hover))
                            })
                            .focus_visible(|s| s.border_color(rgba_of(t.text)))
                            .on_click(self.doc_action(move |d, cx| {
                                d.edit_redact_kinds(
                                    |c| {
                                        c.toggle(k);
                                    },
                                    cx,
                                )
                            }))
                            .child(shrink_text(SharedString::from(label)))
                            .into_any_element()
                    })
                    .collect();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.))
                            .child(
                                shrink_text(SharedString::from(title.clone()))
                                    .text_size(px(13.))
                                    .text_color(rgba_of(t.text)),
                            )
                            .child(
                                switch(t, ("redact-group", g), title, s.kinds.group_on(g), true)
                                    .on_click(self.doc_action(move |d, cx| {
                                        d.edit_redact_kinds(
                                            |c| {
                                                c.toggle_group(g);
                                            },
                                            cx,
                                        )
                                    })),
                            ),
                    )
                    .child(div().flex().flex_wrap().gap(px(6.)).children(chips))
                    .into_any_element()
            })
            .collect();
        let is_default = s.kinds.is_default();
        div()
            .flex()
            .flex_col()
            .id("redact-kinds")
            .role(Role::Group)
            .aria_label(tr!("Kinds to look for"))
            .gap(px(12.))
            .children(groups)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(
                        button(
                            t,
                            "redact-kinds-defaults",
                            tr_ctx!("button", "Defaults"),
                            ButtonKind::Secondary,
                        )
                        .when(is_default, |b| b.opacity(0.5))
                        .on_click(self.doc_action(|d, cx| d.edit_redact_kinds(|c| c.reset(), cx))),
                    )
                    .child(
                        button(
                            t,
                            "redact-search-again",
                            tr_ctx!("button", "Search again"),
                            ButtonKind::Primary,
                        )
                        .on_click(self.doc_action(|d, cx| d.auto_redact(cx))),
                    ),
            )
            .into_any_element()
    }

    /// The save-status card pinned to the bottom of the inspector: the status pill with its
    /// icon (saved, saving, failed) and the selection summary under it.
    fn statusbar(&self, t: &Theme, s: &Snap) -> AnyElement {
        let (text, kind) = match &s.status {
            Status::Saved => (tr!("All changes saved"), StatusKind::Ok),
            Status::Saving => (tr!("Saving\u{2026}"), StatusKind::Pending),
            Status::Failed(e) => (tr!("Couldn\u{2019}t save: {e}", e = e), StatusKind::Warning),
        };
        let summary = s.summary.clone();
        card(t)
            .id("editor-statusbar")
            .role(Role::Status)
            .aria_label(SharedString::from(format!("{text}. {summary}")))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .child(status(t, kind, SharedString::from(text))),
            )
            .child(
                div()
                    .min_w(px(0.))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.text_3))
                    .mono()
                    .child(SharedString::from(summary)),
            )
            .into_any_element()
    }
}

/// A stable element index for a kind inside its group.
fn chips_index(g: usize, k: cm_render::ocr::RedactionKind) -> usize {
    crate::editor::redact::group_kinds(g)
        .iter()
        .position(|x| *x == k)
        .unwrap_or_default()
}
