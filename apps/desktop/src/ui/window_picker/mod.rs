//! The window picker palette (board R19, "Window picker (screenshot)").
//!
//! The Window tile, the Window shortcut and the tray open it instead of the hover-only mode. It is
//! a thin view over [`PickerState`](crate::window_picker::state::PickerState): a borderless
//! floating panel centred on the pointer's display, built like the command launcher (it takes the
//! keyboard by activating the app, hands focus back to the previous app on close and closes when
//! the app is deactivated). The selected window is outlined on the real screen
//! ([`outline`]); thumbnails are loaded lazily ([`Thumbs`](crate::window_picker::thumbs::Thumbs)).
//!
//! Keys: arrows move, Home and End jump, Enter captures, Esc cancels, Tab hands over to the hover
//! mode ("Pick on screen"; Tab there comes back here), Cmd+1..9 jumps to an app of the rail,
//! Cmd+0 shows all apps. Everything else types into the search field.

mod cards;
pub mod outline;
mod rail;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use cm_i18n::{tr, tr_ctx, trn};
use gpui::{
    div, point, prelude::*, px, size, App, Bounds, Context, Entity, FocusHandle, FontWeight,
    Global, KeyDownEvent, Render, Role, ScrollHandle, SharedString, Toggled, Window, WindowHandle,
    WindowOptions,
};

use crate::backend::Backend;
use crate::overlay::{CaptureMode, OverlayError};
use crate::recording::geometry::Rect;
use crate::shortcuts::Platform;
use crate::text_buffer::TextBuffer;
use crate::theme::{derived, scale, type_scale, Theme};
use crate::ui::app::{self, AppModel};
use crate::ui::components::{icon, label_fill};
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};
use crate::window_picker::session::{self, OutlineRect};
use crate::window_picker::state::{Outcome, PickerState};
use crate::window_picker::thumbs::Thumbs;
use crate::window_picker::{DisplayFilter, Nav, PickerDisplay};

/// The palette's size (board: 1000 x 740), shrunk to fit a small display.
pub const WIDTH: f32 = 1000.;
pub const HEIGHT: f32 = 740.;
/// The app rail's width (board: 230).
pub const RAIL_W: f32 = 230.;
/// Space kept free around the palette on a small display.
const MARGIN: f32 = 24.;
/// A palette narrower than this shows two cards per row instead of three.
const NARROW_MAIN_W: f32 = 600.;
/// Pause before the outline follows the selection (arrow keys held down do not flicker).
const OUTLINE_DEBOUNCE: Duration = Duration::from_millis(60);
/// Deactivations right after the outline opened are the outline's, not the person's.
const OUTLINE_GRACE: Duration = Duration::from_millis(500);

struct PickerSlot(Option<WindowHandle<PickerView>>);
impl Global for PickerSlot {}

/// The app that was frontmost when the picker opened (macOS), to hand focus back to.
struct PreviousApp(Option<i32>);
impl Global for PreviousApp {}

// ---------------------------------------------------------------------------------------------
// Texts (also measured by the width tests)

pub fn display_name(d: &PickerDisplay) -> String {
    if d.builtin {
        tr_ctx!("window picker display", "Built-in")
    } else if d.name.trim().is_empty() {
        tr_ctx!("window picker display", "External")
    } else {
        d.name.clone()
    }
}

pub fn filter_all_displays() -> String {
    tr_ctx!("window picker display", "All displays")
}

pub fn filter_built_in() -> String {
    tr_ctx!("window picker display", "Built-in")
}

pub fn filter_external() -> String {
    tr_ctx!("window picker display", "External")
}

/// "22 windows", or "4 of 22 windows" while a search or a filter hides some.
pub fn count_text(shown: usize, total: usize) -> String {
    if shown == total {
        trn!("{n} window", "{n} windows", total)
    } else {
        tr!("{shown} of {total} windows", shown = shown, total = total)
    }
}

pub fn sort_hint(query_blank: bool) -> String {
    if query_blank {
        tr!("Front to back, most recently used first")
    } else {
        tr!("Best matches first, then front to back")
    }
}

