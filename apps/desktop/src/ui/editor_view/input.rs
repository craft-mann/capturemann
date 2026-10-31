use super::*;

impl EditorView {
    pub(super) fn canvas_size(&self) -> (f64, f64) {
        let b = self.canvas_bounds.get();
        (
            f64::from(f32::from(b.size.width)),
            f64::from(f32::from(b.size.height)),
        )
    }

    fn to_image(&self, pos: gpui::Point<Pixels>) -> Pt {
        let b = self.canvas_bounds.get();
        self.viewport.to_image(
            f64::from(f32::from(pos.x - b.origin.x)),
            f64::from(f32::from(pos.y - b.origin.y)),
        )
    }

    fn local(&self, pos: gpui::Point<Pixels>) -> (f64, f64) {
        let b = self.canvas_bounds.get();
        (
            f64::from(f32::from(pos.x - b.origin.x)),
            f64::from(f32::from(pos.y - b.origin.y)),
        )
    }

    fn tol(&self) -> f64 {
        self.viewport.image_len(HIT_PX)
    }

    pub(super) fn leave(&mut self, cx: &mut Context<Self>) {
        app::set_route(&self.app, Route::Screenshots, cx);
    }

    // ---- events -------------------------------------------------------------------------

    pub(super) fn on_key_down(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.doc.clone() else { return };
        let k = &ev.keystroke;
        let mods = mods_of(k.modifiers);
        let editing = doc.read(cx).state.text.is_some();
        if editing {
            if mods.secondary && k.key == "v" {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    doc.update(cx, |d, cx| d.text_paste(&text, cx));
                }
                cx.stop_propagation();
                return;
            }
            let used = doc.update(cx, |d, cx| {
                d.text_key(&k.key, k.key_char.as_deref(), mods, cx)
            });
            if used {
                cx.stop_propagation();
                return;
            }
        }
        if k.key == "escape" && self.shortcuts_open && !editing {
            self.shortcuts_open = false;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let canvas_focused = self.canvas_focus.is_focused(window);
        if k.key == "space" && canvas_focused && !editing {
            self.space_down = true;
            cx.stop_propagation();
            return;
        }
        let Some(cmd) = keys::interpret(&k.key, mods, canvas_focused) else {
            return;
        };
        cx.stop_propagation();
        match cmd {
            KeyCommand::ZoomIn => self.viewport.zoom_step(true),
            KeyCommand::ZoomOut => self.viewport.zoom_step(false),
            KeyCommand::ZoomFit => self.viewport.fit_view(),
            KeyCommand::ZoomActual => self.viewport.actual_size(),
            KeyCommand::Back => self.leave(cx),
            KeyCommand::SaveAs => self.save_as(cx),
            other => {
                let (cw, ch) = self.canvas_size();
                let center = self.viewport.to_image(cw / 2.0, ch / 2.0);
                let out = doc.update(cx, |d, cx| d.command(other, center, cx));
                if out == Outcome::Leave {
                    self.leave(cx);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn on_key_up(&mut self, ev: &KeyUpEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if ev.keystroke.key == "space" && self.space_down {
            self.space_down = false;
            self.pan_from = None;
            cx.notify();
        }
    }

    pub(super) fn on_mouse_down(
        &mut self,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.doc.clone() else { return };
        window.focus(&self.canvas_focus, cx);
        if self.shortcuts_open {
            // A press on the canvas is "outside" the shortcuts popover.
            self.shortcuts_open = false;
            cx.notify();
        }
        if ev.button == MouseButton::Middle || (self.space_down && ev.button == MouseButton::Left) {
            self.pan_from = Some((f32::from(ev.position.x), f32::from(ev.position.y)));
            cx.notify();
            return;
        }
        if ev.button != MouseButton::Left {
            return;
        }
        let p = self.to_image(ev.position);
        let tol = self.tol();
        doc.update(cx, |d, cx| {
            d.pointer_down(p, mods_of(ev.modifiers), ev.click_count, tol, cx)
        });
    }

    pub(super) fn on_mouse_move(
        &mut self,
        ev: &MouseMoveEvent,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.doc.clone() else { return };
        if let Some((lx, ly)) = self.pan_from {
            let (x, y) = (f32::from(ev.position.x), f32::from(ev.position.y));
            self.viewport.pan_by(f64::from(x - lx), f64::from(y - ly));
            self.pan_from = Some((x, y));
            cx.notify();
            return;
        }
        let p = self.to_image(ev.position);
        let tol = self.tol();
        doc.update(cx, |d, cx| {
            d.pointer_move(p, mods_of(ev.modifiers), tol, cx)
        });
    }

    pub(super) fn on_mouse_up(
        &mut self,
        ev: &MouseUpEvent,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = self.doc.clone() else { return };
        if self.pan_from.take().is_some() {
            cx.notify();
            return;
        }
        if ev.button != MouseButton::Left {
            return;
        }
        let p = self.to_image(ev.position);
        let tol = self.tol();
        doc.update(cx, |d, cx| d.pointer_up(p, mods_of(ev.modifiers), tol, cx));
    }

    pub(super) fn on_scroll(
        &mut self,
        ev: &ScrollWheelEvent,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (lx, ly) = self.local(ev.position);
        let d = match ev.delta {
            ScrollDelta::Pixels(p) => (f64::from(f32::from(p.x)), f64::from(f32::from(p.y))),
            ScrollDelta::Lines(l) => (f64::from(l.x) * 20.0, f64::from(l.y) * 20.0),
        };
        let m = mods_of(ev.modifiers);
        if m.secondary || ev.modifiers.control {
            self.viewport.zoom_by_at((d.1 * 0.01).exp(), lx, ly);
        } else {
            self.viewport.pan_by(d.0, d.1);
        }
        cx.notify();
    }

    pub(super) fn on_pinch(&mut self, ev: &PinchEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let (lx, ly) = self.local(ev.position);
        self.viewport
            .zoom_by_at((1.0 + f64::from(ev.delta)).max(0.2), lx, ly);
        cx.notify();
    }

    pub(super) fn save_as(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.clone() else { return };
        let name = crate::editor::session::suggested_file_name(&doc.read(cx).title);
        let dir = dirs::picture_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(std::env::temp_dir);
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |_, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                doc.update(cx, |d, cx| d.save_to(path, cx));
            }
        })
        .detach();
    }

    pub(super) fn reveal_label() -> String {
        if cfg!(target_os = "macos") {
            tr!("Reveal in Finder")
        } else {
            tr!("Show in folder")
        }
    }

    pub(super) fn reveal(&mut self, cx: &mut Context<Self>) {
        if let Some(doc) = &self.doc {
            let dir = doc.read(cx).dir.clone();
            cx.reveal_path(&dir);
        }
    }
}

/// The text annotation as a platform text input, so an input method (Telex, VNI, Pinyin, an
/// IBus engine on X11) can compose in it. Typed keys still arrive through `on_key_down`; this
/// receives only what the input method commits or drafts.
impl gpui::EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range: std::ops::Range<usize>,
        adjusted_range: &mut Option<std::ops::Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let doc = self.doc.clone()?;
        let d = doc.read(cx);
        let t = d.state.text.as_ref()?;
        let (text, used) = t.text_utf16(range);
        *adjusted_range = Some(used);
        Some(text)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::UTF16Selection> {
        let doc = self.doc.clone()?;
        let d = doc.read(cx);
        let t = d.state.text.as_ref()?;
        Some(gpui::UTF16Selection {
            range: t.selection_utf16(),
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        let doc = self.doc.as_ref()?;
        doc.read(cx).state.text.as_ref()?.marked_utf16()
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(doc) = self.doc.clone() {
            doc.update(cx, |d, cx| d.text_unmark(cx));
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(doc) = self.doc.clone() {
            doc.update(cx, |d, cx| d.text_ime(range, text, false, cx));
        }
        window.invalidate_character_coordinates();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<std::ops::Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(doc) = self.doc.clone() {
            doc.update(cx, |d, cx| d.text_ime(range, new_text, true, cx));
        }
        window.invalidate_character_coordinates();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range: std::ops::Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        // The candidate window sits at the caret.
        let doc = self.doc.clone()?;
        let geom = doc.update(cx, |d, _| d.overlay_data().text)?;
        let (x, y) = self.viewport.to_screen(geom.caret);
        let b = self.canvas_bounds.get();
        let z = self.viewport.zoom();
        Some(Bounds::new(
            point(b.origin.x + px(x as f32), b.origin.y + px(y as f32)),
            size(px(1.), px((geom.caret_h * z) as f32)),
        ))
        .or(Some(element_bounds))
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
