use super::*;

/// Width of the left column (the video and the thumbnail), px; the board's is about 370.
const LEFT_COL_W: f32 = 370.;

impl PublishPage {
    /// The page body (two columns: the video and thumbnail on the left, the details and visibility
    /// on the right) and the pinned footer.
    pub(super) fn sheet(
        &self,
        t: &Theme,
        slider_focused: bool,
        cx: &mut Context<Self>,
    ) -> (AnyElement, AnyElement) {
        let Load::Ready(loaded) = &self.load else {
            return (div().into_any_element(), div().into_any_element());
        };
        let id = self.id.clone().unwrap_or_default();
        let (export_path, export_line) = match loaded.export() {
            Some((p, e)) => (Some(p), Some(e.clone())),
            None => (None, None),
        };
        let tracker_view = {
            let tr = lock(&TRACKER);
            (self.stage.view(&tr), self.stage.locked(&tr))
        };
        let (view, locked) = tracker_view;
        let check = self.form.check(
            &self.rules,
            self.readiness(Some(loaded), export_path.as_deref()),
        );
        let exporting = export_ui::status_of(&id);

        // ---- account: the header pill shows a normal single channel; this card stays for the
        // states the pill cannot show (no channel, an unreadable file, several channels) ----
        let account_card = {
            let needs_card = self.accounts_error.is_some()
                || self.account().is_none()
                || self.accounts.len() > 1;
            let body: Option<AnyElement> = match (self.account(), &self.accounts_error) {
                (_, Some(e)) => Some(
                    div()
                        .text_color(rgba_of(t.warning))
                        .text_size(px(type_scale::CAPTION))
                        .child(SharedString::from(tr!(
                            "The accounts file could not be read: {error}",
                            error = e
                        )))
                        .into_any_element(),
                ),
                (None, _) => Some(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(12.))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(220.))
                                .text_size(px(type_scale::UI))
                                .text_color(rgba_of(t.text_2))
                                .child(SharedString::from(tr!("No YouTube channel is connected. Connect one with your own Google Cloud client; it takes a few minutes, once."))),
                        )
                        .child({
                            let app = self.app.clone();
                            button_with_icon(
                                t,
                                "pub-connect",
                                tr!("Connect YouTube"),
                                ButtonKind::Secondary,
                                Some("plug"),
                            )
                            .on_click(move |_, _, cx| {
                                let app = app.clone();
                                cx.defer(move |cx| app::set_route(&app, Route::Destinations, cx));
                            })
                        })
                        .into_any_element(),
                ),
                (Some(_), _) if self.accounts.len() > 1 => Some(
                    segmented(t, "pub-accounts", &tr_ctx!("accounts", "Channel"))
                        .children(self.accounts.iter().enumerate().map(|(i, acc)| {
                            segment(t, ("pub-account", i), &acc.name, i == self.account)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.stage.locked(&lock(&TRACKER)) {
                                        this.account = i;
                                        if let Some(a) = this.accounts.get(i) {
                                            this.form.set_privacy(a.default_privacy);
                                        }
                                        this.load_quota(cx);
                                        cx.notify();
                                    }
                                }))
                        }))
                        .into_any_element(),
                ),
                _ => None,
            };
            needs_card.then(|| {
                card(t)
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(card_title(t, &tr!("YouTube channel")))
                    .children(body)
            })
        };

        // ---- export ----
        let export_card = {
            let running = exporting
                .as_ref()
                .filter(|s| s.phase == ExportPhase::Running)
                .map(|s| s.progress);
            let failed = exporting
                .as_ref()
                .filter(|s| s.phase == ExportPhase::Failed)
                .map(|s| {
                    s.reason
                        .clone()
                        .unwrap_or_else(|| tr!("The export failed."))
                });
            let export_button = |label: String| {
                let (app, id) = (self.app.clone(), id.clone());
                button_with_icon(
                    t,
                    "pub-export",
                    label,
                    ButtonKind::Secondary,
                    Some("rotate-cw"),
                )
                .on_click(move |_, _, cx| {
                    let (app, id) = (app.clone(), id.clone());
                    cx.defer(move |cx| export_ui::request(&app, &id, None, cx));
                })
            };
            let tile = div()
                .size(px(34.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(scale::RADIUS_MD))
                .bg(rgba_of(t.icon_tile()))
                .child(icon("file-video", 18., t.text_2));
            // Design gap: the board draws a video preview with a play button above this row;
            // the page has no player for the export.
            let top: AnyElement = match &export_line {
                Some(e) => div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(tile)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .text_size(px(type_scale::UI))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(SharedString::from(tr!("Video to upload"))),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .mono()
                                            .text_size(px(type_scale::CAPTION))
                                            .text_color(rgba_of(t.mist))
                                            .child(SharedString::from(
                                                e.file.rsplit('/').next().unwrap_or(&e.file).to_owned(),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .mono()
                                            .text_size(px(type_scale::CAPTION))
                                            .text_color(rgba_of(t.mist))
                                            .child(SharedString::from(
                                                [
                                                    Some(e.preset.clone()),
                                                    Some(cm_i18n::fmt::size(e.bytes)),
                                                    e.duration_ms.map(clock_text),
                                                ]
                                                .into_iter()
                                                .flatten()
                                                .collect::<Vec<_>>()
                                                .join(" \u{b7} "),
                                            )),
                                    ),
                            )
                            .children(running.is_none().then(|| export_button(tr!("Export again")))),
                    )
                    .child(caption(
                        t,
                        tr!("This is the latest export. To upload the current edits, export again first."),
                    ))
                    .into_any_element(),
                None => div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(tile)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_size(px(type_scale::UI))
                                    .text_color(rgba_of(t.text_2))
                                    .child(SharedString::from(tr!("This recording has no export yet. YouTube gets the exported MP4, so export it first."))),
                            ),
                    )
                    .children(running.is_none().then(|| {
                        div().flex().child(
                            button_with_icon(
                                t,
                                "pub-export",
                                tr!("Export MP4"),
                                ButtonKind::Secondary,
                                Some("file-video"),
                            )
                            .on_click({
                                let (app, id) = (self.app.clone(), id.clone());
                                move |_, _, cx| {
                                    let (app, id) = (app.clone(), id.clone());
                                    cx.defer(move |cx| export_ui::request(&app, &id, None, cx));
                                }
                            }),
                        )
                    }))
                    .into_any_element(),
            };
            card(t)
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(top)
                .children(running.map(|p| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(progress_bar(t, p))
                        .child(caption(
                            t,
                            tr!("Exporting {pct}%", pct = (p * 100.0).floor() as u32),
                        ))
                }))
                .children(failed.map(|r| {
                    div()
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.warning))
                        .child(SharedString::from(r))
                }))
        };

        // ---- details ----
        let details_card = card(t)
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(card_title(t, &tr_ctx!("card heading", "Details")))
            .child(field_block(
                t,
                "Title", // i18n-ignore: element key
                &tr_ctx!("video field", "Title"),
                Some(format!(
                    "{} / {}",
                    self.form.title.chars().count(),
                    self.rules.title_max_chars
                )),
                self.title_input.clone().into_any_element(),
                Some(tr!(
                    "{count} of {max} characters",
                    count = self.form.title.chars().count(),
                    max = self.rules.title_max_chars
                )),
                check.error_of(Field::Title),
            ))
            .child(field_block(
                t,
                "Description", // i18n-ignore: element key
                &tr_ctx!("video field", "Description"),
                Some(tr!(
                    "{count} / {max} bytes",
                    count = self.form.description.len(),
                    max = self.rules.description_max_bytes
                )),
                self.description_input.clone().into_any_element(),
                Some(tr!("Plain text; a pasted description keeps its line breaks.")),
                check.error_of(Field::Description),
            ))
            .child(field_block(
                t,
                "Tags", // i18n-ignore: element key
                &tr_ctx!("video field", "Tags"),
                Some(tr!("comma separated")),
                self.tags_input.clone().into_any_element(),
                Some(tr!("Separated by commas.")),
                check.error_of(Field::Tags),
            ))
            .child(field_block(
                t,
                &tr!("Playlist (optional)"),
                &tr_ctx!("video field", "Playlist"),
                Some(tr_ctx!("note beside a field", "optional")),
                self.playlist_input.clone().into_any_element(),
                Some(tr!("A playlist ID (it starts with PL) or the address of one of your playlists. The video is added to it after the upload; if YouTube refuses, the video is still published.")),
                check.error_of(Field::Playlist),
            ));

        // ---- visibility, captions and kids ----
        let privacies = [Privacy::Private, Privacy::Unlisted, Privacy::Public]
            .into_iter()
            .map(|p| {
                segment_with_icon(
                    t,
                    ("pub-privacy", p as usize),
                    &privacy_label(p),
                    self.form.privacy == p,
                    Some(privacy_icon(p)),
                )
                .flex_1()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.stage.locked(&lock(&TRACKER)) {
                        this.form.set_privacy(p);
                        cx.notify();
                    }
                }))
            });
        let public_on = self.form.public_confirmed;
        let kids = [
            (true, tr!("Yes, it is made for kids")),
            (false, tr_ctx!("made for kids", "No, it is not")),
        ]
        .into_iter()
        .map(|(yes, label)| {
            segment(
                t,
                ("pub-kids", usize::from(yes)),
                &label,
                self.form.made_for_kids == Some(yes),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.stage.locked(&lock(&TRACKER)) {
                    this.form.made_for_kids = Some(yes);
                    cx.notify();
                }
            }))
        });
        let mut rows: Vec<AnyElement> = Vec::new();
        if loaded.details.has_captions {
            rows.push(
                settings_row(
                    t,
                    Some("captions"),
                    tr!("Upload captions (SRT) with the video"),
                    Some(SharedString::from(tr!(
                        "The captions as they are when you confirm. If YouTube refuses them, the video is still published."
                    ))),
                    Some(
                        switch(
                            t,
                            "pub-captions",
                            tr!("Upload captions (SRT) with the video"),
                            self.form.captions,
                            !locked,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            if !this.stage.locked(&lock(&TRACKER)) {
                                this.form.captions = !this.form.captions;
                                cx.notify();
                            }
                        }))
                        .into_any_element(),
                    ),
                    false,
                )
                .into_any_element(),
            );
        }
        if self.form.privacy == Privacy::Public {
            rows.push(
                settings_row(
                    t,
                    Some("globe"),
                    tr!("Confirm that everyone can see this video"),
                    Some(SharedString::from(tr!(
                        "I understand that everyone will be able to find and watch this video."
                    ))),
                    Some(
                        switch(
                            t,
                            "pub-public-confirm",
                            tr!("Confirm that everyone can see this video"),
                            public_on,
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.stage.locked(&lock(&TRACKER)) {
                                this.form.public_confirmed = !this.form.public_confirmed;
                                cx.notify();
                            }
                        }))
                        .into_any_element(),
                    ),
                    false,
                )
                .into_any_element(),
            );
        }
        // PUB-2.AC4: an explicit Yes/No answer with neither preselected (the board's switch would
        // answer for the user).
        rows.push(
            settings_row(
                t,
                Some("user-plus"),
                tr!("Is this video made for kids?"),
                Some(SharedString::from(tr!(
                    "Required by YouTube, with no default. It decides whether comments and some features are turned off."
                ))),
                Some(
                    segmented(t, "pub-kids-choices", &tr!("Made for kids"))
                        .children(kids)
                        .into_any_element(),
                ),
                true,
            )
            .into_any_element(),
        );
        let visibility_card = card(t)
            .p(px(0.))
            .flex()
            .flex_col()
            .child(
                div()
                    .p(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(card_title(t, &tr_ctx!("card heading", "Visibility")))
                    .child(
                        segmented(
                            t,
                            "pub-privacy-choices",
                            &tr_ctx!("card heading", "Visibility"),
                        )
                        .w_full()
                        .children(privacies),
                    )
                    .child(caption(t, privacy_blurb(self.form.privacy)))
                    .children(privacy_note(&self.rules, self.form.privacy).map(|n| caption(t, n))),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(rgba_of(t.row_divider()))
                    .flex()
                    .flex_col()
                    .children(rows),
            );

        // ---- thumbnail ----
        let can_pick = export_path.is_some() && loaded.duration_secs().is_some() && !locked;
        let duration_ms = loaded.duration_secs().map(|d| (d * 1000.0) as i64);
        let at_ms = loaded
            .duration_secs()
            .map(|d| (thumb::position_secs(d, self.thumb.frac) * 1000.0) as i64);
        let time_text = match (at_ms, duration_ms) {
            (Some(a), Some(d)) => tr!("{at} of {total}", at = clock_text(a), total = clock_text(d)),
            _ => tr!("No export yet"),
        };
        let preview = div()
            .w(px(168.))
            .h(px(94.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .rounded(px(scale::RADIUS_MD))
            .bg(rgba_of(t.ink))
            .border_1()
            .border_color(rgba_of(t.border))
            .child(match &self.thumb.image {
                Some(i) => img(i.clone())
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
                None => div()
                    .p(px(8.))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(if self.thumb.busy {
                        tr!("Getting the picture\u{2026}")
                    } else if self.form.thumbnail.is_some() {
                        tr!("No preview")
                    } else {
                        tr!("YouTube picks a frame unless you choose one.")
                    }))
                    .into_any_element(),
            });
        let thumbnail_card = card(t)
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(card_title(t, &tr_ctx!("card heading", "Thumbnail")))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .child(preview)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .text_size(px(type_scale::CAPTION))
                                            .text_color(rgba_of(t.text_2))
                                            .child(SharedString::from(tr!("Frame from the video"))),
                                    )
                                    .child(
                                        div()
                                            .mono()
                                            .text_size(px(type_scale::CAPTION))
                                            .text_color(rgba_of(t.mist))
                                            .child(SharedString::from(time_text.clone())),
                                    ),
                            )
                            .child({
                                let s = slider::slider(
                                    t,
                                    "pub-frame-slider",
                                    SliderLook {
                                        label: &tr!("Thumbnail frame"),
                                        value_text: SharedString::from(time_text.clone()),
                                        value: self.thumb.frac,
                                        enabled: can_pick,
                                        focused: slider_focused,
                                    },
                                    &self.slider_bounds,
                                )
                                .track_focus(&self.thumb_focus)
                                .tab_stop(can_pick);
                                if can_pick {
                                    s.cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, ev, window, cx| {
                                                this.slider_down(ev, window, cx)
                                            }),
                                        )
                                        .on_mouse_move(cx.listener(|this, ev, _, cx| {
                                            this.slider_move(ev, cx)
                                        }))
                                        .on_mouse_up(
                                            MouseButton::Left,
                                            cx.listener(|this, ev, _, cx| this.slider_up(ev, cx)),
                                        )
                                        .on_mouse_up_out(
                                            MouseButton::Left,
                                            cx.listener(|this, ev, _, cx| this.slider_up(ev, cx)),
                                        )
                                        .on_key_down(cx.listener(|this, ev, _, cx| {
                                            this.slider_key(ev, cx)
                                        }))
                                } else {
                                    s
                                }
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(10.))
                    .child({
                        let b = button_with_icon(
                            t,
                            "pub-image",
                            tr!("Choose image\u{2026}"),
                            ButtonKind::Secondary,
                            Some("image-plus"),
                        );
                        if locked {
                            b.opacity(0.5)
                        } else {
                            b.on_click(cx.listener(|this, _, _, cx| this.choose_image(cx)))
                        }
                    })
                    .children((self.form.thumbnail.is_some() && !locked).then(|| {
                        button_with_icon(
                            t,
                            "pub-thumb-clear",
                            tr_ctx!("button", "Remove"),
                            ButtonKind::Ghost,
                            Some("x"),
                        )
                            .on_click(cx.listener(|this, _, _, cx| this.clear_thumb(cx)))
                    })),
            )
            .child(caption(
                t,
                if self.thumb.from_frame {
                    tr!("Drag the slider or use the arrow keys (Shift for bigger steps). JPG or PNG up to 2 MB; 1280 \u{d7} 720 is best.")
                } else {
                    tr!("JPG or PNG up to 2 MB; 1280 \u{d7} 720 is best. A custom thumbnail needs a verified YouTube account; if YouTube refuses it, the video is still published.")
                },
            ))
            .children(self.thumb.note.as_ref().map(|n| {
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.warning))
                    .child(SharedString::from(n.text()))
            }))
            .children(check.error_of(Field::Thumbnail).map(|e| {
                div()
                    .id("pub-video-error")
                    .role(Role::Alert)
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.warning))
                    .child(SharedString::from(e.to_owned()))
            }));

        let footer = self.footer(t, &view, &check, cx);

        let left = div()
            .flex_none()
            .w(px(LEFT_COL_W))
            .min_w(px(300.))
            .flex_grow(1.)
            .max_w(px(LEFT_COL_W + 80.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(export_card)
            .child(thumbnail_card);
        let right = div()
            .flex_1()
            .min_w(px(400.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(details_card)
            .child(visibility_card);
        let body = div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .children(account_card)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap(px(16.))
                    .child(left)
                    .child(right),
            )
            .children(check.error_of(Field::Video).map(|e| {
                div()
                    .id("pub-thumb-error")
                    .role(Role::Alert)
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.warning))
                    .child(SharedString::from(e.to_owned()))
            }))
            .into_any_element();
        (body, footer)
    }
}
