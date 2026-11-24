//! The Quick Capture bar (QC-1, QC-4): one slim row of icon buttons with the kind, the target, the
//! options of the kind, the preset, the action and a toggle that collapses the bar to its
//! compact form, every key printed under its icon. A borderless floating window like the Launcher: it takes the keyboard (the app is
//! activated, the previous app gets focus back on close), it is excluded from screen capture, and
//! it closes before the capture starts and waits [`CAPTURE_DELAY_MS`] first.
//!
//! The window is the bar plus a strip under it for the tooltips; the preset picker, a value menu
//! and the name screen open as a popover in that place. The window only draws
//! `quick::state::PanelState` and sends what the user chose to `ui::quick_flow`; the capture
//! itself is the existing actions.

use std::time::Duration;

use cm_core::preset::{Preset, PresetKind, PresetTarget};
use cm_i18n::{tr, tr_ctx};
use gpui::{
    deferred, div, prelude::*, px, App, Context, Entity, FocusHandle, FontWeight, Global,
    KeyDownEvent, MouseButton, Render, Role, ScrollHandle, SharedString, Window, WindowHandle,
};

use crate::launcher::commands::CAPTURE_DELAY_MS;
use crate::quick::place::{self, bar, panel_rect};
use crate::quick::presets::summary_text;
use crate::quick::state::{
    controls, kind_icon, kind_key, panel_key, picker_step, seconds_badge, target_icon, target_key,
    tip_text, Ctl, Mods, PanelKey, PanelState, Screen, Tile,
};
use crate::recording::geometry::Rect;
use crate::shortcuts::{Platform, ShortcutAction};
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::{self, AppModel};
use crate::ui::components::{icon, logo};
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::preview_label::{label as preview_label, PreviewFeature, Size};
use crate::ui::recording::{panel_options, pointer_display, visible_part};
use crate::ui::text_field::{TextField, TextFieldEvent};

/// How long the pointer or the keyboard focus rests on a control before its tooltip shows.
pub const TIP_DELAY_MS: u64 = 400;
/// How often the pointer is looked at to let clicks through the strip under the bar.
const STRIP_POLL_MS: u64 = 50;

/// Where the bar opens (QC-1 "Opening").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The shortcut, the dock and the Launcher: near the pointer, on the display that has it.
    Pointer,
    /// The tray or menu-bar item: under the icon.
    Tray,
}

struct PanelSlot(Option<WindowHandle<QuickPanel>>);
impl Global for PanelSlot {}

/// Where the bar was when a run started from it: the bar comes back there once the run is over.
struct LastHome(Option<Rect>);
impl Global for LastHome {}

/// The app that was frontmost when the bar opened (macOS), to hand focus back to.
struct PreviousApp(Option<i32>);
impl Global for PreviousApp {}

fn dismiss(window: &mut Window, cx: &mut App, restore: bool) {
    crate::surfaces::close_self(window);
    cx.set_global(PanelSlot(None));
    let previous = cx.try_global::<PreviousApp>().and_then(|p| p.0);
    cx.set_global(PreviousApp(None));
    if let (true, Some(pid)) = (restore, previous) {
        cx.defer(move |_| crate::launcher::focus::activate(pid));
    }
}

/// The shortcut, the tray item, the dock button and the Launcher row: opens the bar, or closes
/// it when it is already open.
pub fn toggle(app: &Entity<AppModel>, anchor: Anchor, cx: &mut App) {
    let open = cx.try_global::<PanelSlot>().and_then(|s| s.0).filter(|h| {
        h.update(cx, |_, window, cx| dismiss(window, cx, true))
            .is_ok()
    });
    if open.is_some() {
        return;
    }
    open_panel(app, anchor, cx);
}

pub fn open_panel(app: &Entity<AppModel>, anchor: Anchor, cx: &mut App) {
    open_panel_at(app, anchor, None, cx);
}

/// [`open_panel`], or with `at` (the bar's old top-left corner) where the bar comes back after a
/// run: the same place when it is still on the display under the pointer.
fn open_panel_at(app: &Entity<AppModel>, anchor: Anchor, at: Option<Rect>, cx: &mut App) {
    crate::ui::quick_flow::refresh_presets(app, cx);
    let (state, backend) = {
        let m = app.read(cx);
        (m.quick.panel.clone(), m.backend.clone())
    };
    let _ = backend;
    let Some(display) = pointer_display(cx) else {
        return;
    };
    let visible = visible_part(&display.1);
    // One row where the display is wide enough, else two (QC-1 "Size"), or the compact bar when
    // it was left collapsed; the window adds the strip for the tooltips.
    let (win_w, win_h) = place::window_size_view(visible.w, state.collapsed());
    let pointer = crate::ui::launcher_window::pointer(cx)
        .map(|p| (f64::from(f32::from(p.x)), f64::from(f32::from(p.y))));
    let (px_, py_) = crate::quick::place::open_point(pointer, win_h, &visible);
    // From the tray the bar is on the icon where the system gives its position (macOS status
    // item, Windows notification area); where it does not (Linux) it opens near the pointer.
    let icon = (anchor == Anchor::Tray)
        .then(|| app.read(cx).tray.as_ref().and_then(|t| t.icon_frame()))
        .flatten()
        .filter(|(ix, iy, _, _)| visible_contains(&display.1, *ix, *iy));
    let back = at
        .filter(|r| visible.contains(r.x, r.y))
        .map(|r| place::toggled_home(&r, (win_w, win_h), &visible, true));
    let global = match (back, icon) {
        (Some(r), _) => r,
        (None, Some(icon)) => crate::quick::place::tray_rect(icon, win_w, win_h, &visible),
        (None, None) => panel_rect((px_, py_), win_w, win_h, &visible),
    };
    cx.set_global(PreviousApp(crate::launcher::focus::frontmost_other_app()));
    let app2 = app.clone();
    let opened = cx.open_window(
        panel_options(display, global, true, true),
        move |window, cx| {
            // The bar is never in a screenshot or a recording (QC-1.AC2).
            // No system shadow or border: the window is taller than the bar (tooltip strip) and
            // the system would outline the whole rectangle.
            if let Err(e) = crate::platform_window::apply(
                window,
                crate::platform_window::Tweaks {
                    exclude_from_capture: !crate::recording::demo::capturable(),
                    no_shadow: true,
                    ..Default::default()
                },
            ) {
                log::warn!("could not tweak the Quick Capture window: {e}");
            }
            cx.new(|cx| QuickPanel::new(app2, state, global, visible, window, cx))
        },
    );
    match opened {
        Ok(h) => {
            crate::surfaces::opened(crate::surfaces::Surface::QuickCapture, h);
            cx.set_global(PanelSlot(Some(h)));
            crate::surfaces::activate_app(cx, "quick capture opened");
            // Windows only gives the keyboard to the foreground window: with the desktop or another
            // program in front the bar opened without it (Windows QA F-1). The other systems take the
            // keys through the app activation above.
            if cfg!(windows) {
                let _ = h.update(cx, |_, window, _| {
                    crate::platform_window::take_foreground(window);
                });
            }
            crate::surfaces::take_keyboard_x11(cx, h);
            // The system can place a new window a few pixels off the requested frame (3 px high on
            // the lab laptop's Windows, DPI rounding); every later resize used the exact frame, so
            // the bar moved by that amount the first time the picker closed (Windows F-4). Give the
            // window its exact frame once, now.
            let _ = h.update(cx, |panel, window, cx| {
                panel.apply_frame(panel.home, window, cx);
            });
        }
        Err(e) => app::push_toast(
            app,
            crate::flows::error_toast_live(
                || tr!("Couldn\u{2019}t open Quick Capture"),
                e.to_string(),
            ),
            cx,
        ),
    }
}

