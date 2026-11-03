//! Import media on the library page (board I3): the Import… header button, the "Imported" badge,
//! the progress strip on a card, and the cards of files that have no capture yet (waiting, in
//! progress, failed). Words and state: `crate::import_flow`. Not checked by eye in this build.

use cm_i18n::{tr, tr_ctx};
use gpui::{div, prelude::*, px, AnyElement, Entity, FontWeight, Role, SharedString};

use crate::import_flow::{self as f, CardState, ImportUi, Kind};
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::AppModel;
use crate::ui::components::{button_with_icon, icon, ButtonKind};
use crate::ui::import_actions as act;
use crate::ui::paint::{rgba_of, TextExt};

/// "Import…" with the chord, in the library header (board I1).
pub fn header_button(t: &Theme, app: &Entity<AppModel>) -> AnyElement {
    let app = app.clone();
    let key = if cfg!(target_os = "macos") {
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
        .child(SharedString::from(tr_ctx!("card badge", "Imported")))
        .into_any_element()
}

/// The bottom strip of a thumbnail while its file is being imported: the step, the percentage and
/// a bar.
pub fn progress_strip(t: &Theme, step: &str, frac: f64) -> AnyElement {
    let pct = (frac * 100.0).round().clamp(0.0, 100.0);
    div()
        .absolute()
        .left(px(10.))
        .right(px(10.))
        .bottom(px(8.))
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.5))
                .text_color(rgba_of(Theme::DARK.text))
                .child(icon(
                    if step == tr!("Preparing for editing") {
                        "refresh-cw"
                    } else {
                        "cm-import"
                    },
                    14.,
                    Theme::DARK.agent_text,
                ))
                .child(div().flex_1().child(SharedString::from(step.to_owned())))
                .child(div().mono().child(SharedString::from(format!("{pct:.0}%")))),
        )
        .child(
            div()
                .h(px(4.))
                .rounded(px(2.))
                .bg(rgba_of(Theme::DARK.line))
                .child(
                    div()
                        .h_full()
                        .w(gpui::relative((pct / 100.0) as f32))
                        .rounded(px(2.))
                        .bg(rgba_of(t.agent)),
                ),
        )
        .into_any_element()
}

/// The cards above the grid for files that have no capture in the list yet: waiting, in progress
/// or failed. Once a file is imported its capture shows in the grid (with the Imported badge).
pub fn pending_strip(t: &Theme, ui: &ImportUi, app: &Entity<AppModel>) -> Option<AnyElement> {
    let mut cards: Vec<AnyElement> = Vec::new();
    for (ri, run) in ui.runs.iter().enumerate() {
        for (ci, c) in run.cards.iter().enumerate() {
            if c.dismissed {
                continue;
            }
            let (border, body): (_, AnyElement) = match &c.state {
                CardState::Done => continue,
                CardState::Failed(why) => {
                    let a1 = app.clone();
                    let path = c.path.clone();
                    (
                        t.warning,
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
                                            .id(SharedString::from(format!(
                                                "import-dismiss-{ri}-{ci}"
                                            )))
                                            .role(Role::Button)
                                            .aria_label(tr!("Dismiss"))
                                            .focusable()
                                            .cursor_pointer()
                                            .text_size(px(13.))
                                            .text_color(rgba_of(t.text_3))
                                            .child(tr!("Dismiss"))
                                            .on_click(move |_, _, cx| {
                                                act::dismiss_card(&a1, ri, ci, cx)
                                            }),
                                    ),
                            )
                            .into_any_element(),
                    )
                }
                CardState::Waiting => (
                    t.border,
                    div()
                        .text_size(px(12.5))
                        .text_color(rgba_of(t.mist))
                        .child(SharedString::from(tr!("Waiting")))
                        .into_any_element(),
                ),
                CardState::Step(step, frac) => (
                    t.border,
                    div()
                        .text_size(px(12.5))
                        .text_color(rgba_of(t.mist))
                        .child(SharedString::from(format!(
                            "{} \u{b7} {:.0}%",
                            step,
                            frac * 100.0
                        )))
                        .into_any_element(),
                ),
            };
            let failed = matches!(c.state, CardState::Failed(_));
            let glyph = if failed {
                "file-x"
            } else if c.kind == Kind::Audio {
                "cm-import-audio"
            } else {
                "cm-import-video"
            };
            let tile = div()
                .relative()
                .w_full()
                .h(px(130.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(scale::RADIUS_LG))
                .border_1()
                .border_color(rgba_of(border))
                .bg(rgba_of(t.raised))
                .child(icon(glyph, 32., if failed { t.warning } else { t.mist }))
                .child(imported_badge(t))
                .children(match &c.state {
                    CardState::Step(step, frac) => Some(progress_strip(t, step, *frac)),
                    _ => None,
                });
            cards.push(
                div()
                    .w(px(250.))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(tile)
                    .child(
                        div()
                            .truncate()
                            .text_size(px(type_scale::UI))
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(c.title.clone())),
                    )
                    .child(body)
                    .into_any_element(),
            );
        }
    }
    if cards.is_empty() {
        return None;
    }
    Some(
        div()
            .id("import-pending")
            .flex_none()
            .flex()
            .flex_wrap()
            .gap(px(16.))
            .py(px(10.))
            .children(cards)
            .into_any_element(),
    )
}

#[allow(dead_code)]
fn _words() -> String {
    f::drop_title()
}
