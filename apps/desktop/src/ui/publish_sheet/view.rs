use super::*;

impl Render for PublishPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        if self.labels_epoch != cm_i18n::epoch() {
            self.relabel(cx);
        }
        self.sync_form(cx);
        let running = matches!(self.stage.view(&lock(&TRACKER)), View::Running { .. });
        self.cancel.sync(running);
        let slider_focused = self.thumb_focus.is_focused(window);
        let header = self.header(&t, cx);
        let (body, footer): (AnyElement, Option<AnyElement>) = match &self.load {
            Load::Idle | Load::Loading => (
                div()
                    .text_color(rgba_of(t.text_3))
                    .child(SharedString::from(tr!("Loading\u{2026}")))
                    .into_any_element(),
                None,
            ),
            Load::Failed(e) => (
                div()
                    .text_color(rgba_of(t.text_2))
                    .child(SharedString::from(e.to_string()))
                    .into_any_element(),
                None,
            ),
            Load::Ready(_) => {
                let (b, f) = self.sheet(&t, slider_focused, cx);
                (b, Some(f))
            }
        };
        div()
            .id("publish-page")
            .role(Role::Group)
            .aria_label(tr!("Publish to YouTube"))
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("publish-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .child(div().w_full().p(px(24.)).child(body)),
            )
            // The footer is pinned under the scrolling page: it holds Publish, the blocker, the
            // progress and the result.
            .children(footer)
    }
}

