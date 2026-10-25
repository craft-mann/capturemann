//! The permission cards shared by Settings -> Permissions and the setup screen (PRM-1): one kit
//! card per permission with its status pill, why it is needed, which features use it and the
//! buttons that fix it. All state comes from [`crate::permissions`]. [`PermissionsScreen`] is the
//! setup screen: a route of the main window (PRM-1.AC12), not a separate window.

use std::time::Duration;

use gpui::{
    div, prelude::*, px, AnyElement, Context, Entity, FontWeight, Render, Role, SharedString, Task,
    Window,
};

use cm_i18n::{tr, tr_ctx};

use crate::permissions::{self, Action, Id, Snapshot, State};
use crate::theme::{scale, type_scale, Color, Theme};
use crate::ui::app::{self, AppModel, Route};
use crate::ui::components::{button, button_with_icon, card, icon, shrink_text, ButtonKind};
use crate::ui::kit::{pill, status, tone_colors, StatusKind, Tone};
use crate::ui::paint::rgba_of;
use crate::ui::studio::ThemeGlobal;

/// The pill color of a state: green when it works, amber when the user has something to do.
pub fn state_color(t: &Theme, state: State) -> Color {
    match state {
        State::Granted => t.success,
        State::GrantedNeedsRestart | State::NotGranted | State::Denied | State::Restricted => {
            t.warning
        }
        State::NotDetermined => t.text_3,
        State::Unknown => t.mist,
    }
}

/// Extra sentence under a card for the states that need one.
pub fn state_note(id: Id, state: State) -> Option<String> {
    // TRANSLATORS: FACT: owner review required (every permission note states what macOS does)
    match (id, state) {
        (Id::ScreenRecording, State::NotGranted) => Some(
            tr!("macOS shows its own dialog only once. If you already answered it, or the grant was dropped after the app was rebuilt, switch CaptureMann off and on in System Settings. After granting, quit and reopen CaptureMann."),
        ),
        (Id::ScreenRecording, State::GrantedNeedsRestart) => Some(
            tr!("macOS usually keeps refusing capture to an app that was already running when you granted this. CaptureMann can\u{2019}t check whether yours does, so restart it to be sure."),
        ),
        (_, State::Restricted) => {
            Some(tr!("Your organization or Screen Time blocks this. Only an administrator can allow it."))
        }
        (Id::Microphone | Id::Camera, State::Denied) => {
            Some(tr!("You turned this off. Turn it on in System Settings; recording works without it."))
        }
        (Id::InputMonitoring, State::NotGranted) => Some(
            tr!("Optional: without it recordings keep the cursor position but not clicks or scroll. After granting, quit and reopen CaptureMann."),
        ),
        _ => None,
    }
}

/// The tile icon of a permission card.
pub fn permission_icon(id: Id) -> &'static str {
    match id {
        Id::ScreenRecording => "monitor",
        Id::Microphone => "mic",
        Id::Camera => "camera",
        Id::InputMonitoring => "keyboard",
    }
}

/// The status a state shows (tone and glyph come from `kit::status_parts`).
pub fn state_status(state: State) -> StatusKind {
    match state {
        State::Granted => StatusKind::Ok,
        State::GrantedNeedsRestart | State::NotGranted | State::Denied | State::Restricted => {
            StatusKind::Warning
        }
        State::NotDetermined => StatusKind::Neutral,
        State::Unknown => StatusKind::Pending,
    }
}

/// The user has something to do about this state: the card is tinted and bordered amber.
pub fn needs_attention(state: State) -> bool {
    state_status(state) == StatusKind::Warning
}

/// "N of M granted" over every listed permission (restart-required is not yet plainly granted).
pub fn granted_count(snap: &Snapshot) -> (usize, usize) {
    let granted = snap
        .rows
        .iter()
        .filter(|(_, s)| *s == State::Granted)
        .count();
    (granted, snap.rows.len())
}

/// The sentence after "N of M granted." on the Settings page, from the real states.
pub fn summary_detail(snap: &Snapshot) -> String {
    if !snap.checked {
        tr!("Checking\u{2026}")
    } else if snap.needs_restart() {
        tr!("Screen Recording was granted. Restart CaptureMann to start using it.")
    } else if snap.all_granted() {
        tr!("Everything CaptureMann can use is allowed.")
    } else if snap.can_continue() {
        tr!("Required permissions are in place. Optional ones can be granted any time.")
    } else {
        tr!("Screen Recording is required for screenshots and recording. The others are optional.")
    }
}

