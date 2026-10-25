//! Agents & CLI page (kit `Settings`, AGT-1, AGT-2): MCP status, the client config with the
//! real `capturemann mcp` command and daemon socket, the permission list from the daemon's `ACTIONS`
//! table, per-client policies and recent gated requests.

use cm_daemon::{ActivityDecision, ClientPolicy};
use cm_i18n::{tr, tr_ctx};
use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, Role, SharedString};

use super::model::SettingsSection;
use super::model::{
    self, capability_rows, client_config, client_rows, endpoint_arg, locate_mcp, policy_hint,
    policy_label, PermissionLabel, POLICIES,
};
use super::widgets::{
    caption, card_row, list_card, segment_with_icon, segmented, small_button,
    small_button_with_icon,
};
use super::SettingsView;
use crate::agent_status::decision_label;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::components::{
    button_with_icon, card, dot, icon, label_fill, shrink_text, ButtonKind,
};
use crate::ui::kit::{self, Tone};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::studio::agent_status_of;

const MAX_RECENT: usize = 10;

/// The stable element id part of a client policy (never translated).
fn policy_id(p: ClientPolicy) -> &'static str {
    match p {
        ClientPolicy::Allowed => "Allowed",
        ClientPolicy::ReadOnly => "Read-only",
        ClientPolicy::Blocked => "Blocked",
    }
}

/// The icon of a client policy segment (Allowed, Read-only, Blocked).
pub fn policy_icon(p: ClientPolicy) -> &'static str {
    match p {
        ClientPolicy::Allowed => "circle-check",
        ClientPolicy::ReadOnly => "eye",
        ClientPolicy::Blocked => "ban",
    }
}

/// The pill of a permission tier: Allow is success with a check, Ask first is brass with a hand.
pub fn tier_pill_parts(l: PermissionLabel) -> (Tone, &'static str) {
    match l {
        PermissionLabel::Allow => (Tone::Success, "circle-check"),
        PermissionLabel::AskFirst => (Tone::Brass, "hand"),
    }
}

/// The tone and icon of a recent request's decision pill. Red is not used: a refusal is brass.
pub fn decision_pill_parts(d: ActivityDecision) -> (Tone, &'static str) {
    match d {
        ActivityDecision::Approved | ActivityDecision::ApprovedForSession => {
            (Tone::Success, "circle-check")
        }
        ActivityDecision::Declined => (Tone::Neutral, "x"),
        ActivityDecision::TimedOut => (Tone::Neutral, "clock"),
        ActivityDecision::ClientReadOnly => (Tone::Brass, "eye"),
        ActivityDecision::ClientBlocked => (Tone::Brass, "ban"),
    }
}

