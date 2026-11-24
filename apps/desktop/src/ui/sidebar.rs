//! The one sidebar component with three modes (OD-16, `design/redesign/CLAUDE.md`): `app`
//! (248 px: brand, the two capture buttons and the row of six modes, LIBRARY, INTELLIGENCE,
//! Settings), `rail` (64 px, icons with tooltips, for the full-window pages and, under 960 pt of
//! window width, for every page) and `settings` (248 px: "Back to library" and the sections).
//! Which mode a route and a width use, the rail order and the mode hints are the pure model in
//! `crate::nav`; this file only draws it (release 0.3.7, design S037).

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

use gpui::{
    div, prelude::*, px, AnyElement, App, Context, Entity, FontWeight, Render, Role, SharedString,
    Stateful, Subscription, Window,
};

use cm_i18n::{tr, tr_ctx};

use crate::agent_status::McpStatus;
use crate::nav::{
    self, AgentTone, DockAction, DockFacts, DockMode, ModeEntry, NavCounts, NavEntry, RailEntry,
    SidebarMode, TileState,
};
use crate::overlay::CaptureMode;
use crate::shortcuts::{Platform, ShortcutAction, ShortcutMap};
use crate::theme::{scale, type_scale, Color, Mode, Theme};
use crate::ui::app::{self, AppModel, Command, Route};
use crate::ui::components::{
    ellipsis_text, icon, icon_button, icon_button_tinted, logo, rec_dot, ButtonKind,
};
use crate::ui::kit::{nav_row, nav_row_tinted, NavExtras, NavState, ATTENTION};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::recording::RecState;
use crate::ui::settings::model::SettingsSection;
use crate::ui::settings_view::SettingsView;
use crate::ui::studio::ThemeGlobal;
use crate::ui::tooltip::{rich_tip, tile_lines, tip, TipLine};

/// The blue of the Preview dot and of the Agents & CLI "Ready" dot (design S037).
const PREVIEW_BLUE: Color = Color::rgb(0x8FB0FF);

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
    /// What the Long Shot and Click Guide modes need to know (board S14).
    pub dock: DockFacts,
    /// The scroll position of the rail, kept by the window so it survives a redraw and the thumb
    /// can be drawn from it (0.3.6 P-6).
    pub rail_scroll: gpui::ScrollHandle,
    /// The window's width, logical px: under 960 the sidebar is the icon rail (SB-7).
    pub width: f32,
    /// The Record… button, a view of its own so its timer ticks without redrawing the window.
    pub record: Entity<RecordButton>,
    /// A new version was installed since its notes were last opened (SB-6).
    pub version_dot: bool,
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
    let mode = SidebarMode::for_width(&p.route, p.width);
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
        // The rail holds a dozen 44 px buttons: on a short window (a 150 % screen gives about
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

/// The Audio mode's click: the Record sheet in Audio mode (VOC-1, `RecordSheetMode::Audio`).
pub(crate) fn open_record_audio(app: &Entity<AppModel>, cx: &mut App) {
    let app = app.clone();
    cx.defer(move |cx| crate::ui::recording::open_audio_sheet(&app, cx));
}

/// Quick Capture… (QC-1): the bar opens at the pointer, as the old Capture… button did.
fn open_quick_capture(app: &Entity<AppModel>, cx: &mut App) {
    let app = app.clone();
    cx.defer(move |cx| {
        crate::ui::quick_window::toggle(&app, crate::ui::quick_window::Anchor::Pointer, cx)
    });
}

