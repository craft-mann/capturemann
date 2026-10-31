//! Settings → Destinations → "Publish to" → YouTube (PUB-1): the card (every connected channel,
//! each with Manage or Reconnect, and "Add another channel"), its guided setup panel (the Google
//! Cloud steps, the client ID and masked secret, browser progress, the channel) and its Manage
//! panel for one channel (default channel, default visibility, Test connection, Reconnect,
//! Disconnect, which asks Google to revoke the sign-in first).
//!
//! The flow is `crate::publish_connect`, tested with fakes; this file only draws and wires it.
//! It is a human-only flow in the app process: there is no action, tool or command for it.
//! TikTok and Facebook keep their disabled cards until their providers exist.

use std::collections::HashMap;
use std::sync::Arc;

use cm_auth::ClientCredentials;
use cm_core::ErrorCode;
use cm_i18n::{tr, tr_ctx, trn};
use cm_publish::accounts::{PublishAccount, PublishAccountStore};
use cm_publish::Privacy;
use gpui::{
    div, prelude::*, px, AnyElement, Context, Entity, FontWeight, Role, SharedString, Task,
};

use super::destinations::{alert_line, field_block, outcome_line, step_list};
use super::model::PLATFORMS;
use super::problem::Problem;
use super::widgets::{caption, disabled_button, segment, segmented};
use super::SettingsView;
use crate::publish_connect::{
    self as pc, AccountState, Phase, SetupErrors, SetupForm, Target, YOUTUBE_GUIDE,
};
use crate::theme::{scale, type_scale, Theme};
use crate::ui::components::{button, button_with_icon, card, icon, ButtonKind};
use crate::ui::kit::{pill, status, tone_colors, StatusKind, Tone};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::text_field::{TextField, TextFieldEvent};

/// The state of the YouTube card and its panel.
pub struct YouTubeCard {
    env: Arc<pc::Env>,
    /// Every connected YouTube channel, in the order of `publish_accounts.json`; the first is the
    /// default one the publish sheet starts with.
    accounts: Vec<PublishAccount>,
    /// Signed in or "Reconnect", by account id; absent while the keychain is being looked at.
    states: HashMap<String, AccountState>,
    /// The channel the Manage panel and the test act on.
    selected: Option<String>,
    /// What the last Disconnect did, when the user has to read it (Google could not be reached).
    notice: Option<Problem>,
    sign_out_task: Option<Task<()>>,
    load_error: Option<String>,
    panel: Option<Panel>,
    client_id_input: Entity<TextField>,
    client_secret_input: Entity<TextField>,
    connect_task: Option<Task<()>>,
    test_task: Option<Task<()>>,
    probe_task: Option<Task<()>>,
    testing: bool,
    test_result: Option<Result<String, Problem>>,
    /// The Manage panel asks "Disconnect?" before it forgets the account.
    removing: bool,
    error: Option<Problem>,
    /// The language the text fields' labels were made in.
    labels_epoch: std::cell::Cell<u64>,
}

enum Panel {
    Setup(Box<Setup>),
    Manage,
}

struct Setup {
    target: Target,
    step: SetupStep,
    errors: SetupErrors,
    error: Option<Problem>,
    /// The client ID and secret already in the keychain (a reconnect can reuse them).
    saved: Option<ClientCredentials>,
    use_saved: bool,
}

enum SetupStep {
    Form,
    Working(Phase),
    Done(pc::Connected),
}

fn store() -> PublishAccountStore {
    PublishAccountStore::new(crate::settings::config_dir())
}

/// The overall state of the YouTube card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CardState {
    NotConnected,
    /// This many channels, all signed in.
    Connected(usize),
    Reconnect {
        total: usize,
        needing: usize,
    },
}

/// The card's state from the number of channels and how many need to reconnect.
pub(super) fn card_state(total: usize, needing: usize) -> CardState {
    match (total, needing) {
        (0, _) => CardState::NotConnected,
        (total, needing) if needing > 0 => CardState::Reconnect { total, needing },
        (total, _) => CardState::Connected(total),
    }
}

/// The accessible name of a platform card.
pub(super) fn card_label(name: &str, state: CardState) -> String {
    match state {
        CardState::NotConnected => tr!("{name}, not connected.", name = name),
        CardState::Connected(n) => tr!("{name}, {n} connected.", name = name, n = n),
        CardState::Reconnect { total, needing } => {
            tr!(
                "{name}, {total} connected, {needing} need to reconnect.",
                name = name,
                total = total,
                needing = needing
            )
        }
    }
}

