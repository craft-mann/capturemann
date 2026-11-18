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
use crate::nav::{
    self, DockAction, DockFacts, ExtraTile, NavCounts, NavEntry, RailEntry, SidebarMode, TileState,
};
use crate::overlay::CaptureMode;
use crate::shortcuts::{Platform, ShortcutMap};
use crate::theme::{scale, type_scale, Mode, Theme};
use crate::ui::app::{self, AppModel, Command, Route};
use crate::ui::components::{
    badge, button_with_icon, ellipsis_text, icon, icon_button, icon_button_tinted, logo, ButtonKind,
};
use crate::ui::kit::{nav_row, nav_row_tinted, NavExtras, NavStatus};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::preview_label::{PreviewFeature, Size as PreviewSize};
use crate::ui::settings::model::SettingsSection;
use crate::ui::settings_view::SettingsView;
use crate::ui::tooltip::{rich_tip, tile_lines, tip, TipLine};

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
    /// The recording editor shows an audio recording: Audio is the highlighted item, not Recordings.
    pub audio_open: bool,
    /// What the second dock row (Long Shot, Click Guide) needs to know (board S14).
    pub dock: DockFacts,
    /// The scroll position of the rail, kept by the window so it survives a redraw and the thumb
    /// can be drawn from it (0.3.6 P-6).
    pub rail_scroll: gpui::ScrollHandle,
}

