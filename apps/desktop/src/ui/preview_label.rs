//! The Preview label (design S24): one small neutral component that marks a feature as a
//! preview (Long Shot and Click Guide in 0.3.4) wherever people start it, plus the popover it
//! opens on a click and the one-time intro card.
//!
//! The label is a 1 px dashed pill with no fill, in three sizes: XS (14 px, 9/600, a corner tag on
//! tiles), S (18 px, 11/600, next to a row title) and M (22 px, 12/600, in a page title, with a
//! flask icon). Colours are neutral only (no new brand colour): dark border `#55555C` and text
//! `#C9C9CF`, light `#8A8A90` and `#44444B`; hover `#8B8B93` with a 6 % fill; keyboard focus a 2 px
//! `text` ring with a 2 px gap. It is never shown on HUDs while capturing, on captured or exported
//! images, library cards, toasts or in the editor canvas, and at most once per surface.
//!
//! Which features are previews is the feature-stage registry's business (`cm_core::features`); a
//! preview is only a label, the feature is always available (the Settings switch was dropped on
//! Nov 16, 2026). This file writes one app-internal flag per feature
//! (`ui.preview_intro.<feature>`) for the intro card.

use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, prelude::*, px, AnyElement, App, ElementId, Entity, FontWeight, MouseButton, Role,
    SharedString,
};
use serde_json::{Map, Value};

use crate::theme::{Color, Mode, Theme};
use crate::ui::app::AppModel;
use crate::ui::components::{button, icon, ButtonKind};
use crate::ui::paint::{hsla_of, rgba_of};
use crate::ui::tooltip::{rich_tip, TipLine};

/// A feature that carries the label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewFeature {
    LongShot,
    ClickGuide,
}

impl PreviewFeature {
    pub const ALL: [PreviewFeature; 2] = [PreviewFeature::LongShot, PreviewFeature::ClickGuide];

    /// The stable key: `ui.preview_intro.<key>` is its flag.
    pub fn key(self) -> &'static str {
        match self {
            PreviewFeature::LongShot => "long_shot",
            PreviewFeature::ClickGuide => "click_guide",
        }
    }

    /// The feature's name as the sidebar tile shows it.
    pub fn name(self) -> String {
        match self {
            PreviewFeature::LongShot => crate::nav::ExtraTile::LongShot.label(),
            PreviewFeature::ClickGuide => crate::nav::ExtraTile::ClickGuide.label(),
        }
    }

    fn intro_key(self) -> String {
        format!("ui.preview_intro.{}", self.key())
    }
}

/// The three sizes of the label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// A corner tag on tiles and mode buttons.
    Xs,
    /// Beside a row title, a menu row or a nav row.
    S,
    /// In a page title, with the flask icon.
    M,
}

/// The measures of one size, px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub height: f32,
    pub pad_x: f32,
    pub font: f32,
    /// The flask icon (size M only).
    pub icon: Option<f32>,
}

pub fn metrics(size: Size) -> Metrics {
    match size {
        Size::Xs => Metrics {
            height: 14.,
            pad_x: 5.,
            font: 9.,
            icon: None,
        },
        Size::S => Metrics {
            height: 18.,
            pad_x: 7.,
            font: 11.,
            icon: None,
        },
        Size::M => Metrics {
            height: 22.,
            pad_x: 9.,
            font: 12.,
            icon: Some(12.),
        },
    }
}

/// The colours of the label (tokens `preview.border`, `preview.text`, `preview.border-hover`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub border: Color,
    pub text: Color,
    pub border_hover: Color,
}

pub fn palette(mode: Mode) -> Palette {
    match mode {
        Mode::Dark => Palette {
            border: Color::rgb(0x55555C),
            text: Color::rgb(0xC9C9CF),
            border_hover: Color::rgb(0x8B8B93),
        },
        Mode::Light => Palette {
            border: Color::rgb(0x8A8A90),
            text: Color::rgb(0x44444B),
            border_hover: Color::rgb(0x55555C),
        },
    }
}

/// "Preview" (Vietnamese "Xem trước"). Never truncated.
pub fn label_text() -> String {
    tr_ctx!("preview label", "Preview")
}