/// The bar stays until the user closes it (QC-1 "Window behaviour"): it only steps aside for a
/// run (a screenshot on Linux would show it) and comes back, where it was, once the capture, the
/// recording or the Long Shot, the after-capture bar and the toast are over. Pressing the shortcut meanwhile
/// opens it at once and ends the wait.
fn return_when_idle(app: &Entity<AppModel>, cx: &mut App) {
    let app = app.clone();
    cx.spawn(async move |cx| {
        let mut calm = 0u32;
        for _ in 0..place::RETURN_MAX_POLLS {
            cx.background_executor()
                .timer(Duration::from_millis(place::RETURN_POLL_MS))
                .await;
            let stop = cx.update(|cx| {
                if cx.try_global::<PanelSlot>().is_some_and(|s| s.0.is_some()) {
                    return true; // the user opened it again
                }
                let busy = app.read(cx).capture_busy
                    || app.read(cx).toast_window.is_some()
                    || crate::ui::long_shot::is_running(cx)
                    || crate::ui::recording::is_active(&app, cx)
                    || crate::ui::quick_after::is_open(cx);
                calm = place::calm_step(calm, busy);
                if calm >= place::RETURN_CALM_POLLS {
                    let at = cx.try_global::<LastHome>().and_then(|h| h.0);
                    open_panel_at(&app, Anchor::Pointer, at, cx);
                    return true;
                }
                false
            });
            if stop {
                return;
            }
        }
    })
    .detach();
}

fn visible_contains(display: &crate::recording::geometry::Rect, x: f64, y: f64) -> bool {
    display.contains(x, y)
}

/// What the popover under the bar shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Body {
    Main,
    /// The preset picker (QC-4).
    Picker,
    /// "Save current as preset…": asks for the name.
    Name,
}

/// Where a tooltip sits against its control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TipAt {
    Left,
    Center,
    Right,
}

/// A corner badge on a button.
enum Badge {
    /// A value ("3s").
    Text(String),
    /// A menu (the chevron).
    Menu,
}

/// What a bar button looks like.
struct Cell {
    ctl: Ctl,
    id: SharedString,
    glyph: &'static str,
    /// The key printed under the icon.
    key: Option<String>,
    /// The tooltip text, also the accessible name.
    tip: String,
    /// The raised chip in a recessed track (the selected kind and target).
    selected: bool,
    /// The light fill and the dot (an option that is on).
    on: bool,
    badge: Option<Badge>,
    at: TipAt,
}

pub struct QuickPanel {
    app: Entity<AppModel>,
    st: PanelState,
    focus: FocusHandle,
    scroll: ScrollHandle,
    body: Body,
    /// The highlighted picker row: 0 is "No preset", then the presets, then the two links.
    pick_at: usize,
    /// The window at rest (the bar and the tooltip strip), global points.
    home: Rect,
    /// The visible part of the display the bar is on.
    visible: Rect,
    /// Two rows, on a narrow display (the expanded bar only: the collapsed bar is one row).
    two_rows: bool,
    /// The window can be moved (macOS, Windows): a popover without room under the bar opens
    /// above it.
    movable: bool,
    /// A drag in progress on X11: the pointer and the window's corner (root pixels) at the press.
    x11_drag: Option<(f64, f64, i32, i32)>,
    /// The window lets the mouse through right now (the pointer is over the strip under the bar).
    ignoring: bool,
    /// The popover is open above the bar, and its height.
    upward: bool,
    pop_h: f64,
    /// The control with the keyboard focus (Tab).
    ctl: Option<Ctl>,
    /// The control under the pointer.
    hover: Option<Ctl>,
    /// The control whose tooltip is waiting or showing, and whether it shows.
    tip: Option<Ctl>,
    tip_shown: bool,
    tip_seq: u64,
    /// The value control whose menu is open.
    menu: Option<Tile>,
    name: Entity<TextField>,
    name_error: Option<String>,
}