impl SidebarProps {
    /// The route the highlight follows: the page's own, except that an audio recording's editor
    /// belongs under Audio.
    fn highlight(&self) -> Route {
        nav::highlight_route(&self.route, self.audio_open)
    }
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
    let side = div()
        .id("sidebar")
        .role(Role::Navigation)
        .aria_label(tr_ctx!("landmark", "Main"))
        .when(mode == SidebarMode::Rail, |d| d.w_full())
        .when(mode != SidebarMode::Rail, |d| d.w(px(mode.width())))
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(4.))
        // The rail holds fourteen 44 px buttons: on a short window (a 150 % screen gives about
        // 660 px) it scrolls instead of letting Settings fall below the window (0.3.5 Q-7).
        .when(mode == SidebarMode::Rail, |d| {
            d.items_center()
                .px(px(8.))
                .py(px(10.))
                .track_scroll(&p.rail_scroll)
                .overflow_y_scroll()
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
        .children(body);
    if mode != SidebarMode::Rail {
        return side.into_any_element();
    }
    // The rail scrolls when the window is short; a thin thumb on its edge says so.
    div()
        .relative()
        .w(px(mode.width()))
        .h_full()
        .flex_none()
        .child(side)
        .child(crate::ui::scroll::thumb_overlay(t, &p.rail_scroll, 3.))
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

/// The mic button's click: the Record sheet in Audio mode (VOC-1, `RecordSheetMode::Audio`).
pub(crate) fn open_record_audio(app: &Entity<AppModel>, cx: &mut App) {
    let app = app.clone();
    cx.defer(move |cx| crate::ui::recording::open_audio_sheet(&app, cx));
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

/// The Agents & CLI icon shows the MCP status: agent blue only when the app serves the IPC
/// endpoint; offline and "another app owns the socket" use mist (board S14). Other items keep the
/// text colour.
fn item_tint(t: &Theme, p: &SidebarProps, item: nav::NavItem) -> Option<crate::theme::Color> {
    (item == nav::NavItem::Agents).then(|| {
        if p.status.is_ready() {
            if t.mode == Mode::Dark {
                t.agent
            } else {
                t.agent_text
            }
        } else {
            t.mist
        }
    })
}

/// The status shape and the accessible-name tail of the Agents & CLI row.
fn item_status(p: &SidebarProps, item: nav::NavItem) -> Option<NavStatus> {
    (item == nav::NavItem::Agents).then(|| NavStatus {
        mark: p.status.mark(),
        aria: p.status.aria_status(),
    })
}

/// The Agents & CLI tooltip: the status line carries the state, so colour is not the only cue.
fn agents_tip(p: &SidebarProps) -> impl Fn(&mut gpui::Window, &mut App) -> gpui::AnyView + 'static {
    let tip = p.status.tip();
    let mark = p.status.mark();
    let mut lines = vec![TipLine::Status(tip.line, mark)];
    lines.extend(tip.detail.into_iter().map(|d| {
        if mark == crate::agent_status::StatusMark::Dot {
            TipLine::Hint(d)
        } else {
            TipLine::Body(d)
        }
    }));
    rich_tip(nav::NavItem::Agents.label(), lines, 280.)
}

fn nav_button(
    t: &Theme,
    app: &Entity<AppModel>,
    e: &NavEntry,
    label: Option<String>,
    extras: NavExtras,
) -> Stateful<gpui::Div> {
    let route = e.item.route();
    let app = app.clone();
    nav_row_tinted(
        t,
        e.item.id(),
        e.item.icon(),
        label.unwrap_or_else(|| e.item.label()),
        e.count,
        e.active,
        extras,
    )
    .on_click(move |_, _, cx| app::set_route(&app, route.clone(), cx))
}

/// One LIBRARY or INTELLIGENCE row; Agents & CLI also carries the status shape and tooltip.
fn nav_entry(t: &Theme, p: &SidebarProps, app: &Entity<AppModel>, e: &NavEntry) -> AnyElement {
    let row = nav_button(
        t,
        app,
        e,
        None,
        NavExtras {
            tint: item_tint(t, p, e.item),
            status: item_status(p, e.item),
        },
    );
    if e.item == nav::NavItem::Agents {
        row.tooltip(agents_tip(p)).into_any_element()
    } else if e.item == nav::NavItem::Guides {
        // The label shrinks with an ellipsis in a narrow sidebar: the full name is the tooltip.
        row.tooltip(tip(e.item.label(), Vec::new()))
            .into_any_element()
    } else {
        row.into_any_element()
    }
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
        // "v0.2.1": the exact version of this build, the same text as Settings > About (it showed
        // only major.minor until Oct 28, 2026, and a patch release looked like no release).
        .child(badge(t, format!("v{}", env!("CARGO_PKG_VERSION"))))
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
    // The mic button takes 48 px of the row: tighter padding and gaps keep the label off the icon
    // when the text is 30 % longer (QA 0.3.6 D2).
    .px(px(10.))
    .gap(px(4.))
    .overflow_hidden()
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
    // Without a recorder (early Linux builds) the buttons stay, so the shortcut and the "not
    // available yet" answer are still there, but they read as unavailable.
    let record_row = if crate::backend::recording_supported() {
        record_row
    } else {
        record_row.opacity(0.45)
    };

    // QC-1: "Capture…" under Record…, a neutral primary button with its keycap.
    let qc_keys = p
        .shortcuts
        .get(crate::shortcuts::ShortcutAction::OpenQuickCapture)
        .keycaps(p.platform);
    let qc_app = app.clone();
    let qc_label = tr_ctx!("capture dock", "Capture\u{2026}");
    let capture_btn = button_with_icon(
        t,
        "qc-open",
        qc_label.clone(),
        ButtonKind::Primary,
        Some("cm-capture-region"),
    )
    .w_full()
    .h(px(42.))
    .aria_label(SharedString::from(tr_ctx!(
        "capture dock",
        "{label}, {keys}",
        label = qc_label,
        keys = qc_keys.join(" ")
    )))
    .child(div().flex_1())
    .child(
        div()
            .h(px(22.))
            .px(px(6.))
            .flex()
            .items_center()
            .rounded(px(scale::RADIUS_XS))
            .bg(rgba_of(t.primary_fg.with_alpha(22)))
            .text_size(px(11.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgba_of(t.primary_fg))
            .mono()
            .child(SharedString::from(nav::tile_keys(&qc_keys).join(""))),
    )
    .on_click(move |_, _, cx| {
        let app = qc_app.clone();
        cx.defer(move |cx| {
            crate::ui::quick_window::toggle(&app, crate::ui::quick_window::Anchor::Pointer, cx)
        });
    });

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
            .child(keycap_row(t, &e.keys))
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
        .child(capture_btn)
        .child(div().flex().gap(px(6.)).children(tiles))
        // Row 2 (board S14): Long Shot and Click Guide, two to a row.
        .child(
            div().flex().gap(px(6.)).children(
                nav::ExtraTile::ALL
                    .into_iter()
                    .map(|tile| extra_tile(t, p, app, tile, false)),
            ),
        )
        .into_any_element()
}

// ---- the second dock row (board S14) ---------------------------------------------------------

/// The hover border of a tile (kit `#3A3A42` on dark; the strong line on light).
fn tile_hover_border(t: &Theme) -> crate::theme::Color {
    if t.mode == Mode::Dark {
        crate::theme::Color::rgb(0x3A3A42)
    } else {
        t.line
    }
}

/// The keyboard focus ring of a tile: 2 px of `text` with a 2 px gap (DESIGN section 7), drawn as
/// two spread-only shadows so nothing moves.
fn tile_ring(t: &Theme, gap: crate::theme::Color) -> Vec<gpui::BoxShadow> {
    let shadow = |color: crate::theme::Color, spread: f32| gpui::BoxShadow {
        color: crate::ui::paint::hsla_of(color),
        offset: gpui::point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(spread),
        inset: false,
    };
    vec![shadow(t.text, 4.), shadow(gap, 2.)]
}

/// The chord of a dock tile as small keycaps under its label (nothing when there is no chord).
fn keycap_row(t: &Theme, keys: &[String]) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .children(nav::tile_keys(keys).into_iter().map(|k| {
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
        }))
}

/// Where the XS Preview tag sits on a dock tile: its top-right corner straddling the border (S24),
/// px from the tile's right and top edges (negative: outside).
pub(crate) const TAG_RIGHT: f32 = -4.;
pub(crate) const TAG_TOP: f32 = -9.;

/// The label of a dock tile: wraps to at most `LABEL_LINES` lines of `LABEL_LINE` px at the width
/// of the tile, each cut with an ellipsis at the end when it still does not fit, never clipped or
/// cut at the start. Its height follows the text (one or two lines) and the tile centres it.
pub(crate) fn tile_label(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .w_full()
        .min_w(px(0.))
        .max_h(px(
            nav::tile_metrics::LABEL_LINE * nav::tile_metrics::LABEL_LINES
        ))
        .text_center()
        .line_height(px(nav::tile_metrics::LABEL_LINE))
        .line_clamp(nav::tile_metrics::LABEL_LINES as usize)
        .text_ellipsis()
        .child(text.into())
}

/// A Long Shot or Click Guide tile: in the dock a 54 px tile with a 16 px icon and a label, in the
/// rail a 44 px icon button. Ready starts the capture; while it runs the border is `rec` at 70 % with
/// a 7 px rec dot at the top right and a click brings its HUD to front; disabled is dimmed, not a Tab
/// stop, and still shows its reason as the tooltip.
fn extra_tile(
    t: &Theme,
    p: &SidebarProps,
    app: &Entity<AppModel>,
    tile: ExtraTile,
    rail: bool,
) -> AnyElement {
    let state = nav::tile_state(tile, &p.dock);
    let keys = tile
        .shortcut()
        .map(|a| p.shortcuts.get(a).keycaps(p.platform))
        .unwrap_or_default();
    // Both tiles of the row get the same height: room for the chord when either has one.
    let row_has_keys = nav::ExtraTile::ALL.into_iter().any(|x| {
        x.shortcut()
            .is_some_and(|a| !p.shortcuts.get(a).keycaps(p.platform).is_empty())
    });
    // The most label lines either tile of the row takes: both get the height that keeps the icon
    // clear of the Preview tag.
    let row_lines = nav::ExtraTile::ALL
        .into_iter()
        .map(|x| {
            nav::tile_metrics::label_lines(crate::ui::bar_layout::text_w(
                &x.label(),
                11.,
                false,
                400,
            ) as f32)
        })
        .fold(1.0_f32, f32::max);
    let tip_model = nav::tile_tip(tile, &state, &keys);
    let mut lines = tile_lines(&tip_model);
    if rail && !disabled_state(&state) {
        // The rail has no room for the tag: the tooltip says it (design S24).
        lines.insert(1, TipLine::Hint(crate::ui::preview_label::rail_hint()));
    }
    let tooltip = rich_tip(tip_model.name.clone(), lines, 200.);
    let active = state == TileState::Active;
    let disabled = state.is_disabled();
    let (hover_bg, hover_border, text) = (
        rgba_of(t.raised),
        rgba_of(tile_hover_border(t)),
        rgba_of(t.text),
    );
    let ring = tile_ring(t, if rail { t.sidebar } else { t.surface_2 });
    let app_for_label = app.clone();
    let app = app.clone();
    let border = if active {
        rgba_of(t.rec.with_alpha(70))
    } else if rail {
        rgba_of(crate::theme::Color(0))
    } else {
        rgba_of(t.border)
    };
    let d = div()
        .id(tile.id())
        .role(Role::Button)
        .aria_label(SharedString::from(nav::tile_aria(tile, &state, &keys)))
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(if rail { scale::RADIUS_MD } else { 9. }))
        .border_1()
        .border_color(border)
        .text_color(rgba_of(t.text_2))
        .text_size(px(11.))
        .tooltip(tooltip);
    let d = if rail {
        d.size(px(scale::TARGET)).flex_none()
    } else {
        d.flex_1()
            .min_w(px(0.))
            .h(px(nav::tile_metrics::height(row_has_keys, row_lines)))
            .flex_col()
            .gap(px(3.))
            .px(px(4.))
            .bg(rgba_of(t.surface))
    };
    let d = if disabled {
        // Hoverable for the tooltip, but no pointer, no Tab stop and no click.
        d.opacity(0.4)
    } else {
        d.focusable()
            .tab_stop(true)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg).border_color(hover_border).text_color(text))
            .focus_visible(move |s| s.shadow(ring))
            .on_click(move |_, _, cx| match (tile, active) {
                (ExtraTile::LongShot, true) => crate::ui::long_shot::bring_to_front(cx),
                (ExtraTile::ClickGuide, true) => crate::ui::guide_view::hud::bring_to_front(cx),
                (ExtraTile::LongShot, false) => {
                    app::handle_command(&app, app::Command::LongShot, cx)
                }
                (ExtraTile::ClickGuide, false) => {
                    app::handle_command(&app, app::Command::NewClickGuide, cx)
                }
            })
    };
    let d = d.child(icon(tile.icon(), 16., t.text_2));
    let d = if rail {
        d
    } else {
        // Two lines at most (Vietnamese "Hướng dẫn / thao tác"), each cut with an ellipsis when it
        // still does not fit; the tooltip has the name. The box is a block (not a flex row, whose
        // text item would size to its whole text and be clipped on both sides) as wide as the tile,
        // so the text wraps at the tile's width (`tile_label`).
        d.child(tile_label(tile.label()))
            .when(!keys.is_empty(), |d| d.child(keycap_row(t, &keys)))
    };
    // The Preview tag (design S24): XS, on the tile's top-right corner, straddling its border.
    // The run's red dot moves to the top-left so the two never meet. In the rail the tile is an
    // icon button: the tooltip and the accessible name carry "preview" there.
    let feature = match tile {
        ExtraTile::LongShot => PreviewFeature::LongShot,
        ExtraTile::ClickGuide => PreviewFeature::ClickGuide,
    };
    // The tag is drawn in a deferred layer: after every tile and row of the dock, so nothing (the
    // neighbour tile, the row above, the tile's own border or hover fill) is painted over it. It
    // fills with the dock's own ground (`surface_2`), so the tile's border does not run through it.
    let d = d.when(!rail, |d| {
        d.child(
            gpui::deferred(
                div()
                    .absolute()
                    .right(px(TAG_RIGHT))
                    .top(px(TAG_TOP))
                    .child(crate::ui::preview_label::label(
                        t,
                        PreviewSize::Xs,
                        feature,
                        Some(t.surface_2),
                        Some(app_for_label),
                    )),
            )
            .with_priority(1),
        )
    });
    d.when(active, |d| {
        d.child(
            div()
                .absolute()
                .top(px(6.))
                .left(px(6.))
                .child(crate::ui::components::rec_dot(t, 7.)),
        )
    })
    .into_any_element()
}