/// The accessible name of the label.
pub fn aria_name() -> String {
    tr!("Preview. New and still changing.")
}

fn tooltip_body() -> String {
    tr!("New and still changing. Your files are safe and stay readable. Click to send feedback.")
}

/// The line the rail's tooltip adds, where the tag has no room.
pub fn rail_hint() -> String {
    tr!("Preview: new and still changing.")
}

/// The suffix a native menu item gets where no pill can be drawn: "New Click Guide (Preview)".
pub fn menu_text(title: &str) -> String {
    tr!("{title} (Preview)", title = title)
}

/// The label as an element. `ground` is the colour behind it: a corner tag straddling a border
/// fills with it so the border does not run through the text; `None` leaves it transparent.
/// With `app` a click opens the popover, without it the label only explains itself on hover.
pub fn label(
    t: &Theme,
    size: Size,
    feature: PreviewFeature,
    ground: Option<Color>,
    app: Option<Entity<AppModel>>,
) -> AnyElement {
    let m = metrics(size);
    let p = palette(t.mode);
    let ring = ring(t, ground.unwrap_or(t.ground));
    let hover_fill = rgba_of(t.text.with_alpha(6));
    let hover_border = rgba_of(p.border_hover);
    let id: ElementId = format!("preview-{}-{:?}", feature.key(), size).into();
    let tip = rich_tip(label_text(), vec![TipLine::Body(tooltip_body())], 240.);
    let interactive = app.is_some();
    let mut d = div()
        .id(id)
        .role(if interactive {
            Role::Button
        } else {
            Role::Image
        })
        .aria_label(SharedString::from(aria_name()))
        .h(px(m.height))
        .px(px(m.pad_x))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.))
        .rounded(px(999.))
        .border_1()
        .border_dashed()
        .border_color(rgba_of(p.border))
        .text_size(px(m.font))
        .line_height(px(m.height - 2.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgba_of(p.text))
        .whitespace_nowrap()
        .tooltip(tip)
        .hover(move |s| s.border_color(hover_border).bg(hover_fill));
    if interactive {
        d = d
            .focusable()
            .tab_stop(true)
            .focus_visible(move |s| s.shadow(ring.clone()));
    }
    if let Some(g) = ground {
        d = d.bg(rgba_of(g));
    }
    if let Some(sz) = m.icon {
        d = d.child(icon("flask-conical", sz, p.text));
    }
    let d = d.child(SharedString::from(label_text()));
    match app {
        Some(app) => d
            .cursor_pointer()
            .on_click(move |_, _, cx| open_popover(&app, feature, cx))
            .into_any_element(),
        None => d.into_any_element(),
    }
}

/// The label's width, px, in the current language (for rows that must reserve room for it).
pub fn width(size: Size) -> f32 {
    let m = metrics(size);
    let icon = m.icon.map_or(0.0, |i| i + 4.0);
    crate::ui::bar_layout::text_w(&label_text(), m.font, false, 600) as f32
        + icon
        + 2.0 * m.pad_x
        + 2.0
}

/// The keyboard focus ring: 2 px of `text` with a 2 px gap.
fn ring(t: &Theme, gap: Color) -> Vec<gpui::BoxShadow> {
    let shadow = |color: Color, spread: f32| gpui::BoxShadow {
        color: hsla_of(color),
        offset: gpui::point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(spread),
        inset: false,
    };
    vec![shadow(t.text, 4.), shadow(gap, 2.)]
}

// ---- feedback ----------------------------------------------------------------------------------

const ISSUES_NEW: &str = "https://github.com/craft-mann/capturemann/issues/new";

/// Percent-encodes `s` for a URL query value.
pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(b));
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The GitHub issue form for feedback on a preview: the feature's name and the app version are
/// filled in; no logs, screenshots or paths are attached.
pub fn feedback_url(feature_name: &str, version: &str) -> String {
    let title = format!("Preview feedback: {feature_name}");
    let body = format!("Feature: {feature_name} (preview)\nApp version: {version}\n\nWhat works, and what does not:\n");
    format!(
        "{ISSUES_NEW}?title={}&body={}&labels=preview",
        url_encode(&title),
        url_encode(&body)
    )
}