impl QuickPanel {
    fn new(
        app: Entity<AppModel>,
        st: PanelState,
        home: Rect,
        visible: Rect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        // The bar no longer closes when it loses the activation, so it takes the keys back
        // whenever it is active again (after another app or window had them).
        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                window.focus(&this.focus, cx);
            }
        })
        .detach();
        let name = cx.new(|cx| {
            TextField::new(
                tr_ctx!("field", "Preset name"),
                tr!("Preset name, e.g. Bug report"),
                cx,
            )
            .without_icon()
        });
        cx.subscribe_in(
            &name,
            window,
            |this, _, ev: &TextFieldEvent, window, cx| match ev {
                TextFieldEvent::Submitted => this.save_named(window, cx),
                // QC-1 "Keys": Esc on the name screen goes back to the picker it came from.
                TextFieldEvent::Escaped => {
                    window.focus(&this.focus, cx);
                    this.open_picker(window, cx);
                }
                TextFieldEvent::Changed(_) => {
                    this.name_error = None;
                    cx.notify();
                }
            },
        )
        .detach();
        // The strip under the bar is transparent: while the pointer is over it, the window lets
        // the mouse through (release 0.3.7, macOS K5).
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(STRIP_POLL_MS))
                .await;
            if this
                .update_in(cx, |this, window, _| this.strip_hit_test(window))
                .is_err()
            {
                break;
            }
        })
        .detach();
        QuickPanel {
            app,
            st,
            focus,
            scroll: ScrollHandle::new(),
            body: Body::Main,
            pick_at: 0,
            home,
            visible,
            two_rows: place::two_rows(visible.w),
            movable: crate::platform_window::NativeWindow::for_frame(window).is_some(),
            x11_drag: None,
            ignoring: false,
            upward: false,
            pop_h: 0.,
            ctl: None,
            hover: None,
            tip: None,
            tip_shown: false,
            tip_seq: 0,
            menu: None,
            name,
            name_error: None,
        }
    }

    /// The bar is drawn in two rows: a narrow display and the expanded bar.
    fn rows2(&self) -> bool {
        self.two_rows && !self.st.collapsed()
    }

    /// Lets the mouse through the window while the pointer is over the empty strip under the bar
    /// and takes it back everywhere else (the bar, a popover, a drag, outside the window).
    fn strip_hit_test(&mut self, window: &mut Window) {
        let popover = !matches!(self.body, Body::Main) || self.menu.is_some() || self.pop_h > 0.;
        let over = self.x11_drag.is_none()
            && crate::platform_window::pointer_in_window(window).is_some_and(|p| {
                let b = window.bounds().size;
                place::over_strip(
                    p,
                    (
                        f64::from(f32::from(b.width)),
                        f64::from(f32::from(b.height)),
                    ),
                    self.bar_h(),
                    popover,
                )
            });
        if over != self.ignoring {
            self.ignoring = over;
            crate::platform_window::set_ignores_mouse(window, over);
        }
    }

    fn bar_h(&self) -> f64 {
        if self.rows2() {
            place::TWO_ROW_H
        } else {
            place::BAR_H
        }
    }

    /// Writes the bar's choices to the app and to `quick_capture.json`.
    fn changed(&mut self, cx: &mut Context<Self>) {
        let st = self.st.clone();
        self.app.update(cx, |m, _| m.quick.panel = st);
        crate::ui::quick_flow::remember_panel(&self.app, cx);
        cx.notify();
    }

    /// One step back: the popover closes and the bar has the keys again.
    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.body = Body::Main;
        self.menu = None;
        self.close_popover(window, cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    // ---- the popover and the window around it ---------------------------------------------

    /// Opens the popover under the bar (above it when the display has no room under it) with
    /// `want` px of height.
    fn show_popover(&mut self, want: f64, window: &mut Window, cx: &mut Context<Self>) {
        self.set_tip(None, cx);
        let p = place::popover_place(&self.home, self.bar_h(), want, &self.visible, self.movable);
        self.upward = p.upward;
        self.pop_h = p.pop_h;
        self.apply_frame(p.frame, window, cx);
    }

    fn close_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pop_h == 0. {
            return;
        }
        self.upward = false;
        self.pop_h = 0.;
        self.apply_frame(self.home, window, cx);
    }

    /// Gives the window `frame` (global points). Moving the window and resizing it must not
    /// happen inside a window update on macOS, so it goes through a task; elsewhere the size
    /// changes in place.
    fn apply_frame(&self, frame: Rect, window: &mut Window, cx: &mut Context<Self>) {
        if self.movable {
            if let Some(n) = crate::platform_window::NativeWindow::for_frame(window) {
                cx.spawn(async move |_, _| n.set_frame(frame.x, frame.y, frame.w, frame.h))
                    .detach();
                return;
            }
        }
        cx.defer_in(window, move |_, window, _| {
            window.resize(gpui::size(px(frame.w as f32), px(frame.h as f32)));
        });
    }

    /// Rows the picker's keys move over: "No preset", every preset, "Save current...", "Edit...".
    fn pick_rows(&self, cx: &Context<Self>) -> usize {
        self.app.read(cx).quick.presets.len() + 3
    }

    fn open_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let presets = self.app.read(cx).quick.presets.clone();
        self.pick_at = self.st.preset.as_ref().map_or(0, |p| {
            presets
                .iter()
                .position(|q| q.id == p.id)
                .map_or(0, |i| i + 1)
        });
        self.body = Body::Picker;
        self.menu = None;
        self.scroll
            .scroll_to_item(self.pick_at.min(self.pick_rows(cx) - 3));
        self.show_popover(place::popover_height(presets.len(), f64::MAX), window, cx);
        cx.notify();
    }

    /// Enter in the picker: the highlighted row does what a click on it does.
    fn pick_go(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let presets = self.app.read(cx).quick.presets.clone();
        let n = presets.len();
        match self.pick_at {
            0 => self.pick_preset(None, window, cx),
            i if i <= n => self.pick_preset(presets.get(i - 1).cloned(), window, cx),
            i if i == n + 1 => self.open_name(window, cx),
            _ => self.open_settings(window, cx),
        }
    }

    /// The toggle or the key O (QC-1.AC21): the bar is collapsed or expanded, the window takes
    /// the new size to the right of its left edge and stays on the display, the choice is
    /// remembered. A value menu (of a control that may now be hidden) closes; the picker and the
    /// name screen stay open and are placed again.
    fn toggle_collapsed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let collapsed = !self.st.collapsed();
        self.st.set_collapsed(collapsed);
        self.menu = None;
        let shown = controls(self.st.kind(), collapsed);
        if self.ctl.is_some_and(|c| !shown.contains(&c)) {
            self.ctl = None;
        }
        if self.tip.is_some_and(|c| !shown.contains(&c)) {
            self.set_tip(None, cx);
        }
        let size = place::window_size_view(self.visible.w, collapsed);
        self.home = place::toggled_home(&self.home, size, &self.visible, self.movable);
        let want = match self.body {
            Body::Picker => {
                let n = self.app.read(cx).quick.presets.len();
                Some(place::popover_height(n, f64::MAX))
            }
            Body::Name => Some(place::NAME_H),
            Body::Main => None,
        };
        match want {
            Some(h) => self.show_popover(h, window, cx),
            None => {
                self.upward = false;
                self.pop_h = 0.;
                self.apply_frame(self.home, window, cx);
            }
        }
        self.changed(cx);
    }

    fn set_kind(&mut self, k: PresetKind, window: &mut Window, cx: &mut Context<Self>) {
        self.st.set_kind(k);
        self.ctl = None;
        self.changed(cx);
        if self.menu.take().is_some() {
            self.close_popover(window, cx);
        }
    }

    /// Enter or Space: close, wait, run (QC-1 "Window behaviour").
    fn go(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let run = self.st.run();
        let preset = self.st.preset.as_ref().map(|p| p.name.clone());
        self.changed(cx);
        cx.set_global(LastHome(Some(self.home)));
        dismiss(window, cx, true);
        let app = self.app.clone();
        // The bar is gone by then: the wait runs on the app, not on the closed window.
        cx.defer(move |cx| {
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(CAPTURE_DELAY_MS))
                    .await;
                cx.update(|cx| {
                    crate::ui::quick_flow::execute(&app, run, preset, cx);
                    return_when_idle(&app, cx);
                });
            })
            .detach();
        });
    }

    /// A click or the key of an option: a toggle flips, a value control opens its menu.
    fn click_tile(&mut self, t: Tile, window: &mut Window, cx: &mut Context<Self>) {
        if t.has_values() {
            self.menu = Some(t);
            self.body = Body::Main;
            self.show_popover(place::MENU_H, window, cx);
            cx.notify();
        } else {
            self.st.toggle(t);
            self.changed(cx);
        }
    }

    fn step_tile(&mut self, t: Tile, down: bool, cx: &mut Context<Self>) {
        if t.has_values() {
            let choices = self.st.value_choices(t);
            let at = choices
                .iter()
                .position(|c| *c == self.st.value_of(t))
                .unwrap_or(0);
            let next = if down {
                (at + 1).min(choices.len() - 1)
            } else {
                at.saturating_sub(1)
            };
            self.st.set_value(t, choices[next]);
        } else {
            self.st.toggle(t);
        }
        self.changed(cx);
    }

    fn set_target(&mut self, t: PresetTarget, window: &mut Window, cx: &mut Context<Self>) {
        self.st.set_target(t);
        self.changed(cx);
        if self.menu.take().is_some() {
            self.close_popover(window, cx);
        }
    }

    fn cycle_target(&mut self, forward: bool, cx: &mut Context<Self>) {
        let ts = self.st.targets();
        let Some(cur) = self
            .st
            .target()
            .and_then(|t| ts.iter().position(|x| *x == t))
        else {
            return;
        };
        let next = if forward {
            (cur + 1) % ts.len()
        } else {
            (cur + ts.len() - 1) % ts.len()
        };
        self.st.set_target(ts[next]);
        self.changed(cx);
    }

    fn pick_preset(&mut self, p: Option<Preset>, window: &mut Window, cx: &mut Context<Self>) {
        match p {
            Some(p) => self.st.apply_preset(&p),
            None => self.st.clear_preset(),
        }
        self.ctl = None;
        self.changed(cx);
        self.back(window, cx);
    }

    fn save_named(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).text().trim().to_owned();
        if name.is_empty() {
            self.name_error = Some(tr!("Give the preset a name."));
            cx.notify();
            return;
        }
        let json = self.st.to_preset_json(&name);
        let this = cx.entity();
        let handle = window.window_handle();
        crate::ui::quick_flow::preset_call(
            &self.app,
            "save_preset",
            serde_json::json!({ "preset": json }),
            move |_, res, cx| {
                let _ = handle.update(cx, |_, window, cx| {
                    this.update(cx, |this, cx| match res {
                        Ok(v) => {
                            // The new preset is the picked one: the bar names it.
                            if let Ok(p) = serde_json::from_value::<Preset>(
                                v.get("preset").cloned().unwrap_or(v),
                            ) {
                                this.st.apply_preset(&p);
                                this.changed(cx);
                            }
                            this.back(window, cx);
                        }
                        Err(e) => {
                            this.name_error = Some(e);
                            cx.notify();
                        }
                    });
                });
            },
            cx,
        );
    }

    fn open_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.body = Body::Name;
        self.menu = None;
        self.name_error = None;
        self.name.update(cx, |f, cx| f.set_text("", cx));
        self.show_popover(place::NAME_H, window, cx);
        let handle = self.name.read(cx).focus.clone();
        window.focus(&handle, cx);
        cx.notify();
    }

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = self.app.clone();
        dismiss(window, cx, false);
        cx.defer(move |cx| crate::ui::quick_flow::open_presets_page(&app, cx));
    }

    /// What a click, or Enter on a focused button, does.
    fn activate(&mut self, ctl: Ctl, window: &mut Window, cx: &mut Context<Self>) {
        match ctl {
            Ctl::Kind(k) => self.set_kind(k, window, cx),
            Ctl::Target(t) => self.set_target(t, window, cx),
            Ctl::Tile(t) => self.click_tile(t, window, cx),
            Ctl::Preset => self.open_picker(window, cx),
            Ctl::Action => self.go(window, cx),
            Ctl::Toggle => self.toggle_collapsed(window, cx),
        }
    }

    // ---- tooltips -------------------------------------------------------------------------

    fn hover_ctl(&mut self, ctl: Ctl, over: bool, cx: &mut Context<Self>) {
        if over {
            self.hover = Some(ctl);
        } else if self.hover == Some(ctl) {
            self.hover = None;
        }
        let want = self.hover.or(self.ctl);
        self.set_tip(want, cx);
    }

    /// The tooltip waits [`TIP_DELAY_MS`] on the control the pointer or the keyboard focus is on
    /// before it shows.
    fn set_tip(&mut self, want: Option<Ctl>, cx: &mut Context<Self>) {
        if self.tip == want {
            return;
        }
        self.tip = want;
        self.tip_shown = false;
        self.tip_seq += 1;
        cx.notify();
        if want.is_none() {
            return;
        }
        let seq = self.tip_seq;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(TIP_DELAY_MS))
                .await;
            this.update(cx, |this, cx| {
                if this.tip_seq == seq {
                    this.tip_shown = true;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    // ---- keys -----------------------------------------------------------------------------

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        let m = k.modifiers;
        let screen = match self.body {
            Body::Main => Screen::Main,
            Body::Picker => Screen::Picker,
            Body::Name => Screen::Name,
        };
        let mods = Mods {
            shift: m.shift,
            ctrl: m.control,
            alt: m.alt,
            platform: m.platform,
        };
        let Some(key) = panel_key(
            &k.key,
            mods,
            screen,
            self.menu.is_some(),
            self.st.kind(),
            self.st.collapsed(),
        ) else {
            return;
        };
        match key {
            PanelKey::Close => dismiss(window, cx, true),
            PanelKey::Back => self.back(window, cx),
            PanelKey::Go => {
                if self.menu.take().is_some() {
                    self.close_popover(window, cx);
                    cx.notify();
                } else if let Some(c @ (Ctl::Preset | Ctl::Toggle)) = self.ctl {
                    self.activate(c, window, cx);
                } else {
                    self.go(window, cx);
                }
            }
            PanelKey::NextControl | PanelKey::PrevControl => {
                let all = controls(self.st.kind(), self.st.collapsed());
                let n = all.len();
                let at = self.ctl.and_then(|c| all.iter().position(|x| *x == c));
                let back = key == PanelKey::PrevControl;
                let next = match (at, back) {
                    (None, false) => 0,
                    (None, true) => n - 1,
                    (Some(i), false) => (i + 1) % n,
                    (Some(i), true) => (i + n - 1) % n,
                };
                self.ctl = Some(all[next]);
                let want = self.hover.or(self.ctl);
                self.set_tip(want, cx);
                cx.notify();
            }
            // An open value menu takes the arrows: they step its values (QC-1 "Keys").
            PanelKey::TileUp | PanelKey::TileDown | PanelKey::TargetPrev | PanelKey::TargetNext
                if self.menu.is_some() =>
            {
                if let Some(t) = self.menu {
                    let down = matches!(key, PanelKey::TileDown | PanelKey::TargetNext);
                    self.step_tile(t, down, cx);
                }
            }
            PanelKey::TileUp | PanelKey::TileDown => {
                if let Some(Ctl::Tile(t)) = self.ctl {
                    self.step_tile(t, key == PanelKey::TileDown, cx);
                }
            }
            PanelKey::Target(t) => self.set_target(t, window, cx),
            PanelKey::Tile(t) => self.click_tile(t, window, cx),
            PanelKey::TargetPrev => self.cycle_target(false, cx),
            PanelKey::TargetNext => self.cycle_target(true, cx),
            PanelKey::Picker => self.open_picker(window, cx),
            PanelKey::Preset(i) => {
                if let Some(p) = self.app.read(cx).quick.presets.get(i).cloned() {
                    self.pick_preset(Some(p), window, cx);
                }
            }
            PanelKey::Toggle => self.toggle_collapsed(window, cx),
            PanelKey::PickerUp | PanelKey::PickerDown => {
                let rows = self.pick_rows(cx);
                self.pick_at = picker_step(self.pick_at, key == PanelKey::PickerDown, rows);
                self.scroll
                    .scroll_to_item(self.pick_at.min(self.pick_rows(cx) - 3));
                cx.notify();
            }
            PanelKey::PickerGo => self.pick_go(window, cx),
            PanelKey::Kind(kind) => self.set_kind(kind, window, cx),
        }
        cx.stop_propagation();
    }

    // ---- texts ----------------------------------------------------------------------------

    fn platform(&self, cx: &Context<Self>) -> Platform {
        self.app.read(cx).platform
    }

    /// The chord of a global shortcut as the app has it set ("⌥⇧R", "Ctrl+Alt+Shift+R");
    /// `None` when the action has no chord.
    fn global_chord(&self, a: ShortcutAction, cx: &Context<Self>) -> Option<String> {
        let m = self.app.read(cx);
        let caps = m.shortcuts.get(a).keycaps(m.platform);
        (!caps.is_empty()).then(|| {
            if m.platform == Platform::Mac {
                caps.concat()
            } else {
                caps.join("+")
            }
        })
    }

    /// The first four presets' keys in the picker: "⌃1", "Ctrl+1".
    fn preset_key(&self, n: usize, cx: &Context<Self>) -> String {
        if self.platform(cx) == Platform::Mac {
            format!("\u{2303}{n}")
        } else {
            format!("Ctrl+{n}") // i18n-ignore: a key chord, as shortcuts elsewhere
        }
    }

    /// The tooltip and accessible name of a control (QC-1 "Tooltips").
    fn tip_of(&self, ctl: Ctl, cx: &Context<Self>) -> String {
        match ctl {
            Ctl::Kind(k) => {
                let action = match k {
                    PresetKind::Screenshot => ShortcutAction::CaptureRegion,
                    PresetKind::Video => ShortcutAction::ToggleRecording,
                    PresetKind::Audio => ShortcutAction::ToggleAudioRecording,
                };
                let chord = self.global_chord(action, cx);
                self.st.kind_tip(k, chord.as_deref())
            }
            Ctl::Target(t) => self.st.target_tip(t),
            Ctl::Tile(t) => self.st.tile_tip(t),
            Ctl::Preset => self.st.preset_tip(),
            Ctl::Action => tip_text(
                &self.st.action_text().0,
                None,
                Some(&tr_ctx!("quick capture", "Enter")),
                None,
            ),
            Ctl::Toggle => self.st.toggle_tip(),
        }
    }
}

// ---- drawing ----------------------------------------------------------------------------

fn pill_button(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    aria: String,
) -> gpui::Stateful<gpui::Div> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(aria))
        .focusable()
        .tab_stop(true)
        .cursor_pointer()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .hover(move |s| s.bg(hover))
        .focus_visible(move |s| s.border_color(focus))
}

