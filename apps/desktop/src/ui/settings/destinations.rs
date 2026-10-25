//! Destinations page (kit `Settings`): "Save to" rows and "Publish to" cards.
//!
//! The local-folder part is real (STO-1): the library folder, then one row per stored
//! destination (`destinations.json`, written only here, in the app process), with Manage to
//! edit, make default or remove, and "Add local folder" below. The rules live in
//! `crate::destinations`; this file only draws and wires.
//!
//! Google Drive connects here (STO-2): "Connect" on its row opens the guided setup panel under
//! the row (what to create in Google Cloud Console, paste the client ID and secret), then the
//! system browser sign-in, then the destination appears in the list with a Manage panel (the
//! same panel as a folder, plus the link mode, Test connection and Disconnect). The flow itself
//! is `crate::cloud_connect`, tested with fakes; this file only draws and wires it. It is a
//! human-only flow in the app process: there is no action, tool or command for it. OneDrive
//! (Microsoft Entra app, no secret) and Dropbox (scoped app, no secret, full Dropbox or App
//! folder) connect the same way through their rows of `cloud_connect::PROVIDERS`; social
//! accounts need cm-publish.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use cm_auth::ClientCredentials;
use cm_core::ErrorCode;
use cm_i18n::{tr, tr_ctx, trn};
use cm_storage::{AutoUpload, Destination, DestinationStore, Kind, LinkMode, What};
use gpui::{
    div, prelude::*, px, AnyElement, Context, Entity, FontWeight, PathPromptOptions, Role,
    SharedString, Task,
};

use super::model::{DRIVES, PLATFORMS};
use super::problem::Problem;
use super::widgets::{
    caption, footer_row, segment, segmented, settings_group, settings_row, small_button_with_icon,
};
use super::SettingsView;
use crate::cloud_connect::{self as cloud, AccountState, Phase, ProviderInfo, SetupForm, Target};
use crate::destinations::{self as rules, Edit};
use crate::library::LibraryStatus;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::Route;
use crate::ui::components::{button, button_with_icon, ButtonKind};
use crate::ui::kit::{callout, pill, status, StatusKind, Tone};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::text_field::{TextField, TextFieldEvent};

/// The state of the "Save to" list.
pub struct DestState {
    items: Vec<Destination>,
    /// `destinations.json` could not be read (kept as it is on disk, not replaced).
    load_error: Option<String>,
    /// Folder reachable now, by id; a destination missing here is still being checked.
    online: HashMap<String, bool>,
    /// The destination whose Manage panel is open.
    editing: Option<String>,
    edit_what: What,
    edit_auto: AutoUpload,
    edit_link: LinkMode,
    /// The panel asks "Remove ...?" (or "Disconnect ...?") before it forgets the destination.
    removing: bool,
    error: Option<Problem>,
    name_input: Entity<TextField>,
    pattern_input: Entity<TextField>,
    probe_task: Option<Task<()>>,
    _poll: Task<()>,
    /// What the connect flow talks to: keychain, system browser, the drive's API.
    env: Arc<cloud::Env>,
    /// Signed in or "Reconnect", by destination id; absent while it is being checked.
    accounts: HashMap<String, AccountState>,
    /// The guided setup panel (connect or reconnect), when open.
    setup: Option<Setup>,
    client_id_input: Entity<TextField>,
    client_secret_input: Entity<TextField>,
    connect_task: Option<Task<()>>,
    /// "Test connection": the destination being tested, and the last answer by id.
    testing: Option<String>,
    test_results: HashMap<String, Result<(), Problem>>,
    test_task: Option<Task<()>>,
    /// "Disconnect": asking Google to revoke, then forgetting the destination.
    sign_out_task: Option<Task<()>>,
    /// The language the text fields' labels were made in.
    labels_epoch: std::cell::Cell<u64>,
}

/// The state of the guided setup panel.
struct Setup {
    provider: &'static ProviderInfo,
    target: Target,
    step: SetupStep,
    id_error: Option<cloud::FieldError>,
    secret_error: Option<cloud::FieldError>,
    error: Option<Problem>,
    /// The client ID and secret already in the keychain (a reconnect can reuse them).
    saved: Option<ClientCredentials>,
    use_saved: bool,
    /// Dropbox: the app has "App folder" access (otherwise full Dropbox).
    app_folder: bool,
}

enum SetupStep {
    Form,
    Working(Phase),
    Done(cloud::Connected),
}