/// Record… while idle opens the Record sheet; while a recording or its countdown runs the button
/// reads Stop and stops it.
fn record_click(app: &Entity<AppModel>, recording: bool, cx: &mut App) {
    if recording {
        let app = app.clone();
        cx.defer(move |cx| crate::ui::recording::stop(&app, cx));
    } else {
        open_capture(app, DockAction::Record, cx);
    }
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

/// The state written after "Agents & CLI" (SB-5): a dot and Ready, Off or Busy.
fn agent_state(t: &Theme, p: &SidebarProps) -> NavState {
    let tone = AgentTone::of(&p.status);
    NavState {
        dot: match tone {
            AgentTone::Ready => PREVIEW_BLUE,
            AgentTone::Off => t.mist,
            AgentTone::Busy => ATTENTION,
        },
        word: tone.word(),
        text: t.mist,
        aria: p.status.aria_status(),
    }
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

/// One LIBRARY or INTELLIGENCE row. All captures carries the amber attention dot (SB-3); Agents &
/// CLI the written state and its tooltip (SB-5).
fn nav_entry(t: &Theme, p: &SidebarProps, app: &Entity<AppModel>, e: &NavEntry) -> AnyElement {
    let agents = e.item == nav::NavItem::Agents;
    let row = nav_button(
        t,
        app,
        e,
        None,
        NavExtras {
            tint: None,
            state: agents.then(|| agent_state(t, p)),
            attention: e.item == nav::NavItem::AllCaptures && p.counts.needs_attention(),
        },
    );
    if agents {
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

// ---- the version (SB-6) --------------------------------------------------------------------

/// 0 not read yet, 1 the notes of this version were opened, 2 an update not seen yet.
static VERSION_SEEN: AtomicU8 = AtomicU8::new(0);

/// Whether the dot beside the version shows: the version stored when "What's new" was last opened
/// differs from this build's. A first run stores this version and shows nothing. Read from the
/// app's own settings once, then kept in memory.
pub fn version_unseen(app: &Entity<AppModel>, cx: &App) -> bool {
    match VERSION_SEEN.load(Ordering::Relaxed) {
        1 => return false,
        2 => return true,
        _ => {}
    }
    let backend = app.read(cx).backend.clone();
    let current = env!("CARGO_PKG_VERSION");
    let stored = backend
        .settings
        .snapshot()
        .get(nav::KEY_LAST_SEEN_VERSION)
        .and_then(|v| v.as_str().map(str::to_owned));
    let unseen = nav::show_version_dot(stored.as_deref(), current);
    if stored.is_none() {
        let _ = backend.settings.set_internal(
            nav::KEY_LAST_SEEN_VERSION,
            serde_json::Value::String(current.to_owned()),
        );
    }
    VERSION_SEEN.store(if unseen { 2 } else { 1 }, Ordering::Relaxed);
    unseen
}

/// A click on the version: remembers it as seen (the dot goes away) and opens Settings > About,
/// the app's version and licence page (the app has no separate release-notes page).
fn open_whats_new(app: &Entity<AppModel>, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    if let Err(e) = backend.settings.set_internal(
        nav::KEY_LAST_SEEN_VERSION,
        serde_json::Value::String(env!("CARGO_PKG_VERSION").to_owned()),
    ) {
        log::warn!("could not remember the seen version: {e}");
    }
    VERSION_SEEN.store(1, Ordering::Relaxed);
    app.update(cx, |m, _| {
        m.settings_page_request = Some(SettingsSection::About)
    });
    app::set_route(app, Route::Settings, cx);
}

fn brand_row(t: &Theme, p: &SidebarProps, app: &Entity<AppModel>) -> AnyElement {
    let app = app.clone();
    let (hover, focus) = (rgba_of(t.text_2), rgba_of(t.text));
    let version = div()
        .id("version")
        .role(Role::Button)
        .aria_label(SharedString::from(tr!(
            "Version {version}, open what\u{2019}s new",
            version = env!("CARGO_PKG_VERSION")
        )))
        .focusable()
        .tab_stop(true)
        .flex()
        .items_center()
        .gap(px(5.))
        .rounded(px(scale::RADIUS_XS))
        .border_1()
        .border_color(gpui::transparent_black())
        .text_size(px(11.))
        .text_color(rgba_of(t.faint))
        .mono()
        .cursor_pointer()
        .hover(move |s| s.text_color(hover))
        .focus_visible(move |s| s.border_color(focus))
        .tooltip(tip(tr!("What\u{2019}s new"), Vec::new()))
        .on_click(move |_, _, cx| open_whats_new(&app, cx))
        // The exact version of this build, the same text as Settings > About.
        .child(SharedString::from(env!("CARGO_PKG_VERSION")))
        .when(p.version_dot, |d| {
            d.child(
                div()
                    .size(px(5.))
                    .flex_none()
                    .rounded(px(2.5))
                    .bg(rgba_of(PREVIEW_BLUE)),
            )
        });
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
        .child(div().flex_1())
        .child(version)
        .into_any_element()
}

// ---- app mode ------------------------------------------------------------------------------

/// The chord chip at the right of a button: one string with the whole chord (the last key alone
/// when it has four, as the mode row does).
fn key_chip(t: &Theme, keys: &[String], on_rec: bool) -> Option<gpui::Div> {
    let text = nav::tile_keys(keys).join("");
    (!text.is_empty()).then(|| {
        div()
            .flex_none()
            .h(px(22.))
            .px(px(6.))
            .flex()
            .items_center()
            .rounded(px(scale::RADIUS_XS))
            .bg(rgba_of(if on_rec {
                t.ink.with_alpha(18)
            } else {
                t.text.with_alpha(8)
            }))
            .text_size(px(11.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgba_of(if on_rec { t.on_rec } else { t.text_2 }))
            .mono()
            .child(SharedString::from(text))
    })
}

/// The base of the two dock buttons: 40 px, the calm Secondary look, label and chip on one row.
fn dock_button(t: &Theme, id: &'static str, kind: ButtonKind) -> Stateful<gpui::Div> {
    let c = kind.colors(t);
    let (hover, focus) = (rgba_of(c.hover), rgba_of(t.text));
    div()
        .id(id)
        .role(Role::Button)
        .focusable()
        .tab_stop(true)
        .w_full()
        .h(px(scale::CONTROL_H))
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(9.))
        .overflow_hidden()
        .rounded(px(scale::RADIUS_MD))
        .border_1()
        .border_color(rgba_of(c.border))
        .bg(rgba_of(c.bg))
        .text_color(rgba_of(c.fg))
        .text_size(px(type_scale::UI))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .focus_visible(move |s| s.border_color(focus))
}

/// Record… (SB-1): dark with a red dot; red only while it records, then "Stop mm:ss".
fn record_button(
    t: &Theme,
    app: &Entity<AppModel>,
    keys: &[String],
    timer: Option<String>,
) -> AnyElement {
    let recording = timer.is_some();
    let kind = if recording {
        ButtonKind::Record
    } else {
        ButtonKind::Secondary
    };
    let aria = match &timer {
        Some(time) => tr_ctx!(
            "capture dock",
            "Stop recording, {time}, shortcut {chord}",
            time = time.as_str(),
            chord = nav::tile_keys(keys).join("")
        ),
        None => DockAction::Record.aria(keys),
    };
    let app = app.clone();
    let b = dock_button(t, DockAction::Record.id(), kind)
        .aria_label(SharedString::from(aria))
        .on_click(move |_, _, cx| record_click(&app, recording, cx));
    let b = if recording {
        b.font_weight(FontWeight::SEMIBOLD)
            // The ink square of Stop.
            .child(
                div()
                    .size(px(10.))
                    .flex_none()
                    .rounded(px(2.))
                    .bg(rgba_of(t.on_rec)),
            )
            .child(div().flex_none().child(nav::stop_label()))
            .children(timer.map(|time| {
                div()
                    .flex_none()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .mono()
                    .tnum()
                    .child(SharedString::from(time))
            }))
            .child(div().flex_1())
    } else {
        b.child(icon("cm-record", 16., t.rec)).child(
            div()
                .flex_1()
                .min_w(px(0.))
                .truncate()
                .child(DockAction::Record.label()),
        )
    };
    // Without a recorder (early Linux builds) the buttons stay, so the shortcut and the "not
    // available yet" answer are still there, but they read as unavailable.
    b.children(key_chip(t, keys, recording))
        .tooltip(tip(
            if recording {
                nav::stop_label()
            } else {
                DockAction::Record.label()
            },
            keys.to_vec(),
        ))
        .when(!crate::backend::recording_supported(), |d| d.opacity(0.45))
        .into_any_element()
}

/// The Record… button as a view of its own: the timer ticks here and does not redraw the window.
pub struct RecordButton {
    app: Entity<AppModel>,
    rec: Entity<RecState>,
    _sub: Subscription,
}

impl RecordButton {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        let rec = app.read(cx).rec.clone();
        let sub = cx.observe(&rec, |_, _, cx| cx.notify());
        RecordButton {
            app,
            rec,
            _sub: sub,
        }
    }
}

impl Render for RecordButton {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        let keys = {
            let m = self.app.read(cx);
            m.shortcuts
                .get(ShortcutAction::ToggleRecording)
                .keycaps(m.platform)
        };
        let hud = &self.rec.read(cx).hud;
        let timer = hud
            .phase
            .is_active()
            .then(|| hud.timer_text(Instant::now()));
        record_button(&t, &self.app, &keys, timer)
    }
}

/// Quick Capture… (SB-1): a launcher of the same weight as Record…, with its chord.
fn quick_capture_button(t: &Theme, p: &SidebarProps, app: &Entity<AppModel>) -> AnyElement {
    let keys = p
        .shortcuts
        .get(ShortcutAction::OpenQuickCapture)
        .keycaps(p.platform);
    let app = app.clone();
    dock_button(t, "qc-open", ButtonKind::Secondary)
        .aria_label(SharedString::from(nav::quick_capture_aria(&keys)))
        .on_click(move |_, _, cx| open_quick_capture(&app, cx))
        .tooltip(tip(nav::quick_capture_label(), keys.clone()))
        .child(icon("cm-capture-region", 16., t.text))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .truncate()
                .child(nav::quick_capture_label()),
        )
        .children(key_chip(t, &keys, false))
        .into_any_element()
}

/// The hover border of the running mode: `rec` at 70 %.
fn mode_running_border(t: &Theme) -> Color {
    t.rec.with_alpha(70)
}

/// One of the six modes (SB-2): its icon with the chord printed under it as plain Geist Mono 8.5,
/// a 5 px blue dot for a Preview feature (the word is in the tooltip). Each does what its old tile
/// or the mic button did; Long Shot and Click Guide keep their S14 states: while one runs the
/// cell has a `rec` border and a dot and a click brings its HUD to front; when it cannot start it
/// is dimmed, not a Tab stop, and its tooltip is the reason.
fn mode_cell(t: &Theme, p: &SidebarProps, app: &Entity<AppModel>, e: &ModeEntry) -> AnyElement {
    let mode = e.mode;
    let tile = mode.extra();
    let state = tile
        .map(|x| nav::tile_state(x, &p.dock))
        .unwrap_or(TileState::Ready);
    let active = state == TileState::Active;
    let disabled = state.is_disabled();
    let aria = match tile {
        Some(x) => nav::tile_aria(x, &state, &e.keys),
        None => nav::mode_aria(mode, &e.keys),
    };
    let (hover_bg, text, ring) = (rgba_of(t.raised), rgba_of(t.text), mode_ring(t));
    let app = app.clone();
    let d = div()
        .id(mode.id())
        .role(Role::Button)
        .aria_label(SharedString::from(aria))
        .relative()
        .w(px(32.))
        .h(px(46.))
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(4.))
        .rounded(px(8.))
        .border_1()
        .border_color(if active {
            rgba_of(mode_running_border(t))
        } else {
            rgba_of(Color(0))
        })
        .text_color(rgba_of(t.text_2));
    let d = match tile {
        Some(x) => {
            let model = nav::tile_tip(x, &state, &e.keys);
            let mut lines = tile_lines(&model);
            if mode.is_preview() && !disabled {
                // The Preview word lives here, not on the cell (SB-2).
                lines.insert(1, TipLine::Hint(crate::ui::preview_label::rail_hint()));
            }
            d.tooltip(rich_tip(model.name.clone(), lines, 200.))
        }
        None => d.tooltip(tip(mode.tip_name(), e.keys.clone())),
    };
    let d = if disabled {
        // Hoverable for the tooltip, but no pointer, no Tab stop and no click.
        d.opacity(0.4)
    } else {
        d.focusable()
            .tab_stop(true)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg).text_color(text))
            .focus_visible(move |s| s.shadow(ring))
            .on_click(move |_, _, cx| match (mode, active) {
                (DockMode::LongShot, true) => crate::ui::long_shot::bring_to_front(cx),
                (DockMode::ClickGuide, true) => crate::ui::guide_view::hud::bring_to_front(cx),
                (DockMode::LongShot, false) => app::handle_command(&app, Command::LongShot, cx),
                (DockMode::ClickGuide, false) => {
                    app::handle_command(&app, Command::NewClickGuide, cx)
                }
                (DockMode::Audio, _) => open_record_audio(&app, cx),
                (m, _) => {
                    if let Some(a) = m.dock_action() {
                        open_capture(&app, a, cx)
                    }
                }
            })
    };
    // Audio is a recording: it reads as unavailable where recording is not (as the mic button did).
    let d = if mode == DockMode::Audio && !crate::backend::recording_supported() {
        d.opacity(0.45)
    } else {
        d
    };
    d.child(icon(mode.icon(), 16., t.text_2))
        .child(
            div()
                .text_size(px(8.5))
                .text_color(rgba_of(t.mist))
                .mono()
                .whitespace_nowrap()
                .child(SharedString::from(e.printed())),
        )
        .when(mode.is_preview(), |d| {
            d.child(
                div()
                    .absolute()
                    .right(px(4.))
                    .top(px(5.))
                    .size(px(5.))
                    .rounded(px(2.5))
                    .bg(rgba_of(PREVIEW_BLUE)),
            )
        })
        .when(active, |d| {
            d.child(
                div()
                    .absolute()
                    .left(px(4.))
                    .top(px(5.))
                    .child(rec_dot(t, 6.)),
            )
        })
        .into_any_element()
}

