use super::*;

impl SettingsView {
    // ---- connecting a cloud drive (STO-2) -----------------------------------------------------

    /// Opens the guided setup under the provider's row (`Target::New`) or under the destination
    /// that needs a new sign-in. A reconnect offers the client ID saved in the keychain.
    pub(super) fn open_setup(
        &mut self,
        provider: &'static ProviderInfo,
        target: Target,
        cx: &mut Context<Self>,
    ) {
        let saved = match &target {
            Target::Reconnect { destination_id } => self
                .dest
                .items
                .iter()
                .find(|d| &d.id == destination_id)
                .and_then(|d| d.account.as_ref())
                .and_then(|a| {
                    cm_auth::credentials::load_credentials(
                        self.dest.env.secrets.as_ref(),
                        (provider.config)().id,
                        &a.account,
                    )
                    .ok()
                    .flatten()
                }),
            Target::New => None,
        };
        self.dest.editing = None;
        self.dest.removing = false;
        self.dest.error = None;
        self.dest.connect_task = None;
        let app_folder = match &target {
            Target::Reconnect { destination_id } => self
                .dest
                .items
                .iter()
                .find(|d| &d.id == destination_id)
                .and_then(|d| d.account.as_ref())
                .is_some_and(|a| a.app_folder),
            Target::New => false,
        };
        self.dest.setup = Some(Setup {
            provider,
            target,
            step: SetupStep::Form,
            id_error: None,
            secret_error: None,
            error: None,
            use_saved: saved.is_some(),
            saved,
            app_folder,
        });
        self.dest.client_id_input.update(cx, |f, cx| {
            f.set_labels(provider.id_label(), provider.client_id_hint, cx);
            f.clear(cx)
        });
        self.dest.client_secret_input.update(cx, |f, cx| {
            f.set_labels(provider.secret_label(), "GOCSPX-\u{2026}", cx);
            f.clear(cx)
        });
        cx.notify();
    }

    pub(super) fn reconnect_destination(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(provider) = self
            .dest
            .items
            .iter()
            .find(|d| d.id == id)
            .and_then(cloud::provider_of)
        else {
            return;
        };
        self.open_setup(
            provider,
            Target::Reconnect {
                destination_id: id.to_owned(),
            },
            cx,
        );
    }

    /// Closes the panel. A sign-in in progress is abandoned (the browser tab can just be closed).
    pub(super) fn cancel_setup(&mut self, cx: &mut Context<Self>) {
        self.dest.setup = None;
        self.dest.connect_task = None;
        cx.notify();
    }

    /// "Continue in browser": checks the pasted values, then runs the flow on the app's runtime.
    pub(super) fn submit_setup(&mut self, cx: &mut Context<Self>) {
        let Some(setup) = self.dest.setup.as_mut() else {
            return;
        };
        if !matches!(setup.step, SetupStep::Form) {
            return;
        }
        let provider = setup.provider;
        let creds = match (&setup.saved, setup.use_saved) {
            (Some(saved), true) => saved.clone(),
            _ => {
                let form = SetupForm {
                    client_id: self.dest.client_id_input.read(cx).text().to_owned(),
                    client_secret: self.dest.client_secret_input.read(cx).text().to_owned(),
                };
                match cloud::validate_setup(provider, &form) {
                    Ok(c) => c,
                    Err(e) => {
                        setup.id_error = e.client_id;
                        setup.secret_error = e.client_secret;
                        cx.notify();
                        return;
                    }
                }
            }
        };
        let target = setup.target.clone();
        let opts = cloud::Options {
            app_folder: setup.app_folder,
        };
        setup.step = SetupStep::Working(Phase::Browser);
        setup.error = None;
        let env = self.dest.env.clone();
        let (rt, daemon) = {
            let b = &self.app.read(cx).backend;
            (b.rt.clone(), b.daemon.clone())
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Phase>();
        let work = rt.spawn(async move {
            cloud::connect_with(&env, provider, creds, &target, opts, &move |p| {
                let _ = tx.send(p);
            })
            .await
        });
        self.dest.connect_task = Some(cx.spawn(async move |this, cx| {
            // `tx` is dropped when the flow ends, which ends this loop.
            while let Some(phase) = rx.recv().await {
                let Ok(()) = this.update(cx, |this, cx| {
                    if let Some(s) = this.dest.setup.as_mut() {
                        if matches!(s.step, SetupStep::Working(_)) {
                            s.step = SetupStep::Working(phase);
                        }
                    }
                    cx.notify();
                }) else {
                    return;
                };
            }
            let result = work
                .await
                .unwrap_or_else(|e| Err(cm_core::CmError::internal(format!("sign-in task: {e}"))));
            this.update(cx, |this, cx| {
                this.finish_setup(provider, result, daemon, cx);
            })
            .ok();
        }));
        cx.notify();
    }

    fn finish_setup(
        &mut self,
        provider: &'static ProviderInfo,
        result: Result<cloud::Connected, cm_core::CmError>,
        daemon: Option<cm_daemon::Daemon>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(done) => {
                // STO-2.AC5: uploads that waited for this account go on.
                if let Some(d) = daemon {
                    d.resume_auth_expired_jobs();
                }
                self.dest.test_results.remove(&done.destination_id);
                self.dest.accounts.remove(&done.destination_id);
                crate::ui::upload_ui::reconnected(&self.app, &done.destination_id, cx);
                if let Some(s) = self.dest.setup.as_mut() {
                    s.step = SetupStep::Done(done);
                }
                self.reload_destinations(cx);
            }
            Err(e) => {
                if let Some(s) = self.dest.setup.as_mut() {
                    s.step = SetupStep::Form;
                    s.error = Some(Problem::connect(Some(provider), &e));
                }
            }
        }
        cx.notify();
    }

