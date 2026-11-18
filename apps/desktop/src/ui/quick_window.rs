//! The Quick Capture panel (QC-1, QC-4): one small card with the kind, the target, four option
//! tiles and one action bar. A borderless floating panel like the Launcher: it takes the keyboard
//! (the app is activated, the previous app gets focus back on close), it is excluded from screen
//! capture, and it closes before the capture starts and waits [`CAPTURE_DELAY_MS`] first.
//!
//! The window only draws `quick::state::PanelState` and sends what the user chose to
//! `ui::quick_flow`; the capture itself is the existing actions.

use std::time::Duration;

use cm_core::preset::{Preset, PresetKind};
use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, prelude::*, px, App, Context, Entity, FocusHandle, FontWeight, Global, KeyDownEvent,
    MouseButton, Render, Role, ScrollHandle, SharedString, Window, WindowHandle,
};

use crate::launcher::commands::CAPTURE_DELAY_MS;
use crate::quick::place::panel_rect;
use crate::quick::presets::summary_text;
use crate::quick::state::{
    kind_icon, kind_label, panel_key, picker_step, target_label, PanelKey, PanelState, Screen, Tile,
};
use crate::theme::{scale, type_scale, Color, Theme};
use crate::ui::app::{self, AppModel};
use crate::ui::components::{icon, icon_button, logo};
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::preview_label::{label as preview_label, send_feedback, PreviewFeature, Size};
use crate::ui::recording::{panel_options, pointer_display, visible_part};
use crate::ui::text_field::{TextField, TextFieldEvent};

pub const WIDTH: f32 = 352.;
pub const HEIGHT: f32 = 400.;

/// Where the panel opens (QC-1 "Opening").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The shortcut, the dock and the Launcher: near the pointer, on the display that has it.
    Pointer,
    /// The tray or menu-bar item: under the icon.
    Tray,
}

struct PanelSlot(Option<WindowHandle<QuickPanel>>);
impl Global for PanelSlot {}

/// The app that was frontmost when the panel opened (macOS), to hand focus back to.
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

