//! The publish sheet (PUB-2, DESIGN §5.6): publish a recording to YouTube. A full-window page
//! in the style of the recording pages (toolbar with a crumb back to the recording, a centered
//! column), opened with the Publish button of the recording details page and the recording
//! editor.
//!
//! Everything it decides lives in `crate::publish` (form, validation mapping, job, stage), tested
//! without a display; the daemon calls are `publish::backend`. This file draws and wires.
//! The daemon, not this page, asks the user to confirm before anything is sent (Confirm tier).

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use cm_daemon::events::Event;
use cm_i18n::{tr, tr_ctx};
use cm_publish::accounts::{PublishAccount, PublishAccountStore};
use cm_publish::{PlatformRules, Privacy};
use gpui::{
    div, img, prelude::*, px, AnyElement, App, Bounds, ClipboardItem, Context, Entity, FocusHandle,
    FontWeight, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    PathPromptOptions, Pixels, Render, RenderImage, Role, SharedString, Task, Window,
};
use image::Frame;

use crate::poster::decode_poster;
use crate::publish::backend as pback;
use crate::publish::cancel::{CancelState, Step};
use crate::publish::form::{privacy_note, Check, Field, PublishForm, Quota, Readiness};
use crate::publish::job::{Failure, Outcome, OwnFailure, Tracker};
use crate::publish::sheet::{progress_text, request_key, retry_key, Stage, View};
use crate::publish::thumb;
use crate::publish_connect::privacy_label;
use crate::recording::exports::Phase as ExportPhase;
use crate::recording::files::{self, Details};
use crate::recording::model::clock_text;
use crate::theme::{scale, type_scale, Theme};
use crate::toast::{ToastKind, ToastSpec};
use crate::ui::app::{self, AppModel, Route};
use crate::ui::components::{
    button, button_with_icon, card, icon, icon_button, label_fill, shrink_text, switch, ButtonKind,
};
use crate::ui::export_ui;
use crate::ui::kit;
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::widgets::{
    caption, disabled_button, segment, segment_with_icon, segmented, settings_row,
};
use crate::ui::slider::{self, SliderLook};
use crate::ui::studio::ThemeGlobal;
use crate::ui::text_field::{TextField, TextFieldEvent};

static TRACKER: LazyLock<Mutex<Tracker>> = LazyLock::new(Mutex::default);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Feeds a daemon `job.*` event to the publish jobs and wakes the page.
pub fn on_event(app: &gpui::Entity<AppModel>, ev: &Event, cx: &mut App) {
    if !ev.name.starts_with("job.") {
        return;
    }
    if lock(&TRACKER).on_event(&ev.name, &ev.data) {
        app.update(cx, |_, cx| cx.notify());
    }
}

/// Opens the sheet for a recording.
pub fn open(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    app::set_route(app, Route::Publish(capture_id.to_owned()), cx);
}

/// What the page knows about the recording.
struct Loaded {
    details: Details,
}

impl Loaded {
    /// The export that would be uploaded: the newest one still on disk.
    fn export(&self) -> Option<(PathBuf, &files::ExportLine)> {
        self.details
            .exports
            .iter()
            .rev()
            .find(|e| e.exists)
            .map(|e| (self.details.dir.join(&e.file), e))
    }

    fn duration_secs(&self) -> Option<f64> {
        self.export()
            .and_then(|(_, e)| e.duration_ms)
            .map(|ms| ms as f64 / 1000.0)
            .or(self.details.duration_s)
    }
}

enum Load {
    Idle,
    Loading,
    Ready(Box<Loaded>),
    Failed(files::FileError),
}

/// The thumbnail picker.
struct Thumb {
    image: Option<Arc<RenderImage>>,
    /// The slider position (0 to 1) over the export.
    frac: f32,
    /// The picture shown is the frame at `frac` (not an image file).
    from_frame: bool,
    busy: bool,
    /// The pointer is down on the slider: the frame is taken when it is released.
    dragging: bool,
    /// Counts slider moves, so a frame is only taken for the last of a burst.
    seq: u64,
    note: Option<ThumbNote>,
}

