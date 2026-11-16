//! Settings > General > Previews (design S24): one row per feature that is in the Preview stage
//! (`cm_core::features`), each with a switch (on by default, key `previews.<id>`) and a Feedback
//! button that opens a GitHub issue form with the feature name and the app version filled in.
//! Turning a preview off hides its entry points (the UI reads the key) and the daemon answers
//! `feature_disabled` for starting it; what was made with it stays in the library.
//!
//! The pill is [`preview_pill`], a plain local drawing until the shared Preview label component
//! lands; it is the one place to swap.

use cm_core::features::{self, Feature};
use cm_i18n::tr_ctx;
use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, Role, SharedString};
use serde_json::json;

use super::widgets::{
    caption, group_header, list_card, small_button_with_icon, ROW_HINT_SIZE, ROW_MIN_H, ROW_TILE,
};
use super::SettingsView;
use crate::theme::{scale, type_scale, Color, Mode, Theme};
use crate::ui::app;
use crate::ui::components::{icon, icon_size, switch};
use crate::ui::paint::rgba_of;

/// The GitHub issue form of the project.
const NEW_ISSUE_URL: &str = "https://github.com/craft-mann/capturemann/issues/new";

/// The translated texts of the section (built in one place for the width tests).
pub(crate) struct PreviewsText {
    pub title: String,
    pub intro: String,
    pub feedback: String,
    pub footer: String,
    pub general_feedback: String,
    pub pill: String,
}

impl PreviewsText {
    pub(crate) fn current() -> Self {
        PreviewsText {
            title: tr_ctx!("previews page", "Previews"),
            intro: tr_ctx!(
                "previews page",
                "New features that are ready to use but may still change. Turning one off hides its buttons and menus; anything you made with it stays in your library."
            ),
            feedback: tr_ctx!("previews page", "Feedback"),
            footer: tr_ctx!("previews page", "Previews are on by default."),
            general_feedback: tr_ctx!("previews page", "Send general feedback"),
            pill: tr_ctx!("previews page", "Preview"),
        }
    }
}

/// The name and the one-line description of a preview, in the UI language. A feature this build
/// has no text for shows its registry name and no description (a new preview needs its entry here;
/// a test pins that every registered preview has one).
pub(crate) fn feature_text(f: &Feature) -> (String, Option<String>) {
    match f.id {
        "long_shot" => (
            tr_ctx!("capture dock", "Long Shot"),
            Some(tr_ctx!(
                "previews page",
                "Capture a scrolling page as one image."
            )),
        ),
        "click_guide" => (
            tr_ctx!("capture dock", "Click Guide"),
            Some(tr_ctx!(
                "previews page",
                "Turn your clicks into a numbered guide."
            )),
        ),
        _ => (f.name.to_owned(), None),
    }
}

/// The icon tile of a preview row.
fn feature_tile(f: &Feature) -> &'static str {
    match f.id {
        "long_shot" => "cm-capture-region",
        "click_guide" => "mouse-pointer-click",
        _ => "app-window",
    }
}

/// The `&'static` settings key of a registered feature (`app::set_setting` takes a static key).
fn static_key(f: &Feature) -> Option<&'static str> {
    match f.id {
        "long_shot" => Some("previews.long_shot"),
        "click_guide" => Some("previews.click_guide"),
        _ => None,
    }
}

/// The percent-encoded form of `s` for a URL query value (RFC 3986 unreserved characters stay).
pub(super) fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The issue-form link of one feature: the title starts `"<feature> (preview): "`, the body holds
/// the feature name and the app version and nothing else (no logs, no screenshots, no paths).
pub(super) fn feedback_url(feature: &str, version: &str) -> String {
    let title = format!("{feature} (preview): ");
    let body = format!(
        "Feature: {feature} (preview)\nApp version: {version}\n\nWhat worked, and what did not:\n"
    );
    format!(
        "{NEW_ISSUE_URL}?title={}&body={}",
        encode(&title),
        encode(&body)
    )
}