impl PublishPage {
    /// The top bar (board R10): back, the send tile, the title with the recording under it, and
    /// on the right the channel pill and the quota line.
    pub(super) fn header(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let id = self.id.clone().unwrap_or_default();
        let back = {
            let (app, id) = (self.app.clone(), id);
            icon_button(
                t,
                "pub-back",
                "chevron-left",
                tr!("Back to the recording"),
                false,
                Some(Vec::new()),
            )
            .on_click(move |_, _, cx| {
                let (app, id) = (app.clone(), id.clone());
                cx.defer(move |cx| app::set_route(&app, Route::Recording(id), cx));
            })
        };
        let sub = match &self.load {
            Load::Ready(l) => {
                let mut s = l.details.title.clone();
                if let Some(d) = l.duration_secs() {
                    s.push_str(&format!(" \u{b7} {}", clock_text((d * 1000.0) as i64)));
                }
                s
            }
            _ => String::new(),
        };
        let channel = self.account().map(|a| {
            div()
                .id("pub-channel")
                .role(Role::Status)
                .aria_label(SharedString::from(tr!(
                    "YouTube channel {name}",
                    name = a.name
                )))
                .h(px(32.))
                .pl(px(6.))
                .pr(px(10.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(8.))
                .rounded(px(scale::RADIUS_PILL))
                .border_1()
                .border_color(rgba_of(t.line))
                .child(
                    div()
                        .size(px(20.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(scale::RADIUS_PILL))
                        .bg(rgba_of(t.raised))
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(SharedString::from(initial(&a.name))),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .child(SharedString::from(a.name.clone())),
                )
                .child(icon("circle-check", 14., t.success))
        });
        let quota = self.quota.map(|q| {
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(if q.exhausted() { t.warning } else { t.mist }))
                .mono()
                .child(icon(
                    "gauge",
                    14.,
                    if q.exhausted() { t.warning } else { t.mist },
                ))
                .child(shrink_text(q.label()))
        });
        let _ = cx;
        div()
            .id("publish-toolbar")
            .role(Role::Toolbar)
            .aria_label(tr!("Publish toolbar"))
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(12.))
            .px(px(24.))
            .py(px(14.))
            .bg(rgba_of(t.ground))
            .border_b_1()
            .border_color(rgba_of(t.border))
            .child(back)
            .child(
                div()
                    .size(px(40.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(scale::RADIUS_MD))
                    .bg(rgba_of(t.icon_tile()))
                    .child(icon(crate::ui::brand::YOUTUBE.icon, 20., t.text_2)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(200.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id("publish-toolbar-title")
                            .role(Role::Heading)
                            .aria_level(1)
                            .aria_label(tr!("Publish to YouTube"))
                            .text_size(px(type_scale::TITLE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(tr!("Publish to YouTube"))),
                    )
                    .children((!sub.is_empty()).then(|| {
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.mist))
                            .child(SharedString::from(sub))
                    })),
            )
            .children(channel)
            .children(quota)
            .into_any_element()
    }

    /// The pinned footer: Publish (the neutral primary) with the reason it is disabled, the
    /// note about the daemon's confirmation dialog, and the confirming, running, failed and done
    /// states.
    pub(super) fn footer(
        &self,
        t: &Theme,
        view: &View,
        check: &Check,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let note = tr!("When you press Publish, CaptureMann asks you to confirm in a separate dialog. Nothing is sent to YouTube before you do. Chapters are not added yet.");
        let bar = |children: Vec<AnyElement>| {
            div()
                .id("publish-footer")
                .role(Role::Group)
                .flex_none()
                .max_h(px(320.))
                .overflow_y_hidden()
                .px(px(24.))
                .py(px(14.))
                .bg(rgba_of(t.ground))
                .border_t_1()
                .border_color(rgba_of(t.border))
                .flex()
                .flex_col()
                .gap(px(12.))
                .children(children)
                .into_any_element()
        };
        match view {
            View::Editing | View::Failed(_) => {
                let failure: Option<AnyElement> = match view {
                    View::Failed(f) => Some(self.failure_block(t, f, cx)),
                    _ => None,
                };
                // The board's Cancel next to Publish only leaves the page, which the back button
                // already does, so none is drawn here; cancelling a running upload is on the
                // progress line (no board draws that state).
                let (publish, status): (AnyElement, AnyElement) = match &check.blocker {
                    None => (
                        button_with_icon(
                            t,
                            "pub-publish",
                            tr_ctx!("button", "Publish\u{2026}"),
                            ButtonKind::Primary,
                            Some("send"),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.publish(cx)))
                        .into_any_element(),
                        caption(t, note.clone())
                            .flex_1()
                            .min_w(px(240.))
                            .into_any_element(),
                    ),
                    Some(why) => (
                        disabled_button(
                            t,
                            "pub-publish",
                            &tr_ctx!("button", "Publish\u{2026}"),
                            why,
                            false,
                        )
                            .into_any_element(),
                        div()
                            .id("pub-blocker")
                            .role(Role::Status)
                            .flex_1()
                            .min_w(px(240.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(type_scale::UI))
                            .text_color(rgba_of(t.warning))
                            .child(icon("triangle-alert", 16., t.warning))
                            .child(label_fill(why.clone()))
                            .into_any_element(),
                    ),
                };
                let mut parts = Vec::new();
                parts.extend(failure);
                parts.push(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(16.))
                        .child(status)
                        .child(publish)
                        .into_any_element(),
                );
                // With a blocker the note about the confirmation dialog moves under the row.
                if check.blocker.is_some() {
                    parts.push(caption(t, note).into_any_element());
                }
                bar(parts)
            }
            View::Confirming => bar(vec![div()
                .id("pub-confirming")
                .role(Role::Status)
                .text_size(px(type_scale::UI))
                .child(SharedString::from(tr!("Waiting for your confirmation. A dialog asks you to confirm the channel, visibility, title and file before anything is sent.")))
                .into_any_element()]),
            View::Running { progress, message } => {
                let cancelling =
                    matches!(&self.stage, Stage::Job(j) if self.cancel.is_cancelling(j));
                let line = if cancelling {
                    tr!("Cancelling\u{2026}")
                } else {
                    progress_text(*progress, message.as_deref())
                };
                let stop: AnyElement = if cancelling {
                    disabled_button(
                        t,
                        "pub-cancel",
                        &tr_ctx!("button", "Cancelling\u{2026}"),
                        &tr!("The upload is stopping."),
                        false,
                    )
                    .into_any_element()
                } else {
                    button(
                        t,
                        "pub-cancel",
                        tr_ctx!("button", "Cancel"),
                        ButtonKind::Secondary,
                    )
                    .aria_label(SharedString::from(tr!("Cancel the upload to YouTube")))
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_publish(cx)))
                    .into_any_element()
                };
                bar(vec![
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(16.))
                        .child(
                            div()
                                .id("pub-progress")
                                .role(Role::Status)
                                .flex_1()
                                .min_w(px(240.))
                                .text_size(px(type_scale::UI))
                                .child(SharedString::from(line)),
                        )
                        .child(stop)
                        .into_any_element(),
                    progress_bar(t, *progress).into_any_element(),
                    caption(t, tr!("You can leave this page; the upload continues in the background and resumes if the connection drops."))
                        .into_any_element(),
                ])
            }
            View::Done(o) => bar(self.done_block(t, o, cx)),
        }
    }

    fn failure_block(&self, t: &Theme, f: &Failure, cx: &mut Context<Self>) -> AnyElement {
        let retry = f.can_retry_job().then(|| {
            let job = f.job_id.clone().unwrap_or_default();
            button_with_icon(
                t,
                "pub-retry",
                tr_ctx!("button", "Retry"),
                ButtonKind::Primary,
                Some("rotate-cw"),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.retry(job.clone(), cx)))
        });
        let again = (!f.can_retry_job() && !f.needs_reconnect()).then(|| {
            button(
                t,
                "pub-again",
                tr!("Edit and try again"),
                ButtonKind::Secondary,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.stage = Stage::Editing;
                cx.notify();
            }))
        });
        let reconnect = f.needs_reconnect().then(|| {
            let app = self.app.clone();
            button_with_icon(
                t,
                "pub-reconnect",
                tr!("Open Destinations"),
                ButtonKind::Secondary,
                Some("plug"),
            )
            .on_click(move |_, _, cx| {
                let app = app.clone();
                cx.defer(move |cx| app::set_route(&app, Route::Destinations, cx));
            })
        });
        div()
            .id("pub-failure")
            .role(Role::Alert)
            .child(kit::callout(
                t,
                kit::Tone::Brass,
                "triangle-alert",
                SharedString::from(f.text()),
                None,
                Some(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(10.))
                        .children(retry)
                        .children(again)
                        .children(reconnect)
                        .into_any_element(),
                ),
            ))
            .into_any_element()
    }

    fn done_block(&self, t: &Theme, o: &Outcome, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (copy_url, open_url) = (o.url.clone(), o.url.clone());
        let copied = self.copied;
        let actions = div()
            .flex()
            .flex_wrap()
            .gap(px(10.))
            .child(
                button_with_icon(
                    t,
                    "pub-open",
                    tr_ctx!("button", "Open"),
                    ButtonKind::Primary,
                    Some("external-link"),
                )
                .on_click(move |_, _, cx| {
                    let url = open_url.clone();
                    cx.defer(move |cx| cx.open_url(&url));
                }),
            )
            .child(
                button_with_icon(
                    t,
                    "pub-copy",
                    if copied {
                        tr_ctx!("button, after the link was copied", "Copied")
                    } else {
                        tr!("Copy link")
                    },
                    ButtonKind::Secondary,
                    Some(if copied { "check" } else { "link" }),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(copy_url.clone()));
                    this.copied = true;
                    cx.notify();
                })),
            )
            .into_any_element();
        let title = if o.privacy_locked {
            tr!("Uploaded to YouTube, but kept private")
        } else {
            tr!("Uploaded to YouTube")
        };
        let mut out = vec![div()
            .id("pub-done")
            .role(Role::Status)
            .child(kit::callout(
                t,
                if o.privacy_locked {
                    kit::Tone::Brass
                } else {
                    kit::Tone::Success
                },
                if o.privacy_locked {
                    "triangle-alert"
                } else {
                    "circle-check"
                },
                title,
                Some(SharedString::from(o.url.clone())),
                Some(actions),
            ))
            .into_any_element()];
        out.extend(o.warnings.iter().map(|w| {
            div()
                .id("pub-warning")
                .role(Role::Status)
                .child(kit::callout(
                    t,
                    kit::Tone::Brass,
                    "triangle-alert",
                    SharedString::from(w.text()),
                    None,
                    None,
                ))
                .into_any_element()
        }));
        out
    }
}
