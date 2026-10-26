use super::*;

impl SettingsView {
    pub(in super::super) fn destinations(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        if self.dest.labels_epoch.get() != cm_i18n::epoch() {
            self.relabel_dest_fields(cx);
        }
        let (root, status_now) = {
            let m = self.app.read(cx);
            (
                m.backend.library_root.clone(),
                m.backend.initial_status.clone(),
            )
        };
        let path = root
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| tr!("Not set"));
        let unavailable = matches!(status_now, LibraryStatus::Unavailable { .. });

        // Where captures are always saved. It is not a copy destination: copies go to the
        // folders listed below it.
        let library = settings_row(
            t,
            Some("hard-drive"),
            tr!("CaptureMann library"),
            Some(SharedString::from(tr!(
                "{path} \u{b7} always on",
                path = path
            ))),
            Some(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .when(unavailable, |d| {
                        d.child(status(
                            t,
                            StatusKind::Warning,
                            tr_ctx!("status", "Unavailable"),
                        ))
                    })
                    .child(pill(t, Tone::Neutral, Some("lock"), tr!("Built in")))
                    .into_any_element(),
            ),
            false,
        )
        .id("dest-local")
        .role(Role::ListItem)
        .aria_label(SharedString::from(tr!(
            "Library folder, {path}. Captures are always saved here.",
            path = path
        )))
        .into_any_element();

        let mut save_rows: Vec<AnyElement> = vec![library];
        save_rows.extend((0..self.dest.items.len()).map(|i| self.destination_row(t, i, cx)));

