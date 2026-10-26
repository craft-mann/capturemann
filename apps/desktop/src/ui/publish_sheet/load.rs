use super::*;

impl PublishPage {
    pub(super) fn load_accounts(&mut self, cx: &mut Context<Self>) {
        let store = PublishAccountStore::new(crate::settings::config_dir());
        match store.list() {
            Ok(list) => {
                self.accounts = list
                    .into_iter()
                    .filter(|a| a.platform == "youtube")
                    .collect();
                self.accounts_error = None;
            }
            Err(e) => {
                self.accounts.clear();
                self.accounts_error = Some(e.to_string());
            }
        }
        self.account = 0;
        if let Some(a) = self.accounts.first() {
            self.form.privacy = a.default_privacy;
        }
        self.load_quota(cx);
    }

    pub(super) fn account(&self) -> Option<&PublishAccount> {
        self.accounts.get(self.account)
    }

    /// Today's quota, when the daemon reports it.
    pub(super) fn load_quota(&mut self, cx: &mut Context<Self>) {
        self.quota = None;
        let (Some(a), Some(daemon)) = (
            self.account().map(|a| a.id.clone()),
            self.app.read(cx).backend.daemon.clone(),
        ) else {
            return;
        };
        let rt = self.app.read(cx).backend.rt.clone();
        let work = rt.spawn(async move { pback::quota(&daemon, &a).await });
        self.tasks.push(cx.spawn(async move |this, cx| {
            let q = work.await.ok().flatten();
            this.update(cx, |this, cx| {
                this.quota = q;
                cx.notify();
            })
            .ok();
        }));
    }

    /// Reads the recording's details again (an export may have finished).
    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.id.clone() else {
            return;
        };
        let root = self.app.read(cx).backend.library_root.clone();
        self.tasks.push(cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    let root = root.ok_or(files::FileError::NoLibrary)?;
                    let dir = files::find_bundle(&root, &id).ok_or(files::FileError::NoFolder)?;
                    files::read_details(&dir).map(|d| (id, d))
                })
                .await;
            this.update(cx, |this, cx| {
                match loaded {
                    Ok((id, details)) if this.id.as_deref() == Some(id.as_str()) => {
                        // The first load gives the title field its default.
                        if matches!(this.load, Load::Loading) {
                            this.form.title = details.title.clone();
                            this.title_input
                                .update(cx, |f, cx| f.set_text(&details.title, cx));
                        }
                        this.load = Load::Ready(Box::new(Loaded { details }));
                    }
                    Ok(_) => {}
                    Err(e) => this.load = Load::Failed(e),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// Copies what the text fields hold into the form (the form is what is checked and sent).
    pub(super) fn sync_form(&mut self, cx: &mut Context<Self>) {
        self.form.title = self.title_input.read(cx).text().to_owned();
        self.form.description = self.description_input.read(cx).text().to_owned();
        self.form.tags_text = self.tags_input.read(cx).text().to_owned();
        self.form.playlist_text = self.playlist_input.read(cx).text().to_owned();
    }

    pub(super) fn readiness<'a>(
        &'a self,
        loaded: Option<&'a Loaded>,
        export: Option<&'a std::path::Path>,
    ) -> Readiness<'a> {
        Readiness {
            account: self.account().is_some(),
            export,
            duration_secs: loaded.and_then(Loaded::duration_secs),
            quota: self.quota,
        }
    }
}
