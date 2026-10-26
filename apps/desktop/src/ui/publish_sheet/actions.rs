use super::*;

impl PublishPage {
    // ---- publishing ----------------------------------------------------------------------------

    fn next_key(&mut self) -> String {
        self.counter += 1;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        request_key(self.counter, nanos)
    }

    /// Publish: the daemon shows its confirmation dialog, and only then starts the job.
    pub(super) fn publish(&mut self, cx: &mut Context<Self>) {
        self.sync_form(cx);
        let (Some(id), Load::Ready(l)) = (self.id.clone(), &self.load) else {
            return;
        };
        let export = l.export().map(|(p, _)| p);
        let check = self
            .form
            .check(&self.rules, self.readiness(Some(l), export.as_deref()));
        let (Some(account), Some(export)) = (self.account().cloned(), export) else {
            return;
        };
        if !check.can_publish() || self.stage.locked(&lock(&TRACKER)) {
            return;
        }
        let meta = self.form.meta(l.duration_secs());
        let captions = self.form.captions && l.details.has_captions;
        let key = self.next_key();
        let params = pback::publish_params(&id, &account.id, &meta, Some(&export), captions, &key);
        let Some(daemon) = self.app.read(cx).backend.daemon.clone() else {
            self.stage = Stage::Failed(Failure::own(OwnFailure::NoDaemon, None, None));
            cx.notify();
            return;
        };
        let rt = self.app.read(cx).backend.rt.clone();
        self.stage = Stage::Starting;
        let work = rt.spawn(async move { pback::submit(&daemon, params).await });
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result = work.await.unwrap_or_else(|e| {
                Err(Failure {
                    code: None,
                    message: format!("publish task: {e}"),
                    own: None,
                    job_id: None,
                })
            });
            this.update(cx, |this, cx| {
                match result {
                    Ok(job_id) => {
                        this.stage = Stage::Job(job_id.clone());
                        this.watch_job(job_id, cx);
                    }
                    Err(f) => this.stage = Stage::Failed(f),
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Asks `get_job` every few seconds (every half second while a cancel is pending) until the
    /// job has ended, so a result is never missed (events are lossy for a slow client, and a
    /// cancelled job sends none).
    fn watch_job(&mut self, job_id: String, cx: &mut Context<Self>) {
        let Some(daemon) = self.app.read(cx).backend.daemon.clone() else {
            return;
        };
        let rt = self.app.read(cx).backend.rt.clone();
        self.tasks.push(cx.spawn(async move |this, cx| loop {
            let wait = this
                .update(cx, |this, _| this.cancel.poll_interval())
                .unwrap_or(crate::publish::cancel::POLL_IDLE);
            cx.background_executor().timer(wait).await;
            let id = job_id.clone();
            let d = daemon.clone();
            let answer = rt.spawn(async move { pback::job(&d, &id).await }).await;
            let Ok(Ok(job)) = answer else {
                continue;
            };
            let keep_going = this
                .update(cx, |this, cx| {
                    if this.stage != Stage::Job(job_id.clone()) {
                        return false;
                    }
                    this.apply_job(&job_id, &job, cx)
                })
                .unwrap_or(false);
            if !keep_going {
                break;
            }
        }));
    }

    /// Takes a `get_job` answer; `true` while the job is still going.
    fn apply_job(&mut self, job_id: &str, job: &serde_json::Value, cx: &mut Context<Self>) -> bool {
        let state = job["state"].as_str().unwrap_or_default();
        let step = self.cancel.on_job(job_id, state);
        if !self.apply_cancel_step(job_id, step, cx) {
            return false;
        }
        lock(&TRACKER).on_job(job);
        cx.notify();
        matches!(state, "queued" | "running" | "paused")
    }

    /// Acts on what a cancel answer or job state meant. Returns `false` when the page went back
    /// to the form (the job was cancelled), `true` otherwise.
    fn apply_cancel_step(&mut self, job_id: &str, step: Step, cx: &mut Context<Self>) -> bool {
        match step {
            Step::Wait => true,
            Step::Cancelled => {
                lock(&TRACKER).reset(job_id);
                self.stage = Stage::Editing;
                app::push_toast(
                    &self.app,
                    ToastSpec::new(ToastKind::Info, tr!("Publish cancelled")),
                    cx,
                );
                cx.notify();
                false
            }
            Step::Ended => {
                // The job finished by itself: its result is read like any other.
                self.poll_job_now(job_id.to_owned(), cx);
                true
            }
            Step::Error { message, .. } => {
                app::push_toast(
                    &self.app,
                    crate::flows::error_toast_live(
                        || tr!("Couldn\u{2019}t cancel the publish"),
                        message,
                    ),
                    cx,
                );
                cx.notify();
                true
            }
        }
    }

    /// One `get_job` now (after a cancel that raced the end of the job).
    fn poll_job_now(&mut self, job_id: String, cx: &mut Context<Self>) {
        let Some(daemon) = self.app.read(cx).backend.daemon.clone() else {
            return;
        };
        let rt = self.app.read(cx).backend.rt.clone();
        self.tasks.push(cx.spawn(async move |this, cx| {
            let id = job_id.clone();
            let answer = rt
                .spawn(async move { pback::job(&daemon, &id).await })
                .await;
            if let Ok(Ok(job)) = answer {
                this.update(cx, |this, cx| {
                    if this.stage == Stage::Job(job_id.clone()) {
                        lock(&TRACKER).on_job(&job);
                        cx.notify();
                    }
                })
                .ok();
            }
        }));
    }

    /// Cancel on the progress line: no question, `cancel_job` as the user, "Cancelling..."
    /// until the job has stopped.
    pub(super) fn cancel_publish(&mut self, cx: &mut Context<Self>) {
        let Stage::Job(job_id) = self.stage.clone() else {
            return;
        };
        let running = matches!(self.stage.view(&lock(&TRACKER)), View::Running { .. });
        if !self.cancel.press(&job_id, running) {
            return;
        }
        let Some(daemon) = self.app.read(cx).backend.daemon.clone() else {
            self.cancel = CancelState::Idle;
            return;
        };
        let rt = self.app.read(cx).backend.rt.clone();
        let id = job_id.clone();
        let work = rt.spawn(async move { pback::cancel(&daemon, &id).await });
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                if this.stage != Stage::Job(job_id.clone()) {
                    return;
                }
                let step = match &result {
                    Ok(Ok(job)) => this.cancel.on_reply(&job_id, Ok(job)),
                    Ok(Err(e)) => this
                        .cancel
                        .on_reply(&job_id, Err((e.code, e.message.as_str()))),
                    Err(e) => this.cancel.on_reply(&job_id, Err((None, &e.to_string()))),
                };
                this.apply_cancel_step(&job_id, step, cx);
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Retry on a failed job (`retry_job`); the upload continues from the confirmed byte.
    pub(super) fn retry(&mut self, job_id: String, cx: &mut Context<Self>) {
        let Some(daemon) = self.app.read(cx).backend.daemon.clone() else {
            return;
        };
        let rt = self.app.read(cx).backend.rt.clone();
        self.counter += 1;
        let key = retry_key(&job_id, self.counter);
        let id = job_id.clone();
        let work = rt.spawn(async move { pback::retry(&daemon, &id, &key).await });
        self.tasks.push(cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(())) => {
                        lock(&TRACKER).reset(&job_id);
                        this.stage = Stage::Job(job_id.clone());
                        this.watch_job(job_id, cx);
                    }
                    Ok(Err(f)) => this.stage = Stage::Failed(f),
                    Err(e) => {
                        this.stage = Stage::Failed(Failure {
                            code: None,
                            message: format!("retry task: {e}"),
                            own: None,
                            job_id: Some(job_id),
                        })
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }
}
