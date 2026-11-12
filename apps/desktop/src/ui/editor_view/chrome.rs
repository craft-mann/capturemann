use super::*;

impl EditorView {
    pub(super) fn status_screen(
        &self,
        t: &Theme,
        title: &str,
        detail: Option<String>,
    ) -> AnyElement {
        let app = self.app.clone();
        div()
            .id("editor-status")
            .role(Role::Group)
            .aria_label(SharedString::from(title.to_owned()))
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .bg(rgba_of(t.ground))
            .text_color(rgba_of(t.text))
            .child(
                div()
                    .text_size(px(type_scale::TITLE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(title.to_owned())),
            )
            .when_some(detail, |d, e| {
                d.child(
                    div()
                        .max_w(px(420.))
                        .text_color(rgba_of(t.text_2))
                        .child(SharedString::from(e)),
                )
            })
            .child(
                button(
                    t,
                    "editor-back",
                    tr!("Back to library"),
                    ButtonKind::Secondary,
                )
                .on_click(move |_, _, cx| app::set_route(&app, Route::Screenshots, cx)),
            )
            .into_any_element()
    }

    pub(super) fn ready(
        &mut self,
        t: &Theme,
        doc_e: Entity<EditorDoc>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Keep the viewport in step with the canvas size, the crop and the window's DPR.
        let measured = self.canvas_size();
        if measured.0 > 8.0 && measured.1 > 8.0 {
            self.viewport.set_canvas(measured);
        }
        self.viewport.set_insets(CHROME_INSETS);
        self.viewport.dpr = f64::from(window.scale_factor());
        let content = doc_e.read(cx).state.content_rect();
        self.viewport.set_content(content);

        let overlay = doc_e.update(cx, |d, _| d.overlay_data());
        let snap = Snap::of(doc_e.read(cx), self.viewport.zoom());
        let toolbar = self.toolbar(t, &snap, cx);
        let canvas = self.canvas_pane(t, &snap, overlay, cx);
        // The image strip (0.3.3): the Library's list, shown under the canvas.
        let list = self.strip_list(cx);
        self.warm_neighbors(&list, cx);
        let images = self.images_state(&list);
        let strip = crate::editor::strip_model::strip_visible(
            &list,
            &self.capture_id,
            f32::from(window.viewport_size().height),
            self.strip.pref_shown,
        )
        .then(|| {
            let live = doc_e.read(cx).state.ops.len();
            self.image_strip(t, &list, live, window, cx)
        });
        let pills = self.floating_chrome(t, &snap, images, cx);
        let popover = self.shortcuts_open.then(|| self.shortcuts_popover(t, cx));
        let inspector = self.inspector(t, &snap, cx);
        let warn = snap.warning.clone();

        div()
            .id("editor")
            .role(Role::Group)
            .aria_label(tr!("Annotation editor"))
            // i18n-ignore (key context id)
            .key_context("Editor")
            .track_focus(&self.root_focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_key_up(cx.listener(Self::on_key_up))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgba_of(t.ground))
            .text_color(rgba_of(t.text))
            .child(toolbar)
            .when_some(warn, |d, w| {
                d.child(
                    div()
                        .id("editor-warning")
                        .role(Role::Alert)
                        .aria_label(SharedString::from(tr!("Warning: {w}", w = w)))
                        .px(px(16.))
                        .py(px(6.))
                        .bg(rgba_of(crate::theme::derived::WARNING_TINT))
                        .text_color(rgba_of(t.warning))
                        .text_size(px(type_scale::CAPTION))
                        .child(SharedString::from(w)),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .gap(px(16.))
                    .px(px(24.))
                    .py(px(16.))
                    // The stage: the canvas fills it; the pills are absolute siblings above it,
                    // so the canvas element and its hit testing are untouched.
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .min_h(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(10.))
                            .child(
                                div()
                                    .relative()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .min_h(px(0.))
                                    .flex()
                                    .child(canvas)
                                    .child(pills)
                                    .children(popover),
                            )
                            .children(strip),
                    )
                    .child(inspector),
            )
            .into_any_element()
    }

    // ---- toolbar ------------------------------------------------------------------------

    /// A DESIGN toolbar button with keycaps; `enabled == false` dims it and drops the action.
    #[allow(clippy::too_many_arguments)]
    fn tbtn(
        &self,
        t: &Theme,
        id: &'static str,
        label: String,
        glyph: Option<&'static str>,
        kind: ButtonKind,
        keys: &[String],
        enabled: bool,
        on: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let b = button_with_icon(t, id, label.clone(), kind, glyph)
            .when(kind == ButtonKind::Primary && !keys.is_empty(), |b| {
                b.child(primary_keycaps(t, keys))
            })
            .tooltip(tooltip::tip(label, keys.to_vec()));
        if enabled {
            b.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| on(this, window, cx)))
                .into_any_element()
        } else {
            b.opacity(0.45).cursor_default().into_any_element()
        }
    }