    /// "Test connection": uploads and deletes a 1 KB file with the destination's account.
    pub(super) fn test_destination(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(dest) = self.dest.items.iter().find(|d| d.id == id).cloned() else {
            return;
        };
        if self.dest.testing.is_some() {
            return;
        }
        self.dest.testing = Some(dest.id.clone());
        self.dest.test_results.remove(&dest.id);
        let provider = cloud::provider_of(&dest);
        let env = self.dest.env.clone();
        let rt = self.app.read(cx).backend.rt.clone();
        let id = dest.id.clone();
        let work = rt.spawn(async move { cloud::test_connection(&env, &dest).await });
        self.dest.test_task = Some(cx.spawn(async move |this, cx| {
            let result = work
                .await
                .unwrap_or_else(|e| Err(cm_core::CmError::internal(format!("test task: {e}"))));
            this.update(cx, |this, cx| {
                this.dest.testing = None;
                if matches!(&result, Err(e) if e.code() == ErrorCode::AuthExpired) {
                    this.dest.accounts.insert(
                        id.clone(),
                        AccountState::Reconnect(crate::cloud_connect::ReconnectWhy::Refused),
                    );
                } else if result.is_ok() {
                    this.dest
                        .accounts
                        .insert(id.clone(), AccountState::Connected);
                }
                this.dest
                    .test_results
                    .insert(id, result.map_err(|e| Problem::connect(provider, &e)));
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Asks Google to revoke the sign-in (best effort), then forgets the destination and its
    /// keychain secrets. Nothing is deleted remotely. When Google cannot be reached the
    /// destination is still disconnected and the page says how to finish at Google.
    pub(super) fn disconnect_destination(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.dest.sign_out_task.is_some() {
            return;
        }
        let name = self
            .dest
            .items
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        let env = self.dest.env.clone();
        let rt = self.app.read(cx).backend.rt.clone();
        let rid = id.to_owned();
        let work = rt.spawn({
            let rid = rid.clone();
            async move { cloud::disconnect_and_revoke(&env, &rid).await }
        });
        self.dest.sign_out_task = Some(cx.spawn(async move |this, cx| {
            let result = work
                .await
                .unwrap_or_else(|e| Err(cm_core::CmError::internal(format!("sign-out task: {e}"))));
            this.update(cx, |this, cx| {
                this.dest.sign_out_task = None;
                match result {
                    Ok(outcome) => {
                        this.dest.accounts.remove(&rid);
                        this.dest.test_results.remove(&rid);
                        this.close_destination(cx);
                        this.reload_destinations(cx);
                        // Only a revoke that failed needs the user's attention.
                        if outcome.needs_manual_removal() {
                            this.dest.error = Some(Problem::SignOutCloud {
                                name: name.clone(),
                                outcome,
                            });
                        }
                    }
                    Err(e) => this.dest.error = Some(Problem::raw(e.to_string())),
                }
                cx.notify();
            })
            .ok();
        }));
    }
}
