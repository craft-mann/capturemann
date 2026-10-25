//! General page: library folder (read-only today), appearance and after-capture behavior, as
//! kit settings rows (redesign Phase 5a).

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, Role, SharedString};
use serde_json::json;

use super::rows::{plain_row, tile, toggle_row};
use super::widgets::{
    caption, group_with_card, list_card, segment, segment_with_icon, segmented, settings_row,
    small_button_with_icon,
};
use super::SettingsView;
use crate::library::LibraryStatus;
use crate::save_folder::SaveFolderConfig;
use crate::settings::{KEY_COPY_AFTER_CAPTURE, KEY_LANGUAGE, KEY_SAVE_FOLDER_ENABLED, KEY_THEME};
use crate::theme::{type_scale, Theme, ThemePreference};
use crate::ui::app;
use crate::ui::components::{button_with_icon, logo, shrink_text, switch, ButtonKind};
use crate::ui::kit::{status, StatusKind};
use crate::ui::paint::{rgba_of, TextExt};
use cm_i18n::{tr, tr_ctx, Lang};

/// The page's translated texts (I18N P1: the first wrapped screen). Built in one place so a test
/// can pin that the English catalogue renders exactly the strings the page had before, and that
/// Vietnamese differs where it is translated. Every other string of the page is wrapped by P2.
pub(super) struct GeneralText {
    pub library: String,
    pub library_folder: String,
    pub appearance: String,
    pub behavior: String,
    pub theme: String,
    pub theme_hint: String,
    pub theme_system: String,
    pub theme_dark: String,
    pub theme_light: String,
    pub language: String,
    pub language_hint: String,
    pub copy_label: String,
    pub copy_hint: String,
    pub save_png: String,
    pub login: String,
    pub login_hint: String,
}

impl GeneralText {
    pub(super) fn current() -> Self {
        GeneralText {
            library: tr!("Library"),
            library_folder: tr!("Library folder"),
            appearance: tr!("Appearance"),
            behavior: tr!("Behavior"),
            theme: tr!("Theme"),
            theme_hint: tr!(
                "System follows the OS: dark by default, light when the OS is light."
            ),
            theme_system: tr_ctx!("theme", "System"),
            theme_dark: tr_ctx!("theme", "Dark"),
            theme_light: tr_ctx!("theme", "Light"),
            language: tr!("Language"),
            language_hint: tr!(
                "The language of menus, buttons and messages. Captions, titles and recordings stay as they are."
            ),
            copy_label: tr!("Copy image to clipboard after capture"),
            copy_hint: tr!("The capture is always saved to the library."),
            save_png: tr!("Also save a PNG to a folder"),
            login: tr!("Launch at login"),
            login_hint: tr!(
                "Starts CaptureMann in the menu bar with no window. Takes effect for the installed app only."
            ),
        }
    }
}

