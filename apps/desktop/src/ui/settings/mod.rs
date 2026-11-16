//! Settings (DESIGN 5.8, kit `Settings`): one page per section. The section list lives in the
//! Studio sidebar's settings mode (OD-16; no inner nav). Destinations and Agents & CLI have their
//! own routes that show the same view with that page selected.

use gpui::{
    div, prelude::*, px, AnyElement, ClipboardItem, Context, Entity, FocusHandle, KeyDownEvent,
    Render, SharedString, Task, Window,
};
use std::time::Duration;

use cm_i18n::{tr, tr_ctx};

use crate::theme::Theme;
use crate::ui::app::{self, AppModel, Route};
use crate::ui::studio::ThemeGlobal;
use crate::ui::text_field::{TextField, TextFieldEvent};

pub mod agents;
mod audio_check;
pub(crate) mod audio_check_model;
mod check_widgets;
pub mod destinations;
mod diagnostics;
mod general;
mod library_folder;
pub mod model;
mod privacy;
mod problem;
mod publish_card;
mod recording;
pub(crate) mod reset;
mod rows;
mod shortcuts_page;
pub(crate) mod webcam_model;
mod webcam_view;
#[cfg(test)]
mod webcam_width_tests;
pub(crate) mod widgets;

use model::SettingsSection;

/// Why a change on the Agents page did not take (kept as a reason; worded when drawn).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PolicyProblem {
    /// The policy applies for this run but could not be saved (the OS text).
    NotSaved(String),
    /// The typed client name was refused.
    Name(crate::settings::ClientNameError),
}

impl PolicyProblem {
    pub(crate) fn text(&self) -> String {
        match self {
            PolicyProblem::NotSaved(reason) => tr!(
                "Applied for now, but it could not be saved: {reason}",
                reason = reason
            ),
            PolicyProblem::Name(e) => e.text(),
        }
    }
}
use widgets::page_header_tile;

pub struct SettingsView {
    pub(super) app: Entity<AppModel>,
    /// The page shown under `Route::Settings` (Destinations and Agents have their own routes).
    section: SettingsSection,
    /// The Copy button shows "Copied" briefly.
    pub(super) copied: bool,
    copied_timer: Option<Task<()>>,
    /// Agents page: "Add client" name field, the policy chosen for it, and the last error
    /// (invalid name, or the policies could not be saved).
    pub(super) client_input: Entity<TextField>,
    pub(super) new_policy: cm_daemon::ClientPolicy,
    pub(super) policy_error: Option<PolicyProblem>,
    /// The i18n epoch the two fields' labels were written for (see `refresh_field_labels`).
    labels_epoch: u64,
    /// Privacy: "add an excluded app" bundle id field and its last error (REC-6).
    pub(super) exclude_input: Entity<TextField>,
    pub(super) exclude_error: Option<crate::recording::start::BundleIdError>,
    /// Shortcuts page: the action waiting for a key press (CAP-2.AC2) and the last refusal.
    rebinding: Option<crate::shortcuts::ShortcutAction>,
    /// General page: why the login item could not be changed.
    pub(super) login_error: Option<String>,
    /// General page: the library's size and free space, measured off the UI thread (LIB-2).
    pub(super) lib_size: std::cell::RefCell<crate::library_move::SizeCache>,
    rebind_error: Option<app::RebindError>,
    rebind_focus: FocusHandle,
    /// Ends the recording when the page loses focus (the global shortcuts must not stay released).
    _rebind_blur: Option<gpui::Subscription>,
    _perm_poll: Task<()>,
    /// What the Clients list showed at the last poll (AGT-2.AC15).
    clients_sig: (u64, usize),
    /// Agents page: scroll state of the Permissions list (it scrolls inside the page).
    pub(super) caps_scroll: gpui::ScrollHandle,
    /// Destinations page: the stored "Save to" folders and the open Manage panel (STO-1).
    pub(super) dest: destinations::DestState,
    /// Destinations page: the YouTube card of "Publish to" and its panel (PUB-1).
    pub(super) yt: publish_card::YouTubeCard,
    /// Webcam & audio page: the device lists, the open dropdown and the live preview (R13).
    pub(super) webcam: webcam_view::WebcamState,
    /// Webcam & audio page: the microphone and system audio checks (R13b).
    pub(super) audio: audio_check::AudioCheckState,
    /// About page: the Diagnostics card (DIAG-1) and the task that reads it.
    pub(super) diag: diagnostics::DiagState,
    _diag_task: Option<Task<()>>,
}