/// The two letters of a platform's tile ("YT", "TT", "FB").
pub(super) fn platform_letters(name: &str) -> String {
    match name {
        "YouTube" => "YT".to_owned(),  // i18n-ignore: the tile letters
        "TikTok" => "TT".to_owned(),   // i18n-ignore: the tile letters
        "Facebook" => "FB".to_owned(), // i18n-ignore: the tile letters
        other => other.chars().take(2).collect::<String>().to_uppercase(),
    }
}

/// The 40 px tile of a platform card: the service's mark, in the text colour when the platform is live
/// and in the muted colour when it is not built yet; the two letters when we have no mark.
fn platform_tile(t: &Theme, name: &str, live: bool) -> gpui::Div {
    let tile = div()
        .size(px(40.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_MD))
        .bg(rgba_of(t.icon_tile()));
    match crate::ui::brand::by_name(name) {
        Some(b) => tile.child(icon(b.icon, 20., if live { t.text_2 } else { t.mist })),
        None => tile
            .text_size(px(type_scale::UI))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgba_of(t.text_2))
            .child(SharedString::from(platform_letters(name))),
    }
}

/// A platform that is not built yet (TikTok, Facebook): dimmed, with its real description and a
/// button that cannot be used (it stays in the tab order with its reason, NFR-13).
pub(super) fn coming_soon_card(t: &Theme, i: usize, name: &str, detail: &str) -> AnyElement {
    card(t)
        .id(("publish-card", i))
        .role(Role::Group)
        .aria_label(SharedString::from(tr!(
            "{name}, not connected. Coming soon.",
            name = name
        )))
        .p(px(18.))
        .flex_1()
        .min_w(px(240.))
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .child(platform_tile(t, name, false))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(type_scale::BODY))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgba_of(t.text_3))
                                .child(SharedString::from(name.to_owned())),
                        )
                        .child(
                            div()
                                .text_size(px(type_scale::UI))
                                .text_color(rgba_of(t.mist))
                                .child(SharedString::from(super::model::later_milestone())),
                        ),
                )
                .child(icon("clock", 20., t.mist)),
        )
        .child(
            div()
                .text_size(px(13.))
                .text_color(rgba_of(t.mist))
                .line_height(px(19.))
                .child(SharedString::from(detail.to_owned())),
        )
        .child(disabled_button(
            t,
            ("publish-connect", i),
            &tr!("Not available yet"),
            &super::model::later_milestone(),
            true,
        ))
        .into_any_element()
}

impl YouTubeCard {
    pub(super) fn new(cx: &mut Context<SettingsView>) -> YouTubeCard {
        let client_id_input = cx.new(|cx| {
            TextField::new(
                tr_ctx!("client credential", "Client ID"),
                YOUTUBE_GUIDE.client_id_hint,
                cx,
            )
            .without_icon()
        });
        let client_secret_input = cx.new(|cx| {
            TextField::new(
                tr_ctx!("client credential", "Client secret"),
                "GOCSPX-\u{2026}",
                cx,
            )
            .without_icon()
            .secret()
        });
        for field in [&client_id_input, &client_secret_input] {
            cx.subscribe(field, |this, _, ev: &TextFieldEvent, cx| match ev {
                TextFieldEvent::Submitted => this.yt_submit(cx),
                TextFieldEvent::Changed(_) => {
                    if let Some(Panel::Setup(s)) = this.yt.panel.as_mut() {
                        s.errors = SetupErrors::default();
                        s.error = None;
                    }
                    cx.notify();
                }
                TextFieldEvent::Escaped => this.yt_close(cx),
            })
            .detach();
        }
        YouTubeCard {
            env: Arc::new(pc::Env::system(store())),
            accounts: Vec::new(),
            states: HashMap::new(),
            selected: None,
            notice: None,
            sign_out_task: None,
            load_error: None,
            panel: None,
            client_id_input,
            client_secret_input,
            connect_task: None,
            test_task: None,
            probe_task: None,
            testing: false,
            test_result: None,
            removing: false,
            error: None,
            labels_epoch: std::cell::Cell::new(cm_i18n::epoch()),
        }
    }
}

/// What the user reads when a connect, test or reconnect step fails.
pub(super) fn yt_error_text(code: ErrorCode, message: &str) -> String {
    match code {
        ErrorCode::AuthExpired => tr!("{error} Use Reconnect to sign in again.", error = message),
        ErrorCode::PlatformRejected => tr!(
            "Google or YouTube said no: {error} Check the client ID and secret, and that the YouTube Data API v3 is enabled in your project.",
            error = message
        ),
        ErrorCode::Network => tr!(
            "Could not reach the server: {error} Check your connection and try again.",
            error = message
        ),
        _ => message.to_owned(),
    }
}