pub fn search_placeholder() -> String {
    tr!("Search windows or apps")
}

pub fn pick_on_screen() -> String {
    tr!("Pick on screen")
}

pub fn capture_window() -> String {
    tr!("Capture window")
}

pub fn match_note() -> String {
    tr!("Matches the window title and the app name.")
}

pub fn all_windows() -> String {
    tr!("All windows")
}

/// The modifier of the app shortcut: Cmd on a Mac, Ctrl elsewhere.
pub fn app_keys(platform: Platform) -> String {
    match platform {
        Platform::Mac => "\u{2318}1\u{2013}9".to_owned(),
        Platform::Windows => "Ctrl+1\u{2013}9".to_owned(),
    }
}

pub fn footer_hints(platform: Platform) -> [(String, String); 4] {
    [
        (
            "\u{2191}\u{2193}\u{2190}\u{2192}".to_owned(),
            tr_ctx!("window picker footer", "Move"),
        ),
        (
            "\u{21B5}".to_owned(),
            tr_ctx!("window picker footer", "Capture"),
        ),
        (app_keys(platform), tr_ctx!("window picker footer", "App")),
        ("Esc".to_owned(), tr_ctx!("window picker footer", "Cancel")),
    ]
}

// ---------------------------------------------------------------------------------------------
// Opening

/// Hotkey, dock tile or tray: opens the picker, or closes it when it is already open. Ignored
/// while another capture or delay runs (CAP-2).
pub fn open(app: &Entity<AppModel>, cx: &mut App) {
    crate::trace::mark("window_picker_open");
    let open = cx.try_global::<PickerSlot>().and_then(|s| s.0).filter(|h| {
        h.update(cx, |this, window, cx| this.close(window, cx, true))
            .is_ok()
    });
    if open.is_some() {
        cx.set_global(PickerSlot(None));
        return;
    }
    let (backend, busy) = {
        let m = app.read(cx);
        (m.backend.clone(), m.capture_busy)
    };
    if busy {
        return;
    }
    let Some(daemon) = backend.daemon.clone() else {
        app::push_toast(
            app,
            crate::flows::error_toast_live(
                || tr!("Capture unavailable"),
                tr!("The library couldn\u{2019}t be opened, so there is nowhere to save captures."),
            ),
            cx,
        );
        return;
    };
    // Before the app is activated for the palette: this is the app to give focus back to.
    cx.set_global(PreviousApp(crate::launcher::focus::frontmost_other_app()));
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let answer = backend
            .rt
            .spawn(async move {
                Backend::call_coded(&daemon, "list_targets", serde_json::json!({})).await
            })
            .await;
        cx.update(|cx| match answer {
            Ok(Ok(v)) => present(&app2, &v, cx),
            Ok(Err(e)) => fail(&app2, &e, cx),
            Err(e) => fail(
                &app2,
                &crate::backend::CallError::message(e.to_string()),
                cx,
            ),
        });
    })
    .detach();
}

fn fail(app: &Entity<AppModel>, e: &crate::backend::CallError, cx: &mut App) {
    cx.set_global(PreviousApp(None));
    let text = crate::daemon_text::call_error_text(e);
    let spec = if e.code == Some(cm_core::ErrorCode::PermissionMissing) {
        // The same toast the hover mode shows, with its buttons for Permissions.
        crate::flows::overlay_error_toast(&OverlayError::PermissionMissing(text))
    } else {
        crate::flows::error_toast_live(|| tr!("Capture didn\u{2019}t start"), text)
    };
    app::push_toast(app, spec, cx);
}

