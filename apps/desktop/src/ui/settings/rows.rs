//! The row model of the Settings pages (redesign Phase 5a): which icon tile each row carries and
//! the builders that put a control in a [`settings_row`]. The pages keep their element ids, aria
//! labels and setting keys; only the row chrome is shared.

use cm_i18n::tr_ctx;
use gpui::{prelude::*, AnyElement, Div, Role, SharedString};
use serde_json::json;

use super::widgets::{list_card, segment_with_icon, segmented, settings_row};
use crate::theme::Theme;
use crate::ui::app::{self, AppModel};
use crate::ui::components::switch;

/// The icon tile of each row, by name from the icon registry (`paint::ASSETS`).
pub mod tile {
    pub const LIBRARY: &str = "hard-drive";
    pub const THEME: &str = "moon";
    pub const LANGUAGE: &str = "globe";
    pub const CLIPBOARD: &str = "copy";
    pub const SAVE_FOLDER: &str = "folder-open";
    pub const LOGIN: &str = "zap";
    pub const RESOLUTION: &str = "monitor";
    pub const FPS: &str = "gauge";
    pub const CODEC: &str = "file-video";
    pub const QUALITY: &str = "sliders-horizontal";
    pub const CURSOR: &str = "mouse-pointer-2";
    pub const AUTO_EXPORT: &str = "file-output";
    pub const COUNTDOWN: &str = "timer";
    pub const SCREENSHOT_DELAY: &str = "camera";
    pub const BUBBLE_SHAPE: &str = "cm-webcam-bubble";
    pub const BUBBLE_SIZE: &str = "maximize";
    pub const MIRROR: &str = "flip-horizontal-2";
    pub const POSITION: &str = "move-up-right";
    pub const CAMERA: &str = "camera";
    pub const NOISE: &str = "audio-lines";
    pub const DEVICES: &str = "mic";
    pub const APP: &str = "app-window";
    pub const SHORTCUT_FALLBACK: &str = "keyboard";

    /// Every tile above, for the registry test.
    #[cfg(test)]
    pub const ALL: &[&str] = &[
        LIBRARY,
        THEME,
        LANGUAGE,
        CLIPBOARD,
        SAVE_FOLDER,
        LOGIN,
        RESOLUTION,
        FPS,
        CODEC,
        QUALITY,
        CURSOR,
        AUTO_EXPORT,
        COUNTDOWN,
        SCREENSHOT_DELAY,
        BUBBLE_SHAPE,
        BUBBLE_SIZE,
        MIRROR,
        POSITION,
        CAMERA,
        NOISE,
        DEVICES,
        APP,
        SHORTCUT_FALLBACK,
    ];
}

/// The tile of a shortcut row (`design/icons/icon-map.md`: capture and recording actions).
pub fn shortcut_tile(key: &str) -> &'static str {
    match key {
        "shortcuts.capture_region" => "cm-capture-region",
        "shortcuts.capture_window" => "app-window",
        "shortcuts.capture_display" => "monitor",
        "shortcuts.toggle_recording" => "cm-record",
        "shortcuts.pause_recording" => "rec-pause",
        "shortcuts.open_library" => "layout-grid",
        "shortcuts.open_launcher" => "search",
        "shortcuts.discard_recording" => "trash-2",
        "shortcuts.restart_recording" => "rotate-cw",
        "shortcuts.annotate_last" => "pencil",
        _ => tile::SHORTCUT_FALLBACK,
    }
}

/// Countdown choices in seconds (REC-4: 0 to 10).
pub const COUNTDOWNS: [u32; 6] = [0, 1, 2, 3, 5, 10];

/// The options a countdown row offers: [`COUNTDOWNS`] plus the stored value when it is custom.
pub fn countdown_options(current: u32) -> Vec<u32> {
    let mut options = COUNTDOWNS.to_vec();
    if !options.contains(&current) {
        options.push(current);
        options.sort_unstable();
    }
    options
}

/// "Off" or "N s".
pub fn seconds_label(n: u32) -> String {
    if n == 0 {
        tr_ctx!("countdown", "Off")
    } else {
        format!("{n} s")
    }
}

/// A settings card with an id, role and aria label (the pages' lists keep theirs).
pub fn card_group(t: &Theme, id: &'static str, aria: &str) -> gpui::Stateful<Div> {
    list_card(t)
        .id(id)
        .role(Role::Group)
        .aria_label(SharedString::from(aria.to_owned()))
}