/// The summary bar of Settings -> Permissions: one segment per permission (green when granted,
/// amber when the user has something to do, grey otherwise), then "N of M granted." and the detail.
pub fn summary_bar(t: &Theme, snap: &Snapshot) -> AnyElement {
    let (granted, total) = granted_count(snap);
    let text = tr!(
        "{granted} of {total} granted.",
        granted = granted,
        total = total
    );
    card(t)
        .id("permissions-summary")
        .role(Role::Status)
        .aria_label(SharedString::from(format!(
            "{text} {}",
            summary_detail(snap)
        ))) // i18n-ignore: two translated sentences joined by a space
        .py(px(14.))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(16.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .children(snap.rows.iter().map(|(_, s)| {
                    let c = match s {
                        State::Granted => t.success,
                        s if needs_attention(*s) => t.warning,
                        _ => t.line,
                    };
                    div().w(px(34.)).h(px(5.)).rounded(px(3.)).bg(rgba_of(c))
                })),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(220.))
                .text_size(px(type_scale::UI))
                .text_color(rgba_of(t.text_3))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgba_of(t.text))
                                .child(SharedString::from(text)),
                        )
                        .child(shrink_text(summary_detail(snap))),
                ),
        )
        .into_any_element()
}

/// The card for one permission: a tile, the name with its Required or Optional pill and state
/// pill, why it is needed, which features use it, and (when there is something to do) a tinted
/// box with the note and the buttons that fix it. Shared with the setup screen.
pub fn permission_card(
    t: &Theme,
    app: &Entity<AppModel>,
    spec: &permissions::Spec,
    state: State,
    busy: bool,
    os: permissions::Os,
    tried: bool,
) -> AnyElement {
    let buttons = permissions::actions(os, spec.id, state);
    let key = spec.id.key();
    let note = state_note(spec.id, state);
    let stale_hint = permissions::stale_grant_hint(os, spec.id, state, tried).then(|| {
        tr!("If the switch for CaptureMann already looks on in System Settings, select it, press the minus (\u{2212}) button to remove it, then grant access again.")
    });
    let attention = needs_attention(state);
    let amber = tone_colors(t, Tone::Brass);
    let actions_row = (!buttons.is_empty()).then(|| {
        div()
            .flex()
            .flex_wrap()
            .gap(px(8.))
            .children(buttons.into_iter().enumerate().map(|(i, action)| {
                let kind = if i == 0 {
                    ButtonKind::Primary
                } else {
                    ButtonKind::Secondary
                };
                let waiting = busy && action == Action::Grant;
                let label = if waiting {
                    tr!("Waiting for the system\u{2026}")
                } else {
                    action.label(os)
                };
                let leading = match action {
                    Action::Grant => "shield-check",
                    Action::OpenSettings => "external-link",
                    Action::Restart => "rotate-cw",
                };
                let id = spec.id;
                let app = app.clone();
                let b = button_with_icon(
                    t,
                    SharedString::from(format!("permission-{key}-{}", action_key(action))),
                    label,
                    kind,
                    Some(leading),
                );
                if waiting {
                    b.opacity(0.55).cursor_default()
                } else {
                    b.on_click(move |_, _, cx| app::permission_action(&app, id, action, cx))
                }
            }))
    });
    let fix_box = (note.is_some() || stale_hint.is_some() || actions_row.is_some()).then(|| {
        div()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(if attention { amber.border } else { t.border }))
            .bg(rgba_of(if attention { amber.bg } else { t.surface_2 }))
            .children(note.map(|n| {
                div()
                    .flex()
                    .gap(px(8.))
                    .items_start()
                    .children(attention.then(|| icon("triangle-alert", 14., t.warning)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(if attention { t.warning } else { t.text_2 }))
                            .child(n),
                    )
            }))
            .children(stale_hint.map(|h| {
                div()
                    .id(SharedString::from(format!("permission-{key}-stale-hint")))
                    .min_w(px(0.))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.text_2))
                    .child(SharedString::from(h))
            }))
            .children(actions_row)
    });
    card(t)
        .id(SharedString::from(format!("permission-{key}")))
        .role(Role::Group)
        .aria_label(SharedString::from(if spec.required {
            tr!(
                "{name}, required, {state}",
                name = spec.name,
                state = state.label()
            )
        } else {
            tr!(
                "{name}, optional, {state}",
                name = spec.name,
                state = state.label()
            )
        }))
        .p(px(20.))
        .flex()
        .gap(px(16.))
        .items_start()
        .when(attention, |d| d.border_color(rgba_of(amber.border)))
        .child(
            div()
                .size(px(40.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(scale::RADIUS_MD))
                .bg(rgba_of(t.icon_tile()))
                .child(icon(permission_icon(spec.id), 20., t.text_2)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .mr_auto()
                                .text_size(px(type_scale::TITLE))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(SharedString::from(spec.name.clone())),
                        )
                        .child(pill(
                            t,
                            Tone::Neutral,
                            None,
                            if spec.required {
                                tr_ctx!("permission", "Required")
                            } else {
                                tr_ctx!("permission", "Optional")
                            },
                        ))
                        .child(
                            div()
                                .id(SharedString::from(format!("permission-state-{key}")))
                                .role(Role::Status)
                                .aria_label(SharedString::from(state.label()))
                                .child(status(t, state_status(state), state.label())),
                        ),
                )
                .child(
                    div()
                        .text_size(px(type_scale::UI))
                        .text_color(rgba_of(t.text_2))
                        .child(SharedString::from(spec.why.clone())),
                )
                .child(
                    div()
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.mist))
                        .child(SharedString::from(tr!(
                            "Needed for: {features}",
                            features = spec.features
                        ))),
                )
                .children(fix_box),
        )
        .into_any_element()
}