impl DestState {
    pub(super) fn new(cx: &mut Context<SettingsView>) -> DestState {
        let name_input = cx.new(|cx| {
            TextField::new(
                tr!("Destination name"),
                tr_ctx!("destination name field hint", "Name"),
                cx,
            )
            .without_icon()
        });
        let pattern_input = cx.new(|cx| {
            TextField::new(
                tr!("Folder pattern"),
                "CaptureMann/{yyyy}/{MM}/{title}", // i18n-ignore: an example pattern
                cx,
            )
            .without_icon()
        });
        for field in [&name_input, &pattern_input] {
            cx.subscribe(field, |this, _, ev: &TextFieldEvent, cx| match ev {
                TextFieldEvent::Submitted => this.save_destination(cx),
                TextFieldEvent::Changed(_) => {
                    this.dest.error = None;
                    cx.notify();
                }
                TextFieldEvent::Escaped => this.close_destination(cx),
            })
            .detach();
        }
        let client_id_input = cx.new(|cx| {
            TextField::new(
                cloud::GOOGLE_DRIVE.id_label(),
                cloud::GOOGLE_DRIVE.client_id_hint,
                cx,
            )
            .without_icon()
        });
        let client_secret_input = cx.new(|cx| {
            TextField::new(cloud::GOOGLE_DRIVE.secret_label(), "GOCSPX-\u{2026}", cx)
                .without_icon()
                .secret()
        });
        for field in [&client_id_input, &client_secret_input] {
            cx.subscribe(field, |this, _, ev: &TextFieldEvent, cx| match ev {
                TextFieldEvent::Submitted => this.submit_setup(cx),
                TextFieldEvent::Changed(_) => {
                    if let Some(s) = this.dest.setup.as_mut() {
                        s.id_error = None;
                        s.secret_error = None;
                        s.error = None;
                    }
                    cx.notify();
                }
                TextFieldEvent::Escaped => this.cancel_setup(cx),
            })
            .detach();
        }
        // A NAS or an external drive comes and goes: while the page shows, look again every 5 s.
        // A cloud account is looked at when first shown and then every 30 s (it reads the
        // keychain and the waiting uploads).
        let mut ticks = 0u32;
        let poll = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            ticks += 1;
            let Ok(()) = this.update(cx, |this, cx| {
                if this.app.read(cx).route == Route::Destinations {
                    let unseen = this
                        .dest
                        .items
                        .iter()
                        .any(|d| d.kind.is_cloud() && !this.dest.accounts.contains_key(&d.id));
                    this.probe_destinations(cx, unseen || ticks.is_multiple_of(6));
                }
            }) else {
                break;
            };
        });
        let mut state = DestState {
            items: Vec::new(),
            load_error: None,
            online: HashMap::new(),
            editing: None,
            edit_what: What::Original,
            edit_auto: AutoUpload::Off,
            edit_link: LinkMode::Anyone,
            removing: false,
            error: None,
            name_input,
            pattern_input,
            probe_task: None,
            _poll: poll,
            env: Arc::new(cloud::Env::system(store())),
            accounts: HashMap::new(),
            setup: None,
            client_id_input,
            client_secret_input,
            connect_task: None,
            testing: None,
            test_results: HashMap::new(),
            test_task: None,
            sign_out_task: None,
            labels_epoch: std::cell::Cell::new(cm_i18n::epoch()),
        };
        state.reload();
        state
    }

    fn reload(&mut self) {
        match store().list() {
            Ok(items) => {
                self.items = items;
                self.load_error = None;
            }
            Err(e) => self.load_error = Some(e.to_string()),
        }
    }
}

// ---- shared panel pieces (also used by `publish_card.rs`) ----------------------------------

/// The expanded panel under a row (Manage, Connect, the YouTube panels): a recessed block inside
/// the group card, divided like a row.
pub(super) fn panel_shell(t: &Theme) -> gpui::Div {
    div()
        .px(px(18.))
        .py(px(16.))
        .flex()
        .flex_col()
        .gap(px(16.))
        .border_b_1()
        .border_color(rgba_of(t.row_divider()))
        .bg(rgba_of(t.surface_2))
}

/// A labelled control of a panel: the label (14/500), the control, an optional hint.
pub(super) fn field_block(
    t: &Theme,
    label: &str,
    hint: Option<String>,
    control: impl IntoElement,
) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .text_size(px(type_scale::UI))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba_of(t.text_2))
                .child(SharedString::from(label.to_owned())),
        )
        .child(control)
        .children(hint.map(|h| caption(t, h)))
}