/// A line under the thumbnail picker; the words are made when drawn.
enum ThumbNote {
    /// The picture could not be decoded for the preview (the reason).
    NoPreview(files::PosterError),
    /// What the frame extraction said.
    Said(thumb::ExtractError),
}

impl ThumbNote {
    fn text(&self) -> String {
        match self {
            ThumbNote::NoPreview(e) => tr!(
                "The picture can\u{2019}t be shown here ({reason}); it is still uploaded if the type and size are accepted.",
                reason = e.to_string()
            ),
            ThumbNote::Said(e) => e.text(),
        }
    }
}

impl Default for Thumb {
    fn default() -> Self {
        Thumb {
            image: None,
            frac: 0.5,
            from_frame: false,
            busy: false,
            dragging: false,
            seq: 0,
            note: None,
        }
    }
}

/// How long the slider rests before the frame is taken (keys and slow drags).
const FRAME_DEBOUNCE: Duration = Duration::from_millis(300);

pub struct PublishPage {
    app: Entity<AppModel>,
    id: Option<String>,
    load: Load,
    rules: PlatformRules,
    form: PublishForm,
    title_input: Entity<TextField>,
    description_input: Entity<TextField>,
    tags_input: Entity<TextField>,
    playlist_input: Entity<TextField>,
    thumb_focus: FocusHandle,
    slider_bounds: Rc<Cell<Bounds<Pixels>>>,
    accounts: Vec<PublishAccount>,
    account: usize,
    accounts_error: Option<String>,
    quota: Option<Quota>,
    stage: Stage,
    /// Cancel was pressed for the running job and it has not stopped yet.
    cancel: CancelState,
    thumb: Thumb,
    counter: u64,
    copied: bool,
    epoch: u64,
    /// The language the text fields' labels were made in.
    labels_epoch: u64,
    tasks: Vec<Task<()>>,
}

/// The label and the hint of the title, tags and playlist fields (the description has its own).
struct FieldLabels {
    title: (String, String),
    description: (String, String),
    tags: (String, String),
    playlist: (String, String),
}

impl FieldLabels {
    fn current() -> Self {
        FieldLabels {
            title: (
                tr_ctx!("video field", "Title"),
                tr_ctx!("video field hint", "Title"),
            ),
            description: (
                tr_ctx!("video field", "Description"),
                tr!("What is this video about?"),
            ),
            tags: (
                tr_ctx!("video field", "Tags"),
                tr!("demo, screen recording"),
            ),
            playlist: (
                tr_ctx!("video field", "Playlist"),
                tr!("PLxxxxxxxxxxxxxxxx or a playlist address"),
            ),
        }
    }
}

impl PublishPage {
    /// Puts the text fields' labels and hints into the current language.
    fn relabel(&mut self, cx: &mut Context<Self>) {
        self.labels_epoch = cm_i18n::epoch();
        let l = FieldLabels::current();
        for (f, (label, hint)) in [
            (&self.title_input, l.title),
            (&self.description_input, l.description),
            (&self.tags_input, l.tags),
            (&self.playlist_input, l.playlist),
        ] {
            f.update(cx, |f, cx| f.set_labels(label, hint, cx));
        }
    }
}

