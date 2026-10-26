use super::*;

/// A field of the details card: the label with a mono note on the right (the counter, "comma
/// separated", "optional"), the input, its error and its hint. `key` keeps the error's element id.
pub(super) fn field_block(
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
pub(super) fn card_title(t: &Theme, text: &str) -> gpui::Div {
    let _ = t;
    div()
        .text_size(px(16.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(SharedString::from(text.to_owned()))
}

/// The icon of a visibility option.
pub(super) fn privacy_icon(p: Privacy) -> &'static str {
    match p {
        Privacy::Private => "lock",
        Privacy::Unlisted => "link",
        Privacy::Public => "globe",
    }
}

/// One line under the visibility choice: what the chosen visibility means on YouTube.
pub(super) fn privacy_blurb(p: Privacy) -> String {
    match p {
        Privacy::Private => tr!("Only you can watch it."),
        Privacy::Unlisted => {
            tr!("Anyone with the link can watch. It won\u{2019}t appear in search or on your channel.")
        }
        Privacy::Public => tr!("Anyone can find and watch it."),
    }
}

/// The first letter of a channel name for its tile (no avatar is fetched).
pub(super) fn initial(name: &str) -> String {
    name.trim()
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".to_owned())
}

pub(super) fn progress_bar(t: &Theme, fraction: f64) -> gpui::Div {
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