/// A warning or error line of a panel: a brass callout that keeps its id and alert role.
pub(super) fn alert_line(
    t: &Theme,
    id: &'static str,
    text: impl Into<SharedString>,
) -> impl IntoElement {
    callout(t, Tone::Brass, "triangle-alert", text, None, None)
        .id(id)
        .role(Role::Alert)
}

/// The line next to a "Test connection" button: the outcome as a tinted callout that keeps its
/// id and status role.
pub(super) fn outcome_line(
    t: &Theme,
    id: &'static str,
    result: Result<String, String>,
) -> impl IntoElement {
    let (tone, glyph, text) = match result {
        Ok(m) => (Tone::Success, "circle-check", m),
        Err(e) => (Tone::Brass, "triangle-alert", e),
    };
    callout(t, tone, glyph, text, None, None)
        .id(id)
        .role(Role::Status)
        .flex_1()
        .min_w(px(200.))
}

/// The guided steps of a connect panel: a numbered badge and the step text, which wraps inside
/// the panel (a flex child never shrinks below its text unless its min width is 0).
pub(super) fn step_list<'a>(t: &Theme, steps: impl Iterator<Item = &'a str>) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .children(steps.enumerate().map(|(n, text)| {
            div()
                .flex()
                .gap(px(12.))
                .items_start()
                .child(
                    div()
                        .flex_none()
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(scale::RADIUS_PILL))
                        .bg(rgba_of(t.raised))
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.text_2))
                        .mono()
                        .child(SharedString::from((n + 1).to_string())),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .pt(px(2.))
                        .text_size(px(type_scale::UI))
                        .text_color(rgba_of(t.text_2))
                        .child(SharedString::from(text.to_owned())),
                )
        }))
}

/// The status of a stored destination's row: a cloud account is signed in or needs "Reconnect"; a
/// folder is reachable now (Online), not (Offline) or not looked at yet (Checking).
pub(super) fn row_status(
    is_cloud: bool,
    account: Option<&AccountState>,
    online: Option<bool>,
) -> (StatusKind, String) {
    if is_cloud {
        match account {
            Some(AccountState::Connected) => {
                (StatusKind::Ok, tr_ctx!("account status", "Connected"))
            }
            Some(AccountState::Reconnect(_)) => (StatusKind::Warning, tr!("Reconnect needed")),
            None => (StatusKind::Pending, tr_ctx!("status", "Checking")),
        }
    } else {
        match online {
            Some(true) => (StatusKind::Ok, tr_ctx!("folder status", "Online")),
            Some(false) => (StatusKind::Warning, tr_ctx!("folder status", "Offline")),
            None => (StatusKind::Pending, tr_ctx!("status", "Checking")),
        }
    }
}

/// The glyph of a destination's icon tile.
pub(super) fn kind_icon(kind: Kind) -> &'static str {
    match kind {
        Kind::Local => "folder",
        Kind::GoogleDrive => crate::ui::brand::GOOGLE_DRIVE.icon,
        Kind::OneDrive => crate::ui::brand::ONEDRIVE.icon,
        Kind::Dropbox => crate::ui::brand::DROPBOX.icon,
    }
}

/// The glyph of a cloud-drive row by its display name (the brand mark when we have it).
fn drive_icon(name: &str) -> &'static str {
    crate::ui::brand::by_name(name).map_or("cloud", |b| b.icon)
}

/// The hint of a drive row: what it does while nothing is connected, then how many accounts.
pub(super) fn drive_hint(connected: usize) -> String {
    match connected {
        0 => tr!("Upload copies to a folder in your Drive."),
        n => trn!("{n} account connected", "{n} accounts connected", n),
    }
}

