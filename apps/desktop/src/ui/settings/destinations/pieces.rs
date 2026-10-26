use super::*;

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
pub(in super::super) fn field_block(
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
pub(in super::super) fn alert_line(
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
pub(in super::super) fn outcome_line(
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
pub(in super::super) fn step_list<'a>(
    t: &Theme,
    steps: impl Iterator<Item = &'a str>,
) -> gpui::Div {
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
pub(in super::super) fn kind_icon(kind: Kind) -> &'static str {
    match kind {
        Kind::Local => "folder",
        Kind::GoogleDrive => crate::ui::brand::GOOGLE_DRIVE.icon,
        Kind::OneDrive => crate::ui::brand::ONEDRIVE.icon,
        Kind::Dropbox => crate::ui::brand::DROPBOX.icon,
    }
}

/// The glyph of a cloud-drive row by its display name (the brand mark when we have it).
pub(super) fn drive_icon(name: &str) -> &'static str {
    crate::ui::brand::by_name(name).map_or("cloud", |b| b.icon)
}

/// The hint of a drive row: what it does while nothing is connected, then how many accounts.
pub(super) fn drive_hint(connected: usize) -> String {
    match connected {
        0 => tr!("Upload copies to a folder in your Drive."),
        n => trn!("{n} account connected", "{n} accounts connected", n),
    }
}

pub(super) fn store() -> DestinationStore {
    DestinationStore::new(crate::settings::config_dir())
}
