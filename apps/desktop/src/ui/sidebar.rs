//! The one sidebar component with three modes (OD-16, `design/redesign/CLAUDE.md`): `app`
//! (248 px: brand, capture dock, LIBRARY, INTELLIGENCE, Settings, MCP card), `rail` (64 px, icons
//! with tooltips, for the full-window pages) and `settings` (248 px: "Back to library" and the 8
//! sections). Which mode a route uses, the rail order and the dock hints are the pure model in
//! `crate::nav`; this file only draws it.

use gpui::{
    div, prelude::*, px, AnyElement, App, Entity, FontWeight, Role, SharedString, Stateful,
};

use cm_i18n::{tr, tr_ctx};

use crate::agent_status::McpStatus;
use crate::nav::{self, DockAction, NavCounts, NavEntry, RailEntry, SidebarMode};
use crate::overlay::CaptureMode;
use crate::shortcuts::{Platform, ShortcutMap};
use crate::theme::{scale, type_scale, Mode, Theme};
use crate::ui::app::{self, AppModel, Command, Route};
use crate::ui::components::{
    badge, button_with_icon, dot, ellipsis_text, icon, icon_button, label_fill, logo, ButtonKind,
};
use crate::ui::kit::nav_row;
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::model::SettingsSection;
use crate::ui::settings_view::SettingsView;
use crate::ui::tooltip::tip;

/// Everything the sidebar reads, gathered by the Studio each frame.
pub struct SidebarProps {
    pub route: Route,
    pub counts: NavCounts,
    pub platform: Platform,
    pub shortcuts: ShortcutMap,
    pub status: McpStatus,
    /// Where "Back to library" goes.
    pub library_route: Route,
    /// The settings section the page shows (highlighted in settings mode).
    pub section: SettingsSection,
}

/// Draws the sidebar for the route's mode. On the setup screen the sidebar is dimmed: still there,
/// but the screen is the point.
pub fn sidebar(
    t: &Theme,
    p: &SidebarProps,
    app: &Entity<AppModel>,
    settings: &Entity<SettingsView>,
) -> AnyElement {
    let mode = SidebarMode::of_route(&p.route);
    let body = match mode {
        SidebarMode::App => app_body(t, p, app),
        SidebarMode::Rail => rail_body(t, p, app),
        SidebarMode::Settings => settings_body(t, p, app, settings),
    };
    div()
        .id("sidebar")
        .role(Role::Navigation)
        .aria_label(tr_ctx!("landmark", "Main"))
        .w(px(mode.width()))
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(4.))
        .when(mode == SidebarMode::Rail, |d| {
            d.items_center().px(px(8.)).py(px(14.))
        })
        .when(mode != SidebarMode::Rail, |d| d.px(px(12.)).py(px(18.)))
        .bg(rgba_of(t.sidebar))
        .when(p.route == Route::Permissions, |d| d.opacity(0.5))
        .border_r_1()
        .border_color(rgba_of(if t.mode == Mode::Dark {
            t.raised
        } else {
            t.border
        }))
        .children(body)
        .into_any_element()
}

// ---- shared pieces -------------------------------------------------------------------------

fn open_capture(app: &Entity<AppModel>, action: DockAction, cx: &mut App) {
    match action {
        DockAction::Record => {
            // Opens the Record sheet, as the old header's Record… button did.
            let app = app.clone();
            cx.defer(move |cx| crate::ui::recording::open_sheet(&app, None, None, cx));
        }
        DockAction::Region => app::handle_command(app, Command::Capture(CaptureMode::Region), cx),
        DockAction::Window => app::handle_command(app, Command::Capture(CaptureMode::Window), cx),
        DockAction::Display => app::handle_command(app, Command::Capture(CaptureMode::Display), cx),
    }
}

/// The mic button's click: the Record sheet for audio (VOC-1, `RecordSheetMode::Audio`).
///
/// TODO(voice P1.1/P1.3): the Record sheet has no Audio mode yet. Until its owner wires
/// `nav::RecordSheetMode::Audio` into `open_sheet_with`, this opens the sheet as it is.
pub(crate) fn open_record_audio(app: &Entity<AppModel>, cx: &mut App) {
    let mode = nav::RecordSheetMode::Audio;
    let app = app.clone();
    cx.defer(move |cx| {
        let _ = mode;
        crate::ui::recording::open_sheet(&app, None, None, cx)
    });
}