/// The keyboard focus ring of a mode: 2 px of `text` with a 2 px gap (DESIGN section 7), drawn as
/// two spread-only shadows so nothing moves.
fn mode_ring(t: &Theme) -> Vec<gpui::BoxShadow> {
    let shadow = |color: Color, spread: f32| gpui::BoxShadow {
        color: crate::ui::paint::hsla_of(color),
        offset: gpui::point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(spread),
        inset: false,
    };
    vec![shadow(t.text, 4.), shadow(t.surface, 2.)]
}

/// The capture block (SB-1, SB-2): Quick Capture… and Record… over one row of six modes.
fn dock(t: &Theme, p: &SidebarProps, app: &Entity<AppModel>) -> AnyElement {
    let modes = nav::modes(&p.shortcuts, p.platform);
    div()
        .id("capture-dock")
        .role(Role::Group)
        .aria_label(tr_ctx!("capture dock", "Capture"))
        .p(px(10.))
        .flex()
        .flex_col()
        .gap(px(6.))
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.surface))
        .child(quick_capture_button(t, p, app))
        .child(p.record.clone())
        .child(
            div()
                .mt(px(2.))
                .flex()
                .justify_between()
                .children(modes.iter().map(|e| mode_cell(t, p, app, e))),
        )
        .into_any_element()
}

