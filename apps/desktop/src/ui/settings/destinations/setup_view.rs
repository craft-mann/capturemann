use super::*;

/// What the user reads when a connect, test or reconnect step fails.
pub(in super::super) fn connect_error_text(
    provider: Option<&ProviderInfo>,
    code: ErrorCode,
    message: &str,
) -> String {
    match code {
        ErrorCode::AuthExpired => tr!("{error} Use Reconnect to sign in again.", error = message),
        ErrorCode::PlatformRejected => match provider {
            Some(p) => tr!(
                "{provider} said no: {error} {hint}",
                provider = p.name,
                error = message,
                hint = p.refusal_hint()
            ),
            None => tr!("The provider said no: {error}", error = message),
        },
        ErrorCode::Network => {
            tr!(
                "Could not reach the server: {error} Check your connection and try again.",
                error = message
            )
        }
        _ => message.to_owned(),
    }
}

/// The line next to "Test connection": nothing before a run, then the outcome.
pub(super) fn testing_note(t: &Theme, result: Option<Result<(), Problem>>) -> Option<AnyElement> {
    let result = result?
        .map(|()| tr!("Works: a small test file was uploaded and deleted."))
        .map_err(|p| p.text());
    Some(outcome_line(t, "dest-test-result", result).into_any_element())
}