fn action_key(a: Action) -> &'static str {
    match a {
        Action::Grant => "grant",
        Action::OpenSettings => "open-settings",
        Action::Restart => "restart",
    }
}

/// All cards of `snap`, required first, plus the Windows note.
pub fn cards(
    t: &Theme,
    app: &Entity<AppModel>,
    snap: &Snapshot,
    busy: Option<Id>,
    tried: &std::collections::HashSet<Id>,
) -> AnyElement {
    let specs = permissions::specs(snap.os);
    div()
        .flex()
        .flex_col()
        .gap(px(12.))
        .children(specs.iter().map(|s| {
            permission_card(
                t,
                app,
                s,
                snap.state(s.id),
                busy == Some(s.id),
                snap.os,
                tried.contains(&s.id),
            )
        }))
        .when(snap.os == permissions::Os::Windows, |d| {
            d.child(
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(permissions::windows_screen_note()),
            )
        })
        .into_any_element()
}

/// The setup screen in the main window's content area: header, the cards, and a footer with
/// Not now / Skip optional / Continue (or Restart CaptureMann). Refreshes every second while it is
/// showing, and moves on to the library by itself once everything is granted (PRM-1.AC11).
pub struct PermissionsScreen {
    app: Entity<AppModel>,
    /// Everything was granted: "All set" shows for a second, then the library.
    all_set: bool,
    _poll: Task<()>,
}

impl PermissionsScreen {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        let app2 = app.clone();
        // Live refresh: the user switches CaptureMann on in System Settings and comes back.
        let poll = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let Ok(showing) = this.update(cx, |_, cx| app2.read(cx).route == Route::Permissions)
            else {
                break;
            };
            if !showing {
                let _ = this.update(cx, |this, _| this.all_set = false);
                continue;
            }
            cx.update(|cx| app::refresh_permissions(&app2, cx));
            if !cx.update(|cx| app2.read(cx).perms.all_granted()) {
                let _ = this.update(cx, |this, cx| {
                    if this.all_set {
                        this.all_set = false;
                        cx.notify();
                    }
                });
                continue;
            }
            let _ = this.update(cx, |this, cx| {
                this.all_set = true;
                cx.notify();
            });
            cx.update(|cx| app::finish_first_run(&app2, cx));
            cx.background_executor().timer(Duration::from_secs(1)).await;
            cx.update(|cx| {
                if app2.read(cx).route == Route::Permissions {
                    app::set_route(&app2, Route::Library, cx);
                }
            });
        });
        PermissionsScreen {
            app,
            all_set: false,
            _poll: poll,
        }
    }
}