impl PublishPage {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        // An export finishing (`library_epoch`) is what the page waits for when there is none.
        cx.observe(&app, |this, app, cx| {
            let epoch = app.read(cx).library_epoch;
            if epoch != this.epoch {
                this.epoch = epoch;
                this.reload(cx);
            } else {
                cx.notify();
            }
        })
        .detach();
        let labels = FieldLabels::current();
        let field = |cx: &mut Context<Self>, (label, hint): (String, String)| {
            let f = cx.new(|cx| TextField::new(label, hint, cx).without_icon());
            cx.subscribe(&f, |_, _, ev: &TextFieldEvent, cx| {
                if matches!(ev, TextFieldEvent::Changed(_)) {
                    cx.notify();
                }
            })
            .detach();
            f
        };
        let title_input = field(cx, labels.title);
        let description_input = {
            let f = cx.new(|cx| {
                TextField::new(labels.description.0, labels.description.1, cx)
                    .without_icon()
                    .multiline(4, 10)
            });
            cx.subscribe(&f, |_, _, ev: &TextFieldEvent, cx| {
                if matches!(ev, TextFieldEvent::Changed(_)) {
                    cx.notify();
                }
            })
            .detach();
            f
        };
        let tags_input = field(cx, labels.tags);
        let playlist_input = field(cx, labels.playlist);
        let thumb_focus = cx.focus_handle();
        let epoch = app.read(cx).library_epoch;
        PublishPage {
            app,
            id: None,
            load: Load::Idle,
            rules: PlatformRules::youtube(),
            form: PublishForm::new("", Privacy::Unlisted),
            title_input,
            description_input,
            tags_input,
            playlist_input,
            thumb_focus,
            slider_bounds: Rc::default(),
            accounts: Vec::new(),
            account: 0,
            accounts_error: None,
            quota: None,
            stage: Stage::Editing,
            cancel: CancelState::default(),
            thumb: Thumb::default(),
            counter: 0,
            copied: false,
            epoch,
            labels_epoch: cm_i18n::epoch(),
            tasks: Vec::new(),
        }
    }

    /// Shows the sheet for recording `id` (a no-op while it is already shown).
    pub fn set_capture(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.id.as_deref() == Some(id) {
            return;
        }
        self.id = Some(id.to_owned());
        self.load = Load::Loading;
        self.stage = Stage::Editing;
        self.thumb = Thumb::default();
        self.copied = false;
        self.form = PublishForm::new("", Privacy::Unlisted);
        for f in [
            &self.description_input,
            &self.tags_input,
            &self.playlist_input,
        ] {
            f.update(cx, |f, cx| f.clear(cx));
        }
        self.load_accounts(cx);
        self.reload(cx);
    }

    pub fn is_open(&self) -> bool {
        self.id.is_some()
    }

    /// Forgets the recording when the page is left.
    pub fn close(&mut self, _cx: &mut Context<Self>) {
        self.id = None;
        self.load = Load::Idle;
        self.tasks.clear();
    }

    fn load_accounts(&mut self, cx: &mut Context<Self>) {
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

    fn account(&self) -> Option<&PublishAccount> {
        self.accounts.get(self.account)
    }

    /// Today's quota, when the daemon reports it.
    fn load_quota(&mut self, cx: &mut Context<Self>) {
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
    fn reload(&mut self, cx: &mut Context<Self>) {
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
    fn sync_form(&mut self, cx: &mut Context<Self>) {
        self.form.title = self.title_input.read(cx).text().to_owned();
        self.form.description = self.description_input.read(cx).text().to_owned();
        self.form.tags_text = self.tags_input.read(cx).text().to_owned();
        self.form.playlist_text = self.playlist_input.read(cx).text().to_owned();
    }

    fn readiness<'a>(
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

    fn slider_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.stage.locked(&lock(&TRACKER)) {
            return;
        }
        window.focus(&self.thumb_focus, cx);
        self.thumb.dragging = true;
        let f = self.slider_x(ev.position.x);
        self.set_slider(f, false, cx);
    }

    fn slider_move(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.thumb.dragging {
            let f = self.slider_x(ev.position.x);
            self.set_slider(f, false, cx);
        }
    }

    fn slider_up(&mut self, _ev: &MouseUpEvent, cx: &mut Context<Self>) {
        if self.thumb.dragging {
            self.thumb.dragging = false;
            let f = self.thumb.frac;
            self.set_slider(f, true, cx);
        }
    }

    fn slider_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
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

    fn choose_image(&mut self, cx: &mut Context<Self>) {
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

    fn clear_thumb(&mut self, cx: &mut Context<Self>) {
        self.thumb = Thumb::default();
        self.form.thumbnail = None;
        cx.notify();
    }

    // ---- publishing ----------------------------------------------------------------------------

    fn next_key(&mut self) -> String {
        self.counter += 1;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        request_key(self.counter, nanos)
    }

    /// Publish: the daemon shows its confirmation dialog, and only then starts the job.
    fn publish(&mut self, cx: &mut Context<Self>) {
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
    fn cancel_publish(&mut self, cx: &mut Context<Self>) {
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
    fn retry(&mut self, job_id: String, cx: &mut Context<Self>) {
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

// ---- drawing -----------------------------------------------------------------------------------

/// A field of the details card: the label with a mono note on the right (the counter, "comma
/// separated", "optional"), the input, its error and its hint. `key` keeps the error's element id.
fn field_block(
    t: &Theme,
    key: &str,
    label: &str,
    side: Option<String>,
    input: AnyElement,
    hint: Option<String>,
    error: Option<&str>,
) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .flex()
                .items_baseline()
                .justify_between()
                .gap(px(8.))
                .child(
                    div()
                        .min_w(px(0.))
                        .text_size(px(type_scale::UI))
                        .text_color(rgba_of(t.text_2))
                        .child(SharedString::from(label.to_owned())),
                )
                .children(side.map(|s| {
                    div()
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.mist))
                        .mono()
                        .child(SharedString::from(s))
                })),
        )
        .child(input)
        .children(error.map(|e| {
            div()
                .id(SharedString::from(format!("pub-error-{key}")))
                .role(Role::Alert)
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.warning))
                .child(SharedString::from(e.to_owned()))
        }))
        .children(hint.map(|h| caption(t, h)))
}