impl SettingsView {
    /// Dropbox: which access type the user chose when creating the app.
    fn access_choice(&self, t: &Theme, app_folder: bool, cx: &mut Context<Self>) -> AnyElement {
        let choices = [
            (false, tr_ctx!("dropbox access type", "Full Dropbox")),
            (true, tr_ctx!("dropbox access type", "App folder")),
        ]
        .into_iter()
        .map(|(value, label)| {
            segment(
                t,
                ("dest-setup-access", value as usize),
                &label,
                app_folder == value,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(s) = this.dest.setup.as_mut() {
                    s.app_folder = value;
                }
                cx.notify();
            }))
        });
        field_block(
            t,
            &tr!("Access type of your app"),
            Some(tr!("Choose the one you picked in the App Console. App folder: files go into Apps/<your app> in your Dropbox. Full Dropbox: files go into a CaptureMann folder at the top.")),
            div().flex().child(
                segmented(t, "dest-setup-access-choices", &tr!("Access type of your app"))
                    .children(choices),
            ),
        )
        .into_any_element()
    }

    /// The guided setup under a provider row or a destination (kit `Settings` cards, flat
    /// panel like Manage): steps, the two fields, then progress and the result.
    pub(super) fn setup_panel(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(s) = self.dest.setup.as_ref() else {
            return div().into_any_element();
        };
        let name = s.provider.name;
        let reconnect = matches!(s.target, Target::Reconnect { .. });
        let heading = div()
            .text_size(px(type_scale::BODY))
            .font_weight(FontWeight::SEMIBOLD)
            .child(SharedString::from(if reconnect {
                tr!("Reconnect {name}", name = name)
            } else {
                tr!("Connect {name}", name = name)
            }));
        let error = |text: Option<String>, id: &'static str| text.map(|e| alert_line(t, id, e));
        let access = s
            .provider
            .access_choice
            .then(|| self.access_choice(t, s.app_folder, cx));
        let cancel = |label: String| {
            button(t, "dest-setup-cancel", label, ButtonKind::Secondary)
                .on_click(cx.listener(|this, _, _, cx| this.cancel_setup(cx)))
        };
        let body: AnyElement = match &s.step {
            SetupStep::Form => {
                let reuse = s.use_saved.then(|| s.saved.clone()).flatten();
                if let Some(saved) = reuse {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .child(caption(
                            t,
                            tr!("Your sign-in expired or was removed. Sign in again with the app credentials already saved in your Keychain:"),
                        ))
                        .child(caption(t, saved.client_id).mono())
                        .children(error(s.error.as_ref().map(Problem::text), "dest-setup-error"))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(10.))
                                .child(
                                    button_with_icon(
                                        t,
                                        "dest-setup-go",
                                        tr!("Sign in again"),
                                        ButtonKind::Primary,
                                        Some("external-link"),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| this.submit_setup(cx))),
                                )
                                .child(
                                    button(
                                        t,
                                        "dest-setup-other",
                                        tr!("Use a different client ID\u{2026}"),
                                        ButtonKind::Ghost,
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(s) = this.dest.setup.as_mut() {
                                            s.use_saved = false;
                                        }
                                        cx.notify();
                                    })),
                                )
                                .child(cancel(tr_ctx!("button", "Cancel"))),
                        )
                        .into_any_element()
                } else {
                    let step_texts = s.provider.steps();
                    let steps = step_list(t, step_texts.iter().map(String::as_str));
                    let field = |label: &str, input: &Entity<TextField>, err: Option<String>, id: &'static str| {
                        field_block(t, label, None, input.clone()).children(error(err, id))
                    };
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(14.))
                        .child(caption(
                            t,
                            if s.provider.needs_secret() {
                                tr!(
                                    "{name} needs an app that you create yourself, so the sign-in belongs to you. It takes a few minutes, once. The client ID and secret go to your Keychain, never to a file, and are only sent to {service}.",
                                    name = name,
                                    service = (s.provider.config)().display_name
                                )
                            } else {
                                tr!(
                                    "{name} needs an app that you create yourself, so the sign-in belongs to you. It takes a few minutes, once. The ID you paste (no secret is involved) go to your Keychain, never to a file, and are only sent to {service}.",
                                    name = name,
                                    service = (s.provider.config)().display_name
                                )
                            },
                        ))
                        .child(steps)
                        .child(caption(t, s.provider.note()))
                        .child(field(
                            &s.provider.id_label(),
                            &self.dest.client_id_input,
                            s.id_error.as_ref().map(ToString::to_string),
                            "dest-setup-id-error",
                        ))
                        .when(s.provider.needs_secret(), |p| {
                            p.child(field(
                                &s.provider.secret_label(),
                                &self.dest.client_secret_input,
                                s.secret_error.as_ref().map(ToString::to_string),
                                "dest-setup-secret-error",
                            ))
                        })
                        .children(access)
                        .children(error(s.error.as_ref().map(Problem::text), "dest-setup-error"))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(10.))
                                .child(
                                    button_with_icon(
                                        t,
                                        "dest-setup-go",
                                        tr!("Continue in browser"),
                                        ButtonKind::Primary,
                                        Some("external-link"),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| this.submit_setup(cx))),
                                )
                                .child(cancel(tr_ctx!("button", "Cancel"))),
                        )
                        .into_any_element()
                }
            }
            SetupStep::Working(phase) => div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .id("dest-setup-phase")
                        .role(Role::Status)
                        .text_size(px(type_scale::UI))
                        .child(SharedString::from(phase.label())),
                )
                .child(caption(
                    t,
                    tr!("A browser window opened. Sign in there and allow access; this page continues by itself. Closing the window without allowing it ends the attempt after a few minutes, or press Cancel."),
                ))
                .child(div().flex().child(cancel(tr_ctx!("button", "Cancel"))))
                .into_any_element(),
            SetupStep::Done(done) => {
                let id = done.destination_id.clone();
                let tested = self.dest.test_results.get(&id).cloned();
                let testing = self.dest.testing.as_deref() == Some(id.as_str());
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(div().flex().child(status(
                        t,
                        StatusKind::Ok,
                        tr!("Connected as {email}", email = done.email),
                    )))
                    .child(caption(
                        t,
                        tr!(
                            "Recordings go into a folder named \u{201c}{folder}\u{201d}. Manage the destination below to choose what is copied and when.",
                            folder = done.folder_name
                        ),
                    ))
                    .children(done.note.map(|n| caption(t, n.text())))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                button_with_icon(
                                    t,
                                    "dest-setup-test",
                                    if testing {
                                        tr!("Testing\u{2026}")
                                    } else {
                                        tr!("Test connection")
                                    },
                                    ButtonKind::Secondary,
                                    Some("plug"),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.test_destination(&id, cx)
                                })),
                            )
                            .child(
                                button(
                                    t,
                                    "dest-setup-done",
                                    tr_ctx!("button", "Done"),
                                    ButtonKind::Primary,
                                )
                                    .on_click(cx.listener(|this, _, _, cx| this.cancel_setup(cx))),
                            )
                            .children(testing_note(t, tested)),
                    )
                    .into_any_element()
            }
        };
        panel_shell(t)
            .id("dest-setup")
            .role(Role::Group)
            .aria_label(SharedString::from(tr!("Connect {name}", name = name)))
            .child(heading)
            .child(body)
            .into_any_element()
    }
}