        let add = footer_row(
            t,
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(12.))
                .child(
                    small_button_with_icon(
                        t,
                        "dest-add",
                        &tr!("Add local folder\u{2026}"),
                        Some("folder-plus"),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.add_destination_folder(cx))),
                )
                .child(caption(
                    t,
                    tr!("A folder on this Mac, an external drive or a mounted network folder."),
                ))
                .into_any_element(),
        )
        .id("dest-add-row")
        .role(Role::ListItem)
        .aria_label(tr!("Add a local folder, drive or network folder"));

        let last = DRIVES.len() - 1;
        let mut drives: Vec<AnyElement> = Vec::new();
        for (i, (name, _glyph)) in DRIVES.iter().enumerate() {
            let Some(provider) = cloud::provider_named(name).filter(|_| cloud::is_offered(name))
            else {
                // OneDrive and Dropbox are built but not offered: "Coming soon".
                drives.push(
                    settings_row(
                        t,
                        Some(drive_icon(name)),
                        *name,
                        Some(SharedString::from(tr!("Not available yet."))),
                        Some(
                            pill(
                                t,
                                Tone::Neutral,
                                Some("clock"),
                                super::super::model::later_milestone(),
                            )
                            .into_any_element(),
                        ),
                        i == last,
                    )
                    .id(("dest-drive", i))
                    .role(Role::ListItem)
                    .aria_label(SharedString::from(tr!(
                        "{name}, not connected. Coming soon.",
                        name = name
                    )))
                    .into_any_element(),
                );
                continue;
            };
            let connected = self
                .dest
                .items
                .iter()
                .filter(|d| d.kind == provider.kind)
                .count();
            let status_text = match connected {
                0 => tr_ctx!("drive status", "Not connected"),
                n => trn!("{n} account connected", "{n} accounts connected", n),
            };
            let setup_here = matches!(
                self.dest.setup.as_ref(),
                Some(s) if s.provider.key == provider.key && s.target == Target::New
            );
            let verb = if connected == 0 {
                tr_ctx!("button", "Connect")
            } else {
                tr!("Connect another")
            };
            let row =
                settings_row(
                    t,
                    Some(drive_icon(name)),
                    *name,
                    Some(SharedString::from(drive_hint(connected))),
                    Some(
                        button_with_icon(
                            t,
                            ("dest-drive-connect", i),
                            verb.clone(),
                            if connected == 0 {
                                ButtonKind::Primary
                            } else {
                                ButtonKind::Secondary
                            },
                            Some("plug"),
                        )
                        .aria_label(SharedString::from(if connected == 0 {
                            tr!("Connect {name}", name = name)
                        } else {
                            tr!("Connect another {name}", name = name)
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_setup(provider, Target::New, cx)
                        }))
                        .into_any_element(),
                    ),
                    i == last && !setup_here,
                )
                .id(("dest-drive", i))
                .role(Role::ListItem)
                .aria_label(SharedString::from(tr!(
                    "{name}, {status}.",
                    name = name,
                    status = status_text
                )));
            drives.push(if setup_here {
                div()
                    .flex()
                    .flex_col()
                    .child(row)
                    .child(self.setup_panel(t, cx))
                    .into_any_element()
            } else {
                row.into_any_element()
            });
        }

        let copy_link = self.copy_link_row(t, cx);
        let cards = PLATFORMS.iter().enumerate().map(|(i, (name, detail))| {
            if i == 0 {
                // YouTube is real (PUB-1): `publish_card.rs`.
                return self.youtube_card(t, cx);
            }
            let detail =
                super::super::model::platform_detail(name).unwrap_or_else(|| detail.to_string());
            super::super::publish_card::coming_soon_card(t, i, name, &detail)
        });

        div()
            .flex()
            .flex_col()
            .gap(px(28.))
            .children(self.dest.load_error.clone().map(|e| {
                alert_line(
                    t,
                    "dest-load-error",
                    tr!(
                        "The destinations file could not be read, so none are listed: {error}",
                        error = e
                    ),
                )
            }))
            .child(
                settings_group(
                    t,
                    &tr!("Save to"),
                    Some(tr!("Exports go to the default first").into()),
                    save_rows,
                    Some(add.into_any_element()),
                )
                .id("save-to")
                .role(Role::List)
                .aria_label(tr!("Save to")),
            )
            .children(
                (self.dest.editing.is_none() && self.dest.error.is_some()).then(|| {
                    alert_line(
                        t,
                        "dest-add-error",
                        self.dest
                            .error
                            .as_ref()
                            .map(Problem::text)
                            .unwrap_or_default(),
                    )
                }),
            )
            .child(settings_group(
                t,
                &tr!("Cloud drives"),
                None,
                drives,
                None,
            ))
            .child(settings_group(
                t,
                &tr_ctx!("settings group", "Sharing"),
                None,
                vec![copy_link],
                None,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(12.))
                            .child(
                                div()
                                    .text_size(px(type_scale::UI))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgba_of(t.text_2))
                                    .child(SharedString::from(tr!("Publish to"))),
                            )
                            .child(caption(t, tr!("Accounts used by Publish\u{2026}"))),
                    )
                    .child(div().flex().flex_wrap().gap(px(14.)).children(cards))
                    .children(self.youtube_panel(t, cx)),
            )
            .child(caption(
                t,
                tr!("Copies go to the folders above when you upload a capture. Drives and accounts are connected with your own credentials."),
            ))
            .into_any_element()
    }

    /// STO-4.AC1: "Copy the link when an upload finishes" (`storage.copy_link_when_done`, on by
    /// default). Agents may change it too.
    fn copy_link_row(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let on = crate::recording::uploads::copy_link_enabled(
            &self.app.read(cx).backend.settings.snapshot(),
        );
        let app = self.app.clone();
        settings_row(
            t,
            Some("link"),
            tr!("Copy the link after an upload"),
            Some(SharedString::from(tr!(
                "Cloud uploads put their share link on the clipboard. Turn it off and use Copy link in the library instead."
            ))),
            Some(
                crate::ui::components::switch(
                    t,
                    "copy-link-when-done",
                    tr!("Copy the link when an upload finishes"),
                    on,
                    true,
                )
                .on_click(move |_, _, cx| {
                    crate::ui::app::set_setting(
                        &app,
                        crate::settings::KEY_COPY_LINK_WHEN_DONE,
                        serde_json::json!(!on),
                        cx,
                    )
                })
                .into_any_element(),
            ),
            true,
        )
        .into_any_element()
    }
}