/// The mic button of the dock and the rail: a rec-tinted square beside Record (board
/// `UiSidebar`: 42 px in the dock, 14 % rec fill, 45 % rec border, soft-red mic).
fn record_audio_button(
    t: &Theme,
    p: &SidebarProps,
    app: &Entity<AppModel>,
    size: f32,
) -> Stateful<gpui::Div> {
    let keys = nav::record_audio_keys(&p.shortcuts, p.platform);
    let app = app.clone();
    let focus = rgba_of(t.text);
    div()
        .id(nav::RECORD_AUDIO_ID)
        .role(Role::Button)
        .aria_label(SharedString::from(nav::record_audio_aria(&keys)))
        .focusable()
        .tab_stop(true)
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_MD))
        .border_1()
        .border_color(rgba_of(t.rec.with_alpha(45)))
        .bg(rgba_of(t.rec.with_alpha(14)))
        .cursor_pointer()
        .hover(|s| s.opacity(0.85))
        .focus_visible(move |s| s.border_color(focus))
        .tooltip(tip(nav::record_audio_label(), keys))
        .on_click(move |_, _, cx| open_record_audio(&app, cx))
        .child(icon("mic", 18., t.rec_soft))
}

fn nav_button(
    t: &Theme,
    app: &Entity<AppModel>,
    e: &NavEntry,
    label: Option<String>,
) -> Stateful<gpui::Div> {
    let route = e.item.route();
    let app = app.clone();
    nav_row(
        t,
        e.item.id(),
        e.item.icon(),
        label.unwrap_or_else(|| e.item.label()),
        e.count,
        e.active,
    )
    .on_click(move |_, _, cx| app::set_route(&app, route.clone(), cx))
}

fn group_label(t: &Theme, text: String) -> AnyElement {
    div()
        .px(px(10.))
        .pt(px(14.))
        .pb(px(4.))
        .text_size(px(10.5))
        .text_color(rgba_of(t.faint))
        .mono()
        .child(text)
        .into_any_element()
}

fn brand_row(t: &Theme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .px(px(10.))
        .pt(px(6.))
        .pb(px(14.))
        .child(logo(t, 22.))
        .child(
            div()
                .text_size(px(16.))
                .font_weight(FontWeight::SEMIBOLD)
                .child("CaptureMann"),
        )
        .child(badge(t, "v0.1"))
        .into_any_element()
}