/// A card's title row: 16/600 title with an optional leading icon.
fn card_title(t: &Theme, glyph: Option<&'static str>, title: &str) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .children(glyph.map(|g| icon(g, 18., t.text_2)))
        .child(
            div()
                .text_size(px(16.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(title.to_owned())),
        )
}

/// One legend entry above the tool list: the tier's icon and name in its tone.
fn legend(t: &Theme, l: PermissionLabel) -> gpui::Div {
    let (tone, glyph) = tier_pill_parts(l);
    let c = kit::tone_colors(t, tone);
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .text_color(rgba_of(c.fg))
        .child(icon(glyph, 14., c.icon))
        .child(shrink_text(l.text()))
}

/// The 34 px icon tile of a derived settings row.
fn row_tile(t: &Theme, glyph: &'static str) -> gpui::Div {
    div()
        .size(px(34.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_MD))
        .bg(rgba_of(t.icon_tile()))
        .child(icon(glyph, 18., t.text_2))
}

impl SettingsView {
    /// Applies a policy to the daemon and saves the table (AGT-2.AC4). A failed save keeps the
    /// policy for this run and says so.
    pub(super) fn set_client_policy(
        &mut self,
        name: &str,
        p: ClientPolicy,
        cx: &mut Context<Self>,
    ) {
        let backend = self.app.read(cx).backend.clone();
        if let Some(d) = backend.daemon.as_ref() {
            d.policies().set(name, p);
            self.policy_error = backend
                .settings
                .set_client_policies(&d.policies().all())
                .err()
                .map(|e| super::PolicyProblem::NotSaved(e.to_string()));
        }
        cx.notify();
    }

    /// "Allow unknown clients": the default policy goes back to Allowed and the choice is saved,
    /// so the next start does not block unknown clients again.
    pub(super) fn allow_unknown_clients(&mut self, cx: &mut Context<Self>) {
        let backend = self.app.read(cx).backend.clone();
        if let Some(d) = backend.daemon.as_ref() {
            d.policies().set_default(ClientPolicy::Allowed);
            self.policy_error = backend
                .settings
                .set_deny_unknown(false)
                .err()
                .map(|e| super::PolicyProblem::NotSaved(e.to_string()));
        }
        cx.notify();
    }

    /// The "Add client" form: validate the name as `session.hello` does, then apply and save.
    pub(super) fn add_client(&mut self, cx: &mut Context<Self>) {
        let raw = self.client_input.read(cx).text().to_owned();
        match crate::settings::validate_client_name(&raw) {
            Ok(name) => {
                let p = self.new_policy;
                self.set_client_policy(&name, p, cx);
                if self.policy_error.is_none() {
                    self.client_input.update(cx, |f, cx| f.clear(cx));
                }
            }
            Err(why) => {
                self.policy_error = Some(super::PolicyProblem::Name(why));
                cx.notify();
            }
        }
    }

    /// The agent-blue status pill in the page header: "MCP ready" / "MCP offline".
    pub(super) fn agent_pill(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let status = agent_status_of(&self.app.read(cx).backend);
        let ready = status.is_ready();
        div()
            .id("mcp-status")
            .role(Role::Status)
            .aria_label(SharedString::from(format!(
                "{}. {}",
                status.title(),
                status.lines().join(". ")
            )))
            .h(px(32.))
            .px(px(12.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .rounded(px(scale::RADIUS_PILL))
            .border_1()
            .border_color(rgba_of(if ready { t.agent_border } else { t.line }))
            .bg(rgba_of(if ready { t.agent_fill } else { t.surface }))
            .text_size(px(13.))
            .text_color(rgba_of(if ready { t.agent_text } else { t.text_2 }))
            .child(dot(if ready { t.agent } else { t.mist }, 8.))
            .child(status.title())
            .into_any_element()
    }

    /// The page hero: an agent-blue card with the icon tile, the title, the subtitle and, on the
    /// right, the MCP status pill with its last-request line.
    fn hero(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let status = agent_status_of(&self.app.read(cx).backend);
        let c = kit::tone_colors(t, Tone::Agent);
        let last = status.lines().join(" \u{00B7} ");
        div()
            .id("agents-hero")
            .p(px(24.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(16.))
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(c.border))
            .bg(rgba_of(c.bg))
            .child(
                div()
                    .size(px(52.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(scale::RADIUS_LG))
                    .border_1()
                    .border_color(rgba_of(c.border))
                    .bg(rgba_of(t.agent_fill))
                    .child(icon("bot", 26., t.agent_text)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(260.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(type_scale::H1))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SettingsSection::Agents.title()),
                    )
                    .child(
                        div()
                            .text_size(px(type_scale::UI))
                            .text_color(rgba_of(t.text_3))
                            .child(SettingsSection::Agents.subtitle()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(8.))
                    .child(self.agent_pill(t, cx))
                    .child(
                        div()
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.mist))
                            .mono()
                            .child(SharedString::from(last)),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn agents(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let (endpoint, activity, policies, status) = {
            let m = self.app.read(cx);
            let d = m.backend.daemon.as_ref();
            (
                m.backend.endpoint.clone(),
                d.map(|d| d.activity()).unwrap_or_default(),
                d.map(|d| d.policies().all()).unwrap_or_default(),
                agent_status_of(&m.backend),
            )
        };
        let exe = std::env::current_exe().ok();
        let mcp = locate_mcp(exe.as_deref(), std::env::var_os("PATH"));
        let socket = endpoint_arg(endpoint.as_ref());
        let config = client_config(&mcp.command, socket.as_deref());

        // ---- Client config card -----------------------------------------------------
        let copy_text = config.clone();
        let copied = self.copied;
        let copy = small_button_with_icon(
            t,
            "copy-config",
            &if copied {
                tr_ctx!("button", "Copied")
            } else {
                tr_ctx!("button", "Copy")
            },
            Some(if copied { "check" } else { "copy" }),
        )
        .aria_label(SharedString::from(tr!(
            "Copy client config to the clipboard"
        )))
        .on_click(cx.listener(move |this, _, _, cx| this.copy_text(copy_text.clone(), cx)));
        let lines = config.lines().map(|l| {
            div()
                .w_full()
                .whitespace_nowrap()
                .child(SharedString::from(l.to_owned()))
        });
        let mut notes: Vec<(String, bool)> = vec![(
            tr!("Add this to your MCP client\u{2019}s config. The command is the capturemann program (its mcp subcommand is the MCP server) and --socket is the socket this app is serving."),
            false,
        )];
        if !mcp.found {
            notes.push((model::missing_mcp_hint(exe.as_deref()), true));
        }
        if socket.is_none() {
            notes.push((
                tr!("This app is not serving a socket, so the snippet has no --socket."),
                true,
            ));
        }
        if !status.is_ready() {
            notes.push((
                tr!(
                    "MCP is offline: {reason}.",
                    reason = status.lines().join(" ")
                ),
                true,
            ));
        }
        let config_card = card(t)
            .p(px(18.))
            .flex_1()
            .min_w(px(380.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(card_title(t, None, &tr!("Client config")))
                    .child(copy),
            )
            .child(
                div()
                    .id("config-code")
                    .role(Role::Group)
                    .aria_label(SharedString::from(tr!("Client config snippet")))
                    .overflow_x_scroll()
                    .p(px(14.))
                    .rounded(px(scale::RADIUS_MD))
                    .border_1()
                    .border_color(rgba_of(t.raised))
                    .bg(rgba_of(t.ground))
                    .text_size(px(13.))
                    .line_height(px(21.))
                    .text_color(rgba_of(t.text_2))
                    .mono()
                    .children(lines),
            )
            .children(notes.into_iter().map(|(n, warn)| {
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(if warn { t.warning } else { t.mist }))
                    .child(SharedString::from(n))
            }));

        // ---- Permissions card -------------------------------------------------------
        let caps = capability_rows();
        let cap_rows = caps.into_iter().enumerate().map(|(i, c)| {
            let (tone, glyph) = tier_pill_parts(c.label);
            div()
                .id(("cap", i))
                .role(Role::ListItem)
                .aria_label(SharedString::from(if c.locked {
                    tr!(
                        "{title}, {tool}: {tier}, locked",
                        title = c.title,
                        tool = c.tool,
                        tier = c.label.text()
                    )
                } else {
                    tr!(
                        "{title}, {tool}: {tier}",
                        title = c.title,
                        tool = c.tool,
                        tier = c.label.text()
                    )
                }))
                .min_h(px(56.))
                .py(px(8.))
                .flex()
                .items_center()
                .gap(px(12.))
                .border_t_1()
                .border_color(rgba_of(t.row_divider()))
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
                                .child(SharedString::from(c.title.clone())),
                        )
                        .child(
                            div()
                                .text_size(px(type_scale::CAPTION))
                                .text_color(rgba_of(t.mist))
                                .mono()
                                .child(SharedString::from(c.tool.to_owned())),
                        ),
                )
                .child(kit::pill(t, tone, Some(glyph), c.label.text()))
        });
        let perms_card = card(t)
            .px(px(18.))
            .py(px(12.))
            .flex_1()
            .min_w(px(380.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .py(px(4.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(card_title(t, None, &tr!("Tool permissions")))
                    .child(
                        div()
                            .flex()
                            .gap(px(12.))
                            .text_size(px(type_scale::CAPTION))
                            .child(legend(t, PermissionLabel::Allow))
                            .child(legend(t, PermissionLabel::AskFirst)),
                    ),
            )
            // The list is long: it scrolls inside the card (the page does not scroll with it) and
            // shows a scrollbar.
            .child(crate::ui::scroll::nested_scroll(
                t,
                &self.caps_scroll,
                360.,
                div()
                    .id("capability-list")
                    .role(Role::List)
                    .aria_label(SharedString::from(tr!("Agent permissions")))
                    .flex()
                    .flex_col()
                    .children(cap_rows),
            ))
            .child(div().pt(px(8.)).child(caption(
                t,
                // TRANSLATORS: FACT: owner review required (confirmation guarantee)
                tr!("Allow runs immediately. Ask first always shows a confirmation on screen and is locked: starting a recording, publishing and deleting can\u{2019}t be switched off. Restrict a whole client below."),
            )));

        // ---- Client policies --------------------------------------------------------
        let policies_lost = policies.is_empty()
            && self
                .app
                .read(cx)
                .backend
                .settings
                .load_report()
                .policies_unreadable;
        let deny_unknown = self
            .app
            .read(cx)
            .backend
            .daemon
            .as_ref()
            .is_some_and(|d| d.policies().default_policy() == ClientPolicy::Blocked);
        let clients = client_rows(&policies, &activity);
        let nclients = clients.len();
        let client_items = clients.into_iter().enumerate().map(|(i, c)| {
            let name = c.name.clone();
            let opts = POLICIES.iter().map(|p| {
                let p: ClientPolicy = *p;
                let who = name.clone();
                segment_with_icon(
                    t,
                    SharedString::from(format!("policy-{i}-{}", policy_id(p))),
                    &policy_label(p),
                    c.policy == p,
                    Some(policy_icon(p)),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_client_policy(&who, p, cx);
                }))
            });
            card_row(t, i + 1 == nclients)
                .id(("client", i))
                .role(Role::ListItem)
                .aria_label(SharedString::from(tr!(
                    "{name}, {policy}. {hint}",
                    name = c.name,
                    policy = policy_label(c.policy),
                    hint = policy_hint(c.policy)
                )))
                .child(row_tile(t, "bot"))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(160.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(type_scale::UI))
                                .mono()
                                .child(SharedString::from(c.name.clone())),
                        )
                        .child(caption(
                            t,
                            match &c.last_tool {
                                Some(tool) => tr!(
                                    "{hint} \u{00B7} last request: {tool}",
                                    hint = policy_hint(c.policy),
                                    tool = tool
                                ),
                                None => policy_hint(c.policy),
                            },
                        )),
                )
                .child(segmented(t, ("client-policy", i), &tr!("Client policy")).children(opts))
        });
        let clients_card = list_card(t)
            .id("client-list")
            .role(Role::List)
            .aria_label(SharedString::from(tr!("Agent clients")))
            .when(policies_lost, |d| {
                d.child(
                    card_row(t, false)
                        .id("policies-lost")
                        .role(Role::Alert)
                        .aria_label(SharedString::from(tr!("Client permissions were lost")))
                        .text_color(rgba_of(t.warning))
                        .child(tr!("The settings file was unreadable and the per-client permissions could not be restored. Review the clients below: set Blocked or Read-only for the ones you don\u{2019}t trust.")),
                )
            })
            .when(deny_unknown, |d| {
                d.child(
                    card_row(t, false)
                        .id("unknown-clients-blocked")
                        .role(Role::Alert)
                        .aria_label(SharedString::from(tr!("Unknown clients are blocked")))
                        .justify_between()
                        .text_color(rgba_of(t.warning))
                        .child(tr!("Unknown clients are blocked until you review the list"))
                        .child(
                            small_button(t, "allow-unknown-clients", &tr!("Allow unknown clients"))
                                .aria_label(SharedString::from(tr!(
                                    "Allow clients that are not in the list"
                                )))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.allow_unknown_clients(cx)
                                })),
                        ),
                )
            })
            .when(nclients == 0, |d| {
                d.child(
                    div().p(px(12.)).child(
                        div()
                            .id("no-clients")
                            .px(px(16.))
                            .py(px(14.))
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .rounded(px(scale::RADIUS_MD))
                            .border_1()
                            .border_dashed()
                            .border_color(rgba_of(t.line))
                            .text_color(rgba_of(t.text_3))
                            .child(icon("bot", 18., t.mist))
                            .child(label_fill(tr!("No clients yet. A client appears here after its first confirmed or refused request, or when you add it below."))),
                    ),
                )
            })
            .children(client_items);

        // ---- Add a client by name (pre-allow / pre-block) --------------------------
        let add_policy = self.new_policy;
        let add_opts = POLICIES.iter().map(|p| {
            let p: ClientPolicy = *p;
            segment_with_icon(
                t,
                SharedString::from(format!("new-client-policy-{}", policy_id(p))),
                &policy_label(p),
                add_policy == p,
                Some(policy_icon(p)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.new_policy = p;
                cx.notify();
            }))
        });
        let add_card = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .id("add-client")
                    .role(Role::Group)
                    .aria_label(SharedString::from(tr!("Add a client by name")))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(220.))
                            .child(self.client_input.clone()),
                    )
                    .child(
                        segmented(t, "new-client-policy", &tr!("Policy for the new client"))
                            .children(add_opts),
                    )
                    .child(
                        button_with_icon(
                            t,
                            "add-client-button",
                            tr_ctx!("button", "Add client"),
                            ButtonKind::Secondary,
                            Some("user-plus"),
                        )
                        .aria_label(SharedString::from(tr!("Add client with the chosen policy")))
                        .on_click(cx.listener(|this, _, _, cx| this.add_client(cx))),
                    ),
            )
            .children(
                self.policy_error
                    .as_ref()
                    .map(super::PolicyProblem::text)
                    .map(|e| {
                        div()
                            .id("add-client-error")
                            .role(Role::Alert)
                            .aria_label(SharedString::from(e.clone()))
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.warning))
                            .child(SharedString::from(e))
                    }),
            );

        // ---- Recent requests --------------------------------------------------------
        let recent: Vec<_> = activity.iter().rev().take(MAX_RECENT).cloned().collect();
        let nrecent = recent.len();
        let recent_rows = recent.into_iter().enumerate().map(|(i, e)| {
            card_row(t, i + 1 == nrecent)
                .id(("agent-row", i))
                .role(Role::ListItem)
                .aria_label(SharedString::from(tr!(
                    "{tool} from {client}, {decision}",
                    tool = e.tool,
                    client = e.client,
                    decision = decision_label(e.decision)
                )))
                .min_h(px(scale::ROW_H))
                .justify_between()
                .child(
                    div()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .truncate()
                                .text_color(rgba_of(t.agent_text))
                                .mono()
                                .child(SharedString::from(e.tool.clone())),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(type_scale::CAPTION))
                                .text_color(rgba_of(t.mist))
                                .mono()
                                .child(SharedString::from(e.client.clone())),
                        ),
                )
                .child({
                    let (tone, glyph) = decision_pill_parts(e.decision);
                    kit::pill(t, tone, Some(glyph), decision_label(e.decision))
                })
        });
        let recent_card = list_card(t)
            .id("agent-list")
            .role(Role::List)
            .aria_label(SharedString::from(tr!("Recent agent requests")))
            .when(nrecent == 0, |d| {
                d.child(
                    card_row(t, true).child(
                        div()
                            .text_color(rgba_of(t.text_3))
                            .child(tr!("No agent requests yet.")),
                    ),
                )
            })
            .children(recent_rows);

        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(self.hero(t, cx))
            .child(div().flex().flex_wrap().gap(px(16.)).child(config_card).child(perms_card))
            .child(
                card(t)
                    .p(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(card_title(t, None, &tr!("Clients")))
                    .child(clients_card)
                    .child(add_card)
                    .child(caption(
                        t,
                        tr!("A client policy is stricter than the permissions above and is remembered. Read-only refuses every action that changes something. Add a client by the name it connects with to allow or block it before its first request."),
                    )),
            )
            .child(
                card(t)
                    .p(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(card_title(t, Some("clock"), &tr!("Recent agent requests")))
                    .child(recent_card)
                    .child(caption(
                        t,
                        tr!("Requests that needed your confirmation, and refused ones, are listed for 30 days."),
                    )),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policies_have_distinct_icons() {
        let icons: Vec<_> = POLICIES.iter().map(|p| policy_icon(*p)).collect();
        assert_eq!(icons, ["circle-check", "eye", "ban"]);
    }

    #[test]
    fn tiers_are_told_apart_by_icon_and_tone() {
        assert_eq!(
            tier_pill_parts(PermissionLabel::Allow),
            (Tone::Success, "circle-check")
        );
        assert_eq!(
            tier_pill_parts(PermissionLabel::AskFirst),
            (Tone::Brass, "hand")
        );
    }

    #[test]
    fn decisions_never_use_the_danger_tone() {
        use ActivityDecision::*;
        for d in [
            Approved,
            ApprovedForSession,
            Declined,
            TimedOut,
            ClientReadOnly,
            ClientBlocked,
        ] {
            assert_ne!(decision_pill_parts(d).0, Tone::Danger, "{d:?}");
        }
        assert_eq!(decision_pill_parts(Approved).0, Tone::Success);
        assert_eq!(decision_pill_parts(ClientBlocked), (Tone::Brass, "ban"));
        assert_eq!(decision_pill_parts(TimedOut), (Tone::Neutral, "clock"));
    }
}