impl Render for PermissionsScreen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        let (snap, busy, tried) = {
            let m = self.app.read(cx);
            (m.perms.clone(), m.perm_busy, m.perm_tried.clone())
        };
        let can_continue = snap.can_continue();
        let restart = snap.needs_restart();
        let skip_optional = permissions::can_skip_optional(&snap);
        let hint = setup_hint(&snap, self.all_set);
        let (app_b, app_c, app_d, app_e) = (
            self.app.clone(),
            self.app.clone(),
            self.app.clone(),
            self.app.clone(),
        );
        let footer = div()
            .id("permissions-footer")
            .flex_none()
            .px(px(28.))
            .py(px(16.))
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .border_t_1()
            .border_color(rgba_of(t.line))
            .child(
                div()
                    .flex_1()
                    .min_w(px(220.))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.text_2))
                    .child(hint),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        button(
                            &t,
                            "permissions-not-now",
                            tr_ctx!("permissions setup", "Not now"),
                            ButtonKind::Ghost,
                        )
                        .on_click(move |_, _, cx| app::set_route(&app_e, Route::Library, cx)),
                    )
                    .children((skip_optional && !self.all_set).then(|| {
                        button(
                            &t,
                            "permissions-skip-optional",
                            tr_ctx!("permissions setup", "Skip optional"),
                            ButtonKind::Ghost,
                        )
                        .on_click(move |_, _, cx| {
                            app::finish_first_run(&app_d, cx);
                            app::set_route(&app_d, Route::Library, cx);
                        })
                    }))
                    .child(if self.all_set {
                        button(
                            &t,
                            "permissions-all-set",
                            tr_ctx!("permissions setup", "All set"),
                            ButtonKind::Primary,
                        )
                        .cursor_default()
                    } else if restart {
                        button(
                            &t,
                            "permissions-restart",
                            tr!("Restart CaptureMann"),
                            ButtonKind::Primary,
                        )
                        .on_click(move |_, _, cx| app::restart(&app_b, cx))
                    } else {
                        let b = button(
                            &t,
                            "permissions-continue",
                            tr_ctx!("permissions setup", "Continue"),
                            ButtonKind::Primary,
                        );
                        if can_continue {
                            b.on_click(move |_, _, cx| {
                                app::finish_first_run(&app_c, cx);
                                app::set_route(&app_c, Route::Library, cx);
                            })
                        } else {
                            b.opacity(0.45).cursor_default()
                        }
                    }),
            );
        let body = div()
            .id("permissions-body")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .px(px(28.))
            .pt(px(28.))
            .pb(px(20.))
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(
                div()
                    .max_w(px(720.))
                    .flex()
                    .flex_col()
                    .gap(px(18.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(type_scale::H1))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(tr!("CaptureMann needs a few permissions")),
                            )
                            .child(
                                div()
                                    .text_size(px(type_scale::UI))
                                    .text_color(rgba_of(t.text_2))
                                    .child(
                                        // TRANSLATORS: FACT: owner review required (nothing is requested until a button is pressed)
                                        tr!("macOS keeps these under your control. Nothing is requested until you press a button; you can change any of them later in Settings \u{2192} Permissions."),
                                    ),
                            ),
                    )
                    .child(cards(&t, &self.app, &snap, busy, &tried)),
            );
        div()
            .id("permissions-screen")
            .role(Role::Group)
            .aria_label(SharedString::from(tr!("CaptureMann permissions setup")))
            .size_full()
            .flex()
            .flex_col()
            .child(body)
            .child(footer)
    }
}

/// The footer line of the setup screen.
pub fn setup_hint(snap: &Snapshot, all_set: bool) -> String {
    if all_set {
        tr!("All set. Every permission is granted.")
    } else if snap.needs_restart() {
        tr!("Screen Recording was granted. Restart CaptureMann to start using it.")
    } else if snap.can_continue() {
        tr!("Required permissions are in place. Optional ones can be granted any time from Settings \u{2192} Permissions.")
    } else if snap.state(Id::ScreenRecording) == State::Unknown {
        tr!("Checking\u{2026}")
    } else {
        tr!("Screen Recording is required for screenshots and recording. The others are optional.")
    }
}