/// The sidebar card: the real MCP status only (agent blue): "MCP ready" or "MCP offline" with its
/// dot. The last request and the offline reason are not drawn here (owner, Oct 23, 2026); they stay in
/// the accessible label and on the Agents & CLI page, which the card opens.
fn mcp_card(t: &Theme, p: &SidebarProps, app: &Entity<AppModel>) -> AnyElement {
    let status = &p.status;
    let lines = status.lines();
    let app = app.clone();
    let focus = rgba_of(t.text);
    div()
        .id("mcp-card")
        .role(Role::Button)
        .aria_label(SharedString::from(tr!(
            "{title}. {details}. Opens Agents and CLI",
            title = status.title(),
            details = lines.join(". ")
        )))
        .focusable()
        .tab_stop(true)
        .mt(px(12.))
        .px(px(14.))
        .py(px(12.))
        .flex()
        .flex_col()
        .rounded(px(scale::RADIUS_LG - 2.))
        .border_1()
        .border_color(rgba_of(t.agent_border))
        .bg(rgba_of(t.agent_fill))
        .cursor_pointer()
        .focus_visible(move |s| s.border_color(focus))
        .on_click(move |_, _, cx| app::set_route(&app, Route::Agents, cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .child(icon(
                    "bot",
                    14.,
                    if status.is_ready() {
                        t.agent_text
                    } else {
                        t.mist
                    },
                ))
                .child(label_fill(status.title()))
                .child(dot(if status.is_ready() { t.agent } else { t.mist }, 7.)),
        )
        .into_any_element()
}

// ---- app mode ------------------------------------------------------------------------------

/// The capture dock: the red Record… row over three tiles. The keycaps are the live chords.
fn dock(t: &Theme, p: &SidebarProps, app: &Entity<AppModel>) -> AnyElement {
    let entries = nav::dock(&p.shortcuts, p.platform);
    let record = &entries[0];
    let rec_app = app.clone();
    let record_btn = button_with_icon(
        t,
        record.action.id(),
        record.action.label(),
        ButtonKind::Record,
        Some(record.action.icon()),
    )
    .flex_1()
    .min_w(px(0.))
    .h(px(42.))
    .aria_label(SharedString::from(record.action.aria(&record.keys)))
    .child(div().flex_1())
    // One chip with the whole chord (board `UiSidebar`), so the mic button fits beside it.
    .child(
        div()
            .h(px(22.))
            .px(px(6.))
            .flex()
            .items_center()
            .rounded(px(scale::RADIUS_XS))
            .bg(rgba_of(t.ink.with_alpha(16)))
            .text_size(px(11.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgba_of(t.on_rec))
            .mono()
            .child(SharedString::from(nav::tile_keys(&record.keys).join(""))),
    )
    .on_click(move |_, _, cx| open_capture(&rec_app, DockAction::Record, cx));
    let record_row = div()
        .flex()
        .gap(px(6.))
        .child(record_btn)
        .child(record_audio_button(t, p, app, 42.));

    let tiles = entries.iter().skip(1).map(|e| {
        let action = e.action;
        let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
        let app = app.clone();
        div()
            .id(action.id())
            .role(Role::Button)
            .aria_label(SharedString::from(action.aria(&e.keys)))
            .focusable()
            .tab_stop(true)
            .flex_1()
            .min_w(px(0.))
            .h(px(64.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(3.))
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .text_color(rgba_of(t.text_2))
            .text_size(px(12.))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .focus_visible(move |s| s.border_color(focus))
            .tooltip(tip(
                match action {
                    DockAction::Region => tr!("Capture region"),
                    DockAction::Window => tr!("Capture window"),
                    _ => tr!("Capture display"),
                },
                e.keys.clone(),
            ))
            .on_click(move |_, _, cx| open_capture(&app, action, cx))
            .child(icon(action.icon(), 18., t.text_2))
            .child(ellipsis_text(action.label()))
            .child(div().flex().items_center().gap(px(2.)).children(
                nav::tile_keys(&e.keys).into_iter().map(|k| {
                    div()
                        .min_w(px(15.))
                        .h(px(15.))
                        .px(px(3.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.))
                        .bg(rgba_of(t.raised))
                        .text_size(px(10.))
                        .text_color(rgba_of(t.mist))
                        .mono()
                        .child(SharedString::from(k))
                }),
            ))
    });

    div()
        .id("capture-dock")
        .role(Role::Group)
        .aria_label(tr_ctx!("capture dock", "Capture"))
        .p(px(8.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.surface_2))
        .child(record_row)
        .child(div().flex().gap(px(6.)).children(tiles))
        .into_any_element()
}

fn app_body(t: &Theme, p: &SidebarProps, app: &Entity<AppModel>) -> Vec<AnyElement> {
    let mut out = vec![
        brand_row(t),
        dock(t, p, app),
        group_label(t, tr_ctx!("sidebar group", "LIBRARY")),
    ];
    out.extend(
        nav::library_entries(&p.route, &p.counts)
            .iter()
            .map(|e| nav_button(t, app, e, None).into_any_element()),
    );
    out.push(group_label(t, tr_ctx!("sidebar group", "INTELLIGENCE")));
    out.extend(
        nav::intelligence_entries(&p.route, &p.counts)
            .iter()
            .map(|e| nav_button(t, app, e, None).into_any_element()),
    );
    out.push(div().flex_1().into_any_element());
    let settings = nav::entries(&p.route, &p.counts)
        .into_iter()
        .find(|e| e.item == nav::NavItem::Settings)
        .expect("Settings is a nav item");
    out.push(nav_button(t, app, &settings, None).into_any_element());
    out.push(mcp_card(t, p, app));
    out
}

// ---- rail mode -----------------------------------------------------------------------------

fn rail_body(t: &Theme, p: &SidebarProps, app: &Entity<AppModel>) -> Vec<AnyElement> {
    let entries = nav::dock(&p.shortcuts, p.platform);
    let counts = &p.counts;
    nav::rail_layout()
        .into_iter()
        .map(|slot| match slot {
            RailEntry::Brand => div()
                .h(px(scale::TARGET))
                .flex()
                .items_center()
                .justify_center()
                .mb(px(6.))
                .child(logo(t, 24.))
                .into_any_element(),
            RailEntry::Record => {
                let e = &entries[0];
                let app = app.clone();
                let focus = rgba_of(t.text);
                div()
                    .id(e.action.id())
                    .role(Role::Button)
                    .aria_label(SharedString::from(e.action.aria(&e.keys)))
                    .focusable()
                    .tab_stop(true)
                    .size(px(scale::TARGET))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(scale::RADIUS_MD))
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .bg(rgba_of(t.rec))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.9))
                    .focus_visible(move |s| s.border_color(focus))
                    .tooltip(tip(
                        tr_ctx!("capture dock", "Record\u{2026}"),
                        e.keys.clone(),
                    ))
                    .on_click(move |_, _, cx| open_capture(&app, DockAction::Record, cx))
                    .child(icon(e.action.icon(), 18., t.on_rec))
                    .into_any_element()
            }
            RailEntry::Region => {
                let e = &entries[1];
                let app = app.clone();
                icon_button(
                    t,
                    e.action.id(),
                    e.action.icon(),
                    e.action.aria(&e.keys),
                    false,
                    Some(e.keys.clone()),
                )
                .on_click(move |_, _, cx| open_capture(&app, DockAction::Region, cx))
                .into_any_element()
            }
            RailEntry::RecordAudio => {
                record_audio_button(t, p, app, scale::TARGET).into_any_element()
            }
            RailEntry::Divider => div()
                .w(px(32.))
                .h(px(1.))
                .my(px(8.))
                .bg(rgba_of(t.border))
                .into_any_element(),
            RailEntry::Nav(item) => {
                let route = item.route();
                let app = app.clone();
                let active = nav::NavItem::of_route(&p.route) == item;
                let label = match item.count(counts) {
                    Some(n) => tr!("{name}, {n}", name = item.label(), n = n),
                    None => item.label(),
                };
                icon_button(t, item.id(), item.icon(), label, active, Some(Vec::new()))
                    .on_click(move |_, _, cx| app::set_route(&app, route.clone(), cx))
                    .into_any_element()
            }
            RailEntry::Spacer => div().flex_1().into_any_element(),
            RailEntry::Mcp => {
                let status = &p.status;
                let app = app.clone();
                let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
                div()
                    .id("mcp-card")
                    .role(Role::Button)
                    .aria_label(SharedString::from(tr!(
                        "{title}. {details}. Opens Agents and CLI",
                        title = status.title(),
                        details = status.lines().join(". ")
                    )))
                    .focusable()
                    .tab_stop(true)
                    .relative()
                    .size(px(scale::TARGET))
                    .mt(px(4.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(scale::RADIUS_MD))
                    .border_1()
                    .border_color(rgba_of(t.agent_border))
                    .bg(rgba_of(t.agent_fill))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .focus_visible(move |s| s.border_color(focus))
                    .tooltip(tip(status.title(), Vec::new()))
                    .on_click(move |_, _, cx| app::set_route(&app, Route::Agents, cx))
                    .child(icon(
                        "bot",
                        18.,
                        if status.is_ready() {
                            t.agent_text
                        } else {
                            t.mist
                        },
                    ))
                    .child(
                        div()
                            .absolute()
                            .top(px(7.))
                            .right(px(7.))
                            .child(dot(if status.is_ready() { t.agent } else { t.mist }, 7.)),
                    )
                    .into_any_element()
            }
        })
        .collect()
}

// ---- settings mode -------------------------------------------------------------------------

fn settings_body(
    t: &Theme,
    p: &SidebarProps,
    app: &Entity<AppModel>,
    settings: &Entity<SettingsView>,
) -> Vec<AnyElement> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    let back_app = app.clone();
    let back_to = p.library_route.clone();
    let back = div()
        .id("settings-back")
        .role(Role::Button)
        .aria_label(tr!("Back to library"))
        .focusable()
        .tab_stop(true)
        .h(px(scale::CONTROL_H))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(scale::RADIUS_SM))
        .border_1()
        .border_color(gpui::transparent_black())
        .text_size(px(type_scale::UI))
        .text_color(rgba_of(t.text_2))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .focus_visible(move |s| s.border_color(focus))
        .on_click(move |_, _, cx| app::set_route(&back_app, back_to.clone(), cx))
        .child(icon("chevron-left", 16., t.text_2))
        .child(ellipsis_text(tr!("Back to library")));
    let title = div()
        .px(px(10.))
        .pt(px(10.))
        .pb(px(10.))
        .text_size(px(type_scale::H2))
        .font_weight(FontWeight::SEMIBOLD)
        .child(tr_ctx!("navigation", "Settings"));
    let rows = SettingsSection::ALL.iter().map(|&s| {
        let (app, settings) = (app.clone(), settings.clone());
        nav_row(t, s.id(), s.icon(), s.label(), None, s == p.section).on_click(move |_, _, cx| {
            settings.update(cx, |v, cx| v.select(s, cx));
            app::set_route(&app, s.route(), cx);
        })
    });
    let mut out = vec![
        back.into_any_element(),
        title.into_any_element(),
        div()
            .id("settings-sections")
            .role(Role::List)
            .aria_label(tr!("Settings sections"))
            .flex()
            .flex_col()
            .gap(px(2.))
            .children(rows.map(IntoElement::into_any_element))
            .into_any_element(),
    ];
    out.push(div().flex_1().into_any_element());
    out.push(mcp_card(t, p, app));
    out
}