fn store() -> DestinationStore {
    DestinationStore::new(crate::settings::config_dir())
}

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

    fn open_destination(&mut self, id: &str, cx: &mut Context<Self>) {
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

    fn close_destination(&mut self, cx: &mut Context<Self>) {
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

    fn make_default_destination(&mut self, id: &str, cx: &mut Context<Self>) {
        let id = id.to_owned();
        self.change_destinations(cx, |list| rules::set_default(list, &id));
    }

    /// Forgets the destination. The files already copied there stay where they are.
    fn remove_destination(&mut self, id: &str, cx: &mut Context<Self>) {
        let id = id.to_owned();
        if self.change_destinations(cx, |list| {
            rules::remove(list, &id)
                .map(|_| ())
                .ok_or(rules::RuleError::DestinationGone)
        }) {
            self.close_destination(cx);
        }
    }

    // ---- connecting a cloud drive (STO-2) -----------------------------------------------------

    /// Opens the guided setup under the provider's row (`Target::New`) or under the destination
    /// that needs a new sign-in. A reconnect offers the client ID saved in the keychain.
    fn open_setup(
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

    fn reconnect_destination(&mut self, id: &str, cx: &mut Context<Self>) {
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
    fn cancel_setup(&mut self, cx: &mut Context<Self>) {
        self.dest.setup = None;
        self.dest.connect_task = None;
        cx.notify();
    }

    /// "Continue in browser": checks the pasted values, then runs the flow on the app's runtime.
    fn submit_setup(&mut self, cx: &mut Context<Self>) {
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
    fn test_destination(&mut self, id: &str, cx: &mut Context<Self>) {
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
    fn disconnect_destination(&mut self, id: &str, cx: &mut Context<Self>) {
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

    fn destination_row(&self, t: &Theme, i: usize, cx: &mut Context<Self>) -> AnyElement {
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
    fn relabel_dest_fields(&self, cx: &mut Context<Self>) {
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

    pub(super) fn destinations(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
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
                                super::model::later_milestone(),
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
            let detail = super::model::platform_detail(name).unwrap_or_else(|| detail.to_string());
            super::publish_card::coming_soon_card(t, i, name, &detail)
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
}

impl SettingsView {
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

/// What the user reads when a connect, test or reconnect step fails.
pub(super) fn connect_error_text(
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
fn testing_note(t: &Theme, result: Option<Result<(), Problem>>) -> Option<AnyElement> {
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
    fn setup_panel(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_status_follows_the_probe() {
        assert_eq!(
            row_status(false, None, Some(true)),
            (StatusKind::Ok, "Online".to_owned())
        );
        assert_eq!(
            row_status(false, None, Some(false)),
            (StatusKind::Warning, "Offline".to_owned())
        );
        assert_eq!(
            row_status(false, None, None),
            (StatusKind::Pending, "Checking".to_owned())
        );
    }

    #[test]
    fn cloud_status_follows_the_account_not_the_folder_probe() {
        let connected = AccountState::Connected;
        let reconnect =
            AccountState::Reconnect(crate::cloud_connect::ReconnectWhy::Os("expired".into()));
        assert_eq!(
            row_status(true, Some(&connected), Some(false)),
            (StatusKind::Ok, "Connected".to_owned())
        );
        assert_eq!(
            row_status(true, Some(&reconnect), Some(true)),
            (StatusKind::Warning, "Reconnect needed".to_owned())
        );
        assert_eq!(
            row_status(true, None, Some(true)),
            (StatusKind::Pending, "Checking".to_owned())
        );
    }

    #[test]
    fn drive_hint_counts_accounts() {
        assert!(drive_hint(0).contains("Upload copies"));
        assert_eq!(drive_hint(1), "1 account connected");
        assert_eq!(drive_hint(3), "3 accounts connected");
    }

    #[test]
    fn tiles_tell_folders_from_drives() {
        assert_eq!(kind_icon(Kind::Local), "folder");
        assert_eq!(kind_icon(Kind::GoogleDrive), "brand-google-drive");
        assert_eq!(kind_icon(Kind::OneDrive), "brand-onedrive");
        assert_eq!(kind_icon(Kind::Dropbox), "brand-dropbox");
        // The "Coming soon" rows (OneDrive, Dropbox) take the same marks as the connectable ones.
        for (name, _) in DRIVES {
            assert!(drive_icon(name).starts_with("brand-"), "{name}");
        }
    }

    #[test]
    fn counts_use_plural_forms_in_every_language() {
        use cm_i18n::{with_language, Lang};
        assert_eq!(drive_hint(1), "1 account connected");
        assert_eq!(drive_hint(2), "2 accounts connected");
        for lang in [Lang::Vi, Lang::EnXa] {
            with_language(lang, || {
                for n in [1usize, 2, 5] {
                    let h = drive_hint(n);
                    assert!(
                        h.contains(&n.to_string()) && !h.contains("{n}"),
                        "{lang:?}: {h}"
                    );
                }
            });
        }
        assert_eq!(
            with_language(Lang::Vi, || row_status(false, None, Some(true)).0),
            StatusKind::Ok
        );
    }
}