fn present(app: &Entity<AppModel>, targets: &serde_json::Value, cx: &mut App) {
    let listing = match session::parse_targets(targets, &crate::platform_window::display_frame) {
        Ok(l) => l,
        Err(e) => {
            fail(app, &crate::backend::CallError::message(e), cx);
            return;
        }
    };
    let (backend, platform) = {
        let m = app.read(cx);
        (m.backend.clone(), m.platform)
    };
    let last_used = session::read_last_used(&backend.settings.snapshot());
    let mut state = PickerState::new(listing.windows, listing.displays, last_used, false);
    let display = crate::ui::recording::pointer_display(cx);
    let (dw, dh) = match display {
        Some((_, r)) => (r.w as f32, r.h as f32),
        None => (1440., 900.),
    };
    let visible = display
        .map(|(_, r)| crate::ui::recording::visible_part(&r))
        .unwrap_or_else(|| Rect::new(0., 0., f64::from(dw), f64::from(dh)));
    let w = WIDTH.min(dw - 2. * MARGIN).max(560.);
    let h = HEIGHT.min(dh - 2. * MARGIN).max(420.);
    state.set_columns(columns_for(w));
    let (x, y, w, h) = crate::platform_window::centered_frame(
        (visible.x, visible.y, visible.w, visible.h),
        f64::from(w),
        f64::from(h),
    );
    let global = Rect::new(x, y, w, h);
    let opened = match display {
        Some(d) => cx.open_window(
            crate::ui::recording::panel_options(d, global, true, false),
            view_builder(app.clone(), state, listing.outlines, platform),
        ),
        None => cx.open_window(
            WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(Bounds {
                    origin: point(
                        px(((dw - w as f32) / 2.).max(0.)),
                        px(((dh - h as f32) / 2.).max(0.)),
                    ),
                    size: size(px(w as f32), px(h as f32)),
                })),
                titlebar: None,
                focus: true,
                show: true,
                kind: gpui::WindowKind::PopUp,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: gpui::WindowBackgroundAppearance::Transparent,
                ..crate::platform_window::popup_base()
            },
            view_builder(app.clone(), state, listing.outlines, platform),
        ),
    };
    match opened {
        Ok(handle) => {
            cx.set_global(PickerSlot(Some(handle)));
            // The panel needs the keyboard, and a non-activating window gets none from macOS.
            cx.activate(true);
        }
        Err(e) => {
            cx.set_global(PreviousApp(None));
            app::push_toast(
                app,
                crate::flows::error_toast_live(
                    || tr!("Couldn\u{2019}t open the window picker"),
                    e.to_string(),
                ),
                cx,
            );
        }
    }
}

/// Cards per row for a palette `width` wide.
pub fn columns_for(width: f32) -> usize {
    let main = width - RAIL_W - 36.;
    if main >= NARROW_MAIN_W {
        crate::window_picker::state::GRID_COLUMNS
    } else {
        2
    }
}

fn view_builder(
    app: Entity<AppModel>,
    state: PickerState,
    outlines: HashMap<u32, OutlineRect>,
    platform: Platform,
) -> impl FnOnce(&mut Window, &mut App) -> Entity<PickerView> {
    move |window, cx| {
        // A screenshot taken while the picker is open must not contain it (R4-6).
        crate::platform_window::exclude_from_capture(window);
        cx.new(|cx| PickerView::new(app, state, outlines, platform, window, cx))
    }
}

// ---------------------------------------------------------------------------------------------
// The view

pub(crate) struct PickerView {
    app: Entity<AppModel>,
    state: PickerState,
    outlines: HashMap<u32, OutlineRect>,
    platform: Platform,
    input: TextBuffer,
    focus: FocusHandle,
    scroll: ScrollHandle,
    thumbs: Entity<Thumbs>,
    /// The window ids last asked of `thumbs` (a repaint asks again only when they change).
    requested: Vec<u32>,
    scroll_to_selected: bool,
    was_active: bool,
    outline: Option<WindowHandle<outline::OutlineView>>,
    outline_seq: u64,
    outline_for: Option<u32>,
    outline_grace_until: Option<Instant>,
    /// Set once a close was started, so the deactivation it causes is not a second close.
    closing: bool,
    /// Where the pointer was last seen over a card.
    last_pointer: Option<gpui::Point<gpui::Pixels>>,
}