fn app_body(t: &Theme, p: &SidebarProps, app: &Entity<AppModel>) -> Vec<AnyElement> {
    let mut out = vec![
        brand_row(t),
        dock(t, p, app),
        group_label(t, tr_ctx!("sidebar group", "LIBRARY")),
    ];
    out.extend(
        nav::library_entries(&p.highlight(), &p.counts)
            .iter()
            .map(|e| nav_entry(t, p, app, e)),
    );
    out.push(group_label(t, tr_ctx!("sidebar group", "INTELLIGENCE")));
    out.extend(
        nav::intelligence_entries(&p.highlight(), &p.counts)
            .iter()
            .map(|e| nav_entry(t, p, app, e)),
    );
    out.push(div().flex_1().into_any_element());
    let settings = nav::entries(&p.highlight(), &p.counts)
        .into_iter()
        .find(|e| e.item == nav::NavItem::Settings)
        .expect("Settings is a nav item");
    out.push(nav_button(t, app, &settings, None, NavExtras::default()).into_any_element());
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
                .flex_none()
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
                    .opacity(if crate::backend::recording_supported() {
                        1.0
                    } else {
                        0.45
                    })
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
            RailEntry::RecordAudio => record_audio_button(t, p, app, scale::TARGET)
                .opacity(if crate::backend::recording_supported() {
                    1.0
                } else {
                    0.45
                })
                .into_any_element(),
            RailEntry::Divider => div()
                .flex_none()
                .w(px(32.))
                .h(px(1.))
                .my(px(8.))
                .bg(rgba_of(t.border))
                .into_any_element(),
            RailEntry::Nav(item) => {
                let route = item.route();
                let app = app.clone();
                let active = nav::NavItem::of_route(&p.highlight()) == item;
                let label = match item.count(counts) {
                    Some(n) => tr!("{name}, {n}", name = item.label(), n = n),
                    None if item == nav::NavItem::Agents => {
                        tr!(
                            "{name}, {status}",
                            name = item.label(),
                            status = p.status.aria_status()
                        )
                    }
                    None => item.label(),
                };
                let is_agents = item == nav::NavItem::Agents;
                let button = icon_button_tinted(
                    t,
                    item.id(),
                    item.icon(),
                    label,
                    active,
                    // The Agents & CLI tooltip is the rich status one below.
                    (!is_agents).then(Vec::new),
                    item_tint(t, p, item),
                )
                .on_click(move |_, _, cx| app::set_route(&app, route.clone(), cx));
                if is_agents {
                    // Same shape and tooltip as the sidebar row (board S14); the icon is centred in
                    // the 44 px target, so its corner is 13 px in.
                    let ground = if active { t.raised } else { t.sidebar };
                    button
                        .relative()
                        .tooltip(agents_tip(p))
                        .child(
                            crate::ui::kit::status_badge(t, p.status.mark(), ground)
                                .right(px(8.))
                                .bottom(px(8.)),
                        )
                        .into_any_element()
                } else {
                    button.into_any_element()
                }
            }
            RailEntry::Extra(tile) => extra_tile(t, p, app, tile, true).into_any_element(),
            RailEntry::Spacer => div().flex_1().min_h(px(8.)).into_any_element(),
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
    let rows = SettingsSection::visible().into_iter().map(|s| {
        let (app, settings) = (app.clone(), settings.clone());
        nav_row(t, s.id(), s.icon(), s.nav_label(), None, s == p.section).on_click(
            move |_, _, cx| {
                settings.update(cx, |v, cx| v.select(s, cx));
                app::set_route(&app, s.route(), cx);
            },
        )
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
    out
}

/// A disabled tile's tooltip is its reason only.
fn disabled_state(state: &TileState) -> bool {
    state.is_disabled()
}

#[cfg(test)]
mod tile_label_tests {
    use super::*;
    use gpui::{Context, IntoElement, Render, TestAppContext, Window};

    /// A dock tile's content box: the half row of the 248 px sidebar less the tile's border and
    /// padding (`layout_width_tests::DOCK_ROW2_ROOM`).
    const ROOM: f32 = (scale::SIDEBAR_W - 24. - 2. - 16. - 6.) / 2. - 2. - 8.;

    /// The label in the column the tile gives it, or (`old`) the first layout: a flex row whose
    /// text item sizes to its whole text.
    struct Probe {
        text: String,
        old: bool,
    }

    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let label = if self.old {
                div()
                    .max_w_full()
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .text_center()
                    .line_height(px(13.))
                    .child(
                        div()
                            .debug_selector(|| "tile-label-text".to_owned())
                            .child(SharedString::from(self.text.clone())),
                    )
            } else {
                tile_label(self.text.clone())
            };
            div()
                .w(px(ROOM))
                .flex()
                .flex_col()
                .items_center()
                .text_size(px(11.))
                .child(label.debug_selector(|| "tile-label".to_owned()))
        }
    }

    fn measure(cx: &mut TestAppContext, text: &str, old: bool) -> (f32, f32) {
        let window = cx.add_window(|_, _| Probe {
            text: text.to_owned(),
            old,
        });
        let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
        vcx.run_until_parked();
        let b = vcx
            .debug_bounds(if old { "tile-label-text" } else { "tile-label" })
            .expect("label drawn");
        (f32::from(b.size.width), f32::from(b.size.height))
    }

    /// CG-11: the Vietnamese name of Click Guide wraps inside the tile (its box is never wider
    /// than the tile, so it is neither clipped on both sides nor cut at the start) and takes at
    /// most two 13 px lines. The first layout (a flex row) made the text item as wide as the whole
    /// text, which `overflow_hidden` then clipped at both ends.
    #[gpui::test]
    fn the_vietnamese_label_wraps_to_two_lines_inside_the_tile(cx: &mut TestAppContext) {
        let vi = cm_i18n::with_language(cm_i18n::Lang::Vi, || ExtraTile::ClickGuide.label());
        let (w, h) = measure(cx, &vi, false);
        assert!(
            w <= ROOM + 0.5,
            "{vi:?}: box {w} px wider than the tile ({ROOM})"
        );
        assert!(
            h > nav::tile_metrics::LABEL_LINE + 0.5
                && h <= 2. * nav::tile_metrics::LABEL_LINE + 0.5,
            "{vi:?}: {h} px high, wanted two lines"
        );
    }

    /// S24 board: the XS tag sits at right -4 px, top -8 px of the tile (straddling its top-right
    /// corner, never over the icon or the label), filled with the dock's own ground.
    #[test]
    fn the_xs_tag_sits_on_the_tile_corner_as_on_the_board() {
        assert_eq!((TAG_RIGHT, TAG_TOP), (-4., -9.));
        let m = crate::ui::preview_label::metrics(PreviewSize::Xs);
        // Its 14 px height reaches 5 px into the tile: above the icon row (8 px of padding).
        assert!(m.height + TAG_TOP <= nav::tile_metrics::TAG_REACH);
    }

    /// The same measurement on the first layout: its text item is as wide as the whole text, wider
    /// than the tile, so the clipping box cut it on both sides.
    #[gpui::test]
    fn the_first_layout_overflowed_the_tile(cx: &mut TestAppContext) {
        let vi = cm_i18n::with_language(cm_i18n::Lang::Vi, || ExtraTile::ClickGuide.label());
        let (w, _) = measure(cx, &vi, true);
        assert!(
            w > ROOM + 0.5,
            "{vi:?}: the old box was {w} px, tile {ROOM}"
        );
    }
}