/// Opens the feedback form in the system browser.
pub fn send_feedback(feature: PreviewFeature, cx: &mut App) {
    // The GitHub form takes the English feature name, whatever the UI language is.
    let name = match feature {
        PreviewFeature::LongShot => "Long Shot",
        PreviewFeature::ClickGuide => "Click Guide",
    };
    cx.open_url(&feedback_url(name, env!("CARGO_PKG_VERSION")));
}

// ---- the popover and the intro card -------------------------------------------------------------

/// What the Studio shows above its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewCard {
    /// The card a click on the label opens.
    Popover(PreviewFeature),
    /// The one-time intro, top right.
    Intro(PreviewFeature),
}

pub fn open_popover(app: &Entity<AppModel>, feature: PreviewFeature, cx: &mut App) {
    app.update(cx, |m, cx| {
        m.preview_card = Some(PreviewCard::Popover(feature));
        cx.notify();
    });
}

pub fn close_card(app: &Entity<AppModel>, cx: &mut App) {
    app.update(cx, |m, cx| {
        if m.preview_card.take().is_some() {
            cx.notify();
        }
    });
}

/// Whether the intro of `feature` is still due: the flag is not set.
pub fn intro_due(settings: &Map<String, Value>, feature: PreviewFeature) -> bool {
    !settings
        .get(&feature.intro_key())
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Shows the intro card the first time a preview feature opens (once per feature, remembered in
/// the app's own settings file). Never replaces a popover that is open.
pub fn show_intro_if_due(app: &Entity<AppModel>, feature: PreviewFeature, cx: &mut App) {
    let (settings, open) = {
        let m = app.read(cx);
        (m.backend.settings.clone(), m.preview_card.is_some())
    };
    if open || !intro_due(&settings.snapshot(), feature) {
        return;
    }
    let _ = settings.set_internal(&feature.intro_key(), Value::Bool(true));
    app.update(cx, |m, cx| {
        m.preview_card = Some(PreviewCard::Intro(feature));
        cx.notify();
    });
}

fn popover_lines(feature: PreviewFeature) -> [String; 3] {
    [
        tr!(
            "It works and it is ready to use, but how it looks and behaves may still change in the next releases."
        ),
        match feature {
            PreviewFeature::LongShot => {
                tr!("Your Long Shots are normal screenshots and stay readable.")
            }
            PreviewFeature::ClickGuide => {
                tr!("Your Click Guides are normal guides and stay readable.")
            }
        },
        tr!("Tell us what works and what doesn\u{2019}t."),
    ]
}

fn intro_line(feature: PreviewFeature) -> String {
    match feature {
        PreviewFeature::LongShot => tr!(
            "Pick a window or an area, then scroll; the pictures are joined into one tall image."
        ),
        PreviewFeature::ClickGuide => tr!(
            "Click through a task once; each click becomes a numbered step. Clicks only, never keystrokes."
        ),
    }
}

/// The popover or the intro card, as an absolutely placed overlay for the Studio (`None` when
/// nothing is open). The popover has a transparent backdrop that closes it.
pub fn card_overlay(app: &Entity<AppModel>, t: &Theme, cx: &App) -> Option<AnyElement> {
    let card = app.read(cx).preview_card?;
    let (feature, intro) = match card {
        PreviewCard::Popover(f) => (f, false),
        PreviewCard::Intro(f) => (f, true),
    };
    let name = feature.name();
    let title = if intro {
        name.clone()
    } else {
        tr!("{name} is in preview", name = name.clone())
    };
    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .text_size(px(12.5))
        .text_color(rgba_of(t.text_2));
    if intro {
        body = body.child(SharedString::from(intro_line(feature)));
    } else {
        for line in popover_lines(feature) {
            body = body.child(SharedString::from(line));
        }
    }
    let (app_a, app_b) = (app.clone(), app.clone());
    let feedback = button(
        t,
        "preview-feedback",
        tr!("Send feedback"),
        ButtonKind::Secondary,
    )
    .on_click(move |_, _, cx| {
        send_feedback(feature, cx);
        if intro {
            close_card(&app_a, cx);
        }
    });
    let footer = if intro {
        div()
            .flex()
            .gap(px(8.))
            .justify_end()
            .child(feedback)
            .child(
                button(t, "preview-got-it", tr!("Got it"), ButtonKind::Primary)
                    .on_click(move |_, _, cx| close_card(&app_b, cx)),
            )
    } else {
        div().flex().flex_col().gap(px(8.)).child(feedback)
    };
    let panel = crate::ui::components::card(t)
        .id(if intro {
            "preview-intro"
        } else {
            "preview-popover"
        })
        .role(Role::Dialog)
        .aria_label(SharedString::from(title.clone()))
        .w(px(if intro { 300. } else { 320. }))
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgba_of(t.text))
                        .child(SharedString::from(title)),
                )
                .child(label(t, Size::S, feature, None, None)),
        )
        .child(body)
        .child(footer);
    let app_close = app.clone();
    let mut root = div()
        .id("preview-card-layer")
        .absolute()
        .top_0()
        .left_0()
        .size_full();
    if intro {
        // Non-blocking: no backdrop, top right of the surface.
        root = root.child(div().absolute().top(px(56.)).right(px(16.)).child(panel));
    } else {
        root = root
            .occlude()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                close_card(&app_close, cx)
            })
            .child(
                div()
                    .absolute()
                    .top(px(72.))
                    .left(px(crate::theme::scale::SIDEBAR_W + 8.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(panel),
            );
    }
    Some(root.into_any_element())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_sizes_follow_the_board() {
        assert_eq!(
            (
                metrics(Size::Xs).height,
                metrics(Size::Xs).font,
                metrics(Size::Xs).pad_x
            ),
            (14., 9., 5.)
        );
        assert_eq!((metrics(Size::S).height, metrics(Size::S).font), (18., 11.));
        assert_eq!(
            (
                metrics(Size::M).height,
                metrics(Size::M).font,
                metrics(Size::M).icon
            ),
            (22., 12., Some(12.))
        );
        assert!(metrics(Size::Xs).icon.is_none() && metrics(Size::S).icon.is_none());
    }

    #[test]
    fn the_colours_are_the_neutral_tokens_of_the_board() {
        let d = palette(Mode::Dark);
        assert_eq!(
            (d.border, d.text, d.border_hover),
            (
                Color::rgb(0x55555C),
                Color::rgb(0xC9C9CF),
                Color::rgb(0x8B8B93)
            )
        );
        let l = palette(Mode::Light);
        assert_eq!(
            (l.border, l.text),
            (Color::rgb(0x8A8A90), Color::rgb(0x44444B))
        );
    }

    #[test]
    fn every_registered_preview_has_a_preview_feature() {
        for f in cm_core::features::previews() {
            assert!(
                PreviewFeature::ALL.iter().any(|p| p.key() == f.id),
                "{} has no PreviewFeature",
                f.id
            );
        }
    }

    #[test]
    fn the_intro_is_due_once_per_feature() {
        let mut s = Map::new();
        assert!(intro_due(&s, PreviewFeature::ClickGuide));
        s.insert("ui.preview_intro.click_guide".into(), Value::Bool(true));
        assert!(!intro_due(&s, PreviewFeature::ClickGuide));
        assert!(intro_due(&s, PreviewFeature::LongShot), "per feature");
    }

    #[test]
    fn the_feedback_url_carries_the_feature_and_the_version_only() {
        let u = feedback_url("Long Shot", "0.3.4");
        assert!(u.starts_with("https://github.com/craft-mann/capturemann/issues/new?"));
        assert!(u.contains("Long%20Shot") && u.contains("0.3.4"));
        assert!(!u.contains(' ') && !u.contains('\n'));
        assert_eq!(url_encode("a b&c"), "a%20b%26c");
    }

    #[test]
    fn menu_items_get_the_suffix() {
        assert_eq!(menu_text("Long Shot"), "Long Shot (Preview)");
    }
}