impl PickerView {
    fn new(
        app: Entity<AppModel>,
        state: PickerState,
        outlines: HashMap<u32, OutlineRect>,
        platform: Platform,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let backend = app.read(cx).backend.clone();
        let thumbs = cx.new(|_| Thumbs::new(backend));
        cx.observe(&thumbs, |_, _, cx| cx.notify()).detach();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        // Clicking another app closes the picker without capturing (like the launcher, LNC-1.AC9).
        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.was_active = true;
            } else if this.was_active && !this.closing {
                let in_grace = this.outline_grace_until.is_some_and(|t| Instant::now() < t);
                if !in_grace {
                    this.close(window, cx, false);
                }
            }
        })
        .detach();
        let mut view = PickerView {
            app,
            state,
            outlines,
            platform,
            input: TextBuffer::default(),
            focus,
            scroll: ScrollHandle::new(),
            thumbs,
            requested: Vec::new(),
            scroll_to_selected: true,
            was_active: false,
            outline: None,
            outline_seq: 0,
            outline_for: None,
            outline_grace_until: None,
            closing: false,
            last_pointer: None,
        };
        view.schedule_outline(window, cx);
        view
    }

    // ---- closing ---------------------------------------------------------------------------

    /// Closes the palette and its outline. With `restore`, the previous app gets focus back (Esc
    /// and a second hotkey press); a capture or the hover mode bring their own focus.
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>, restore: bool) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.outline_seq += 1;
        if let Some(h) = self.outline.take() {
            let _ = h.update(cx, |_, w, _| w.remove_window());
        }
        self.thumbs.update(cx, |t, cx| t.release(cx));
        window.remove_window();
        cx.set_global(PickerSlot(None));
        let previous = cx.try_global::<PreviousApp>().and_then(|p| p.0);
        cx.set_global(PreviousApp(None));
        if let (true, Some(pid)) = (restore, previous) {
            cx.defer(move |_| crate::launcher::focus::activate(pid));
        }
    }

    fn apply(&mut self, outcome: Outcome, window: &mut Window, cx: &mut Context<Self>) {
        match outcome {
            Outcome::None => {}
            Outcome::Cancel => {
                self.close(window, cx, true);
                return;
            }
            Outcome::PickOnScreen => {
                // The overlay is its own process and takes the screen and the keyboard.
                self.close(window, cx, false);
                let app = self.app.clone();
                cx.defer(move |cx| app::start_hover_capture(&app, CaptureMode::Window, cx));
                return;
            }
            Outcome::Capture {
                window_id,
                app: owner,
                title,
            } => {
                let remembered = self.state.touch_last_used(&owner, &title).clone();
                self.remember(remembered, cx);
                // Close first (the previous app's focus returns), then capture exactly as the
                // hover mode did.
                self.close(window, cx, true);
                let app = self.app.clone();
                cx.defer(move |cx| app::run_window_capture(&app, window_id, cx));
                return;
            }
        }
        self.after_change(window, cx);
    }

    /// Writes the last-used memory as the user. A failure only loses the memory: no toast.
    fn remember(&self, last_used: crate::window_picker::LastUsed, cx: &mut Context<Self>) {
        let backend = self.app.read(cx).backend.clone();
        let Some(daemon) = backend.daemon.clone() else {
            return;
        };
        let value = session::value_to_store(&last_used);
        backend.rt.spawn(async move {
            let params = serde_json::json!({ "values": { session::KEY_LAST_USED: value } });
            if let Err(e) = Backend::call(&daemon, "set_settings", params).await {
                log::warn!("could not remember the picked window: {e}");
            }
        });
    }

    // ---- state changes ---------------------------------------------------------------------

    fn after_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scroll_to_selected = true;
        self.schedule_outline(window, cx);
        cx.notify();
    }

    fn set_app_filter(
        &mut self,
        filter: crate::window_picker::AppFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.set_app_filter(filter);
        self.after_change(window, cx);
    }

    fn set_display_filter(
        &mut self,
        f: DisplayFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.set_display_filter(f);
        self.after_change(window, cx);
    }

    fn select(&mut self, pos: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.selected_pos() != Some(pos) {
            self.state.select_pos(pos);
            self.schedule_outline(window, cx);
            cx.notify();
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        let m = k.modifiers;
        let cmd = m.platform || m.control;
        let before = self.input.text().to_owned();
        let outcome = match k.key.as_str() {
            "escape" => self.state.escape(),
            "enter" => self.state.enter(),
            "tab" => self.state.tab(),
            "down" => self.nav(Nav::Down),
            "up" => self.nav(Nav::Up),
            "left" => self.nav(Nav::Left),
            "right" => self.nav(Nav::Right),
            "home" => self.nav(Nav::Home),
            "end" => self.nav(Nav::End),
            "backspace" => {
                self.input.backspace();
                Outcome::None
            }
            "delete" => {
                self.input.delete();
                Outcome::None
            }
            "a" if cmd => {
                self.input.select_all();
                Outcome::None
            }
            "v" if cmd => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    self.input.insert(&text);
                }
                Outcome::None
            }
            key if cmd && key.len() == 1 && key.as_bytes()[0].is_ascii_digit() => {
                let n = usize::from(key.as_bytes()[0] - b'0');
                self.state.select_app_number(n);
                Outcome::None
            }
            _ => {
                if cmd || m.function || m.alt && !m.shift && k.key_char.is_none() {
                    return;
                }
                match &k.key_char {
                    Some(ch) => {
                        self.input.insert(ch);
                        Outcome::None
                    }
                    None => return,
                }
            }
        };
        if self.input.text() != before {
            self.state.set_query(self.input.text());
        }
        cx.stop_propagation();
        self.apply(outcome, window, cx);
    }

    fn nav(&mut self, key: Nav) -> Outcome {
        self.state.move_selection(key);
        Outcome::None
    }

    // ---- the outline on the real window -----------------------------------------------------

    /// Moves the outline to the selected window once the selection settles.
    fn schedule_outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.outline_seq += 1;
        let seq = self.outline_seq;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(OUTLINE_DEBOUNCE).await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.outline_seq == seq && !this.closing {
                    this.show_outline(window, cx);
                }
            });
        })
        .detach();
    }

    fn show_outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.state.selected_id();
        if selected == self.outline_for && self.outline.is_some() == selected.is_some() {
            return;
        }
        self.outline_for = selected;
        if let Some(h) = self.outline.take() {
            let _ = h.update(cx, |_, w, _| w.remove_window());
        }
        let Some((w, rect)) = self
            .state
            .selected_window()
            .and_then(|w| self.outlines.get(&w.id).map(|r| (w.clone(), *r)))
        else {
            return;
        };
        let displays = crate::ui::recording::displays(cx);
        let (cx_, cy_) = rect.center();
        let Some(display) = displays
            .iter()
            .find(|(_, r)| r.contains(cx_, cy_))
            .or_else(|| displays.first())
            .copied()
        else {
            return;
        };
        let visible = crate::ui::recording::visible_part(&display.1);
        let layout = outline::outline_frame(rect, visible);
        let title = w.title.clone();
        let opened = cx.open_window(
            outline::options(display, layout.frame),
            move |window, cx| {
                outline::harden(window);
                cx.new(|_| outline::OutlineView::new(layout, title))
            },
        );
        if let Ok(handle) = opened {
            self.outline = Some(handle);
            self.outline_grace_until = Some(Instant::now() + OUTLINE_GRACE);
            // A new window must not keep the keyboard from the palette.
            window.activate_window();
        }
    }

    // ---- thumbnails ------------------------------------------------------------------------

    /// Asks for the thumbnails of the rows on screen and one row beyond, when they changed.
    fn request_thumbs(&mut self, cx: &mut Context<Self>) {
        let cols = if self.state.is_list_layout() {
            1
        } else {
            match self.state.layout() {
                crate::window_picker::Layout::Grid { columns } => columns,
                crate::window_picker::Layout::List => 1,
            }
        };
        let pitch = if self.state.is_list_layout() {
            cards::LIST_ROW_H + cards::LIST_GAP
        } else {
            cards::CARD_H + cards::GRID_GAP
        };
        let offset = -f32::from(self.scroll.offset().y);
        let viewport = f32::from(self.scroll.bounds().size.height);
        let viewport = if viewport > 1. { viewport } else { 4. * pitch };
        let first_row = (offset.max(0.) / pitch).floor() as usize;
        let last_row = ((offset.max(0.) + viewport) / pitch).ceil() as usize + 1;
        let ids: Vec<u32> = self
            .state
            .visible_windows()
            .enumerate()
            .filter(|(i, _)| {
                let row = i / cols;
                row >= first_row && row <= last_row
            })
            .map(|(_, w)| w.id)
            .collect();
        if ids != self.requested {
            self.requested = ids.clone();
            let thumbs = self.thumbs.clone();
            cx.defer(move |cx| thumbs.update(cx, |t, cx| t.request(&ids, cx)));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Painting

impl PickerView {
    fn search_row(&self, t: &Theme, focused: bool) -> gpui::Div {
        let (before, after) = self.input.split();
        let (before, after) = (before.to_owned(), after.to_owned());
        let selected_all = self.input.all_selected();
        let body = if self.input.is_empty() {
            div()
                .flex()
                .items_center()
                .when(focused, |d| {
                    d.child(div().w(px(1.5)).h(px(20.)).bg(rgba_of(t.text)))
                })
                .child(
                    div()
                        .text_color(rgba_of(t.faint))
                        .child(search_placeholder()),
                )
                .into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .child(
                    div()
                        .when(selected_all, |d| d.bg(rgba_of(t.line)))
                        .child(SharedString::from(before)),
                )
                .when(focused && !selected_all, |d| {
                    d.child(div().w(px(1.5)).h(px(20.)).bg(rgba_of(t.text)))
                })
                .child(div().child(SharedString::from(after)))
                .into_any_element()
        };
        let total = self.state.windows().len();
        let shown = self.state.visible_len();
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(12.))
            .px(px(18.))
            .py(px(16.))
            .border_b_1()
            .border_color(rgba_of(t.border))
            .child(
                div()
                    .size(px(34.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(scale::RADIUS_MD))
                    .bg(rgba_of(t.raised))
                    .child(icon("app-window", 17., t.text)),
            )
            .child(
                div()
                    .id("window-picker-input")
                    .role(Role::TextInput)
                    .aria_label(tr!("Search windows"))
                    .aria_placeholder(search_placeholder())
                    .aria_value(SharedString::from(self.input.text().to_owned()))
                    .flex_1()
                    .min_w(px(0.))
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(17.))
                    .child(body),
            )
            .child(
                div()
                    .flex_none()
                    .mono()
                    .text_size(px(12.))
                    .text_color(rgba_of(t.mist))
                    .whitespace_nowrap()
                    .child(SharedString::from(count_text(shown, total))),
            )
    }
}

impl Render for PickerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let focused = self.focus.is_focused(window);
        self.request_thumbs(cx);
        let platform = self.platform;
        let header = self.header(&t, focused, cx);
        let rail = self.rail(&t, cx);
        let main = self.main_column(&t, cx);
        let footer = self.footer(&t, platform, cx);
        div()
            .id("window-picker")
            .role(Role::Dialog)
            .aria_label(tr!("Capture a window"))
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
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded(px(scale::RADIUS_DIALOG_LG))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(derived::PICKER_FILL))
                    .child(header)
                    .child(div().flex_1().min_h(px(0.)).flex().child(rail).child(main))
                    .child(footer),
            )
    }
}

