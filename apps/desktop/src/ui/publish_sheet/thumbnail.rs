use super::*;

impl PublishPage {
    // ---- thumbnail -----------------------------------------------------------------------------

    fn show_thumb_file(&mut self, path: PathBuf, from_frame: bool, cx: &mut Context<Self>) {
        self.thumb.busy = true;
        self.thumb.note = None;
        self.tasks.push(cx.spawn(async move |this, cx| {
            let p2 = path.clone();
            let decoded = cx
                .background_executor()
                .spawn(async move {
                    std::fs::read(&p2)
                        .map_err(|e| files::PosterError::Os(e.to_string()))
                        .and_then(|b| decode_poster(&b, 640))
                })
                .await;
            this.update(cx, |this, cx| {
                this.thumb.busy = false;
                this.thumb.from_frame = from_frame;
                match decoded {
                    Ok(rgba) => {
                        this.thumb.image = Some(Arc::new(RenderImage::new([Frame::new(rgba)])));
                    }
                    Err(e) => {
                        this.thumb.image = None;
                        this.thumb.note = Some(ThumbNote::NoPreview(e));
                    }
                }
                this.form.thumbnail = Some(path);
                cx.notify();
            })
            .ok();
        }));
    }

    /// Picks the frame at the slider's position (FFmpeg writes one JPEG).
    fn pick_frame(&mut self, cx: &mut Context<Self>) {
        let (Some(id), Load::Ready(l)) = (self.id.clone(), &self.load) else {
            return;
        };
        let (Some((video, _)), Some(duration)) = (l.export(), l.duration_secs()) else {
            return;
        };
        let Some(root) = self.app.read(cx).backend.library_root.clone() else {
            return;
        };
        if self.thumb.busy {
            // The picture being made is for an older position: `seq` makes it come again.
            return;
        }
        let seq = self.thumb.seq;
        let out = thumb::frame_path(&root, &id);
        let at = thumb::position_secs(duration, self.thumb.frac);
        self.thumb.busy = true;
        self.thumb.note = None;
        let out2 = out.clone();
        self.tasks.push(cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { thumb::extract(&video, at, &out2) })
                .await;
            this.update(cx, |this, cx| {
                match done {
                    Ok(()) => this.show_thumb_file(out, true, cx),
                    Err(e) => {
                        this.thumb.busy = false;
                        this.thumb.note = Some(ThumbNote::Said(e));
                    }
                }
                // The slider moved on while the frame was made: take the newer one.
                if this.thumb.seq != seq && !this.thumb.dragging {
                    this.schedule_frame(cx);
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Moves the slider; the frame follows once it has rested (or at once with `now`).
    fn set_slider(&mut self, frac: f32, now: bool, cx: &mut Context<Self>) {
        if self.stage.locked(&lock(&TRACKER)) {
            return;
        }
        self.thumb.frac = frac.clamp(0.0, 1.0);
        self.thumb.seq += 1;
        if now {
            self.pick_frame(cx);
        } else {
            self.schedule_frame(cx);
        }
        cx.notify();
    }

    fn schedule_frame(&mut self, cx: &mut Context<Self>) {
        let seq = self.thumb.seq;
        self.tasks.push(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FRAME_DEBOUNCE).await;
            this.update(cx, |this, cx| {
                if this.thumb.seq == seq && !this.thumb.dragging {
                    this.pick_frame(cx);
                }
            })
            .ok();
        }));
    }

    fn slider_x(&self, ev_x: Pixels) -> f32 {
        let b = self.slider_bounds.get();
        slider::fraction_at(
            f32::from(b.origin.x),
            f32::from(b.size.width),
            f32::from(ev_x),
        )
    }

    pub(super) fn slider_down(
        &mut self,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.stage.locked(&lock(&TRACKER)) {
            return;
        }
        window.focus(&self.thumb_focus, cx);
        self.thumb.dragging = true;
        let f = self.slider_x(ev.position.x);
        self.set_slider(f, false, cx);
    }

    pub(super) fn slider_move(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.thumb.dragging {
            let f = self.slider_x(ev.position.x);
            self.set_slider(f, false, cx);
        }
    }

    pub(super) fn slider_up(&mut self, _ev: &MouseUpEvent, cx: &mut Context<Self>) {
        if self.thumb.dragging {
            self.thumb.dragging = false;
            let f = self.thumb.frac;
            self.set_slider(f, true, cx);
        }
    }

    pub(super) fn slider_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        if let Some(v) = slider::nudge(
            self.thumb.frac,
            k.key.as_str(),
            k.modifiers.shift,
            thumb::KEY_STEP,
            thumb::BIG_STEP,
        ) {
            cx.stop_propagation();
            self.set_slider(v, false, cx);
        }
    }

    pub(super) fn choose_image(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr!("Choose a thumbnail").into()),
        });
        self.tasks.push(cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update(cx, |this, cx| this.show_thumb_file(path, false, cx))
                .ok();
        }));
    }

    pub(super) fn clear_thumb(&mut self, cx: &mut Context<Self>) {
        self.thumb = Thumb::default();
        self.form.thumbnail = None;
        cx.notify();
    }
}