impl SettingsView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        // A route or permission change can end the live camera preview at once (R13).
        cx.observe(&app, |this, _, cx| {
            this.webcam_sync(cx);
            cx.notify()
        })
        .detach();
        // A camera plugged in or out refreshes the Camera dropdown (the Record sheet's epoch).
        let rec = app.read(cx).rec.clone();
        cx.observe(&rec, |this, _, cx| {
            this.webcam_sync(cx);
            cx.notify()
        })
        .detach();
        let client_input = cx.new(|cx| {
            TextField::new(
                tr_ctx!("field", "Client name"),
                tr!("Client name, e.g. claude-desktop"),
                cx,
            )
            .without_icon()
        });
        cx.subscribe(&client_input, |this, _, ev: &TextFieldEvent, cx| match ev {
            TextFieldEvent::Submitted => this.add_client(cx),
            TextFieldEvent::Changed(_) => {
                this.policy_error = None;
                cx.notify();
            }
            TextFieldEvent::Escaped => {}
        })
        .detach();
        let exclude_input = cx.new(|cx| {
            TextField::new(
                tr_ctx!("field", "App bundle id"),
                tr!("Bundle id, e.g. com.tinyspeck.slackmacgap"),
                cx,
            )
            .without_icon()
        });
        cx.subscribe(
            &exclude_input,
            |this, _, ev: &TextFieldEvent, cx| match ev {
                TextFieldEvent::Submitted => this.add_exclusion(cx),
                TextFieldEvent::Changed(_) => {
                    this.exclude_error = None;
                    cx.notify();
                }
                TextFieldEvent::Escaped => {}
            },
        )
        .detach();
        // PRM-1.AC2: while the Permissions page is showing, the states follow System Settings.
        let app2 = app.clone();
        let poll = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let Ok((on_page, on_agents)) = this.update(cx, |this, cx| {
                let shown = this.app.read(cx).route == Route::Settings;
                (
                    this.section == SettingsSection::Permissions && shown,
                    this.section == SettingsSection::Agents && shown,
                )
            }) else {
                break;
            };
            if on_page {
                cx.update(|cx| app::refresh_permissions(&app2, cx));
            }
            // AGT-2.AC15: a client that connects (or calls a tool) shows up without leaving
            // and re-entering the page. Redraw only when the list changed.
            if on_agents {
                let _ = this.update(cx, |this, cx| {
                    let sig = this
                        .app
                        .read(cx)
                        .backend
                        .daemon
                        .as_ref()
                        .map(|d| {
                            (
                                d.known_clients().iter().map(|k| k.calls + 1).sum::<u64>(),
                                d.activity().len(),
                            )
                        })
                        .unwrap_or_default();
                    if sig != this.clients_sig {
                        this.clients_sig = sig;
                        cx.notify();
                    }
                });
            }
        });
        let dest = destinations::DestState::new(cx);
        let yt = publish_card::YouTubeCard::new(cx);
        let mut view = SettingsView {
            dest,
            yt,
            webcam: webcam_view::WebcamState::new(),
            audio: audio_check::AudioCheckState::new(),
            diag: diagnostics::DiagState::default(),
            _diag_task: None,
            rebinding: None,
            login_error: None,
            lib_size: Default::default(),
            rebind_error: None,
            rebind_focus: cx.focus_handle(),
            _rebind_blur: None,
            _perm_poll: poll,
            clients_sig: (0, 0),
            caps_scroll: gpui::ScrollHandle::new(),
            labels_epoch: cm_i18n::epoch(),
            exclude_input,
            exclude_error: None,
            client_input,
            // AGT-2: unknown clients are Allowed by default, so the form starts there; a person
            // who types a name usually means to let it in (blocking is one click away).
            new_policy: cm_daemon::ClientPolicy::Allowed,
            policy_error: None,
            app,
            section: SettingsSection::General,
            copied: false,
            copied_timer: None,
        };
        view.yt_reload(cx);
        // Closing Settings while a combination is recorded gives the shortcuts back.
        let app_release = view.app.clone();
        cx.on_release(move |_, cx| app::resume_shortcuts(&app_release, cx))
            .detach();
        // A copy that was running when the app quit resumes by itself; the row shows it again.
        let app3 = view.app.clone();
        crate::ui::library_move_flow::adopt_running(&app3, cx);
        view
    }

    /// The text fields keep their label and hint; after a language switch they are rewritten once.
    fn refresh_field_labels(&mut self, cx: &mut Context<Self>) {
        let epoch = cm_i18n::epoch();
        if epoch == self.labels_epoch {
            return;
        }
        self.labels_epoch = epoch;
        self.client_input.update(cx, |f, cx| {
            f.set_labels(
                tr_ctx!("field", "Client name"),
                tr!("Client name, e.g. claude-desktop"),
                cx,
            )
        });
        self.exclude_input.update(cx, |f, cx| {
            f.set_labels(
                tr_ctx!("field", "App bundle id"),
                tr!("Bundle id, e.g. com.tinyspeck.slackmacgap"),
                cx,
            )
        });
    }

    /// Puts `text` on the system clipboard and flashes "Copied" for 1.5 s.
    pub(super) fn copy_text(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied = true;
        cx.notify();
        self.copied_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1500))
                .await;
            this.update(cx, |this, cx| {
                this.copied = false;
                cx.notify();
            })
            .ok();
        }));
    }

    /// Takes a pending "open this page" request (Open Permissions… from the banner, a toast, the
    /// tray or the launcher). Called by the Studio before it draws the settings sidebar, so the
    /// highlighted section and the page agree.
    pub fn sync_request(&mut self, cx: &mut Context<Self>) {
        if self.app.read(cx).route == Route::Settings {
            if let Some(page) = self.app.update(cx, |m, _| m.settings_page_request.take()) {
                self.section = page;
            }
        }
    }

    /// The section the page shows for `route` (the sidebar highlights it).
    pub fn current_section(&self, route: &Route) -> SettingsSection {
        SettingsSection::for_route(route, self.section).unwrap_or(self.section)
    }

    /// The sidebar picked a section: Destinations has its own route, every other page lives under
    /// `Route::Settings`.
    pub fn select(&mut self, s: SettingsSection, cx: &mut Context<Self>) {
        if !matches!(s, SettingsSection::Destinations | SettingsSection::Agents) {
            self.section = s;
        }
        self.end_rebind(cx);
        cx.notify();
    }

    /// Settings -> Shortcuts: "Change" was chosen. The global shortcuts are released while the page
    /// waits, so a press of a registered combination reaches it (CAP-2.AC2).
    pub(super) fn start_rebind(
        &mut self,
        action: crate::shortcuts::ShortcutAction,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.rebinding = Some(action);
        self.rebind_error = None;
        window.focus(&self.rebind_focus, cx);
        app::suspend_shortcuts(&self.app.clone(), cx);
        self._rebind_blur = Some(cx.on_blur(&self.rebind_focus, window, |this, _, cx| {
            this.end_rebind(cx);
            cx.notify();
        }));
    }

    /// Recording ended (Cancel, Esc, a key taken, focus left): the shortcuts are registered again.
    pub(super) fn end_rebind(&mut self, cx: &mut Context<Self>) {
        self.rebinding = None;
        self.rebind_error = None;
        self._rebind_blur = None;
        app::resume_shortcuts(&self.app.clone(), cx);
    }

    /// A key press while a shortcut waits to be recorded (CAP-2.AC2): Esc cancels, anything else
    /// becomes the new chord if it is valid, free and registrable.
    fn rebind_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(action) = self.rebinding else {
            return;
        };
        let k = &ev.keystroke;
        if k.key == "escape" {
            self.end_rebind(cx);
            cx.notify();
            return;
        }
        let m = k.modifiers;
        // The OS registration is moved (and refused when another app owns the new combination)
        // with the shortcuts registered, so release is undone for the attempt.
        app::resume_shortcuts(&self.app.clone(), cx);
        let result =
            crate::shortcuts::Chord::from_pressed(m.control, m.alt, m.shift, m.platform, &k.key)
                .map_err(|e| match e {
                    crate::shortcuts::ChordError::NoModifier => app::RebindError::NeedsModifier,
                    _ => app::RebindError::BadKey,
                })
                .and_then(|chord| {
                    let app = self.app.clone();
                    app::rebind_shortcut(&app, action, chord, cx)
                });
        match result {
            Ok(()) => self.end_rebind(cx),
            Err(e) => {
                // Still waiting for another try: release the shortcuts again.
                self.rebind_error = Some(e);
                app::suspend_shortcuts(&self.app.clone(), cx);
            }
        }
        cx.notify();
    }

    /// Settings -> Permissions (PRM-1): every permission with its live state and fix.
    fn permissions(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let (snap, busy, tried) = {
            let m = self.app.read(cx);
            (m.perms.clone(), m.perm_busy, m.perm_tried.clone())
        };
        let app = self.app.clone();
        let app_reopen = self.app.clone();
        div()
            .id("permissions-page")
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(
                        widgets::small_button_with_icon(
                            t,
                            "permissions-reopen",
                            &tr!("Reopen setup"),
                            Some("list-checks"),
                        )
                        .aria_label(SharedString::from(tr!("Show the permissions setup screen again")))
                        .on_click(move |_, _, cx| {
                            app::set_route(&app_reopen, Route::Permissions, cx)
                        }),
                    )
                    .child(
                        widgets::small_button_with_icon(
                            t,
                            "permissions-refresh",
                            &tr!("Check again"),
                            Some("refresh-cw"),
                        )
                        .aria_label(SharedString::from(tr!("Check the permission states again")))
                        .on_click(move |_, _, cx| app::refresh_permissions(&app, cx)),
                    ),
            )
            .child(crate::ui::permissions_view::summary_bar(t, &snap))
            .children(
                (crate::permissions::Os::current() == crate::permissions::Os::Linux
                    && self.webcam.devices.loaded)
                    .then(|| {
                        widgets::caption(
                            t,
                            crate::permissions::linux_screen_note(self.webcam.devices.share_dialog),
                        )
                        .id("permissions-linux-screen-note")
                    }),
            )
            .child(crate::ui::permissions_view::cards(t, &self.app, &snap, busy, &tried))
            .child(widgets::caption(
                t,
                if crate::permissions::Os::current() == crate::permissions::Os::Linux {
                    tr!("States are read without asking the system.")
                } else {
                    tr!("States are read without asking the system. Granting only happens when you press \u{201C}Grant access\u{2026}\u{201D}.")
                },
            ))
            .into_any_element()
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        let route = self.app.read(cx).route.clone();
        self.sync_request(cx);
        self.webcam_watch_window(window, cx);
        self.refresh_field_labels(cx);
        let current = self.current_section(&route);
        self.webcam_sync(cx);
        if current == SettingsSection::Webcam
            || (current == SettingsSection::Permissions
                && crate::permissions::Os::current() == crate::permissions::Os::Linux)
        {
            // The Linux Permissions note reads the screen report of the same device list.
            self.webcam_ensure_devices(cx);
        }
        let header = current;
        let body = match current {
            SettingsSection::General => self.general(&t, cx),
            SettingsSection::About => self.about(&t, cx),
            SettingsSection::Capture => self.capture(&t, cx),
            SettingsSection::Webcam => self.webcam(&t, cx),
            SettingsSection::Destinations => self.destinations(&t, cx),
            SettingsSection::Agents => self.agents(&t, cx),
            SettingsSection::Permissions => self.permissions(&t, cx),
            SettingsSection::Privacy => self.privacy(&t, cx),
            SettingsSection::Shortcuts => self.shortcuts(&t, cx),
        };
        // The MCP pill lives in the Agents hero now; no other page has a header action.
        let right: Option<gpui::AnyElement> =
            (current == SettingsSection::Shortcuts).then(|| self.shortcuts_reset_control(&t, cx));
        div().id("settings-page").size_full().flex().child(
            div()
                .id("settings-scroll")
                .flex_1()
                .min_w(px(0.))
                .h_full()
                .overflow_y_scroll()
                .child(
                    div()
                        .max_w(px(match current {
                            SettingsSection::Agents => 1280.,
                            SettingsSection::Webcam => 1060.,
                            _ => 900.,
                        }))
                        .px(px(40.))
                        .pt(px(32.))
                        .pb(px(48.))
                        .flex()
                        .flex_col()
                        .gap(px(24.))
                        // Agents & CLI draws its own hero (title, subtitle and the MCP pill).
                        .children((current != SettingsSection::Agents).then(|| {
                            let subtitle = if current == SettingsSection::Shortcuts {
                                model::shortcuts_subtitle(self.shortcuts_none_registered(cx))
                            } else {
                                header.subtitle()
                            };
                            page_header_tile(&t, header.icon(), &header.title(), &subtitle, right)
                        }))
                        .child(body),
                ),
        )
    }
}