impl PickerView {
    fn header(&mut self, t: &Theme, focused: bool, cx: &mut Context<Self>) -> gpui::Div {
        let mut row = self.search_row(t, focused);
        let displays = self.state.displays();
        let built_in = displays.iter().any(|d| d.builtin);
        let external = displays.iter().any(|d| !d.builtin);
        if built_in && external {
            let current = self.state.display_filter();
            let seg =
                |id: &'static str, label: String, filter: DisplayFilter, cx: &mut Context<Self>| {
                    let active = current == filter;
                    let fg = if active { t.text } else { t.mist };
                    div()
                        .id(id)
                        .role(Role::RadioButton)
                        .aria_label(SharedString::from(label.clone()))
                        .aria_toggled(if active {
                            Toggled::True
                        } else {
                            Toggled::False
                        })
                        .focusable()
                        .tab_stop(true)
                        .h(px(28.))
                        .px(px(10.))
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(6.))
                        .whitespace_nowrap()
                        .rounded(px(7.))
                        .text_size(px(12.5))
                        .text_color(rgba_of(fg))
                        .cursor_pointer()
                        .when(active, |d| d.bg(rgba_of(t.segment_selected())))
                        .child(icon("monitor", 13., fg))
                        .child(SharedString::from(label))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_display_filter(filter, window, cx)
                        }))
                };
            row = row.child(
                div()
                    .id("window-picker-displays")
                    .role(Role::RadioGroup)
                    .aria_label(tr_ctx!("window picker display", "Display"))
                    .flex_none()
                    .flex()
                    .gap(px(2.))
                    .p(px(3.))
                    .rounded(px(scale::RADIUS_MD))
                    .border_1()
                    .border_color(rgba_of(t.border))
                    .bg(rgba_of(t.field))
                    .child(seg(
                        "wp-display-all",
                        filter_all_displays(),
                        DisplayFilter::All,
                        cx,
                    ))
                    .child(seg(
                        "wp-display-builtin",
                        filter_built_in(),
                        DisplayFilter::BuiltIn,
                        cx,
                    ))
                    .child(seg(
                        "wp-display-external",
                        filter_external(),
                        DisplayFilter::External,
                        cx,
                    )),
            );
        }
        row
    }

    fn footer(&self, t: &Theme, platform: Platform, cx: &mut Context<Self>) -> gpui::Div {
        let hint = |keys: String, label: String| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .text_color(rgba_of(t.mist))
                .child(
                    div()
                        .flex_none()
                        .mono()
                        .px(px(6.))
                        .py(px(1.))
                        .rounded(px(5.))
                        .border_1()
                        .border_color(rgba_of(t.line))
                        .bg(rgba_of(t.raised))
                        .text_color(rgba_of(t.text_2))
                        .whitespace_nowrap()
                        .child(SharedString::from(keys)),
                )
                .child(SharedString::from(label))
        };
        let can_capture = self.state.selected_window().is_some();
        let hints = footer_hints(platform);
        div()
            .flex()
            .flex_none()
            .flex_wrap()
            .items_center()
            .gap(px(14.))
            .px(px(18.))
            .py(px(12.))
            .border_t_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(derived::PICKER_FOOTER))
            .children(hints.into_iter().map(|(k, l)| hint(k, l)))
            .child(
                div()
                    .id("window-picker-pick-on-screen")
                    .role(Role::Button)
                    .aria_label(pick_on_screen())
                    .focusable()
                    .tab_stop(true)
                    .ml_auto()
                    .h(px(36.))
                    .px(px(12.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(7.))
                    .whitespace_nowrap()
                    .rounded(px(scale::RADIUS_SM + 1.))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.surface_2))
                    .text_size(px(13.))
                    .text_color(rgba_of(t.text))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba_of(t.raised)))
                    .child(icon("locate", 14., t.text))
                    .child(SharedString::from(pick_on_screen()))
                    .child(
                        div()
                            .mono()
                            .text_size(px(11.))
                            .text_color(rgba_of(t.mist))
                            // i18n-ignore (key name)
                            .child("Tab"),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        let o = this.state.tab();
                        this.apply(o, window, cx);
                    })),
            )
            .child(
                div()
                    .id("window-picker-capture")
                    .role(Role::Button)
                    .aria_label(capture_window())
                    .focusable()
                    .tab_stop(true)
                    .h(px(36.))
                    .px(px(14.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(7.))
                    .whitespace_nowrap()
                    .rounded(px(scale::RADIUS_SM + 1.))
                    .bg(rgba_of(t.primary_bg))
                    .text_size(px(13.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgba_of(t.primary_fg))
                    .when(can_capture, |d| {
                        d.cursor_pointer().hover(|s| s.bg(rgba_of(t.primary_hover)))
                    })
                    .when(!can_capture, |d| d.opacity(0.4))
                    .child(icon("camera", 15., t.primary_fg))
                    .child(SharedString::from(capture_window()))
                    .child(
                        div()
                            .mono()
                            .px(px(6.))
                            .py(px(1.))
                            .rounded(px(5.))
                            .bg(rgba_of(t.ink.with_alpha(12)))
                            .text_size(px(12.))
                            .font_weight(FontWeight::NORMAL)
                            .child("\u{21B5}"),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        let o = this.state.enter();
                        this.apply(o, window, cx);
                    })),
            )
    }

    fn main_column(&mut self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let query_blank = self.state.query().trim().is_empty();
        let toggle = |id: &'static str,
                      glyph: &'static str,
                      label: String,
                      on: bool,
                      list: bool,
                      cx: &mut Context<Self>| {
            let fg = if on { t.text } else { t.mist };
            div()
                .id(id)
                .role(Role::RadioButton)
                .aria_label(SharedString::from(label))
                .aria_toggled(if on { Toggled::True } else { Toggled::False })
                .focusable()
                .tab_stop(true)
                .w(px(28.))
                .h(px(24.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .when(on, |d| d.bg(rgba_of(t.segment_selected())))
                .child(icon(glyph, 13., fg))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.state.set_list_layout(list);
                    this.after_change(window, cx);
                }))
        };
        let list_on = self.state.is_list_layout();
        let toolbar = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.))
            .text_color(rgba_of(t.mist))
            .child(icon("clock", 13., t.mist))
            .child(label_fill(sort_hint(query_blank)))
            .child(
                div()
                    .id("window-picker-layout")
                    .role(Role::RadioGroup)
                    .aria_label(tr!("Layout"))
                    .flex_none()
                    .flex()
                    .gap(px(2.))
                    .p(px(3.))
                    .rounded(px(9.))
                    .border_1()
                    .border_color(rgba_of(t.border))
                    .bg(rgba_of(t.field))
                    .child(toggle(
                        "wp-layout-grid",
                        "layout-grid",
                        tr!("Grid"),
                        !list_on,
                        false,
                        cx,
                    ))
                    .child(toggle(
                        "wp-layout-list",
                        "list",
                        tr!("List"),
                        list_on,
                        true,
                        cx,
                    )),
            );
        let body = self.windows_body(t, cx);
        let note = (!query_blank && self.state.visible_len() > 0).then(|| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(10.))
                .mb(px(14.))
                .px(px(14.))
                .py(px(12.))
                .rounded(px(12.))
                .border_1()
                .border_dashed()
                .border_color(rgba_of(t.line))
                .text_size(px(12.5))
                .text_color(rgba_of(t.mist))
                .child(icon("search", 14., t.mist))
                .child(label_fill(match_note()))
        });
        div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .pt(px(14.))
            .px(px(18.))
            .child(toolbar)
            .child(body)
            .children(note)
    }
}

