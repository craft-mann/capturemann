use super::*;

impl SettingsView {
    pub(super) fn destination_row(
        &self,
        t: &Theme,
        i: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let d = &self.dest.items[i];
        let id = d.id.clone();
        let open = self.dest.editing.as_deref() == Some(d.id.as_str());
        let is_cloud = d.kind.is_cloud();
        let needs_reconnect = matches!(
            self.dest.accounts.get(&d.id),
            Some(AccountState::Reconnect(_))
        );
        let (state_kind, state_label) = row_status(
            is_cloud,
            self.dest.accounts.get(&d.id),
            self.dest.online.get(&d.id).copied(),
        );
        let path = match &d.account {
            Some(a) if is_cloud => format!("{} \u{b7} {}", a.label, a.folder_name),
            _ => d.path.display().to_string(),
        };
        let setup_here = matches!(
            self.dest.setup.as_ref().map(|s| &s.target),
            Some(Target::Reconnect { destination_id }) if destination_id == &d.id
        );
        let rid = id.clone();
        let controls = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(status(t, state_kind, state_label.clone()))
            .when(d.default, |r| {
                r.child(pill(
                    t,
                    Tone::Brass,
                    Some("star"),
                    tr_ctx!("destination", "Default"),
                ))
            })
            .when(needs_reconnect && !setup_here, |r| {
                r.child(
                    small_button_with_icon(
                        t,
                        ("dest-reconnect", i),
                        &tr_ctx!("button", "Reconnect"),
                        Some("plug"),
                    )
                    .aria_label(SharedString::from(tr!("Reconnect {name}", name = d.name)))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.reconnect_destination(&rid, cx)),
                    ),
                )
            })
            .child(
                small_button_with_icon(
                    t,
                    ("dest-manage", i),
                    &if open {
                        tr_ctx!("button", "Close")
                    } else {
                        tr_ctx!("button", "Manage")
                    },
                    Some(if open { "x" } else { "sliders-horizontal" }),
                )
                .aria_label(SharedString::from(if open {
                    tr!("Close {name}", name = d.name)
                } else {
                    tr!("Manage {name}", name = d.name)
                }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.dest.editing.as_deref() == Some(id.as_str()) {
                        this.close_destination(cx);
                    } else {
                        this.open_destination(&id, cx);
                    }
                })),
            );
        let row = settings_row(
            t,
            Some(kind_icon(d.kind)),
            d.name.clone(),
            Some(SharedString::from(path.clone())),
            Some(controls.into_any_element()),
            false,
        )
        .id(("dest-stored", i))
        .role(Role::ListItem)
        .aria_label(SharedString::from(if d.default {
            tr!(
                "{name}, {path}, {status}, default destination",
                name = d.name,
                path = path,
                status = state_label
            )
        } else {
            tr!(
                "{name}, {path}, {status}",
                name = d.name,
                path = path,
                status = state_label
            )
        }));
        if setup_here {
            return div()
                .flex()
                .flex_col()
                .child(row)
                .child(self.setup_panel(t, cx))
                .into_any_element();
        }
        if !open {
            return row.into_any_element();
        }
        div()
            .flex()
            .flex_col()
            .child(row)
            .child(self.destination_panel(t, i, cx))
            .into_any_element()
    }

    fn destination_panel(&self, t: &Theme, i: usize, cx: &mut Context<Self>) -> AnyElement {
        let d = &self.dest.items[i];
        let id = d.id.clone();
        let field = |label: &str, input: &Entity<TextField>, hint: Option<String>| {
            field_block(t, label, hint, input.clone())
        };
        let whats = [What::Export, What::Original, What::Both]
            .into_iter()
            .map(|w| {
                segment(
                    t,
                    ("dest-what", w as usize),
                    &rules::what_label(w),
                    self.dest.edit_what == w,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.dest.edit_what = w;
                    cx.notify();
                }))
            });
        let autos = rules::AUTO_CHOICES.into_iter().map(|a| {
            segment(
                t,
                ("dest-auto", a as usize),
                &rules::auto_label(a),
                self.dest.edit_auto == a,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.dest.edit_auto = a;
                cx.notify();
            }))
        });
        let is_cloud = d.kind.is_cloud();
        let links = rules::link_choices(d.kind).iter().copied().map(|m| {
            segment(
                t,
                ("dest-link", m as usize),
                &rules::link_label(m),
                self.dest.edit_link == m,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.dest.edit_link = m;
                cx.notify();
            }))
        });
        let link = field_block(
            t,
            &tr!("Link after upload"),
            Some(rules::link_hint(self.dest.edit_link)),
            div().flex().child(
                segmented(t, "dest-link-choices", &tr!("Link after upload")).children(links),
            ),
        );
        let tested = self.dest.test_results.get(&d.id).cloned();
        let testing = self.dest.testing.as_deref() == Some(d.id.as_str());
        let test_id = id.clone();
        let test = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.))
            .child(
                button_with_icon(
                    t,
                    "dest-test",
                    if testing {
                        tr!("Testing\u{2026}")
                    } else {
                        tr!("Test connection")
                    },
                    ButtonKind::Secondary,
                    Some("plug"),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.test_destination(&test_id, cx))),
            )
            .children(testing_note(t, tested));
        let auto = field_block(
            t,
            &tr!("Upload automatically"),
            Some(rules::auto_hint(self.dest.edit_auto, self.dest.edit_what)),
            div().flex().child(
                segmented(t, "dest-auto-choices", &tr!("Upload automatically")).children(autos),
            ),
        );
        let buttons = if self.dest.removing {
            let rid = id.clone();
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(220.))
                        .text_size(px(type_scale::UI))
                        .child(SharedString::from(
                            if is_cloud && d.kind == cm_storage::Kind::GoogleDrive {
                                tr!(
                                    "Disconnect \u{201c}{name}\u{201d}? Uploads to it stop and its sign-in is deleted from this Mac. CaptureMann also asks Google to revoke the access you gave it; if Google cannot be reached, you can remove it at myaccount.google.com/permissions. Files already in your drive stay there.",
                                    name = d.name
                                )
                            } else if is_cloud {
                                tr!(
                                    "Disconnect \u{201c}{name}\u{201d}? Uploads to it stop and its sign-in is deleted from this Mac. Files already in your drive stay there, and the access you gave is not revoked at the provider (you can do that in your account settings).",
                                    name = d.name
                                )
                            } else {
                                tr!(
                                    "Remove \u{201c}{name}\u{201d}? It leaves this list. Files already copied there stay where they are.",
                                    name = d.name
                                )
                            },
                        )),
                )
                .child(
                    button_with_icon(
                        t,
                        "dest-remove-yes",
                        if is_cloud {
                            tr_ctx!("button", "Disconnect")
                        } else {
                            tr!("Remove destination")
                        },
                        ButtonKind::Danger,
                        Some(if is_cloud { "unplug" } else { "trash-2" }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if is_cloud {
                            this.disconnect_destination(&rid, cx)
                        } else {
                            this.remove_destination(&rid, cx)
                        }
                    })),
                )
                .child(
                    button(
                        t,
                        "dest-remove-no",
                        tr!("Keep it"),
                        ButtonKind::Secondary,
                    )
                    .on_click(
                        cx.listener(|this, _, _, cx| {
                            this.dest.removing = false;
                            cx.notify();
                        }),
                    ),
                )
        } else {
            let did = id.clone();
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(10.))
                .child(
                    button(t, "dest-save", tr!("Save changes"), ButtonKind::Primary)
                        .on_click(cx.listener(|this, _, _, cx| this.save_destination(cx))),
                )
                .child(
                    button(
                        t,
                        "dest-cancel",
                        tr_ctx!("button", "Cancel"),
                        ButtonKind::Secondary,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.close_destination(cx))),
                )
                .when(!d.default, |r| {
                    r.child(
                        button_with_icon(
                            t,
                            "dest-default",
                            tr!("Make default"),
                            ButtonKind::Secondary,
                            Some("star"),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.make_default_destination(&did, cx)
                        })),
                    )
                })
                .child(div().flex_1())
                .child(
                    button_with_icon(
                        t,
                        "dest-remove",
                        if is_cloud {
                            tr_ctx!("button", "Disconnect\u{2026}")
                        } else {
                            tr_ctx!("button", "Remove\u{2026}")
                        },
                        ButtonKind::Ghost,
                        Some(if is_cloud { "unplug" } else { "trash-2" }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dest.removing = true;
                        cx.notify();
                    })),
                )
        };
        panel_shell(t)
            .id(("dest-panel", i))
            .role(Role::Group)
            .aria_label(SharedString::from(tr!("Settings for {name}", name = d.name)))
            .child(field(
                &tr_ctx!("destination field", "Name"),
                &self.dest.name_input,
                None,
            ))
            .child(field(
                &tr!("Folder pattern"),
                &self.dest.pattern_input,
                Some(tr!(
                    "{yyyy} year, {MM} month, {dd} day, {title} capture title"
                )),
            ))
            .child(field_block(
                t,
                &tr!("What to copy"),
                Some(rules::what_hint(self.dest.edit_what)),
                div().flex().child(
                    segmented(t, "dest-what-choices", &tr!("What to copy")).children(whats),
                ),
            ))
            .when(is_cloud, |p| p.child(link))
            .children(
                (d.kind == Kind::Dropbox)
                    .then_some(d.account.as_ref())
                    .flatten()
                    .map(|a| {
                        caption(
                            t,
                            if a.app_folder {
                                tr!("Access: App folder. Files go into Apps/<your app> in your Dropbox. To change this, create another app and connect it.")
                            } else {
                                tr!("Access: full Dropbox. Files go into a CaptureMann folder at the top of your Dropbox.")
                            },
                        )
                    }),
            )
            .child(auto)
            .when(is_cloud, |p| p.child(test))
            .children(
                self.dest
                    .error
                    .as_ref()
                    .map(|e| alert_line(t, "dest-error", e.text())),
            )
            .child(buttons)
            .into_any_element()
    }

    /// Puts the labels of the page's text fields into the current language (they are made once).
    pub(super) fn relabel_dest_fields(&self, cx: &mut Context<Self>) {
        self.dest.labels_epoch.set(cm_i18n::epoch());
        self.dest.name_input.update(cx, |f, cx| {
            f.set_labels(
                tr!("Destination name"),
                tr_ctx!("destination name field hint", "Name"),
                cx,
            )
        });
        self.dest.pattern_input.update(cx, |f, cx| {
            f.set_labels(
                tr!("Folder pattern"),
                "CaptureMann/{yyyy}/{MM}/{title}", // i18n-ignore: an example pattern
                cx,
            )
        });
        let provider = self
            .dest
            .setup
            .as_ref()
            .map_or(&cloud::GOOGLE_DRIVE, |s| s.provider);
        self.dest.client_id_input.update(cx, |f, cx| {
            f.set_labels(provider.id_label(), provider.client_id_hint, cx)
        });
        self.dest.client_secret_input.update(cx, |f, cx| {
            f.set_labels(provider.secret_label(), "GOCSPX-\u{2026}", cx)
        });
    }
}