/// The shortcut, the tray item, the dock button and the Launcher row: opens the panel, or closes
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
    let pointer = crate::ui::launcher_window::pointer(cx)
        .map(|p| (f64::from(f32::from(p.x)), f64::from(f32::from(p.y))));
    let (px_, py_) = crate::quick::place::open_point(pointer, f64::from(HEIGHT), &visible);
    let max_h = visible.h - 2.0 * crate::quick::place::EDGE;
    // From the tray the panel is on the icon where the system gives its position (macOS status
    // item, Windows notification area); where it does not (Linux) it opens near the pointer.
    let icon = (anchor == Anchor::Tray)
        .then(|| app.read(cx).tray.as_ref().and_then(|t| t.icon_frame()))
        .flatten()
        .filter(|(ix, iy, _, _)| visible_contains(&display.1, *ix, *iy));
    let global = match icon {
        Some(icon) => {
            crate::quick::place::tray_rect(icon, f64::from(WIDTH), f64::from(HEIGHT), &visible)
        }
        None => panel_rect((px_, py_), f64::from(WIDTH), f64::from(HEIGHT), &visible),
    };
    cx.set_global(PreviousApp(crate::launcher::focus::frontmost_other_app()));
    let app2 = app.clone();
    let opened = cx.open_window(
        panel_options(display, global, true, false),
        move |window, cx| {
            // The panel is never in a screenshot or a recording (QC-1.AC2).
            crate::platform_window::exclude_from_capture(window);
            cx.new(|cx| QuickPanel::new(app2, state, max_h, window, cx))
        },
    );
    match opened {
        Ok(h) => {
            crate::surfaces::opened(crate::surfaces::Surface::QuickCapture, h);
            cx.set_global(PanelSlot(Some(h)));
            crate::surfaces::activate_app(cx, "quick capture opened");
            crate::surfaces::take_keyboard_x11(cx, h);
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

fn visible_contains(display: &crate::recording::geometry::Rect, x: f64, y: f64) -> bool {
    display.contains(x, y)
}

/// What the body of the card shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Body {
    Main,
    /// The preset picker (QC-4).
    Picker,
    /// "Save current as preset…": asks for the name.
    Name,
}

pub struct QuickPanel {
    app: Entity<AppModel>,
    st: PanelState,
    focus: FocusHandle,
    scroll: ScrollHandle,
    body: Body,
    /// The highlighted picker row: 0 is "No preset", then the presets, then the two links.
    pick_at: usize,
    /// The tallest the panel may grow while the picker shows every preset.
    max_h: f64,
    /// The tile with the keyboard focus (Tab).
    tile: Option<usize>,
    /// The value tile whose menu is open.
    menu: Option<Tile>,
    name: Entity<TextField>,
    name_error: Option<String>,
    was_active: bool,
}

impl QuickPanel {
    fn new(
        app: Entity<AppModel>,
        st: PanelState,
        max_h: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.was_active = true;
            } else if this.was_active {
                dismiss(window, cx, false);
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
                TextFieldEvent::Escaped => this.back(window, cx),
                TextFieldEvent::Changed(_) => {
                    this.name_error = None;
                    cx.notify();
                }
            },
        )
        .detach();
        QuickPanel {
            app,
            st,
            focus,
            scroll: ScrollHandle::new(),
            body: Body::Main,
            pick_at: 0,
            max_h,
            tile: None,
            menu: None,
            name,
            name_error: None,
            was_active: false,
        }
    }

    /// Writes the panel's choices to the app and to `quick_capture.json`.
    fn changed(&mut self, cx: &mut Context<Self>) {
        let st = self.st.clone();
        self.app.update(cx, |m, _| m.quick.panel = st);
        crate::ui::quick_flow::remember_panel(&self.app, cx);
        cx.notify();
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.body = Body::Main;
        self.menu = None;
        self.fit_height(window, cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// The panel is as tall as the picker's list needs while the picker shows, else normal.
    fn fit_height(&self, window: &mut Window, cx: &mut Context<Self>) {
        let h = match self.body {
            Body::Picker => {
                let presets = self.app.read(cx).quick.presets.len();
                crate::quick::place::picker_height(presets, f64::from(HEIGHT), self.max_h)
            }
            _ => f64::from(HEIGHT),
        };
        cx.defer_in(window, move |_, window, _| {
            window.resize(gpui::size(px(WIDTH), px(h as f32)));
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
        self.scroll
            .scroll_to_item(self.pick_at.min(self.pick_rows(cx) - 3));
        self.fit_height(window, cx);
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

    fn set_kind(&mut self, k: PresetKind, cx: &mut Context<Self>) {
        self.st.set_kind(k);
        self.tile = None;
        self.menu = None;
        self.changed(cx);
    }

    /// Enter or Space: close, wait, run (QC-1 "Window behaviour").
    fn go(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let run = self.st.run();
        let preset = self.st.preset.as_ref().map(|p| p.name.clone());
        self.changed(cx);
        dismiss(window, cx, true);
        let app = self.app.clone();
        // The panel is gone by then: the wait runs on the app, not on the closed window.
        cx.defer(move |cx| {
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(CAPTURE_DELAY_MS))
                    .await;
                cx.update(|cx| crate::ui::quick_flow::execute(&app, run, preset, cx));
            })
            .detach();
        });
    }

    fn click_tile(&mut self, t: Tile, cx: &mut Context<Self>) {
        if t.has_values() {
            self.menu = Some(t);
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
        self.tile = None;
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
        crate::ui::quick_flow::preset_call(
            &self.app,
            "save_preset",
            serde_json::json!({ "preset": json }),
            move |_, res, cx| {
                this.update(cx, |this, cx| match res {
                    Ok(v) => {
                        // The new preset is the picked one: the bar names it.
                        if let Ok(p) =
                            serde_json::from_value::<Preset>(v.get("preset").cloned().unwrap_or(v))
                        {
                            this.st.apply_preset(&p);
                            this.changed(cx);
                        }
                        this.body = Body::Main;
                        cx.notify();
                    }
                    Err(e) => {
                        this.name_error = Some(e);
                        cx.notify();
                    }
                });
            },
            cx,
        );
        let _ = window;
    }

    fn open_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.body = Body::Name;
        self.name_error = None;
        self.name.update(cx, |f, cx| f.set_text("", cx));
        self.fit_height(window, cx);
        let handle = self.name.read(cx).focus.clone();
        window.focus(&handle, cx);
        cx.notify();
    }

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = self.app.clone();
        dismiss(window, cx, false);
        cx.defer(move |cx| crate::ui::quick_flow::open_presets_page(&app, cx));
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        let m = k.modifiers;
        if m.platform || m.control || m.alt {
            return;
        }
        let screen = match self.body {
            Body::Main => Screen::Main,
            Body::Picker => Screen::Picker,
            Body::Name => Screen::Name,
        };
        let Some(key) = panel_key(&k.key, m.shift, screen, self.menu.is_some()) else {
            return;
        };
        match key {
            PanelKey::Close => dismiss(window, cx, true),
            PanelKey::Back => self.back(window, cx),
            PanelKey::Go => {
                if self.menu.take().is_some() {
                    cx.notify();
                } else {
                    self.go(window, cx);
                }
            }
            PanelKey::NextTile | PanelKey::PrevTile => {
                let n = Tile::of(self.st.kind()).len();
                let back = key == PanelKey::PrevTile;
                self.tile = Some(match (self.tile, back) {
                    (None, false) => 0,
                    (None, true) => n - 1,
                    (Some(i), false) => (i + 1) % n,
                    (Some(i), true) => (i + n - 1) % n,
                });
                cx.notify();
            }
            PanelKey::TileUp | PanelKey::TileDown => {
                if let Some(i) = self.tile {
                    self.step_tile(Tile::of(self.st.kind())[i], key == PanelKey::TileDown, cx);
                }
            }
            PanelKey::TargetPrev => self.cycle_target(false, cx),
            PanelKey::TargetNext => self.cycle_target(true, cx),
            PanelKey::Picker => self.open_picker(window, cx),
            PanelKey::PickerUp | PanelKey::PickerDown => {
                let rows = self.pick_rows(cx);
                self.pick_at = picker_step(self.pick_at, key == PanelKey::PickerDown, rows);
                self.scroll
                    .scroll_to_item(self.pick_at.min(self.pick_rows(cx) - 3));
                cx.notify();
            }
            PanelKey::PickerGo => self.pick_go(window, cx),
            PanelKey::Kind(kind) => self.set_kind(kind, cx),
        }
        cx.stop_propagation();
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
        .hover(move |s| s.bg(hover))
        .focus_visible(move |s| s.border_color(focus))
}

impl QuickPanel {
    fn header(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let name = self
            .st
            .preset
            .as_ref()
            .map_or_else(|| tr_ctx!("preset picker", "No preset"), |p| p.name.clone());
        let picker_aria = tr_ctx!("preset picker", "Preset: {name}", name = name.clone());
        let picker = pill_button(t, "qc-preset-pill", picker_aria)
            .h(px(32.))
            .flex_1()
            .min_w(px(0.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(scale::RADIUS_PILL))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface_2))
            .text_size(px(type_scale::UI - 1.))
            .text_color(rgba_of(if self.st.preset.is_some() {
                t.text
            } else {
                t.text_3
            }))
            .on_click(cx.listener(|this, _, window, cx| this.open_picker(window, cx)))
            .child(icon("bookmark", 14., t.text_3))
            .child(crate::ui::components::shrink_text(name).flex_1())
            .child(icon("chevron-down", 14., t.text_3));
        let feedback = pill_button(t, "qc-feedback", tr_ctx!("button", "Send feedback"))
            .h(px(32.))
            .px(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .rounded(px(scale::RADIUS_MD))
            .text_size(px(type_scale::UI - 1.))
            .text_color(rgba_of(t.text_2))
            .on_click(|_, _, cx| send_feedback(PreviewFeature::QuickCapture, cx))
            .child(SharedString::from(tr_ctx!("button", "Send feedback")));
        let settings = icon_button(
            t,
            "qc-settings",
            "settings",
            tr_ctx!("quick capture", "Preset settings"),
            false,
            None,
        )
        .size(px(32.))
        .on_click(cx.listener(|this, _, window, cx| this.open_settings(window, cx)));
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(logo(t, 20.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(type_scale::UI))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .whitespace_nowrap()
                                    .child(SharedString::from(tr!("Quick Capture"))),
                            )
                            .child(preview_label(
                                t,
                                Size::S,
                                PreviewFeature::QuickCapture,
                                None,
                                None,
                            )),
                    )
                    .child(settings),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(picker)
                    .child(feedback),
            )
    }

    fn kind_tabs(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let active = self.st.kind();
        div()
            .flex()
            .gap(px(4.))
            .p(px(3.))
            .rounded(px(scale::RADIUS_MD))
            .bg(rgba_of(t.surface_2))
            .border_1()
            .border_color(rgba_of(t.border))
            .children(
                [PresetKind::Screenshot, PresetKind::Video, PresetKind::Audio]
                    .into_iter()
                    .enumerate()
                    .map(|(i, k)| {
                        let on = k == active;
                        let label = kind_label(k);
                        let mut b = pill_button(
                            t,
                            SharedString::from(format!("qc-kind-{i}")),
                            tr_ctx!(
                                "quick capture kind",
                                "{kind}, key {n}",
                                kind = label.clone(),
                                n = i + 1
                            ),
                        )
                        .h(px(36.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(6.))
                        .rounded(px(scale::RADIUS_SM))
                        .text_size(px(type_scale::UI - 1.))
                        .font_weight(FontWeight::MEDIUM)
                        .aria_selected(on)
                        .on_click(cx.listener(move |this, _, _, cx| this.set_kind(k, cx)));
                        b = if on {
                            b.flex_1().bg(rgba_of(t.raised)).text_color(rgba_of(t.text))
                        } else {
                            b.w(px(44.)).flex_none().text_color(rgba_of(t.text_3))
                        };
                        // Only the active tab shows its name.
                        b.child(icon(kind_icon(k), 16., if on { t.text } else { t.text_3 }))
                            .when(on, |d| d.child(SharedString::from(label)))
                    }),
            )
    }

    fn target_row(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        if self.st.kind() == PresetKind::Audio {
            return div()
                .h(px(36.))
                .flex()
                .items_center()
                .px(px(4.))
                .text_size(px(type_scale::UI - 1.))
                .text_color(rgba_of(t.text_3))
                .child(SharedString::from(tr!("Mic and system sound, no screen")));
        }
        let cur = self.st.target();
        div()
            .flex()
            .gap(px(4.))
            .children(self.st.targets().iter().enumerate().map(|(i, &tg)| {
                let on = Some(tg) == cur;
                pill_button(
                    t,
                    SharedString::from(format!("qc-target-{i}")),
                    tr_ctx!(
                        "quick capture target",
                        "{target}, {state}",
                        target = target_label(tg),
                        state = if on {
                            tr!("selected")
                        } else {
                            tr!("not selected")
                        }
                    ),
                )
                .h(px(32.))
                // Each tab is as wide as its label needs and shares what is left (L-10: at
                // +30 % text "Long Shot" wrapped to two lines and was clipped in an equal cell).
                .flex_auto()
                .min_w(px(0.))
                .px(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(scale::RADIUS_SM))
                .border_1()
                .border_color(rgba_of(if on { t.line } else { t.border }))
                .bg(rgba_of(if on { t.raised } else { Color(0) }))
                .text_size(px(type_scale::UI - 1.))
                .text_color(rgba_of(if on { t.text } else { t.text_3 }))
                .aria_selected(on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.st.set_target(tg);
                    this.changed(cx);
                }))
                .child(crate::ui::components::ellipsis_text(target_label(tg)))
            }))
    }

    fn tiles(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        if let Some(menu) = self.menu {
            return self.value_menu(t, menu, cx);
        }
        let tiles = Tile::of(self.st.kind());
        let cell = |i: usize, tile: Tile, cx: &mut Context<Self>| {
            let on = self.st.is_on(tile);
            let focused = self.tile == Some(i);
            let (circle_bg, circle_fg) = if on {
                (t.primary_bg, t.primary_fg)
            } else {
                (t.raised, t.text_3)
            };
            pill_button(t, tile.id(), self.st.tile_aria(tile))
                .h(px(52.))
                .flex_1()
                .min_w(px(0.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(scale::RADIUS_MD))
                .border_1()
                .border_color(rgba_of(if focused { t.text } else { t.border }))
                .bg(rgba_of(t.surface_2))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tile = Some(i);
                    this.click_tile(tile, cx);
                }))
                .child(
                    div()
                        .size(px(28.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(14.))
                        .bg(rgba_of(circle_bg))
                        .child(icon(tile.icon(), 14., circle_fg)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(type_scale::UI - 1.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgba_of(if on { t.text } else { t.text_2 }))
                                .child(crate::ui::components::ellipsis_text(tile.label())),
                        )
                        .children(self.st.value_text(tile).map(|v| {
                            div()
                                .text_size(px(11.))
                                .text_color(rgba_of(t.text_3))
                                .child(crate::ui::components::ellipsis_text(v))
                        })),
                )
        };
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(cell(0, tiles[0], cx))
                    .child(cell(1, tiles[1], cx)),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(cell(2, tiles[2], cx))
                    .child(cell(3, tiles[3], cx)),
            )
    }

    /// The small menu of a value tile (Delay, Countdown): its choices, in place of the grid.
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
                        .h(px(52.))
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
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.st.set_value(tile, secs);
                            this.menu = None;
                            this.changed(cx);
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

    fn action_bar(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let (text, record) = self.st.action_text();
        let (bg, hover, fg) = if record {
            (t.rec, t.rec_hover, t.on_rec)
        } else {
            (t.primary_bg, t.primary_hover, t.primary_fg)
        };
        let platform = self.app.read(cx).platform;
        let keys = vec![tr_ctx!("quick capture", "Enter")];
        let _ = platform;
        div()
            .id("qc-action")
            .role(Role::Button)
            .aria_label(SharedString::from(text.clone()))
            .focusable()
            .tab_stop(true)
            .h(px(44.))
            .w_full()
            .px(px(14.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .rounded(px(12.))
            .bg(rgba_of(bg))
            .text_color(rgba_of(fg))
            .text_size(px(type_scale::UI))
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .hover(move |s| s.bg(rgba_of(hover)))
            .on_click(cx.listener(|this, _, window, cx| this.go(window, cx)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .child(crate::ui::components::shrink_text(text)),
            )
            .children(keys.into_iter().map(move |k| {
                div()
                    .px(px(7.))
                    .py(px(2.))
                    .rounded(px(scale::RADIUS_XS))
                    .bg(rgba_of(t.ink.with_alpha(18)))
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .mono()
                    .child(SharedString::from(format!("\u{21B5} {k}")))
            }))
    }

    fn picker(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let presets = self.app.read(cx).quick.presets.clone();
        let picked = self.st.preset.as_ref().map(|p| p.id.clone());
        let at = self.pick_at;
        let row = |id: String, title: String, sub: Option<String>, on: bool, cur: bool| {
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
            .children(on.then(|| icon("check", 14., t.text)))
        };
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        rows.push(
            row(
                "qc-pick-none".into(),
                tr_ctx!("preset picker", "No preset"),
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
}

impl Render for QuickPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let body = match self.body {
            Body::Main => div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(self.kind_tabs(&t, cx))
                .child(self.target_row(&t, cx))
                .child(self.tiles(&t, cx))
                .child(div().flex_1())
                .child(self.action_bar(&t, cx)),
            Body::Picker => self.picker(&t, cx),
            Body::Name => self.name_body(&t, cx),
        };
        let _ = window;
        div()
            .id("quick-capture")
            .role(Role::Dialog)
            .aria_label(tr!("Quick Capture"))
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .font_family(UI_FONT)
            .font_features(ui_features())
            .text_color(rgba_of(t.text))
            .text_size(px(type_scale::UI))
            .child(
                div()
                    .size_full()
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .rounded(px(18.))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.surface))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(self.header(&t, cx))
                    .child(body),
            )
    }
}