fn app_body(t: &Theme, p: &SidebarProps, app: &Entity<AppModel>) -> Vec<AnyElement> {
    let mut out = vec![
        brand_row(t, p, app),
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
            RailEntry::QuickCapture => {
                let keys = p
                    .shortcuts
                    .get(ShortcutAction::OpenQuickCapture)
                    .keycaps(p.platform);
                let app = app.clone();
                icon_button(
                    t,
                    "qc-open",
                    "cm-capture-region",
                    nav::quick_capture_aria(&keys),
                    false,
                    Some(keys),
                )
                .on_click(move |_, _, cx| open_quick_capture(&app, cx))
                .into_any_element()
            }
            RailEntry::Record => {
                let keys = p
                    .shortcuts
                    .get(ShortcutAction::ToggleRecording)
                    .keycaps(p.platform);
                let recording = p.dock.recording_running;
                let app = app.clone();
                let focus = rgba_of(t.text);
                let (bg, border, fg) = if recording {
                    (t.rec, crate::theme::Color(0), t.on_rec)
                } else {
                    (t.surface_2, t.line, t.rec)
                };
                div()
                    .id(DockAction::Record.id())
                    .role(Role::Button)
                    .aria_label(SharedString::from(if recording {
                        nav::stop_label()
                    } else {
                        DockAction::Record.aria(&keys)
                    }))
                    .focusable()
                    .tab_stop(true)
                    .size(px(scale::TARGET))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(scale::RADIUS_MD))
                    .border_1()
                    .border_color(rgba_of(border))
                    .bg(rgba_of(bg))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.9))
                    .focus_visible(move |s| s.border_color(focus))
                    .tooltip(tip(
                        if recording {
                            nav::stop_label()
                        } else {
                            DockAction::Record.label()
                        },
                        keys,
                    ))
                    .on_click(move |_, _, cx| record_click(&app, recording, cx))
                    .child(icon(
                        if recording { "cm-stop" } else { "cm-record" },
                        18.,
                        fg,
                    ))
                    .opacity(if crate::backend::recording_supported() {
                        1.0
                    } else {
                        0.45
                    })
                    .into_any_element()
            }
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
                    None,
                )
                .on_click(move |_, _, cx| app::set_route(&app, route.clone(), cx));
                if is_agents {
                    // The rail has no room for the written state: the shape and the tooltip carry
                    // it; the icon is centred in the 44 px target, so its corner is 13 px in.
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
                } else if item == nav::NavItem::AllCaptures && counts.needs_attention() {
                    let ground = if active { t.raised } else { t.sidebar };
                    button
                        .relative()
                        .child(
                            div()
                                .absolute()
                                .right(px(9.))
                                .top(px(9.))
                                .p(px(1.5))
                                .rounded(px(5.))
                                .bg(rgba_of(ground))
                                .child(crate::ui::components::dot(ATTENTION, 6.)),
                        )
                        .into_any_element()
                } else {
                    button.into_any_element()
                }
            }
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
