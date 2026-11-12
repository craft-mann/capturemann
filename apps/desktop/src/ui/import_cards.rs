//! Import media on the library page (board I3): the Import… header button, the "Imported" badge,
//! the progress strip on a card, and the cards of files that have no capture yet (waiting, in
//! progress, failed). Words and state: `crate::import_flow`. Not checked by eye in this build.

use cm_i18n::{tr, tr_ctx};
use gpui::{div, prelude::*, px, AnyElement, Entity, FontWeight, Role, SharedString};

use crate::import_flow::{self as f, CardState, ImportUi, Kind};
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::AppModel;
use crate::ui::components::{button_with_icon, icon, shrink_text, ButtonKind};
use crate::ui::import_actions as act;
use crate::ui::paint::{rgba_of, TextExt};

/// "Import…" with the chord, in the library header (board I1).
pub fn header_button(t: &Theme, app: &Entity<AppModel>) -> AnyElement {
    let app = app.clone();
    let key = if crate::shortcuts::Platform::current() == crate::shortcuts::Platform::Mac {
        "\u{2318}I"
    } else {
        "Ctrl+I"
    };
    div()
        .id("library-import")
        .role(Role::Button)
        .aria_label(SharedString::from(tr!("Import files")))
        .focusable()
        .tab_stop(true)
        .h(px(38.))
        .px(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(scale::RADIUS_MD))
        .border_1()
        .border_color(rgba_of(t.line))
        .bg(rgba_of(t.surface_2))
        .text_size(px(type_scale::UI))
        .text_color(rgba_of(t.text))
        .cursor_pointer()
        .hover(|s| s.opacity(0.85))
        .on_click(move |_, _, cx| act::open(&app, Vec::new(), cx))
        .child(icon("cm-import", 16., t.text))
        .child(SharedString::from(tr_ctx!("button", "Import\u{2026}")))
        .child(
            div()
                .text_size(px(11.))
                .text_color(rgba_of(t.mist))
                .mono()
                .child(SharedString::from(key)),
        )
        .into_any_element()
}

/// The "Imported" badge on a thumbnail.
pub fn imported_badge(_t: &Theme) -> AnyElement {
    div()
        .absolute()
        .top(px(40.))
        .left(px(10.))
        .h(px(24.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(5.))
        .rounded(px(6.))
        .bg(rgba_of(Theme::DARK.ink.with_alpha(78)))
        .text_size(px(11.))
        .text_color(rgba_of(Theme::DARK.text))
        .child(icon("cm-import", 12., Theme::DARK.text))
        .child(shrink_text(tr_ctx!("card badge", "Imported")))
        .into_any_element()
}

/// The step line and the bar of a running import step or transcription: the step (once), the
/// percentage when there is one, and a bar (dimmed and full while there is no progress to show,
/// for example while waiting). `on_dark`: drawn on a thumbnail (the dark palette) rather than on a
/// page (the theme's own). One look for the library strip and for the recording's page.
pub fn progress_body(t: &Theme, step: &str, frac: Option<f64>, on_dark: bool) -> gpui::Div {
    let pct = frac.map(|f| (f * 100.0).round().clamp(0.0, 100.0));
    let p = if on_dark { &Theme::DARK } else { t };
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.5))
                .text_color(rgba_of(p.text))
                .child(icon(
                    if step == tr!("Preparing for editing") {
                        "refresh-cw"
                    } else {
                        "cm-import"
                    },
                    14.,
                    p.agent_text,
                ))
                .child(shrink_text(SharedString::from(step.to_owned())).flex_1())
                .children(pct.map(|p| div().mono().child(SharedString::from(format!("{p:.0}%"))))),
        )
        .child(
            div().h(px(4.)).rounded(px(2.)).bg(rgba_of(p.line)).child(
                div()
                    .h_full()
                    .w(gpui::relative(pct.map_or(1.0, |p| (p / 100.0) as f32)))
                    .rounded(px(2.))
                    .bg(rgba_of(if pct.is_some() {
                        t.agent
                    } else {
                        t.agent.with_alpha(35)
                    })),
            ),
        )
}

/// The bottom strip of a thumbnail while its file is being imported or transcribed.
pub fn progress_strip(t: &Theme, step: &str, frac: Option<f64>) -> AnyElement {
    div()
        .absolute()
        .left(px(10.))
        .right(px(10.))
        .bottom(px(8.))
        .flex()
        .flex_col()
        .child(progress_body(t, step, frac, true))
        .into_any_element()
}

/// One card of a file with no capture on screen yet (waiting, in a step, failed), drawn in the
/// grid (`width` px) at the top of "Today". Its step shows once, in the strip; a failed card has
/// the amber border, the reason, Show file and Dismiss.
pub fn pending_card(
    t: &Theme,
    ui: &ImportUi,
    (ri, ci): (usize, usize),
    width: f32,
    app: &Entity<AppModel>,
) -> AnyElement {
    let Some(c) = ui.runs.get(ri).and_then(|r| r.cards.get(ci)) else {
        return div().into_any_element();
    };
    let failed = matches!(c.state, CardState::Failed(_));
    let glyph = if failed {
        "file-x"
    } else if c.kind == Kind::Audio {
        "cm-import-audio"
    } else if c.kind == Kind::Image {
        "image"
    } else {
        "cm-import-video"
    };
    let tile = div()
        .relative()
        .w_full()
        .h(px((width * 10. / 16.).round()))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .border_color(rgba_of(if failed { t.warning } else { t.border }))
        .bg(rgba_of(t.raised))
        .child(icon(glyph, 32., if failed { t.warning } else { t.mist }))
        .child(imported_badge(t))
        .children(match &c.state {
            CardState::Step(step, frac) => Some(progress_strip(t, step, *frac)),
            CardState::Waiting => Some(progress_strip(t, &tr!("Waiting"), None)),
            _ => None,
        });
    let below = match &c.state {
        CardState::Failed(why) => {
            let (a1, path) = (app.clone(), c.path.clone());
            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(rgba_of(t.warning))
                            .child(SharedString::from(why.clone())),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                button_with_icon(
                                    t,
                                    SharedString::from(format!("import-show-{ri}-{ci}")),
                                    tr!("Show file"),
                                    ButtonKind::Secondary,
                                    Some("folder-open"),
                                )
                                .on_click(move |_, _, _| act::show_file(&path)),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("import-dismiss-{ri}-{ci}")))
                                    .role(Role::Button)
                                    .aria_label(tr!("Dismiss"))
                                    .focusable()
                                    .cursor_pointer()
                                    .text_size(px(13.))
                                    .text_color(rgba_of(t.text_3))
                                    .child(tr!("Dismiss"))
                                    .on_click(move |_, _, cx| act::dismiss_card(&a1, ri, ci, cx)),
                            ),
                    ),
            )
        }
        _ => None,
    };
    div()
        .w(px(width))
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(tile)
        .child(
            div()
                .px(px(2.))
                .truncate()
                .text_size(px(type_scale::UI))
                .font_weight(FontWeight::MEDIUM)
                .child(SharedString::from(c.title.clone())),
        )
        .children(below)
        .into_any_element()
}

#[allow(dead_code)]
fn _words() -> String {
    f::drop_title()
}