/// The link of the general feedback button: no feature, the app version only.
pub(super) fn general_feedback_url(version: &str) -> String {
    let body = format!("App version: {version}\n\nWhat worked, and what did not:\n");
    format!(
        "{NEW_ISSUE_URL}?title={}&body={}",
        encode("Previews: "),
        encode(&body)
    )
}

/// The small "Preview" pill: 1 px dashed border, no fill, neutral colours (S24: border #55555C and
/// text-2 on dark, #8A8A90 and #44444B on light), 18 px high, 11/600. **The one function to
/// replace with the shared Preview label component.**
pub(super) fn preview_pill(t: &Theme, text: &str) -> AnyElement {
    let (border, ink) = match t.mode {
        Mode::Dark => (Color::rgb(0x55555C), t.text_2),
        Mode::Light => (Color::rgb(0x8A8A90), Color::rgb(0x44444B)),
    };
    div()
        .flex_none()
        .h(px(18.))
        .px(px(7.))
        .flex()
        .items_center()
        .rounded_full()
        .border_1()
        .border_dashed()
        .border_color(rgba_of(border))
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgba_of(ink))
        .child(SharedString::from(text.to_owned()))
        .into_any_element()
}

impl SettingsView {
    /// The Previews section of the General page: title, the sentence, one row per preview and the
    /// footer line. `None` when no feature is a preview (every one went stable).
    pub(super) fn previews_group(&self, t: &Theme, cx: &Context<Self>) -> Option<AnyElement> {
        let rows: Vec<&'static Feature> = features::previews().collect();
        if rows.is_empty() {
            return None;
        }
        let text = PreviewsText::current();
        let settings = self.app.read(cx).backend.settings.snapshot();
        let version = env!("CARGO_PKG_VERSION");
        let mut card = list_card(t)
            .id("previews-card")
            .role(Role::Group)
            .aria_label(SharedString::from(text.title.clone()));
        for f in rows {
            let on = features::is_enabled(&settings, f.id);
            card = card.child(self.preview_row(t, f, on, &text, version));
        }
        card = card.child(self.previews_footer(t, &text, version));
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(group_header(t, &text.title, None))
                .child(caption(t, text.intro.clone()))
                .child(card)
                .into_any_element(),
        )
    }

    fn preview_row(
        &self,
        t: &Theme,
        f: &'static Feature,
        on: bool,
        text: &PreviewsText,
        version: &'static str,
    ) -> AnyElement {
        let (name, hint) = feature_text(f);
        let key = static_key(f);
        let url = feedback_url(f.name, version);
        let toggle = {
            let app = self.app.clone();
            switch(
                t,
                SharedString::from(format!("preview-{}-toggle", f.id)),
                SharedString::from(tr_ctx!(
                    "previews page",
                    "{feature}, preview",
                    feature = name.clone()
                )),
                on,
                key.is_some(),
            )
            .on_click(move |_, _, cx| {
                if let Some(key) = key {
                    app::set_setting(&app, key, json!(!on), cx);
                }
            })
        };
        let feedback = small_button_with_icon(
            t,
            SharedString::from(format!("preview-{}-feedback", f.id)),
            &text.feedback,
            Some("message-square"),
        )
        .aria_label(SharedString::from(tr_ctx!(
            "previews page",
            "Send feedback about {feature}",
            feature = name.clone()
        )))
        .on_click(move |_, _, cx| cx.open_url(&url));
        let title = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .text_size(px(type_scale::UI))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgba_of(t.text))
                    .child(SharedString::from(name)),
            )
            .child(preview_pill(t, &text.pill));
        div()
            .min_h(px(ROW_MIN_H))
            .px(px(18.))
            .py(px(14.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(14.))
            .border_b_1()
            .border_color(rgba_of(t.row_divider()))
            .child(
                div()
                    .size(px(ROW_TILE))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(scale::RADIUS_MD))
                    .bg(rgba_of(t.icon_tile()))
                    .child(icon(feature_tile(f), icon_size::NAV, t.text_2)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(180.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(title)
                    .children(hint.map(|h| {
                        div()
                            .text_size(px(ROW_HINT_SIZE))
                            .text_color(rgba_of(t.mist))
                            .child(SharedString::from(h))
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.))
                    .child(feedback)
                    .child(toggle),
            )
            .into_any_element()
    }

    /// "Previews are on by default." and the general feedback link, as the card's footer.
    fn previews_footer(&self, t: &Theme, text: &PreviewsText, version: &'static str) -> AnyElement {
        let url = general_feedback_url(version);
        let link = small_button_with_icon(
            t,
            "previews-general-feedback",
            &text.general_feedback,
            Some("external-link"),
        )
        .on_click(move |_, _, cx| cx.open_url(&url));
        div()
            .px(px(18.))
            .py(px(12.))
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .child(
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(text.footer.clone())),
            )
            .child(link)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_feedback_link_names_the_feature_and_the_version_only() {
        let u = feedback_url("Click Guide", "0.3.4");
        assert!(u.starts_with(
            "https://github.com/craft-mann/capturemann/issues/new?title=Click%20Guide%20%28preview%29%3A%20&body="
        ));
        assert!(u.contains("App%20version%3A%200.3.4"));
        assert!(u.contains("Feature%3A%20Click%20Guide%20%28preview%29"));
        // Nothing else leaves the machine: no path, no user name, no log.
        assert!(!u.contains("Users") && !u.contains("log"));
        assert!(general_feedback_url("0.3.4").contains("App%20version%3A%200.3.4"));
    }

    /// English keeps the board's words; Vietnamese translates the section and the feature texts,
    /// and the feature names use the app's existing names (not the English registry names).
    #[test]
    fn english_keeps_the_board_text_and_vietnamese_translates_it() {
        use cm_i18n::{with_language, Lang};
        let en = with_language(Lang::En, PreviewsText::current);
        assert_eq!(en.title, "Previews");
        assert_eq!(en.pill, "Preview");
        assert_eq!(en.footer, "Previews are on by default.");
        assert_eq!(en.general_feedback, "Send general feedback");
        let vi = with_language(Lang::Vi, PreviewsText::current);
        for (a, b) in [
            (&en.title, &vi.title),
            (&en.intro, &vi.intro),
            (&en.feedback, &vi.feedback),
            (&en.footer, &vi.footer),
            (&en.general_feedback, &vi.general_feedback),
            (&en.pill, &vi.pill),
        ] {
            assert_ne!(a, b, "{a} is not translated");
        }
        assert_eq!(vi.pill, "Xem trước");
        for f in features::previews() {
            let (name_en, hint_en) = with_language(Lang::En, || feature_text(f));
            let (name_vi, hint_vi) = with_language(Lang::Vi, || feature_text(f));
            assert_eq!(name_en, f.name);
            assert_ne!(name_vi, name_en, "{}", f.id);
            assert_ne!(hint_vi, hint_en, "{}", f.id);
        }
        let click = features::feature("click_guide").unwrap();
        assert_eq!(
            with_language(Lang::Vi, || feature_text(click)).0,
            "Hướng dẫn thao tác"
        );
    }

    #[test]
    fn encode_keeps_unreserved_and_escapes_the_rest() {
        assert_eq!(encode("a-b_c.d~E9"), "a-b_c.d~E9");
        assert_eq!(encode("a b\n&=#é"), "a%20b%0A%26%3D%23%C3%A9");
    }

    #[test]
    fn every_registered_preview_has_a_key_a_text_and_a_tile() {
        for f in features::previews() {
            let key = static_key(f).unwrap_or_else(|| panic!("{} has no static key", f.id));
            assert_eq!(key, f.setting_key());
            assert!(feature_text(f).1.is_some(), "{} has no description", f.id);
            let p = format!("icons/{}.svg", feature_tile(f));
            assert!(
                crate::ui::paint::ASSETS.iter().any(|(k, _)| *k == p),
                "tile {p}"
            );
        }
        assert!(crate::ui::paint::ASSETS
            .iter()
            .any(|(k, _)| *k == "icons/message-square.svg"));
    }
}
