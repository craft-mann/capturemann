//! Settings -> Privacy (redesign Phase 5a, board R15): four fact tiles, then the apps that never
//! appear in recordings. The facts are the app's own (`model::privacy_facts`), the list is the real
//! default exclusion list with its non-removable semantics (REC-6.AC3), and the footer keeps the
//! bundle id field and its validation.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, Role, SharedString};

use super::model::{excluded_caption, excluded_note, privacy_facts};
use super::rows::tile;
use super::widgets::{
    caption, footer_row, group_header, list_card, settings_row, small_button_with_icon,
    warning_caption,
};
use super::SettingsView;
use crate::recording::start::default_exclusions;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::components::{card, icon};
use crate::ui::kit::{pill, Tone};
use crate::ui::paint::rgba_of;
use cm_i18n::{tr, tr_ctx};

/// One fact tile: a success-green icon, the title and the body.
fn fact_tile(t: &Theme, glyph: &'static str, title: &str, body: &str, i: usize) -> AnyElement {
    card(t)
        .id(("privacy-fact", i))
        .role(Role::ListItem)
        .aria_label(SharedString::from(tr!(
            "{title}. {body}",
            title = title,
            body = body
        )))
        .flex_1()
        .min_w(px(200.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(icon(glyph, 20., t.success))
        .child(
            div()
                .text_size(px(type_scale::UI))
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(title.to_owned())),
        )
        .child(
            div()
                .text_size(px(13.))
                .text_color(rgba_of(t.text_3))
                .child(SharedString::from(body.to_owned())),
        )
        .into_any_element()
}

impl SettingsView {
    pub(super) fn privacy(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let facts = div()
            .id("privacy-facts")
            .role(Role::List)
            .aria_label(SharedString::from(tr!("Privacy facts")))
            .flex()
            .flex_wrap()
            .gap(px(12.))
            .children(
                privacy_facts()
                    .iter()
                    .enumerate()
                    .map(|(i, (g, title, body))| fact_tile(t, g, title, body, i)),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(facts)
            .child(self.excluded_apps(t, cx))
            .into_any_element()
    }

    /// The apps that never appear in a recording (REC-6). The defaults are listed and cannot be
    /// removed (REC-6.AC3).
    pub(super) fn excluded_apps(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let s = self.recording_settings(cx);
        let defaults = default_exclusions();
        // Design gap: the board lists "Keychain Access" as a default and makes 1Password
        // removable; the app's real defaults (REC-6) win. App names for user entries are not
        // resolved, so those rows show the bundle id.
        let self_row = settings_row(
            t,
            Some(tile::APP),
            "CaptureMann",
            Some(SharedString::from(tr!(
                "Its own windows, toolbars and the webcam bubble preview"
            ))),
            Some(
                pill(
                    t,
                    Tone::Neutral,
                    Some("lock"),
                    tr_ctx!("excluded app", "Always"),
                )
                .into_any_element(),
            ),
            false,
        )
        .id("excluded-self")
        .role(Role::ListItem)
        .aria_label(SharedString::from(tr!(
            "CaptureMann, its own windows, always excluded"
        )));
        let default_rows = defaults.iter().enumerate().map(|(i, (name, id))| {
            settings_row(
                t,
                Some(tile::APP),
                SharedString::from((*name).to_owned()),
                Some(SharedString::from((*id).to_owned())),
                Some(
                    pill(
                        t,
                        Tone::Neutral,
                        Some("lock"),
                        tr_ctx!("excluded app", "Default"),
                    )
                    .into_any_element(),
                ),
                false,
            )
            .id(("excluded-default", i))
            .role(Role::ListItem)
            .aria_label(SharedString::from(tr!(
                "{name}, {id}, always excluded",
                name = name,
                id = id
            )))
        });
        let user_rows =
            s.excluded_apps.iter().enumerate().map(|(i, id)| {
                let remove_id = id.clone();
                settings_row(
                    t,
                    Some(tile::APP),
                    SharedString::from(id.clone()),
                    Some(SharedString::from(tr!("Added by you"))),
                    Some(
                        div()
                            .id(("excluded-remove", i))
                            .role(Role::Button)
                            .aria_label(SharedString::from(tr!("Stop excluding {id}", id = id)))
                            .focusable()
                            .tab_stop(true)
                            .size(px(32.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(scale::RADIUS_SM))
                            .cursor_pointer()
                            .hover({
                                let raised = rgba_of(t.raised);
                                move |s| s.bg(raised)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_exclusion(&remove_id, cx)
                            }))
                            .child(icon("x", 14., t.text_3))
                            .into_any_element(),
                    ),
                    false,
                )
                .id(("excluded-user", i))
                .role(Role::ListItem)
                .aria_label(SharedString::from(tr!("{id}, excluded by you", id = id)))
            });
        let footer = footer_row(
            t,
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(240.))
                        .child(self.exclude_input.clone()),
                )
                .child(
                    small_button_with_icon(t, "add-excluded", &tr!("Add app"), Some("plus"))
                        .aria_label(SharedString::from(tr!("Add this app to the excluded apps")))
                        .on_click(cx.listener(|this, _, _, cx| this.add_exclusion(cx))),
                )
                .into_any_element(),
        );
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(group_header(
                t,
                &tr!("Apps that never appear in recordings"),
                Some(SharedString::from(excluded_note())),
            ))
            .child(
                list_card(t)
                    .id("excluded-apps")
                    .role(Role::List)
                    .aria_label(SharedString::from(tr!("Apps excluded from recordings")))
                    .child(self_row)
                    .children(default_rows)
                    .children(user_rows)
                    .child(footer),
            )
            .children(self.exclude_error.map(|e| e.text()).map(|e| {
                warning_caption(t, SharedString::from(e.clone()))
                    .id("exclude-error")
                    .role(Role::Alert)
                    .aria_label(SharedString::from(e))
            }))
            .child(caption(t, excluded_caption()))
            .into_any_element()
    }
}
