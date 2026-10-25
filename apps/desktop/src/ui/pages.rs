//! Studio pages that are not the library or settings: the Agents & CLI page (real status and the
//! daemon's agent activity log).

use cm_daemon::Endpoint;
use gpui::{div, prelude::*, px, AnyElement, Context, Entity, FontWeight, Role, SharedString};

use cm_i18n::{tr, tr_ctx};

use crate::agent_status::decision_label;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::AppModel;
use crate::ui::components::{card, section_label, status_pill};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::studio::agent_status_of;

const MAX_ROWS: usize = 20;

/// Agents & CLI: MCP status, the IPC endpoint, and recent gated or refused agent requests.
pub fn agents<V: 'static>(t: &Theme, app: &Entity<AppModel>, cx: &mut Context<V>) -> AnyElement {
    let (status, endpoint, entries) = {
        let m = app.read(cx);
        let entries = m
            .backend
            .daemon
            .as_ref()
            .map(|d| d.activity())
            .unwrap_or_default();
        let endpoint = match &m.backend.endpoint {
            Some(Endpoint::Path(p)) => p.display().to_string(),
            Some(Endpoint::Pipe(p)) => p.clone(),
            None => tr!("Not serving"),
        };
        (agent_status_of(&m.backend), endpoint, entries)
    };
    let rows = entries
        .iter()
        .rev()
        .take(MAX_ROWS)
        .enumerate()
        .map(|(i, e)| {
            div()
                .id(("agent-row", i))
                .role(Role::ListItem)
                .aria_label(SharedString::from(tr!(
                    "{tool} from {client}, {decision}",
                    tool = e.tool,
                    client = e.client,
                    decision = decision_label(e.decision)
                )))
                .min_h(px(scale::ROW_H))
                .py(px(8.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .border_b_1()
                .border_color(rgba_of(t.border))
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
                .child(status_pill(t, t.agent, decision_label(e.decision)))
        });
    div()
        .id("agents-page")
        .size_full()
        .overflow_hidden()
        .p(px(24.))
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(
            card(t)
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(section_label(t, &tr!("Agent interface")))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().font_weight(FontWeight::MEDIUM).child(status.title()))
                        .child(status_pill(
                            t,
                            if status.is_ready() { t.agent } else { t.mist },
                            if status.is_ready() {
                                tr!("Listening")
                            } else {
                                tr_ctx!("agent status", "Offline")
                            },
                        )),
                )
                .child(
                    div()
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.text_3))
                        .mono()
                        .child(SharedString::from(endpoint)),
                )
                .child(
                    div()
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.mist))
                        .child(tr!("The CLI and the MCP bridge connect to this app over a local socket. Requests that need your confirmation, and refused ones, are listed below for 30 days.")),
                ),
        )
        .child(
            card(t)
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(section_label(t, &tr!("Recent agent requests")))
                .when(entries.is_empty(), |d| {
                    d.child(
                        div()
                            .py(px(12.))
                            .text_color(rgba_of(t.text_3))
                            .child(tr!("No agent requests yet.")),
                    )
                })
                .child(
                    div()
                        .id("agent-list")
                        .role(Role::List)
                        .aria_label(tr!("Recent agent requests"))
                        .flex()
                        .flex_col()
                        .children(rows),
                ),
        )
        .into_any_element()
}