impl SettingsView {
    pub(super) fn general(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let text = GeneralText::current();
        let (pref, lang, copy, root, status_now, folder_cfg) = {
            let m = self.app.read(cx);
            (
                m.theme_pref,
                m.language,
                m.copy_after_capture,
                m.backend.library_root.clone(),
                m.backend.initial_status.clone(),
                SaveFolderConfig::from_settings(&m.backend.settings.snapshot()),
            )
        };
        // CAP-4.AC2: "Also save a PNG to a folder". Turning it on with no folder yet opens the
        // folder picker first; the switch turns on once a folder is chosen.
        let folder_on = folder_cfg.enabled;
        let folder_toggle = {
            let app = self.app.clone();
            let has_folder = folder_cfg.folder.is_some();
            switch(
                t,
                "save-folder-toggle",
                tr!("Also save a PNG to a folder after capture"),
                folder_on,
                true,
            )
            .on_click(move |_, _, cx| {
                if !folder_on && !has_folder {
                    app::choose_save_folder(&app, true, cx);
                } else {
                    app::set_setting(&app, KEY_SAVE_FOLDER_ENABLED, json!(!folder_on), cx);
                }
            })
        };
        let folder_row = folder_on.then(|| {
            let app = self.app.clone();
            let shown = folder_cfg
                .folder
                .as_ref()
                .map_or_else(|| tr!("No folder chosen"), |p| p.display().to_string());
            settings_row(
                t,
                Some(tile::SAVE_FOLDER),
                SharedString::from(shown),
                Some(SharedString::from(tr!(
                    "Named like {pattern}. A name already taken gets (2), (3)\u{2026}",
                    pattern = crate::save_folder::DEFAULT_PATTERN
                ))),
                Some(
                    small_button_with_icon(
                        t,
                        "save-folder-choose",
                        &tr!("Choose folder\u{2026}"),
                        Some("folder-open"),
                    )
                    .on_click(move |_, _, cx| app::choose_save_folder(&app, false, cx))
                    .into_any_element(),
                ),
                false,
            )
            .into_any_element()
        });
        let themes = [
            (
                text.theme_system.as_str(),
                ThemePreference::System,
                "system",
                "monitor",
            ),
            (
                text.theme_dark.as_str(),
                ThemePreference::Dark,
                "dark",
                "moon",
            ),
            (
                text.theme_light.as_str(),
                ThemePreference::Light,
                "light",
                "sun",
            ),
        ]
        .into_iter()
        .map(|(label, p, value, glyph)| {
            let app = self.app.clone();
            segment_with_icon(t, format!("theme-{value}"), label, pref == p, Some(glyph))
                .on_click(move |_, _, cx| app::set_setting(&app, KEY_THEME, json!(value), cx))
        });
        // The languages by their own names ("English", "Tiếng Việt"): never translated, so a
        // user who picked the wrong language can still find the right one.
        let languages = Lang::ALL.into_iter().map(|l| {
            let app = self.app.clone();
            segment(
                t,
                format!("language-{}", l.code()),
                l.native_name(),
                lang == l,
            )
            .on_click(move |_, _, cx| app::set_setting(&app, KEY_LANGUAGE, json!(l.code()), cx))
        });
        // CAP-2.AC5: the login item is a preference file beside the settings, not a setting an
        // agent could write.
        let login_on = crate::login_item::read_enabled(&crate::settings::config_dir());
        let login_toggle = switch(
            t,
            "login-toggle",
            tr!("Launch CaptureMann at login"),
            login_on,
            true,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.login_error =
                crate::login_item::set_enabled(&crate::settings::config_dir(), !login_on).err();
            cx.notify();
        }));
        let login_error = self.login_error.clone();
        let (library_pill, library_note) = match &status_now {
            LibraryStatus::Unavailable {
                hint,
                fallback_in_use,
                ..
            } => (
                status(t, StatusKind::Warning, tr!("Unavailable")),
                Some(if *fallback_in_use {
                    tr!(
                        "Unavailable \u{2014} connect {hint}. Using a folder on this computer meanwhile.",
                        hint = hint
                    )
                } else {
                    tr!("Unavailable \u{2014} connect {hint}.", hint = hint)
                }),
            ),
            _ => (status(t, StatusKind::Ok, tr!("Available")), None),
        };
        let library_group = self.library_group(
            t,
            (&text.library, &text.library_folder),
            root,
            library_pill.into_any_element(),
            library_note,
            cx,
        );
        let mut behavior = vec![
            toggle_row(
                t,
                &self.app,
                "copy-toggle",
                tile::CLIPBOARD,
                &text.copy_label,
                &text.copy_hint,
                KEY_COPY_AFTER_CAPTURE,
                copy,
                false,
            ),
            plain_row(
                t,
                tile::SAVE_FOLDER,
                &text.save_png,
                &tr!("A copy of each capture, and of each annotation save, in a folder you choose. The library keeps the capture either way."),
                Some(folder_toggle.into_any_element()),
                false,
            ),
        ];
        behavior.extend(folder_row);
        behavior.push(plain_row(
            t,
            tile::LOGIN,
            &text.login,
            &text.login_hint,
            Some(login_toggle.into_any_element()),
            true,
        ));
        div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(library_group)
            .child(group_with_card(
                t,
                &text.appearance,
                None,
                list_card(t)
                    .child(settings_row(
                        t,
                        Some(tile::THEME),
                        text.theme.clone(),
                        Some(SharedString::from(text.theme_hint.clone())),
                        Some(
                            segmented(t, "theme-choices", &text.theme)
                                .children(themes)
                                .into_any_element(),
                        ),
                        false,
                    ))
                    .child(settings_row(
                        t,
                        Some(tile::LANGUAGE),
                        text.language.clone(),
                        Some(SharedString::from(text.language_hint.clone())),
                        Some(
                            segmented(t, "language-choices", &text.language)
                                .children(languages)
                                .into_any_element(),
                        ),
                        true,
                    )),
            ))
            .child(
                group_with_card(t, &text.behavior, None, list_card(t).children(behavior)).children(
                    login_error.map(|e| {
                        div()
                            .id("login-error")
                            .role(Role::Alert)
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.warning))
                            .child(SharedString::from(tr!(
                                "Couldn\u{2019}t change it: {error}",
                                error = e
                            )))
                    }),
                ),
            )
            .into_any_element()
    }
}