impl SettingsView {
    /// Reads the account file now and the keychain in the background.
    pub(super) fn yt_reload(&mut self, cx: &mut Context<Self>) {
        match store().list() {
            Ok(list) => {
                self.yt.accounts = list
                    .into_iter()
                    .filter(|a| a.platform == pc::PLATFORM)
                    .collect();
                self.yt.load_error = None;
            }
            Err(e) => self.yt.load_error = Some(e.to_string()),
        }
        self.yt.states.clear();
        if self
            .yt
            .selected
            .as_ref()
            .is_some_and(|id| !self.yt.accounts.iter().any(|a| &a.id == id))
        {
            self.yt.selected = None;
        }
        if self.yt.accounts.is_empty() {
            cx.notify();
            return;
        }
        let accounts = self.yt.accounts.clone();
        let secrets = self.yt.env.secrets.clone();
        self.yt.probe_task = Some(cx.spawn(async move |this, cx| {
            let states = cx
                .background_executor()
                .spawn(async move {
                    accounts
                        .iter()
                        .map(|a| (a.id.clone(), pc::account_state(secrets.as_ref(), a)))
                        .collect::<HashMap<_, _>>()
                })
                .await;
            this.update(cx, |this, cx| {
                this.yt.states = states;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// The channel the panel is about.
    fn yt_selected(&self) -> Option<&PublishAccount> {
        let id = self.yt.selected.as_ref()?;
        self.yt.accounts.iter().find(|a| &a.id == id)
    }

    fn yt_close(&mut self, cx: &mut Context<Self>) {
        self.yt.panel = None;
        self.yt.connect_task = None;
        self.yt.removing = false;
        self.yt.error = None;
        cx.notify();
    }

    fn yt_open_manage(&mut self, id: &str, cx: &mut Context<Self>) {
        self.yt.selected = Some(id.to_owned());
        self.yt.notice = None;
        self.yt.panel = Some(Panel::Manage);
        self.yt.removing = false;
        self.yt.error = None;
        self.yt.test_result = None;
        cx.notify();
    }

    /// Opens the guided setup (new account) or a new sign-in for the connected one. A reconnect
    /// offers the client ID saved in the keychain.
    fn yt_open_setup(&mut self, reconnect: Option<&str>, cx: &mut Context<Self>) {
        self.yt.notice = None;
        let account = reconnect.and_then(|id| self.yt.accounts.iter().find(|a| a.id == id));
        let (target, saved) = match account {
            Some(a) => (
                Target::Reconnect {
                    account_id: a.id.clone(),
                },
                cm_auth::credentials::load_credentials(
                    self.yt.env.secrets.as_ref(),
                    cm_auth::ProviderConfig::google_youtube().id,
                    &a.id,
                )
                .ok()
                .flatten(),
            ),
            None => (Target::New, None),
        };
        if let Target::Reconnect { account_id } = &target {
            self.yt.selected = Some(account_id.clone());
        }
        self.yt.connect_task = None;
        self.yt.error = None;
        self.yt.removing = false;
        self.yt.panel = Some(Panel::Setup(Box::new(Setup {
            target,
            step: SetupStep::Form,
            errors: SetupErrors::default(),
            error: None,
            use_saved: saved.is_some(),
            saved,
        })));
        self.yt.client_id_input.update(cx, |f, cx| f.clear(cx));
        self.yt.client_secret_input.update(cx, |f, cx| f.clear(cx));
        cx.notify();
    }

    /// "Continue in browser": checks the pasted values, then runs the flow on the app's runtime.
    fn yt_submit(&mut self, cx: &mut Context<Self>) {
        let (id_text, secret_text) = (
            self.yt.client_id_input.read(cx).text().to_owned(),
            self.yt.client_secret_input.read(cx).text().to_owned(),
        );
        let Some(Panel::Setup(setup)) = self.yt.panel.as_mut() else {
            return;
        };
        if !matches!(setup.step, SetupStep::Form) {
            return;
        }
        let creds = match (&setup.saved, setup.use_saved) {
            (Some(saved), true) => saved.clone(),
            _ => {
                match pc::validate_setup(&SetupForm {
                    client_id: id_text,
                    client_secret: secret_text,
                }) {
                    Ok(c) => c,
                    Err(e) => {
                        setup.errors = e;
                        cx.notify();
                        return;
                    }
                }
            }
        };
        let target = setup.target.clone();
        setup.step = SetupStep::Working(Phase::Browser);
        setup.error = None;
        let env = self.yt.env.clone();
        let (rt, daemon) = {
            let b = &self.app.read(cx).backend;
            (b.rt.clone(), b.daemon.clone())
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Phase>();
        let work = rt.spawn(async move {
            pc::connect(&env, creds, &target, &move |p| {
                let _ = tx.send(p);
            })
            .await
        });
        self.yt.connect_task = Some(cx.spawn(async move |this, cx| {
            // `tx` is dropped when the flow ends, which ends this loop.
            while let Some(phase) = rx.recv().await {
                let Ok(()) = this.update(cx, |this, cx| {
                    if let Some(Panel::Setup(s)) = this.yt.panel.as_mut() {
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
            this.update(cx, |this, cx| this.yt_finish(result, daemon, cx))
                .ok();
        }));
        cx.notify();
    }

    fn yt_finish(
        &mut self,
        result: Result<pc::Connected, cm_core::CmError>,
        daemon: Option<cm_daemon::Daemon>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(done) => {
                // Publishes that waited for this account can go on (`retry_job` from the sheet).
                let _ = daemon;
                self.yt.test_result = None;
                if let Some(Panel::Setup(s)) = self.yt.panel.as_mut() {
                    s.step = SetupStep::Done(done);
                }
                self.yt_reload(cx);
            }
            Err(e) => {
                if let Some(Panel::Setup(s)) = self.yt.panel.as_mut() {
                    s.step = SetupStep::Form;
                    s.error = Some(Problem::youtube(&e));
                }
            }
        }
        cx.notify();
    }

    /// "Test connection": renews the token and lists the channel (read-only).
    fn yt_test(&mut self, cx: &mut Context<Self>) {
        let Some(account) = self.yt_selected().cloned() else {
            return;
        };
        if self.yt.testing {
            return;
        }
        self.yt.testing = true;
        self.yt.test_result = None;
        let env = self.yt.env.clone();
        let rt = self.app.read(cx).backend.rt.clone();
        let tested = account.clone();
        let work = rt.spawn(async move { pc::test_connection(&env, &tested.id).await });
        self.yt.test_task = Some(cx.spawn(async move |this, cx| {
            let result = work
                .await
                .unwrap_or_else(|e| Err(cm_core::CmError::internal(format!("test task: {e}"))));
            this.update(cx, |this, cx| {
                this.yt.testing = false;
                match &result {
                    Err(e) if e.code() == ErrorCode::AuthExpired => {
                        this.yt.states.insert(
                            account.id.clone(),
                            AccountState::Reconnect(crate::cloud_connect::ReconnectWhy::Refused),
                        );
                    }
                    Ok(_) => {
                        this.yt
                            .states
                            .insert(account.id.clone(), AccountState::Connected);
                    }
                    Err(_) => {}
                }
                this.yt.test_result = Some(
                    result
                        .map(|c| tr!("Works: signed in as {channel}.", channel = c.title))
                        .map_err(|e| Problem::youtube(&e)),
                );
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    fn yt_set_privacy(&mut self, p: Privacy, cx: &mut Context<Self>) {
        let Some(a) = self.yt_selected() else {
            return;
        };
        match pc::set_default_privacy(&self.yt.env, &a.id.clone(), p) {
            Ok(updated) => {
                if let Some(slot) = self.yt.accounts.iter_mut().find(|x| x.id == updated.id) {
                    *slot = updated;
                }
            }
            Err(e) => self.yt.error = Some(Problem::raw(e.to_string())),
        }
        cx.notify();
    }

    /// Makes the channel the one the publish sheet starts with.
    fn yt_make_default(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.yt.selected.clone() else {
            return;
        };
        match pc::make_default(&self.yt.env, &id) {
            Ok(()) => {
                self.yt.error = None;
                self.yt_reload(cx);
            }
            Err(e) => {
                self.yt.error = Some(Problem::raw(e.to_string()));
                cx.notify();
            }
        }
    }

    /// Asks Google to revoke the sign-in (best effort), then forgets the channel and its
    /// keychain secrets. Videos are not touched. When Google cannot be reached the channel is
    /// still signed out here and the card says how to finish at Google.
    fn yt_disconnect(&mut self, cx: &mut Context<Self>) {
        let Some(a) = self.yt_selected().cloned() else {
            return;
        };
        if self.yt.sign_out_task.is_some() {
            return;
        }
        let env = self.yt.env.clone();
        let rt = self.app.read(cx).backend.rt.clone();
        let id = a.id.clone();
        let work = rt.spawn(async move { pc::disconnect_and_revoke(&env, &id).await });
        self.yt.sign_out_task = Some(cx.spawn(async move |this, cx| {
            let result = work
                .await
                .unwrap_or_else(|e| Err(cm_core::CmError::internal(format!("sign-out task: {e}"))));
            this.update(cx, |this, cx| {
                this.yt.sign_out_task = None;
                match result {
                    Ok(outcome) => {
                        this.yt.test_result = None;
                        this.yt_close(cx);
                        this.yt.selected = None;
                        this.yt.notice =
                            outcome
                                .needs_manual_removal()
                                .then(|| Problem::SignOutYouTube {
                                    name: a.name.clone(),
                                    outcome,
                                });
                        this.yt_reload(cx);
                    }
                    Err(e) => {
                        this.yt.error = Some(Problem::raw(e.to_string()));
                        cx.notify();
                    }
                }
            })
            .ok();
        }));
    }

    /// "Disconnect" on the card: opens the Manage panel of that channel already asking the
    /// question that explains Google is asked to revoke the access (C10: never one click).
    fn yt_open_disconnect(&mut self, id: &str, cx: &mut Context<Self>) {
        self.yt_open_manage(id, cx);
        self.yt.removing = true;
        cx.notify();
    }

    /// The YouTube card of the "Publish to" row (R14): the "YT" tile, the channel (or what the
    /// platform does), the state as an icon, then every connected channel with its Manage or
    /// Reconnect and Disconnect (which asks first), and "Add another channel" (or "Connect
    /// account" for none).
    pub(super) fn youtube_card(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        if self.yt.labels_epoch.get() != cm_i18n::epoch() {
            self.yt.labels_epoch.set(cm_i18n::epoch());
            self.yt.client_id_input.update(cx, |f, cx| {
                f.set_labels(
                    tr_ctx!("client credential", "Client ID"),
                    YOUTUBE_GUIDE.client_id_hint,
                    cx,
                )
            });
            self.yt.client_secret_input.update(cx, |f, cx| {
                f.set_labels(
                    tr_ctx!("client credential", "Client secret"),
                    "GOCSPX-\u{2026}",
                    cx,
                )
            });
        }
        let name = YOUTUBE_GUIDE.name;
        let many = self.yt.accounts.len() > 1;
        let reconnects = self
            .yt
            .accounts
            .iter()
            .filter(|a| matches!(self.yt.states.get(&a.id), Some(AccountState::Reconnect(_))))
            .count();
        let state = card_state(self.yt.accounts.len(), reconnects);
        let label = card_label(name, state);
        let border = match state {
            CardState::NotConnected => t.border,
            CardState::Connected(_) => tone_colors(t, Tone::Success).border,
            CardState::Reconnect { .. } => tone_colors(t, Tone::Brass).border,
        };
        let subtitle = match self.yt.accounts.as_slice() {
            [] => super::model::platform_detail(PLATFORMS[0].0)
                .unwrap_or_else(|| PLATFORMS[0].1.to_owned()),
            [one] => one.name.clone(),
            list => trn!("{n} channel", "{n} channels", list.len()),
        };
        // The wide "Not connected" pill sits under the text, not beside it: beside it the text
        // column of a 240 px card shrank to a few letters per line (seen on Linux, Oct 31, 2026).
        let (state_mark, not_connected): (Option<AnyElement>, Option<AnyElement>) = match state {
            CardState::NotConnected => (
                None,
                Some(
                    div()
                        .flex()
                        .child(status(
                            t,
                            StatusKind::Neutral,
                            tr_ctx!("drive status", "Not connected"),
                        ))
                        .into_any_element(),
                ),
            ),
            CardState::Connected(_) => (
                Some(icon("circle-check", 20., t.success).into_any_element()),
                None,
            ),
            CardState::Reconnect { .. } => (
                Some(icon("triangle-alert", 20., t.warning).into_any_element()),
                None,
            ),
        };
        let rows = self.yt.accounts.iter().enumerate().map(|(i, a)| {
            let needs_reconnect =
                matches!(self.yt.states.get(&a.id), Some(AccountState::Reconnect(_)));
            let id = a.id.clone();
            let did = a.id.clone();
            let action = if needs_reconnect {
                button_with_icon(
                    t,
                    ("publish-account-action", i),
                    tr_ctx!("button", "Reconnect"),
                    ButtonKind::Primary,
                    Some("plug"),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.yt_open_setup(Some(&id), cx)))
            } else {
                button_with_icon(
                    t,
                    ("publish-account-action", i),
                    tr_ctx!("button", "Manage"),
                    ButtonKind::Secondary,
                    Some("sliders-horizontal"),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.yt_open_manage(&id, cx)))
            }
            .flex_1()
            .min_w(px(0.));
            let disconnect = button_with_icon(
                t,
                ("publish-account-disconnect", i),
                tr_ctx!("button", "Disconnect"),
                ButtonKind::Secondary,
                Some("unplug"),
            )
            .aria_label(SharedString::from(tr!("Disconnect {name}", name = a.name)))
            .flex_1()
            .min_w(px(0.))
            .on_click(cx.listener(move |this, _, _, cx| this.yt_open_disconnect(&did, cx)));
            div()
                .id(("publish-account", i))
                .role(Role::Group)
                .aria_label(SharedString::from(
                    match (many && i == 0, needs_reconnect) {
                        (true, true) => {
                            tr!("{name}, default channel, reconnect needed", name = a.name)
                        }
                        (true, false) => {
                            tr!("{name}, default channel, connected", name = a.name)
                        }
                        (false, true) => tr!("{name}, reconnect needed", name = a.name),
                        (false, false) => tr!("{name}, connected", name = a.name),
                    },
                ))
                .flex()
                .flex_col()
                .gap(px(8.))
                .children(many.then(|| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .truncate()
                                .text_size(px(type_scale::UI))
                                .font_weight(FontWeight::MEDIUM)
                                .child(SharedString::from(a.name.clone())),
                        )
                        .children((i == 0).then(|| {
                            pill(t, Tone::Brass, Some("star"), tr_ctx!("channel", "Default"))
                        }))
                }))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.))
                        .child(if needs_reconnect {
                            status(t, StatusKind::Warning, tr_ctx!("status", "Reconnect"))
                        } else {
                            status(t, StatusKind::Ok, tr_ctx!("account status", "Connected"))
                        })
                        .child(caption(
                            t,
                            tr!(
                                "Default visibility: {visibility}",
                                visibility = pc::privacy_label(a.default_privacy)
                            ),
                        )),
                )
                .child(div().flex().gap(px(8.)).child(action).child(disconnect))
        });
        let add = if self.yt.accounts.is_empty() {
            button_with_icon(
                t,
                "publish-connect-0",
                tr!("Connect account"),
                ButtonKind::Primary,
                Some("plug"),
            )
        } else {
            button_with_icon(
                t,
                "publish-connect-0",
                tr!("Add another channel"),
                ButtonKind::Ghost,
                Some("plug"),
            )
        }
        .w_full()
        .on_click(cx.listener(|this, _, _, cx| this.yt_open_setup(None, cx)));
        card(t)
            .id(("publish-card", 0usize))
            .role(Role::Group)
            .aria_label(SharedString::from(label))
            .border_color(rgba_of(border))
            .p(px(18.))
            .flex_1()
            .min_w(px(240.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(platform_tile(t, "YouTube", true))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(type_scale::BODY))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_size(px(type_scale::UI))
                                    .text_color(rgba_of(t.text_3))
                                    .child(SharedString::from(subtitle)),
                            )
                            .children(not_connected),
                    )
                    .children(state_mark),
            )
            .children(rows)
            .children(
                self.yt
                    .notice
                    .as_ref()
                    .map(|n| alert_line(t, "yt-notice", n.text())),
            )
            .child(add)
            .into_any_element()
    }

    /// The setup or Manage panel under the cards, when one is open.
    pub(super) fn youtube_panel(&self, t: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let body = match self.yt.panel.as_ref()? {
            Panel::Setup(s) => self.yt_setup_body(t, s, cx),
            Panel::Manage => self.yt_manage_body(t, cx),
        };
        Some(
            card(t)
                .id("yt-panel")
                .role(Role::Group)
                .aria_label(tr!("YouTube account"))
                .p(px(18.))
                .flex()
                .flex_col()
                .gap(px(16.))
                .bg(rgba_of(t.surface_2))
                .child(body)
                .into_any_element(),
        )
    }

    fn yt_error_line(
        &self,
        t: &Theme,
        text: Option<String>,
        id: &'static str,
    ) -> Option<impl IntoElement> {
        text.map(|e| alert_line(t, id, e))
    }

    fn yt_setup_body(&self, t: &Theme, s: &Setup, cx: &mut Context<Self>) -> AnyElement {
        let reconnect = matches!(s.target, Target::Reconnect { .. });
        let heading = div()
            .text_size(px(type_scale::BODY))
            .font_weight(FontWeight::SEMIBOLD)
            .child(SharedString::from(if reconnect {
                tr!("Reconnect YouTube")
            } else {
                tr!("Connect YouTube")
            }));
        let cancel = |label: String| {
            button(t, "yt-setup-cancel", label, ButtonKind::Secondary)
                .on_click(cx.listener(|this, _, _, cx| this.yt_close(cx)))
        };
        let step: AnyElement = match &s.step {
            SetupStep::Form => match s.use_saved.then(|| s.saved.clone()).flatten() {
                Some(saved) => div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(caption(
                        t,
                        tr!("Your sign-in expired or was removed. Sign in again with the app credentials already saved in your Keychain:"),
                    ))
                    .child(caption(t, saved.client_id).mono())
                    .children(self.yt_error_line(t, s.error.as_ref().map(Problem::text), "yt-setup-error"))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(10.))
                            .child(
                                button_with_icon(
                                    t,
                                    "yt-setup-go",
                                    tr!("Sign in again"),
                                    ButtonKind::Primary,
                                    Some("external-link"),
                                )
                                .on_click(cx.listener(|this, _, _, cx| this.yt_submit(cx))),
                            )
                            .child(
                                button(
                                    t,
                                    "yt-setup-other",
                                    tr!("Use a different client ID\u{2026}"),
                                    ButtonKind::Ghost,
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(Panel::Setup(s)) = this.yt.panel.as_mut() {
                                        s.use_saved = false;
                                    }
                                    cx.notify();
                                })),
                            )
                            .child(cancel(tr_ctx!("button", "Cancel"))),
                    )
                    .into_any_element(),
                None => {
                    let step_texts = YOUTUBE_GUIDE.steps();
                    let steps = step_list(t, step_texts.iter().map(String::as_str));
                    let notes = YOUTUBE_GUIDE
                        .notes()
                        .into_iter()
                        .map(|n| caption(t, n))
                        .collect::<Vec<_>>();
                    let field = |label: &str,
                                 input: &Entity<TextField>,
                                 err: Option<String>,
                                 id: &'static str| {
                        field_block(t, label, None, input.clone())
                            .children(self.yt_error_line(t, err, id))
                    };
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(14.))
                        .child(caption(
                            t,
                            tr!("YouTube needs an app that you create yourself, so the sign-in and the upload limit belong to you. It takes a few minutes, once. The client ID and secret go to your Keychain, never to a file, and are only sent to Google."),
                        ))
                        .child(steps)
                        .child(div().flex().flex_col().gap(px(8.)).children(notes))
                        .child(field(
                            &tr_ctx!("client credential", "Client ID"),
                            &self.yt.client_id_input,
                            s.errors.client_id.as_ref().map(ToString::to_string),
                            "yt-setup-id-error",
                        ))
                        .child(field(
                            &tr_ctx!("client credential", "Client secret"),
                            &self.yt.client_secret_input,
                            s.errors.client_secret.as_ref().map(ToString::to_string),
                            "yt-setup-secret-error",
                        ))
                        .children(self.yt_error_line(t, s.error.as_ref().map(Problem::text), "yt-setup-error"))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(10.))
                                .child(
                                    button_with_icon(
                                        t,
                                        "yt-setup-go",
                                        tr!("Continue in browser"),
                                        ButtonKind::Primary,
                                        Some("external-link"),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| this.yt_submit(cx))),
                                )
                                .child(cancel(tr_ctx!("button", "Cancel"))),
                        )
                        .into_any_element()
                }
            },
            SetupStep::Working(phase) => div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .id("yt-setup-phase")
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
            SetupStep::Done(done) => div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(div().flex().child(status(
                    t,
                    StatusKind::Ok,
                    tr!("Connected as {channel}", channel = done.channel_name),
                )))
                .children(done.channel_changed.then(|| {
                    caption(
                        t,
                        tr!("This is a different channel than before; the account now publishes to it."),
                    )
                }))
                .child(caption(
                    t,
                    tr!("Publish a recording from its page with the Publish button. Videos are Unlisted unless you choose otherwise; Manage sets the default."),
                ))
                .child(
                    div().flex().flex_wrap().items_center().gap(px(10.)).child(
                        button(
                            t,
                            "yt-setup-done",
                            tr_ctx!("button", "Done"),
                            ButtonKind::Primary,
                        )
                            .on_click(cx.listener(|this, _, _, cx| this.yt_close(cx))),
                    ),
                )
                .into_any_element(),
        };
        div()
            .id("yt-setup")
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(heading)
            .child(step)
            .into_any_element()
    }

    fn yt_manage_body(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(a) = self.yt_selected() else {
            return div().into_any_element();
        };
        let is_default = self.yt.accounts.first().is_some_and(|f| f.id == a.id);
        let several = self.yt.accounts.len() > 1;
        let reconnect = match self.yt.states.get(&a.id) {
            Some(AccountState::Reconnect(why)) => Some(why.text()),
            _ => None,
        };
        let reconnect_id = a.id.clone();
        let privacies = [Privacy::Private, Privacy::Unlisted, Privacy::Public]
            .into_iter()
            .map(|p| {
                segment(
                    t,
                    ("yt-privacy", p as usize),
                    &pc::privacy_label(p),
                    a.default_privacy == p,
                )
                .on_click(cx.listener(move |this, _, _, cx| this.yt_set_privacy(p, cx)))
            });
        let test_note = self
            .yt
            .test_result
            .clone()
            .map(|r| outcome_line(t, "yt-test-result", r.map_err(|p| p.text())));
        let buttons = if self.yt.removing {
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
                        .child(SharedString::from(tr!(
                            "Disconnect \u{201c}{name}\u{201d}? CaptureMann asks Google to revoke the access you gave it, then deletes the sign-in and app credentials from this Mac. Videos already on YouTube stay. If Google cannot be reached, it is still signed out here and you can remove the access at myaccount.google.com/permissions.",
                            name = a.name
                        ))),
                )
                .child(
                    button_with_icon(
                        t,
                        "yt-remove-yes",
                        tr_ctx!("button", "Disconnect"),
                        ButtonKind::Danger,
                        Some("unplug"),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.yt_disconnect(cx))),
                )
                .child(
                    button(
                        t,
                        "yt-remove-no",
                        tr!("Keep it"),
                        ButtonKind::Secondary,
                    )
                    .on_click(
                        cx.listener(|this, _, _, cx| {
                            this.yt.removing = false;
                            cx.notify();
                        }),
                    ),
                )
        } else {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(10.))
                .child(
                    button(t, "yt-done", tr_ctx!("button", "Done"), ButtonKind::Primary)
                        .on_click(cx.listener(|this, _, _, cx| this.yt_close(cx))),
                )
                .child(
                    button_with_icon(
                        t,
                        "yt-reconnect",
                        tr_ctx!("button", "Reconnect\u{2026}"),
                        ButtonKind::Secondary,
                        Some("plug"),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.yt_open_setup(Some(&reconnect_id), cx)
                    })),
                )
                .child(div().flex_1())
                .child(
                    button_with_icon(
                        t,
                        "yt-remove",
                        tr_ctx!("button", "Disconnect\u{2026}"),
                        ButtonKind::Ghost,
                        Some("unplug"),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.yt.removing = true;
                        cx.notify();
                    })),
                )
        };
        div()
            .id("yt-manage")
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .text_size(px(type_scale::BODY))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(tr!(
                        "YouTube \u{b7} {name}",
                        name = a.name
                    ))),
            )
            .children(reconnect.map(|why| {
                alert_line(
                    t,
                    "yt-reconnect-note",
                    tr!("{reason} Use Reconnect to sign in again.", reason = why),
                )
            }))
            .children(several.then(|| {
                field_block(
                    t,
                    &tr!("Default channel"),
                    Some(tr!(
                        "The publish sheet starts with the default channel; it can switch to another."
                    )),
                    if is_default {
                        div()
                            .flex()
                            .child(pill(
                                t,
                                Tone::Brass,
                                Some("star"),
                                tr_ctx!("channel", "Default"),
                            ))
                            .into_any_element()
                    } else {
                        div()
                            .flex()
                            .child(
                                button_with_icon(
                                    t,
                                    "yt-make-default",
                                    tr!("Make this the default channel"),
                                    ButtonKind::Secondary,
                                    Some("star"),
                                )
                                .on_click(cx.listener(|this, _, _, cx| this.yt_make_default(cx))),
                            )
                            .into_any_element()
                    },
                )
            }))
            .child(field_block(
                t,
                &tr!("Default visibility"),
                Some(tr!(
                    "The publish sheet starts with this. Public always needs one more confirmation there."
                )),
                div().flex().child(
                    segmented(t, "yt-privacy-choices", &tr!("Default visibility"))
                        .children(privacies),
                ),
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        button_with_icon(
                            t,
                            "yt-test",
                            if self.yt.testing {
                                tr!("Testing\u{2026}")
                            } else {
                                tr!("Test connection")
                            },
                            ButtonKind::Secondary,
                            Some("plug"),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.yt_test(cx))),
                    )
                    .children(test_note),
            )
            .children(self.yt_error_line(t, self.yt.error.as_ref().map(Problem::text), "yt-error"))
            .child(buttons)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_state_prefers_reconnect_over_connected() {
        assert_eq!(card_state(0, 0), CardState::NotConnected);
        assert_eq!(card_state(1, 0), CardState::Connected(1));
        assert_eq!(card_state(3, 0), CardState::Connected(3));
        assert_eq!(
            card_state(3, 1),
            CardState::Reconnect {
                total: 3,
                needing: 1
            }
        );
    }

    #[test]
    fn card_labels_say_the_state_in_words() {
        assert_eq!(
            card_label("YouTube", CardState::NotConnected),
            "YouTube, not connected."
        );
        assert_eq!(
            card_label("YouTube", CardState::Connected(2)),
            "YouTube, 2 connected."
        );
        assert_eq!(
            card_label(
                "YouTube",
                CardState::Reconnect {
                    total: 2,
                    needing: 1
                }
            ),
            "YouTube, 2 connected, 1 need to reconnect."
        );
    }

    #[test]
    fn platform_tiles_use_the_boards_letters() {
        assert_eq!(platform_letters("YouTube"), "YT");
        assert_eq!(platform_letters("TikTok"), "TT");
        assert_eq!(platform_letters("Facebook"), "FB");
        assert_eq!(platform_letters("vimeo"), "VI");
    }
}
