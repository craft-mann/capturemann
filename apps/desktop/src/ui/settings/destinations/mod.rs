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

mod connect;
mod manage;
mod page;
mod panel;
mod pieces;
mod setup_view;

use pieces::*;
pub(super) use pieces::{alert_line, field_block, outcome_line, step_list};
pub(super) use setup_view::connect_error_text;
use setup_view::testing_note;

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