/// What the window list shows when there is nothing to list.
fn empty_card(t: &Theme, title: String, body: String) -> gpui::AnyElement {
    div()
        .flex_1()
        .min_h(px(0.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .px(px(24.))
        .text_color(rgba_of(t.mist))
        .child(icon("search-x", 22., t.mist))
        .child(
            div()
                .max_w_full()
                .text_size(px(type_scale::UI))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba_of(t.text_2))
                .child(SharedString::from(title)),
        )
        .child(
            div()
                .max_w(px(420.))
                .text_size(px(12.5))
                .child(SharedString::from(body)),
        )
        .into_any_element()
}

pub(crate) fn empty_title() -> String {
    tr!("No windows to capture")
}

pub(crate) fn empty_body() -> String {
    tr!("Open the window you want, then try again, or use Pick on screen.")
}

pub(crate) fn no_match_title(query: &str) -> String {
    tr!("No window matches \u{201C}{query}\u{201D}", query = query)
}

pub(crate) fn no_match_body() -> String {
    tr!("Try another word, or clear the search to see every window.")
}

impl PickerView {
    fn windows_body(&mut self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        if self.state.is_empty() {
            return empty_card(t, empty_title(), empty_body());
        }
        if self.state.is_no_match() {
            return empty_card(
                t,
                no_match_title(self.state.query().trim()),
                no_match_body(),
            );
        }
        self.list(t, cx)
    }
}