/// The About page (board R11): the card with the logo tile, the app name and version, the
/// "a CraftMann app" endorsement line (CraftMann brand kit, rule 6: the artwork is drawn at its own
/// size, 130.41 x 24, under the app's name), the licence and privacy notes, and two links.
const WEBSITE_URL: &str = "https://www.craftmann.app/";
const GITHUB_URL: &str = "https://github.com/craft-mann/capturemann";

impl SettingsView {
    pub(super) fn about(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let link = |id: &'static str,
                    label: String,
                    icon: &'static str,
                    url: &'static str,
                    cx: &mut Context<Self>| {
            button_with_icon(t, id, label, ButtonKind::Secondary, Some(icon))
                .on_click(cx.listener(move |_, _, _, cx| cx.open_url(url)))
        };
        let links = div()
            .flex()
            .flex_wrap()
            .gap(px(8.))
            .child(link(
                "about-website",
                tr_ctx!("button", "craftmann.app"),
                "globe",
                WEBSITE_URL,
                cx,
            ))
            .child(link(
                "about-github",
                tr_ctx!("button", "Source on GitHub"),
                "external-link",
                GITHUB_URL,
                cx,
            ));
        about_card(t, links.into_any_element())
    }
}

fn about_card(t: &Theme, links: AnyElement) -> AnyElement {
    let tile = div()
        .flex_none()
        .size(px(52.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(14.))
        .border_1()
        .border_color(rgba_of(t.line))
        .bg(rgba_of(t.field))
        .child(logo(t, 32.));
    let name_row = div()
        .flex()
        .flex_wrap()
        .items_baseline()
        .gap_x(px(8.))
        .child(
            div()
                .text_size(px(type_scale::TITLE))
                .font_weight(FontWeight::SEMIBOLD)
                .child("CaptureMann"),
        )
        .child(
            div()
                .mono()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.mist))
                .child(env!("CARGO_PKG_VERSION")),
        );
    let endorsement = div()
        .id("about-endorsement")
        .role(Role::Image)
        .aria_label(SharedString::from(tr!("a CraftMann app")))
        .flex_none()
        .w(px(130.41))
        .h(px(24.))
        .child(
            gpui::svg()
                .path("brand/endorsement-craftmann.svg")
                .size_full()
                .text_color(rgba_of(t.text_3)),
        );
    let notes = div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(
            shrink_text(tr!("Free and open source under Apache-2.0."))
                .text_color(rgba_of(t.text_2)),
        )
        .child(caption(
            t,
            tr!("UI fonts: Geist and Geist Mono (SIL Open Font License 1.1), bundled. Icons: Lucide (ISC)."),
        ))
        .child(caption(
            t,
            // TRANSLATORS: FACT: owner review required (privacy)
            tr!("No telemetry. No network traffic unless you connect a destination."),
        ));
    div()
        .id("about-card")
        .flex()
        .items_start()
        .gap(px(16.))
        .p(px(16.))
        .rounded(px(14.))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.surface))
        .child(tile)
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(name_row)
                .child(endorsement)
                .child(notes)
                .child(links),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_i18n::with_language;

    /// I18N P1: with the English catalogue the page shows exactly the strings it had before the
    /// wrapping (copied from the sources of Oct 23, 2026).
    #[test]
    fn english_renders_the_previous_strings_byte_for_byte() {
        let t = with_language(Lang::En, GeneralText::current);
        let got = [
            &t.library,
            &t.library_folder,
            &t.appearance,
            &t.behavior,
            &t.theme,
            &t.theme_hint,
            &t.theme_system,
            &t.theme_dark,
            &t.theme_light,
            &t.copy_label,
            &t.copy_hint,
            &t.save_png,
            &t.login,
            &t.login_hint,
        ];
        let want = [
            "Library",
            "Library folder",
            "Appearance",
            "Behavior",
            "Theme",
            "System follows the OS: dark by default, light when the OS is light.",
            "System",
            "Dark",
            "Light",
            "Copy image to clipboard after capture",
            "The capture is always saved to the library.",
            "Also save a PNG to a folder",
            "Launch at login",
            "Starts CaptureMann in the menu bar with no window. Takes effect for the installed app only.",
        ];
        assert_eq!(got.map(String::as_str), want);
        assert_eq!(t.language, "Language");
    }

    /// The draft Vietnamese catalogue covers every string of the page, with diacritics; the
    /// language names in the picker are never translated.
    #[test]
    fn vietnamese_translates_the_page_and_keeps_the_language_names() {
        let en = with_language(Lang::En, GeneralText::current);
        let vi = with_language(Lang::Vi, GeneralText::current);
        assert_eq!(vi.library, "Thư viện");
        assert_eq!(vi.language, "Ngôn ngữ");
        assert_eq!(vi.theme_dark, "Tối");
        for (a, b) in [
            (&en.library_folder, &vi.library_folder),
            (&en.appearance, &vi.appearance),
            (&en.behavior, &vi.behavior),
            (&en.theme, &vi.theme),
            (&en.theme_hint, &vi.theme_hint),
            (&en.theme_system, &vi.theme_system),
            (&en.theme_light, &vi.theme_light),
            (&en.language_hint, &vi.language_hint),
            (&en.copy_label, &vi.copy_label),
            (&en.copy_hint, &vi.copy_hint),
            (&en.save_png, &vi.save_png),
            (&en.login, &vi.login),
            (&en.login_hint, &vi.login_hint),
        ] {
            assert_ne!(a, b, "{a} is not translated");
        }
        let names: Vec<&str> = Lang::ALL.into_iter().map(Lang::native_name).collect();
        assert_eq!(names, ["English", "Tiếng Việt"]);
    }

    /// The pseudo-locale brackets every string of the page (layout stress, not offered in the UI).
    #[test]
    fn pseudo_locale_wraps_the_page() {
        let p = with_language(Lang::EnXa, GeneralText::current);
        for s in [&p.library, &p.theme_hint, &p.language, &p.login_hint] {
            assert!(s.starts_with('[') && s.ends_with(']'), "{s}");
        }
    }

    /// The tray's capture delay choices are a plural message; English is unchanged.
    #[test]
    fn delay_labels_are_plural_messages() {
        assert_eq!(
            with_language(Lang::En, || crate::delay::choice_label(5)),
            "5 seconds"
        );
        assert_eq!(
            with_language(Lang::Vi, || crate::delay::choice_label(5)),
            "5 giây"
        );
        assert_eq!(
            with_language(Lang::Vi, || crate::delay::choice_label(0)),
            "Không trễ"
        );
    }

    /// The tray menu texts come from one function, so the live relabel and the first build agree.
    #[test]
    fn tray_menu_text_follows_the_language() {
        use crate::platform::{ids, menu_text};
        assert_eq!(
            with_language(Lang::En, || menu_text(ids::SETTINGS)),
            "Settings"
        );
        assert_eq!(
            with_language(Lang::Vi, || menu_text(ids::QUIT)),
            "Thoát CaptureMann"
        );
        assert_eq!(
            with_language(Lang::En, || menu_text(ids::QUIT)),
            "Quit CaptureMann"
        );
    }
}