    /// Header (kit `Editor` top bar): breadcrumb and title on the left, Save as / Reveal /
    /// Copy image on the right. It wraps instead of clipping when the window is narrow.
    pub(super) fn toolbar(&self, t: &Theme, s: &Snap, cx: &mut Context<Self>) -> AnyElement {
        let title = if s.title.is_empty() {
            self.capture_id.clone()
        } else {
            s.title.clone()
        };
        let m = mod_label().to_owned();
        let shifted = |k: &str| {
            if cfg!(target_os = "macos") {
                vec![shift_label().to_owned(), m.clone(), k.to_owned()]
            } else {
                vec![m.clone(), shift_label().to_owned(), k.to_owned()]
            }
        };
        let back_keys = vec![m.clone(), "[".to_owned()];
        let crumb = crumb_link(
            t,
            "editor-back",
            tr_ctx!("breadcrumb", "Screenshots /"),
            tr!("Library"),
        )
        .aria_keyshortcuts("[")
        .tooltip(tooltip::tip(tr!("Back to library"), back_keys))
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.leave(cx)));
        let reveal = Self::reveal_label();
        let actions: Vec<AnyElement> = vec![
            // Icon-only: the aria label and the tooltip carry the (platform) name.
            self.reveal_button(t, reveal, cx),
            self.tbtn(
                t,
                "editor-save-as",
                tr!("Save as\u{2026}"),
                Some("save"),
                ButtonKind::Secondary,
                &shifted("S"),
                true,
                |this, _, cx| this.save_as(cx),
                cx,
            ),
            self.tbtn(
                t,
                "editor-copy",
                tr!("Copy image"),
                Some("copy"),
                ButtonKind::Primary,
                &shifted("C"),
                true,
                |this, _, cx| {
                    if let Some(d) = &this.doc {
                        d.update(cx, |d, cx| d.copy_image(cx));
                    }
                },
                cx,
            ),
        ];
        page_toolbar_actions(
            t,
            "editor-toolbar",
            tr!("Editor toolbar"),
            crumb,
            SharedString::from(title),
            actions,
        )
        .into_any_element()
    }

    /// The folder button of the header: a 40 px secondary square with the `folder-open` icon.
    fn reveal_button(&self, t: &Theme, label: String, cx: &mut Context<Self>) -> AnyElement {
        let c = ButtonKind::Secondary.colors(t);
        let (hover, focus) = (rgba_of(c.hover), rgba_of(t.text));
        div()
            .id("editor-reveal")
            .role(Role::Button)
            .aria_label(label.clone())
            .focusable()
            .tab_stop(true)
            .size(px(scale::CONTROL_H))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(c.border))
            .bg(rgba_of(c.bg))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .focus_visible(move |s| s.border_color(focus))
            .tooltip(tooltip::tip(label, Vec::new()))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.reveal(cx)))
            .child(icon("folder-open", 16., c.fg))
            .into_any_element()
    }

    /// The floating chrome over the canvas (redesign R4): the tool pill on the left and the
    /// transport pill at the bottom. Both are absolute SIBLINGS of the canvas: their wrappers
    /// have no hitbox (pointer events beside the pills reach the canvas) and the pills occlude
    /// (events on them never reach the canvas).
    fn floating_chrome(
        &mut self,
        t: &Theme,
        s: &Snap,
        images: Option<bool>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tools = div()
            .absolute()
            .left(px(PILL_MARGIN))
            .top(px(PILL_MARGIN))
            .bottom(px(PILL_MARGIN))
            .flex()
            .items_center()
            .child(self.tool_rail(t, s.tool, cx));
        let transport = div()
            .absolute()
            .left(px(CHROME_INSETS.left as f32))
            .right_0()
            .bottom(px(PILL_MARGIN))
            .flex()
            .justify_center()
            .child(self.view_bar(t, s, images, cx));
        div()
            .absolute()
            .size_full()
            .child(tools)
            .child(transport)
            .into_any_element()
    }

    /// The pill's surface: `surface`, `line` border, radius 14, above the canvas.
    pub(super) fn pill_surface(t: &Theme) -> Div {
        div()
            .occlude()
            .p(px(6.))
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface))
    }

    /// The shortcuts popover: hidden by default, opened by the keyboard button, closed by Esc,
    /// the button again or a press on the canvas. Anchored above the transport pill at the
    /// canvas' bottom-right (the `deferred(anchored())` pattern of the library's card menu).
    fn shortcuts_popover(&self, t: &Theme, _cx: &mut Context<Self>) -> AnyElement {
        let b = self.canvas_bounds.get();
        let at = point(
            b.origin.x + b.size.width - px(PILL_MARGIN),
            b.origin.y + b.size.height - px(PILL_MARGIN + TRANSPORT_H + 8.),
        );
        let m = mod_label().to_owned();
        let row = |keys: Vec<String>, text: String| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .child(
                    div()
                        .min_w(px(0.))
                        .text_size(px(13.))
                        .text_color(rgba_of(t.text_3))
                        .child(text),
                )
                .child(keycaps(t, &keys))
        };
        let body = div()
            .id("editor-shortcuts-popover")
            .occlude()
            .role(Role::Dialog)
            .aria_label(tr!("Keyboard shortcuts"))
            .w(px(280.))
            .p(px(14.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface))
            .text_color(rgba_of(t.text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(type_scale::UI))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(icon("keyboard", 16., t.text_2))
                    .child(shrink_text(tr!("Shortcuts"))),
            )
            .child(row(
                vec![shift_label().to_owned()],
                tr!("Constrain angle or shape"),
            ))
            .child(row(vec!["[".into(), "]".into()], tr!("Pick an annotation")))
            .child(row(
                vec![
                    "\u{2190}".into(),
                    "\u{2191}".into(),
                    "\u{2192}".into(),
                    "\u{2193}".into(),
                ],
                tr!("Move (Alt resizes)"),
            ))
            .child(row(vec!["Enter".into()], tr!("Place at the center")))
            .child(row(vec![m.clone(), "C".into()], tr!("Copy annotation")))
            .child(row(vec![m, "V".into()], tr!("Paste (+10 px)")))
            .child(row(vec!["Space".into()], tr!("Hold and drag to pan")));
        deferred(
            anchored()
                .position(at)
                .anchor(gpui::Anchor::BottomRight)
                .snap_to_window_with_margin(px(8.))
                .child(body),
        )
        .with_priority(10)
        .into_any_element()
    }

    // ---- tool palette -------------------------------------------------------------------

    /// The tool pill: 11 tools in three groups (draw, redact, crop), 44 px targets. The selected
    /// tool is the board's filled tile (accent fill, button-text glyph).
    fn tool_rail(&self, t: &Theme, active: Tool, cx: &mut Context<Self>) -> AnyElement {
        let mut rail = Self::pill_surface(t)
            .id("editor-tools")
            .role(Role::Toolbar)
            .aria_label(tr!("Annotation tools"))
            .aria_orientation(gpui::Orientation::Vertical)
            .w(px(TOOL_RAIL_W))
            .max_h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.))
            .overflow_y_scroll();
        for (i, tool) in Tool::ALL.into_iter().enumerate() {
            if tool_group_starts(tool) {
                rail = rail.child(
                    div()
                        .w(px(24.))
                        .h(px(1.))
                        .my(px(4.))
                        .flex_none()
                        .bg(rgba_of(t.line)),
                );
            }
            let on = tool == active;
            let key = tool.key().to_ascii_uppercase().to_string();
            let tip_key = vec![key.clone()];
            let hover = rgba_of(t.raised);
            rail = rail.child(
                div()
                    .id(ElementId::from(("tool", i)))
                    .role(Role::Button)
                    .aria_label(tool.label())
                    .aria_description(SharedString::from(tr!("Shortcut {key}", key = key)))
                    .aria_keyshortcuts(SharedString::from(key))
                    .aria_toggled(toggled(on))
                    .focusable()
                    .tab_stop(true)
                    .size(px(TOOL_BTN))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(scale::RADIUS_MD))
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .cursor_pointer()
                    .when(on, |d| d.bg(rgba_of(accent(t))))
                    .when(!on, |d| d.hover(move |s| s.bg(hover)))
                    .focus_visible(|s| s.border_color(rgba_of(t.text)))
                    .tooltip(tooltip::tip(tool.label(), tip_key.clone()))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        if let Some(d) = &this.doc {
                            d.update(cx, |d, cx| d.set_tool(tool, cx));
                        }
                        window.focus(&this.canvas_focus, cx);
                    }))
                    .child(icon(
                        tool.icon(),
                        20.,
                        if on { t.primary_fg } else { t.text_2 },
                    )),
            );
        }
        rail.into_any_element()
    }
}
