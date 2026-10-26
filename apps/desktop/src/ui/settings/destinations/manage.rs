use super::*;

impl SettingsView {
    /// Looks at every folder off the UI thread (a dropped share can block a stat). With `cloud`
    /// it also checks each cloud account: its secrets are in the keychain and no upload waits
    /// for it as `auth_expired`.
    pub(super) fn probe_destinations(&mut self, cx: &mut Context<Self>, cloud: bool) {
        let items = self.dest.items.clone();
        let env = self.dest.env.clone();
        let (rt, daemon) = {
            let b = &self.app.read(cx).backend;
            (b.rt.clone(), b.daemon.clone())
        };
        self.dest.probe_task = Some(cx.spawn(async move |this, cx| {
            let accounts = if cloud && items.iter().any(|d| d.kind.is_cloud()) {
                let paused = match daemon {
                    Some(d) => rt
                        .spawn(async move {
                            crate::backend::Backend::call(
                                &d,
                                "list_jobs",
                                serde_json::json!({ "type": "upload", "state": "paused", "limit": 200 }),
                            )
                            .await
                        })
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .map(|v| cloud::auth_paused_destinations(&v))
                        .unwrap_or_default(),
                    None => Default::default(),
                };
                let items = items.clone();
                Some(
                    cx.background_executor()
                        .spawn(async move {
                            items
                                .iter()
                                .filter(|d| d.kind.is_cloud())
                                .map(|d| {
                                    (
                                        d.id.clone(),
                                        cloud::account_state(env.secrets.as_ref(), d, &paused),
                                    )
                                })
                                .collect::<HashMap<_, _>>()
                        })
                        .await,
                )
            } else {
                None
            };
            let found = cx
                .background_executor()
                .spawn(async move { rules::probe(&items) })
                .await;
            this.update(cx, |this, cx| {
                this.dest.online = found;
                if let Some(a) = accounts {
                    this.dest.accounts = a;
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// Re-reads the file, shows the list and checks the folders and accounts.
    pub(super) fn reload_destinations(&mut self, cx: &mut Context<Self>) {
        self.dest.reload();
        self.probe_destinations(cx, true);
        cx.notify();
    }

    /// Applies `change` to the stored list (read fresh) and saves it. Returns whether it saved;
    /// the reason is shown in the panel otherwise.
    fn change_destinations(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Vec<Destination>) -> Result<(), rules::RuleError>,
    ) -> bool {
        let store = store();
        let result = store
            .list()
            .map_err(|e| rules::RuleError::Os(e.to_string()))
            .and_then(|mut list| {
                change(&mut list)?;
                store
                    .save(&list)
                    .map_err(|e| rules::RuleError::Os(e.to_string()))
            });
        let ok = match result {
            Ok(()) => {
                self.dest.error = None;
                true
            }
            Err(e) => {
                self.dest.error = Some(Problem::Rule(e));
                false
            }
        };
        self.reload_destinations(cx);
        ok
    }

    /// "Add local folder": the system folder picker, then the folder is added with defaults and
    /// its panel opens for naming.
    pub(super) fn add_destination_folder(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr!("Choose a folder").into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update(cx, |this, cx| {
                let mut new_id = None;
                let saved = this.change_destinations(cx, |list| {
                    new_id = Some(rules::add_folder(list, &path)?);
                    Ok(())
                });
                if let (true, Some(id)) = (saved, new_id) {
                    this.open_destination(&id, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn open_destination(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(d) = self.dest.items.iter().find(|d| d.id == id).cloned() else {
            return;
        };
        self.dest.editing = Some(d.id.clone());
        self.dest.edit_what = d.what;
        self.dest.edit_auto = d.auto_upload;
        self.dest.edit_link = d.account.as_ref().map(|a| a.link_mode).unwrap_or_default();
        self.dest.removing = false;
        self.dest.error = None;
        self.dest.setup = None;
        self.dest
            .name_input
            .update(cx, |f, cx| f.set_text(&d.name, cx));
        self.dest
            .pattern_input
            .update(cx, |f, cx| f.set_text(&d.folder_pattern, cx));
        // `set_text` reports a change; that clears the error, which is already empty.
        cx.notify();
    }

    pub(super) fn close_destination(&mut self, cx: &mut Context<Self>) {
        self.dest.editing = None;
        self.dest.removing = false;
        self.dest.error = None;
        cx.notify();
    }

    /// Saves the panel's name, pattern and "what to copy".
    pub(super) fn save_destination(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.dest.editing.clone() else {
            return;
        };
        let edit = Edit {
            name: self.dest.name_input.read(cx).text().to_owned(),
            folder_pattern: self.dest.pattern_input.read(cx).text().to_owned(),
            what: self.dest.edit_what,
            auto_upload: self.dest.edit_auto,
            link_mode: self.dest.edit_link,
        };
        if self.change_destinations(cx, |list| rules::apply_edit(list, &id, &edit)) {
            self.close_destination(cx);
        }
    }

    pub(super) fn make_default_destination(&mut self, id: &str, cx: &mut Context<Self>) {
        let id = id.to_owned();
        self.change_destinations(cx, |list| rules::set_default(list, &id));
    }

    /// Forgets the destination. The files already copied there stay where they are.
    pub(super) fn remove_destination(&mut self, id: &str, cx: &mut Context<Self>) {
        let id = id.to_owned();
        if self.change_destinations(cx, |list| {
            rules::remove(list, &id)
                .map(|_| ())
                .ok_or(rules::RuleError::DestinationGone)
        }) {
            self.close_destination(cx);
        }
    }
}