impl QuickPanel {
    /// The tooltip, under its control, in a layer above the rest of the bar.
    fn tip_el(&self, t: &Theme, text: String, at: TipAt) -> impl IntoElement {
        let top = px((bar::CELL_H + 10.) as f32);
        let wrap = div().absolute().top(top).w(px(0.)).flex();
        let wrap = match at {
            TipAt::Left => wrap.left(px(0.)).justify_start(),
            TipAt::Center => wrap.left(px((bar::CELL_W / 2.) as f32)).justify_center(),
            TipAt::Right => wrap.right(px(0.)).justify_end(),
        };
        deferred(
            wrap.child(
                div()
                    .id("qc-tip")
                    .role(Role::Tooltip)
                    .flex_none()
                    .whitespace_nowrap()
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(scale::RADIUS_SM))
                    .bg(rgba_of(t.raised))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .text_color(rgba_of(t.text))
                    .text_size(px(type_scale::CAPTION))
                    .child(SharedString::from(text)),
            ),
        )
        .with_priority(2)
    }

    /// A bar button: 44 x 48, the icon, the key under it, a badge in the corner, a dot when on.
    fn cell(&self, t: &Theme, c: Cell, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let ctl = c.ctl;
        let focused = self.ctl == Some(ctl);
        let showing = self.tip_shown && self.tip == Some(ctl) && self.pop_h == 0.;
        let (fg, key_fg) = if c.selected || c.on {
            (t.text, t.text_2)
        } else {
            (t.mist, t.mist)
        };
        let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
        let mut d = div()
            .id(c.id)
            .role(Role::Button)
            .aria_label(SharedString::from(c.tip.clone()))
            .focusable()
            .tab_stop(true)
            .relative()
            .w(px(bar::CELL_W as f32))
            .h(px(bar::CELL_H as f32))
            .flex()
            .flex_none()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(3.))
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(if focused {
                rgba_of(t.text)
            } else if c.selected {
                rgba_of(t.line)
            } else {
                rgba_of(crate::theme::Color(0))
            })
            .cursor_pointer()
            .focus_visible(move |s| s.border_color(focus))
            .on_hover(cx.listener(move |this, over: &bool, _, cx| this.hover_ctl(ctl, *over, cx)))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, window, cx| this.activate(ctl, window, cx)));
        d = if ctl == Ctl::Toggle {
            d.hover(move |s| s.bg(hover))
                .aria_expanded(!self.st.collapsed())
        } else if c.selected {
            d.bg(rgba_of(t.raised)).aria_selected(true)
        } else if c.on {
            d.bg(rgba_of(t.surface_2))
                .hover(move |s| s.bg(hover))
                .aria_toggled(gpui::Toggled::True)
        } else if matches!(ctl, Ctl::Tile(_)) {
            d.hover(move |s| s.bg(hover))
                .aria_toggled(gpui::Toggled::False)
        } else {
            d.hover(move |s| s.bg(hover)).aria_selected(false)
        };
        d.child(icon(c.glyph, 18., fg))
            .child(
                div()
                    .h(px(10.))
                    .text_size(px(10.))
                    .line_height(px(10.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgba_of(key_fg))
                    .mono()
                    .child(SharedString::from(c.key.unwrap_or_default())),
            )
            .children(c.badge.map(|b| {
                div()
                    .absolute()
                    .top(px(3.))
                    .right(px(3.))
                    .text_size(px(9.))
                    .line_height(px(10.))
                    .text_color(rgba_of(t.text_2))
                    .mono()
                    .child(match b {
                        Badge::Text(s) => div().child(SharedString::from(s)),
                        Badge::Menu => div().child(icon("chevron-down", 9., t.text_2)),
                    })
            }))
            .children((c.on && !c.selected).then(|| {
                div()
                    .absolute()
                    .bottom(px(3.))
                    .left(px(((bar::CELL_W - 3.) / 2.) as f32))
                    .size(px(3.))
                    .rounded(px(1.5))
                    .bg(rgba_of(t.text))
            }))
            .children(showing.then(|| self.tip_el(t, self.tip_of(ctl, cx), c.at)))
    }

    /// The recessed track under a group of selectable buttons.
    fn track(t: &Theme) -> gpui::Div {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(bar::CELL_GAP as f32))
            .p(px(bar::TRACK_PAD as f32))
            .rounded(px(12.))
            .bg(rgba_of(t.field))
            .border_1()
            .border_color(rgba_of(t.border))
    }

    fn sep(t: &Theme) -> gpui::Div {
        div()
            .w(px(1.))
            .h(px(28.))
            .flex_none()
            .mx(px(4.))
            .bg(rgba_of(t.border))
    }

    fn cell_at(&self, ctl: Ctl, kind_cells: bool) -> TipAt {
        if self.rows2() {
            return match ctl {
                Ctl::Preset | Ctl::Action | Ctl::Toggle => TipAt::Right,
                _ => TipAt::Left,
            };
        }
        match ctl {
            Ctl::Kind(_) => TipAt::Left,
            Ctl::Action | Ctl::Toggle => TipAt::Right,
            _ => {
                let _ = kind_cells;
                TipAt::Center
            }
        }
    }

    fn kind_track(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let active = self.st.kind();
        Self::track(t).children(
            [PresetKind::Screenshot, PresetKind::Video, PresetKind::Audio]
                .into_iter()
                .enumerate()
                .map(|(i, k)| {
                    let ctl = Ctl::Kind(k);
                    self.cell(
                        t,
                        Cell {
                            ctl,
                            id: SharedString::from(format!("qc-kind-{i}")),
                            glyph: kind_icon(k),
                            key: Some(kind_key(k).to_owned()),
                            tip: self.tip_of(ctl, cx),
                            selected: k == active,
                            on: false,
                            badge: None,
                            at: self.cell_at(ctl, true),
                        },
                        cx,
                    )
                }),
        )
    }

    /// The targets; for audio the slot shows the microphone level instead.
    fn target_slot(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        if self.st.kind() == PresetKind::Audio {
            return self.level_meter(t);
        }
        let cur = self.st.target();
        Self::track(t).children(self.st.targets().iter().enumerate().map(|(i, &tg)| {
            let ctl = Ctl::Target(tg);
            self.cell(
                t,
                Cell {
                    ctl,
                    id: SharedString::from(format!("qc-target-{i}")),
                    glyph: target_icon(tg),
                    key: Some(target_key(tg).to_owned()),
                    tip: self.tip_of(ctl, cx),
                    selected: Some(tg) == cur,
                    on: false,
                    badge: None,
                    at: self.cell_at(ctl, false),
                },
                cx,
            )
        }))
    }

    /// Audio has no target: the slot holds the microphone level, in the same width so nothing
    /// moves when the kind changes. The bar draws it unlit: a level without recording would
    /// open the microphone (and light the system's recording indicator) just to show a bar, so
    /// the live level stays in the recording sheet and on the HUD.
    fn level_meter(&self, t: &Theme) -> gpui::Div {
        let bars = crate::recording::hud_layout::METER_SHAPE;
        Self::track(t)
            .w(px(bar::track_w(4) as f32))
            .h(px(bar::TRACK_H as f32))
            .gap(px(10.))
            .px(px(14.))
            .child(
                div()
                    .id("qc-mic-level")
                    .role(Role::Meter)
                    .aria_label(tr!("Microphone level"))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(icon("mic", 18., t.mist))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(3.))
                            .children(bars.iter().map(|h| {
                                div()
                                    .w(px(4.))
                                    .h(px(f32::from(*h)))
                                    .rounded(px(1.5))
                                    .bg(rgba_of(t.line))
                            })),
                    ),
            )
    }

    fn options(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(bar::CELL_GAP as f32))
            .children(Tile::of(self.st.kind()).into_iter().map(|tile| {
                let ctl = Ctl::Tile(tile);
                let badge = if tile.has_values() {
                    seconds_badge(self.st.value_of(tile))
                        .map(Badge::Text)
                        .or(Some(Badge::Menu))
                } else {
                    None
                };
                self.cell(
                    t,
                    Cell {
                        ctl,
                        id: SharedString::from(tile.id()),
                        glyph: tile.icon(),
                        key: Some(tile.key().to_owned()),
                        tip: self.tip_of(ctl, cx),
                        selected: false,
                        on: self.st.is_on(tile),
                        badge,
                        at: self.cell_at(ctl, false),
                    },
                    cx,
                )
            }))
    }

    fn preset_cell(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let ctl = Ctl::Preset;
        self.cell(
            t,
            Cell {
                ctl,
                id: "qc-preset-pill".into(),
                glyph: "bookmark",
                key: Some("P".to_owned()),
                tip: self.tip_of(ctl, cx),
                selected: false,
                on: self.st.preset.is_some(),
                badge: Some(Badge::Menu),
                at: self.cell_at(ctl, false),
            },
            cx,
        )
    }

    /// The toggle at the right end: a 38 px wide chevron with its key, "O", under it.
    fn toggle_cell(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let ctl = Ctl::Toggle;
        self.cell(
            t,
            Cell {
                ctl,
                id: "qc-toggle".into(),
                glyph: if self.st.collapsed() {
                    "chevron-right"
                } else {
                    "chevron-left"
                },
                key: Some("O".to_owned()),
                tip: self.tip_of(ctl, cx),
                selected: false,
                on: false,
                badge: None,
                at: self.cell_at(ctl, false),
            },
            cx,
        )
        .w(px(bar::ICON_BTN_W as f32))
    }

    /// The action: the only filled control. Capture is neutral, Record is the record red; the
    /// picked preset's name is a second line.
    fn action(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let ctl = Ctl::Action;
        let (main, name) = self.st.action_lines();
        let (_, record) = self.st.action_text();
        let (bg, hover, fg) = if record {
            (t.rec, t.rec_hover, t.on_rec)
        } else {
            (t.primary_bg, t.primary_hover, t.primary_fg)
        };
        let focused = self.ctl == Some(ctl);
        let showing = self.tip_shown && self.tip == Some(ctl) && self.pop_h == 0.;
        let focus = rgba_of(t.text);
        div()
            .id("qc-action")
            .role(Role::Button)
            .aria_label(SharedString::from(self.tip_of(ctl, cx)))
            .focusable()
            .tab_stop(true)
            .relative()
            .h(px(bar::CELL_H as f32))
            .w(px(bar::ACTION_W as f32))
            .ml(px(bar::ACTION_ML as f32))
            .pl(px(12.))
            .pr(px(8.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .rounded(px(12.))
            .border_1()
            .border_color(if focused { rgba_of(t.ink) } else { rgba_of(bg) })
            .bg(rgba_of(bg))
            .text_color(rgba_of(fg))
            .cursor_pointer()
            .hover(move |s| s.bg(rgba_of(hover)))
            .focus_visible(move |s| s.border_color(focus))
            .on_hover(cx.listener(move |this, over: &bool, _, cx| this.hover_ctl(ctl, *over, cx)))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| this.go(window, cx)))
            // The record dot on Record, the camera shutter's frame on Capture.
            .child(if record {
                div()
                    .size(px(10.))
                    .flex_none()
                    .rounded(px(5.))
                    .bg(rgba_of(t.ink))
                    .into_any_element()
            } else {
                icon("cm-capture-region", 16., fg).into_any_element()
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(type_scale::UI))
                            .line_height(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .whitespace_nowrap()
                            .child(SharedString::from(main)),
                    )
                    .children(name.map(|n| {
                        div()
                            .text_size(px(10.5))
                            .line_height(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .opacity(0.7)
                            .child(crate::ui::components::ellipsis_text(n))
                    })),
            )
            .child(
                div()
                    .h(px(22.))
                    .min_w(px(22.))
                    .px(px(4.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .bg(rgba_of(t.ink.with_alpha(33)))
                    .text_size(px(12.))
                    .mono()
                    .child(SharedString::from("\u{21B5}")),
            )
            .children(showing.then(|| self.tip_el(t, self.tip_of(ctl, cx), TipAt::Right)))
    }

    /// The logo with the Preview tag under it: the tag the spec asks Quick Capture to keep.
    fn logo_slot(&self, t: &Theme) -> gpui::Div {
        div()
            .w(px(bar::LOGO_W as f32))
            .h(px(bar::CELL_H as f32))
            .flex()
            .flex_none()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .child(logo(t, 18.))
            .child(preview_label(
                t,
                Size::Xs,
                PreviewFeature::QuickCapture,
                None,
                None,
            ))
    }

    fn bar(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let frame = div()
            .w(px(
                place::bar_size_view(self.visible.w, self.st.collapsed()).0 as f32,
            ))
            .h(px(self.bar_h() as f32))
            .flex_none()
            .px(px(bar::PAD_X as f32))
            .py(px(bar::PAD_Y as f32))
            .flex()
            .rounded(px(scale::RADIUS_XL))
            .border_1()
            .border_color(rgba_of(t.text_3.with_alpha(150)))
            .bg(rgba_of(t.surface))
            // The bar's background is the grip (its buttons stop the press), so it can be dragged.
            .cursor_grab()
            .map(crate::platform_window::drag_grip)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus, cx);
                    // X11: the bar is an override-redirect window, so the window manager never
                    // moves it; the app follows the pointer itself (Linux QA N-3).
                    if let (Some((px_, py_, _, _)), Some((ox, oy))) = (
                        crate::platform_window::x11_pointer(),
                        crate::platform_window::x11_window_origin(window),
                    ) {
                        log::debug!("x11 drag start at {px_},{py_}, window at {ox},{oy}");
                        this.x11_drag = Some((px_, py_, ox, oy));
                        return;
                    }
                    window.start_window_move();
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &gpui::MouseMoveEvent, window, _| {
                let Some((sx, sy, ox, oy)) = this.x11_drag else {
                    return;
                };
                log::debug!("x11 drag move, pressed {:?}", ev.pressed_button);
                if ev.pressed_button != Some(MouseButton::Left) {
                    this.x11_drag = None;
                    return;
                }
                if let Some((x, y, _, _)) = crate::platform_window::x11_pointer() {
                    crate::platform_window::x11_move_to(
                        window,
                        ox + (x - sx).round() as i32,
                        oy + (y - sy).round() as i32,
                    );
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, _| {
                    if this.x11_drag.take().is_some() {
                        // The bar's place is now where it was dropped.
                        if let Some((ox, oy)) = crate::platform_window::x11_window_origin(window) {
                            this.home.x = f64::from(ox);
                            this.home.y = f64::from(oy);
                        }
                    }
                }),
            );
        let kind = self.kind_track(t, cx);
        let preset = self.preset_cell(t, cx);
        let action = self.action(t, cx);
        let toggle = self.toggle_cell(t, cx);
        let row = || {
            div()
                .flex()
                .items_center()
                .gap(px(bar::GAP as f32))
                .h(px(bar::TRACK_H as f32))
        };
        if self.st.collapsed() {
            return frame
                .items_center()
                .gap(px(bar::GAP as f32))
                .child(self.logo_slot(t))
                .child(kind)
                .child(Self::sep(t))
                .child(preset)
                .child(action)
                .child(toggle);
        }
        let target = self.target_slot(t, cx);
        let options = self.options(t, cx);
        if self.rows2() {
            frame
                .flex_col()
                .gap(px(bar::GAP as f32))
                .child(
                    row()
                        .child(self.logo_slot(t))
                        .child(kind)
                        .child(Self::sep(t))
                        .child(target),
                )
                .child(
                    row()
                        .child(options)
                        .child(Self::sep(t))
                        .child(preset)
                        .child(action)
                        .child(toggle),
                )
        } else {
            frame
                .items_center()
                .gap(px(bar::GAP as f32))
                .child(self.logo_slot(t))
                .child(kind)
                .child(Self::sep(t))
                .child(target)
                .child(Self::sep(t))
                .child(options)
                .child(Self::sep(t))
                .child(preset)
                .child(action)
                .child(toggle)
        }
    }

    /// The small menu of a value control (Delay, Countdown): its choices.
    fn value_menu(&self, t: &Theme, tile: Tile, cx: &mut Context<Self>) -> gpui::Div {
        let current = self.st.value_of(tile);
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .text_size(px(type_scale::UI - 1.))
                    .text_color(rgba_of(t.text_3))
                    .child(SharedString::from(tile.label())),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .children(self.st.value_choices(tile).iter().map(|&secs| {
                        let on = secs == current;
                        let text = crate::quick::state::seconds_text(secs);
                        pill_button(
                            t,
                            SharedString::from(format!("qc-value-{secs}")),
                            tr_ctx!(
                                "quick capture tile",
                                "{tile}, {value}",
                                tile = tile.label(),
                                value = text.clone()
                            ),
                        )
                        .h(px(44.))
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(scale::RADIUS_MD))
                        .border_1()
                        .border_color(rgba_of(if on { t.text } else { t.border }))
                        .bg(rgba_of(if on { t.raised } else { t.surface_2 }))
                        .text_size(px(type_scale::UI))
                        .text_color(rgba_of(if on { t.text } else { t.text_2 }))
                        .aria_selected(on)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.st.set_value(tile, secs);
                            this.menu = None;
                            this.changed(cx);
                            this.close_popover(window, cx);
                        }))
                        .child(SharedString::from(text))
                    })),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgba_of(t.text_3))
                    .child(SharedString::from(tr!("Esc goes back"))),
            )
    }

    fn picker(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let presets = self.app.read(cx).quick.presets.clone();
        let picked = self.st.preset.as_ref().map(|p| p.id.clone());
        let at = self.pick_at;
        let row = |id: String,
                   title: String,
                   sub: Option<String>,
                   key: Option<String>,
                   on: bool,
                   cur: bool| {
            pill_button(
                t,
                SharedString::from(id),
                match &sub {
                    Some(s) => tr_ctx!(
                        "preset picker",
                        "{name}, {summary}",
                        name = title.clone(),
                        summary = s.clone()
                    ),
                    None => title.clone(),
                },
            )
            .min_h(px(crate::quick::place::ROW_MIN_H as f32))
            .px(px(10.))
            .py(px(crate::quick::place::ROW_PAD_Y as f32))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .rounded(px(scale::RADIUS_MD))
            .when(on, |d| d.bg(rgba_of(t.raised)))
            .border_1()
            .border_color(if cur {
                gpui::Hsla::from(rgba_of(t.text))
            } else {
                gpui::transparent_black()
            })
            .aria_selected(on)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(crate::quick::place::ROW_NAME_PX as f32))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgba_of(t.text))
                            .child(crate::ui::components::shrink_text(title)),
                    )
                    .children(sub.map(|s| {
                        div()
                            .text_size(px(crate::quick::place::ROW_SUMMARY_PX as f32))
                            .text_color(rgba_of(t.text_3))
                            .child(crate::ui::components::shrink_text(s))
                    })),
            )
            .children(key.map(|k| {
                div()
                    .flex_none()
                    .text_size(px(11.))
                    .text_color(rgba_of(t.mist))
                    .mono()
                    .child(SharedString::from(k))
            }))
            .children(on.then(|| icon("check", 14., t.text)))
        };
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        rows.push(
            row(
                "qc-pick-none".into(),
                tr_ctx!("preset picker", "No preset"),
                None,
                None,
                picked.is_none(),
                at == 0,
            )
            .on_click(cx.listener(|this, _, window, cx| this.pick_preset(None, window, cx)))
            .into_any_element(),
        );
        for (i, p) in presets.iter().cloned().enumerate() {
            let on = picked.as_deref() == Some(p.id.as_str());
            rows.push(
                row(
                    format!("qc-pick-{i}"),
                    p.name.clone(),
                    Some(summary_text(&p)),
                    (i < crate::quick::state::PRESET_KEYS).then(|| self.preset_key(i + 1, cx)),
                    on,
                    at == i + 1,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.pick_preset(Some(p.clone()), window, cx)
                }))
                .into_any_element(),
            );
        }
        let n_presets = presets.len();
        let link = |id: &'static str, label: String, glyph: &'static str, cur: bool| {
            pill_button(t, id, label.clone())
                .h(px(40.))
                .px(px(10.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(8.))
                .rounded(px(scale::RADIUS_MD))
                .border_1()
                .border_color(if cur {
                    gpui::Hsla::from(rgba_of(t.text))
                } else {
                    gpui::transparent_black()
                })
                .text_size(px(type_scale::UI - 1.))
                .text_color(rgba_of(t.text_2))
                .child(icon(glyph, 14., t.text_3))
                .child(SharedString::from(label))
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .id("qc-picker-list")
                    .role(Role::ListBox)
                    .aria_label(tr_ctx!("preset picker", "Presets"))
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .children(rows),
            )
            .child(div().h(px(1.)).bg(rgba_of(t.border)))
            .child(
                link(
                    "qc-save-current",
                    tr_ctx!("preset picker", "Save current as preset\u{2026}"),
                    "plus",
                    at == n_presets + 1,
                )
                .on_click(cx.listener(|this, _, window, cx| this.open_name(window, cx))),
            )
            .child(
                link(
                    "qc-edit-presets",
                    tr_ctx!("preset picker", "Edit presets\u{2026}"),
                    "pencil",
                    at == n_presets + 2,
                )
                .on_click(cx.listener(|this, _, window, cx| this.open_settings(window, cx))),
            )
    }

    fn name_body(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .text_size(px(type_scale::UI))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(tr_ctx!(
                        "preset picker",
                        "Save current as preset"
                    ))),
            )
            .child(
                div()
                    .text_size(px(type_scale::UI - 1.))
                    .text_color(rgba_of(t.text_3))
                    .child(SharedString::from(tr!(
                        "Keeps the kind, the target and the four tiles. The area is never saved."
                    ))),
            )
            .child(self.name.clone())
            .children(self.name_error.clone().map(|e| {
                div()
                    .text_size(px(type_scale::UI - 1.))
                    .text_color(rgba_of(t.warning))
                    .child(SharedString::from(e))
            }))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        pill_button(t, "qc-name-cancel", tr_ctx!("button", "Cancel"))
                            .h(px(40.))
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(scale::RADIUS_MD))
                            .border_1()
                            .border_color(rgba_of(t.line))
                            .text_size(px(type_scale::UI))
                            .on_click(cx.listener(|this, _, window, cx| this.back(window, cx)))
                            .child(SharedString::from(tr_ctx!("button", "Cancel"))),
                    )
                    .child(
                        pill_button(t, "qc-name-save", tr_ctx!("button", "Save preset"))
                            .h(px(40.))
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(scale::RADIUS_MD))
                            .bg(rgba_of(t.primary_bg))
                            .text_color(rgba_of(t.primary_fg))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(type_scale::UI))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.save_named(window, cx)),
                            )
                            .child(SharedString::from(tr_ctx!("button", "Save preset"))),
                    ),
            )
    }

    /// The popover under (or over) the bar, when one is open.
    fn popover(&self, t: &Theme, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let content = match (self.body, self.menu) {
            (Body::Picker, _) => self.picker(t, cx),
            (Body::Name, _) => self.name_body(t, cx),
            (Body::Main, Some(tile)) => self.value_menu(t, tile, cx),
            (Body::Main, None) => return None,
        };
        Some(
            div()
                .w(px(place::POP_W as f32))
                .h(px(self.pop_h as f32))
                .flex_none()
                .ml_auto()
                .p(px(8.))
                .flex()
                .flex_col()
                .rounded(px(scale::RADIUS_XL))
                .border_1()
                .border_color(rgba_of(t.text_3.with_alpha(150)))
                .bg(rgba_of(t.surface))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(content),
        )
    }
}

impl Render for QuickPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let bar = self.bar(&t, cx);
        let pop = self.popover(&t, cx);
        let root = div()
            .id("quick-capture")
            .key_context("QuickCapture")
            .role(Role::Dialog)
            .aria_label(tr!("Quick Capture"))
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .font_family(UI_FONT)
            .font_features(ui_features())
            .text_color(rgba_of(t.text))
            .text_size(px(type_scale::UI))
            .flex()
            .flex_col()
            .gap(px(place::POP_GAP as f32));
        if self.upward {
            root.children(pop).child(bar)
        } else {
            root.child(bar).children(pop)
        }
    }
}