fn shared(s: &str) -> SharedString {
    SharedString::from(s.to_owned())
}

/// A row with a switch writing a boolean key.
#[allow(clippy::too_many_arguments)]
pub fn toggle_row(
    t: &Theme,
    app: &gpui::Entity<AppModel>,
    id: &'static str,
    tile: &'static str,
    title: &str,
    hint: &str,
    key: &'static str,
    on: bool,
    last: bool,
) -> AnyElement {
    let app = app.clone();
    settings_row(
        t,
        Some(tile),
        shared(title),
        Some(shared(hint)),
        Some(
            switch(t, id, shared(title), on, true)
                .on_click(move |_, _, cx| app::set_setting(&app, key, json!(!on), cx))
                .into_any_element(),
        ),
        last,
    )
    .into_any_element()
}

/// A row with a segmented control writing a string key.
#[allow(clippy::too_many_arguments)]
pub fn choice_row(
    t: &Theme,
    app: &gpui::Entity<AppModel>,
    id: &'static str,
    tile: &'static str,
    title: &str,
    hint: &str,
    key: &'static str,
    options: &[(&'static str, String)],
    current: &str,
    last: bool,
) -> AnyElement {
    let segs = options.iter().map(|(value, label)| {
        let (app, value) = (app.clone(), *value);
        segment_with_icon(t, format!("{id}-{value}"), label, current == value, None)
            .on_click(move |_, _, cx| app::set_setting(&app, key, json!(value), cx))
    });
    settings_row(
        t,
        Some(tile),
        shared(title),
        Some(shared(hint)),
        Some(segmented(t, id, title).children(segs).into_any_element()),
        last,
    )
    .into_any_element()
}

/// A row with a countdown (seconds) control writing a number key.
#[allow(clippy::too_many_arguments)]
pub fn seconds_row(
    t: &Theme,
    app: &gpui::Entity<AppModel>,
    id: &'static str,
    tile: &'static str,
    title: &str,
    hint: &str,
    key: &'static str,
    current: u32,
    last: bool,
) -> AnyElement {
    let segs = countdown_options(current).into_iter().map(|n| {
        let app = app.clone();
        segment_with_icon(
            t,
            format!("{id}-{n}"),
            &seconds_label(n),
            current == n,
            None,
        )
        .on_click(move |_, _, cx| app::set_setting(&app, key, json!(n), cx))
    });
    settings_row(
        t,
        Some(tile),
        shared(title),
        Some(shared(hint)),
        Some(segmented(t, id, title).children(segs).into_any_element()),
        last,
    )
    .into_any_element()
}

/// A row with any control and no setting key attached.
pub fn plain_row(
    t: &Theme,
    tile: &'static str,
    title: &str,
    hint: &str,
    control: Option<AnyElement>,
    last: bool,
) -> AnyElement {
    settings_row(
        t,
        Some(tile),
        shared(title),
        Some(shared(hint)),
        control,
        last,
    )
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registered(name: &str) -> bool {
        let p = format!("icons/{name}.svg");
        crate::ui::paint::ASSETS.iter().any(|(k, _)| *k == p)
    }

    #[test]
    fn every_row_tile_is_a_registered_icon() {
        for n in tile::ALL {
            assert!(registered(n), "tile {n} is not in paint::ASSETS");
        }
        for a in crate::shortcuts::ShortcutAction::ALL {
            assert!(registered(shortcut_tile(a.key())), "{}", a.key());
            assert_ne!(
                shortcut_tile(a.key()),
                tile::SHORTCUT_FALLBACK,
                "{} has no tile of its own",
                a.key()
            );
        }
    }

    #[test]
    fn the_privacy_fact_icons_are_registered() {
        for (g, _, _) in super::super::model::privacy_facts() {
            assert!(registered(g), "{g}");
        }
    }

    #[test]
    fn the_settings_sidebar_icons_are_registered() {
        for s in super::super::model::SettingsSection::ALL {
            assert!(registered(s.icon()), "{}", s.id());
        }
    }

    #[test]
    fn countdown_keeps_a_custom_stored_value() {
        assert_eq!(countdown_options(3), COUNTDOWNS.to_vec());
        assert_eq!(countdown_options(7), [0, 1, 2, 3, 5, 7, 10]);
        assert_eq!(seconds_label(0), "Off");
        assert_eq!(seconds_label(5), "5 s");
    }
}