/// A card title: 16/600.
fn card_title(t: &Theme, text: &str) -> gpui::Div {
    let _ = t;
    div()
        .text_size(px(16.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(SharedString::from(text.to_owned()))
}

/// The icon of a visibility option.
fn privacy_icon(p: Privacy) -> &'static str {
    match p {
        Privacy::Private => "lock",
        Privacy::Unlisted => "link",
        Privacy::Public => "globe",
    }
}

/// One line under the visibility choice: what the chosen visibility means on YouTube.
fn privacy_blurb(p: Privacy) -> String {
    match p {
        Privacy::Private => tr!("Only you can watch it."),
        Privacy::Unlisted => {
            tr!("Anyone with the link can watch. It won\u{2019}t appear in search or on your channel.")
        }
        Privacy::Public => tr!("Anyone can find and watch it."),
    }
}

/// The first letter of a channel name for its tile (no avatar is fetched).
fn initial(name: &str) -> String {
    name.trim()
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".to_owned())
}

fn progress_bar(t: &Theme, fraction: f64) -> gpui::Div {
    div()
        .h(px(6.))
        .w_full()
        .rounded(px(3.))
        .bg(rgba_of(t.raised))
        .child(
            div()
                .h_full()
                .rounded(px(3.))
                .bg(rgba_of(t.text))
                .w(gpui::relative(fraction.clamp(0.0, 1.0) as f32)),
        )
}

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
    fn header(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
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

    /// The page body (two columns: the video and thumbnail on the left, the details and visibility
    /// on the right) and the pinned footer.
    fn sheet(
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
            // TODO(design): the board draws a video preview with a play button above this row;
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

    /// The pinned footer: Publish (the neutral primary) with the reason it is disabled, the
    /// note about the daemon's confirmation dialog, and the confirming, running, failed and done
    /// states.
    fn footer(&self, t: &Theme, view: &View, check: &Check, cx: &mut Context<Self>) -> AnyElement {
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

/// Width of the left column (the video and the thumbnail), px; the board's is about 370.
const LEFT_COL_W: f32 = 370.;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_options_have_distinct_icons_and_lines() {
        let all = [Privacy::Private, Privacy::Unlisted, Privacy::Public];
        let icons: Vec<_> = all.iter().map(|p| privacy_icon(*p)).collect();
        assert_eq!(icons, ["lock", "link", "globe"]);
        let lines: std::collections::HashSet<_> = all.iter().map(|p| privacy_blurb(*p)).collect();
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn the_channel_tile_uses_the_initial_and_survives_an_empty_name() {
        assert_eq!(initial("the AI Explorer"), "T");
        assert_eq!(initial("  \u{e9}cole"), "\u{c9}");
        assert_eq!(initial(""), "?");
    }
}
