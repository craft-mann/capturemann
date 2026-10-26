//! The command launcher window (LNC-1, DESIGN 5.9, kit screen `Launcher`).
//!
//! A thin view over [`LauncherModel`]: a borderless, non-activating floating panel (so the
//! previously active app keeps focus and gets it back on close), transparent only so the
//! 16 px corners can be round. No shadow and no backdrop blur: GPUI 0.3.9 cannot blur, so the
//! kit's 96% fill is used.

use std::time::Duration;

use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, point, prelude::*, px, size, App, Bounds, ClipboardItem, Context, Entity, FocusHandle,
    FontWeight, Global, KeyDownEvent, MouseButton, Render, Role, ScrollHandle, SharedString,
    Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};

use crate::launcher::commands::{
    self, actions_shortcut, shortcut_text, Command, CommandId, Env, Glyph, Group, Hint,
    LaunchOption, CAPTURE_DELAY_MS,
};
use crate::launcher::mcp_config;
use crate::launcher::model::{remember, LauncherModel, Outcome};
use crate::launcher::plan::{record_plan, shot_plan, ShotPlan};
use crate::overlay::CaptureMode;
use crate::shortcuts::Platform;
use crate::text_buffer::TextBuffer;
use crate::theme::{scale, type_scale, Color, Theme};
use crate::toast::{ToastKind, ToastSpec};
use crate::ui::app::{self, AppHandle, AppModel, Route};
use crate::ui::components::{dot, ellipsis_text, logo};
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};

pub const WIDTH: f32 = 760.;
/// Kit: selected row fill (`#232328`, no token).
const SELECTED_FILL: Color = crate::theme::derived::LAUNCHER_SELECTED;
/// The kit's 96% surface fill needs a backdrop blur to read well; GPUI 0.3.9 has none, so the
/// accepted fallback is the opaque surface (nothing of the user's content bleeds through).
const WINDOW_FILL: Color = crate::theme::Theme::DARK.surface;
/// Kit agent tile: `rgba(91,140,255,.14)` fill, `.4` border.
const AGENT_TILE_FILL: Color = crate::theme::derived::AGENT_TILE_FILL;
const AGENT_TILE_BORDER: Color = crate::theme::derived::AGENT_TILE_BORDER;

const ICONS: [(&str, &str); 8] = [
    (
        "icons/launcher-frame.svg",
        include_str!("../../assets/icons/launcher-frame.svg"),
    ),
    (
        "icons/launcher-window.svg",
        include_str!("../../assets/icons/launcher-window.svg"),
    ),
    (
        "icons/launcher-monitor.svg",
        include_str!("../../assets/icons/launcher-monitor.svg"),
    ),
    (
        "icons/launcher-library.svg",
        include_str!("../../assets/icons/launcher-library.svg"),
    ),
    (
        "icons/launcher-settings.svg",
        include_str!("../../assets/icons/launcher-settings.svg"),
    ),
    (
        "icons/launcher-upload.svg",
        include_str!("../../assets/icons/launcher-upload.svg"),
    ),
    (
        "icons/launcher-terminal.svg",
        include_str!("../../assets/icons/launcher-terminal.svg"),
    ),
    // The Permissions row (icons/CLAUDE.md section 5: it no longer shares the settings glyph).
    (
        "icons/shield-check.svg",
        include_str!("../../assets/icons/shield-check.svg"),
    ),
];

/// Launcher-only SVGs for the app's asset source (`paint::Assets` falls back to this).
pub fn asset(path: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
    ICONS
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, s)| std::borrow::Cow::Borrowed(s.as_bytes()))
}

fn glyph_path(g: Glyph) -> Option<&'static str> {
    Some(match g {
        Glyph::Record => return None,
        Glyph::Mic => "icons/mic.svg",
        Glyph::Frame => "icons/launcher-frame.svg",
        Glyph::Window => "icons/launcher-window.svg",
        Glyph::Monitor => "icons/launcher-monitor.svg",
        Glyph::Library => "icons/launcher-library.svg",
        Glyph::Settings => "icons/launcher-settings.svg",
        Glyph::Upload => "icons/launcher-upload.svg",
        Glyph::Cloud => "icons/cloud.svg",
        Glyph::Terminal => "icons/launcher-terminal.svg",
        Glyph::Permissions => "icons/shield-check.svg",
    })
}

struct LauncherSlot(Option<WindowHandle<LauncherView>>);
impl Global for LauncherSlot {}

/// Commands used in this run, most recent first (LNC-1 ordering).
#[derive(Default)]
struct RecentUse(Vec<CommandId>);
impl Global for RecentUse {}

/// The app that was frontmost when the launcher opened (macOS), to hand focus back to.
struct PreviousApp(Option<i32>);
impl Global for PreviousApp {}

/// Closes the launcher; with `restore`, the previously active app gets focus back (Esc, a
/// second hotkey press, commands that do not bring CaptureMann forward themselves).
fn dismiss(window: &mut Window, cx: &mut App, restore: bool) {
    window.remove_window();
    let previous = cx.try_global::<PreviousApp>().and_then(|p| p.0);
    cx.set_global(PreviousApp(None));
    if let (true, Some(pid)) = (restore, previous) {
        cx.defer(move |_| crate::launcher::focus::activate(pid));
    }
}

/// Hotkey or tray: opens the launcher, or closes it when it is already open (LNC-1.AC9).
pub fn toggle(app: &Entity<AppModel>, cx: &mut App) {
    let open = cx
        .try_global::<LauncherSlot>()
        .and_then(|s| s.0)
        .filter(|h| {
            h.update(cx, |_, window, cx| dismiss(window, cx, true))
                .is_ok()
        });
    if open.is_some() {
        cx.set_global(LauncherSlot(None));
        return;
    }
    open_launcher(app, cx);
}

/// Cursor position in GPUI display coordinates (top-left origin), where the platform allows.
#[cfg(target_os = "macos")]
pub(crate) fn pointer(cx: &App) -> Option<gpui::Point<gpui::Pixels>> {
    let p = objc2_app_kit::NSEvent::mouseLocation();
    let primary_h = cx.primary_display()?.bounds().size.height;
    Some(point(px(p.x as f32), primary_h - px(p.y as f32)))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn pointer(_cx: &App) -> Option<gpui::Point<gpui::Pixels>> {
    None
}

/// Height reserved for the window: the full list (all groups, all rows) plus chrome, capped to
/// 80% of the display. The card inside is content-sized; the rest stays transparent.
fn window_height(rows: usize, groups: usize, display_h: f32) -> f32 {
    // Search row 65, list padding 16, headers 32 + 36 each after the first, rows 50 (30 px
    // tile + 2 x 10 padding), footer 49, border and slack 8.
    let headers = 32. + 36. * groups.saturating_sub(1) as f32;
    let full = 65. + 16. + headers + 50. * rows as f32 + 49. + 8.;
    full.min(display_h * 0.8)
}

pub fn open_launcher(app: &Entity<AppModel>, cx: &mut App) {
    open_launcher_with(app, cx, "");
}

/// `--demo-launcher-query`: opens with the field pre-filled (debug and screenshots).
pub fn open_launcher_with(app: &Entity<AppModel>, cx: &mut App, query: &str) {
    crate::trace::mark("launcher_open");
    let (backend, platform, shortcuts, copy) = {
        let m = app.read(cx);
        (
            m.backend.clone(),
            m.platform,
            m.shortcuts.clone(),
            m.copy_after_capture,
        )
    };
    let mcp = crate::ui::studio::agent_status_of(&backend);
    let commands = commands::build(&Env {
        platform,
        shortcuts: &shortcuts,
        mcp: &mcp,
        capture_available: backend.daemon.is_some(),
        copy_after_capture: copy,
        recording_off: recording_off(app, cx),
        fps60_ready: commands::fps60_ready(
            &backend.settings.snapshot(),
            time::OffsetDateTime::now_utc(),
        ),
    });
    let recent = cx
        .try_global::<RecentUse>()
        .map(|r| r.0.clone())
        .unwrap_or_default();
    cx.set_global(PreviousApp(crate::launcher::focus::frontmost_other_app()));
    let mut model = LauncherModel::new(commands, recent);
    // GPUI places a window relative to its display, so the origin is (0, 0) on whichever
    // display holds the pointer.
    let display = crate::ui::recording::pointer_display(cx);
    let (dw, dh, ox, oy) = match display {
        Some((_, r)) => (r.w as f32, r.h as f32, 0., 0.),
        None => (1440., 900., 0., 0.),
    };
    let h = window_height(model.visible_len(), model.groups().len(), dh);
    model.set_query(query);
    let bounds = Bounds {
        origin: point(px(ox + (dw - WIDTH) / 2.), px(oy + dh * 0.14)),
        size: size(px(WIDTH), px(h)),
    };
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: true,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            display_id: display.map(|(id, _)| id),
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        },
        {
            let query = query.to_owned();
            move |window, cx| {
                // A screenshot taken while the launcher is open must not contain it (R4-6).
                crate::platform_window::exclude_from_capture(window);
                cx.new(|cx| LauncherView::new(model, platform, &query, window, cx))
            }
        },
    );
    match opened {
        // A non-activating panel gets no keyboard input from macOS, so the app is activated
        // (measured: keys went to the previous app) and focus is handed back on close.
        Ok(handle) => {
            cx.set_global(LauncherSlot(Some(handle)));
            cx.activate(true);
        }
        Err(e) => app::push_toast(
            app,
            crate::flows::error_toast_live(
                || tr!("Couldn\u{2019}t open the launcher"),
                e.to_string(),
            ),
            cx,
        ),
    }
}

struct LauncherView {
    model: LauncherModel,
    platform: Platform,
    input: TextBuffer,
    focus: FocusHandle,
    scroll: ScrollHandle,
    /// The disabled command whose reason is flashing (LNC-1.AC5).
    flash: Option<CommandId>,
    /// The disabled row of the Actions list whose reason is flashing.
    flash_action: Option<usize>,
    was_active: bool,
    scroll_to_selected: bool,
}

impl LauncherView {
    fn new(
        model: LauncherModel,
        platform: Platform,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut input = TextBuffer::default();
        input.insert(query);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        // Clicking another app closes the launcher without running anything (LNC-1.AC9).
        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.was_active = true;
            } else if this.was_active {
                dismiss(window, cx, false);
            }
        })
        .detach();
        LauncherView {
            model,
            platform,
            input,
            focus,
            scroll: ScrollHandle::new(),
            flash: None,
            flash_action: None,
            was_active: false,
            scroll_to_selected: false,
        }
    }

    fn apply(&mut self, outcome: Outcome, window: &mut Window, cx: &mut Context<Self>) {
        match outcome {
            Outcome::Run(id) => self.run(id, LaunchOption::Default, window, cx),
            Outcome::RunOption(id, option) => self.run(id, option, window, cx),
            Outcome::Flash(_) => {
                if self.model.actions_open() {
                    self.flash_action = Some(self.model.action_selected());
                } else {
                    self.flash = self.model.selected().map(|c| c.id);
                }
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(700))
                        .await;
                    let _ = this.update(cx, |t, cx| {
                        t.flash = None;
                        t.flash_action = None;
                        cx.notify();
                    });
                })
                .detach();
            }
            Outcome::Close => dismiss(window, cx, true),
            Outcome::Nothing | Outcome::Changed => {}
        }
        self.scroll_to_selected = true;
        cx.notify();
    }

    /// Closes the launcher first, then runs the command, so the launcher is never in a
    /// screenshot or the first frame (LNC-1 "Window behavior", AC6).
    fn run(
        &mut self,
        id: CommandId,
        option: LaunchOption,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut recent = cx
            .try_global::<RecentUse>()
            .map(|r| r.0.clone())
            .unwrap_or_default();
        remember(&mut recent, id);
        cx.set_global(RecentUse(recent));
        // Opening the library or settings brings CaptureMann forward on purpose.
        // The record commands open the start sheet, which takes the keyboard itself.
        let restore = !matches!(
            id,
            CommandId::OpenLibrary
                | CommandId::OpenSettings
                | CommandId::OpenPermissions
                | CommandId::RecordScreen
        );
        dismiss(window, cx, restore);
        cx.defer(move |cx| execute(id, option, cx));
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        let m = k.modifiers;
        let cmd = m.platform || m.control;
        let before = self.input.text().to_owned();
        let outcome = match k.key.as_str() {
            "escape" => self.model.escape(),
            "enter" => self.model.activate(),
            "down" => {
                self.model.move_selection(1);
                Outcome::Changed
            }
            "up" => {
                self.model.move_selection(-1);
                Outcome::Changed
            }
            "n" if m.control => {
                self.model.move_selection(1);
                Outcome::Changed
            }
            "p" if m.control => {
                self.model.move_selection(-1);
                Outcome::Changed
            }
            "k" if cmd => self.model.toggle_actions(),
            "backspace" => {
                self.input.backspace();
                Outcome::Changed
            }
            "delete" => {
                self.input.delete();
                Outcome::Changed
            }
            "left" => {
                self.input.left();
                Outcome::Changed
            }
            "right" => {
                self.input.right();
                Outcome::Changed
            }
            "home" => {
                self.input.home();
                Outcome::Changed
            }
            "end" => {
                self.input.end();
                Outcome::Changed
            }
            "a" if cmd => {
                self.input.select_all();
                Outcome::Changed
            }
            "v" if cmd => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    self.input.insert(&text);
                }
                Outcome::Changed
            }
            _ => {
                if cmd || m.function || m.alt && !m.shift && k.key_char.is_none() {
                    return;
                }
                match &k.key_char {
                    Some(ch) => {
                        self.input.insert(ch);
                        Outcome::Changed
                    }
                    None => return,
                }
            }
        };
        if self.input.text() != before {
            self.model.set_query(self.input.text());
        }
        cx.stop_propagation();
        self.apply(outcome, window, cx);
    }
}

/// Why the recording commands are off right now, or `None`.
fn recording_off(app: &Entity<AppModel>, cx: &App) -> Option<String> {
    let backend = app.read(cx).backend.clone();
    if !backend.recording_enabled || backend.daemon.is_none() {
        return Some(tr!("Library unavailable"));
    }
    crate::ui::recording::is_active(app, cx).then(|| tr!("A recording is running"))
}

/// Runs a command after the launcher closed. Only existing actions (LNC-1 "Commands").
fn execute(id: CommandId, option: LaunchOption, cx: &mut App) {
    let Some(app) = cx.try_global::<AppHandle>().map(|h| h.0.clone()) else {
        return;
    };
    match id {
        CommandId::ScreenshotRegion
        | CommandId::ScreenshotWindow
        | CommandId::ScreenshotDisplay => {
            let mode = match id {
                CommandId::ScreenshotRegion => CaptureMode::Region,
                CommandId::ScreenshotWindow => CaptureMode::Window,
                _ => CaptureMode::Display,
            };
            // An option that cannot run (copy only) is disabled in the list and never gets here.
            if let Some(plan) = shot_plan(option) {
                delayed_capture(app, mode, plan, cx);
            }
        }
        CommandId::OpenLibrary => app::navigate(&app, Route::Library, cx),
        CommandId::OpenSettings => app::navigate(&app, Route::Settings, cx),
        CommandId::OpenPermissions => app::open_permissions_page(&app, cx),
        CommandId::CopyMcpConfig => {
            let backend = app.read(cx).backend.clone();
            let command = mcp_config::locate(
                std::env::current_exe().ok().as_deref(),
                std::env::var_os("PATH"),
            );
            let json = mcp_config::snippet(command.as_deref(), backend.endpoint.as_ref());
            cx.write_to_clipboard(ClipboardItem::new_string(json));
            app::push_toast(
                &app,
                ToastSpec::new(ToastKind::Success, tr!("MCP config copied")),
                cx,
            );
        }
        // Both open the start sheet (REC-1), pre-filled by the chosen option (`RecordPlan`): the
        // first with the webcam on by default, the second after the region picker (no screenshot
        // is taken). Pressing Start in the sheet is the confirmation.
        CommandId::RecordScreen | CommandId::RecordRegion => {
            let Some(plan) = record_plan(id, option) else {
                return;
            };
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(CAPTURE_DELAY_MS))
                    .await;
                cx.update(|cx| {
                    if plan.pick_region {
                        crate::ui::recording::pick_then_sheet_with(
                            &app,
                            CaptureMode::Region,
                            plan.camera,
                            plan.preset,
                            cx,
                        )
                    } else {
                        crate::ui::recording::open_sheet_with(
                            &app,
                            None,
                            plan.camera,
                            plan.preset,
                            cx,
                        )
                    }
                });
            })
            .detach();
        }
        // Opens the start sheet in Audio mode (VOC-1); pressing Start is the confirmation.
        CommandId::RecordAudio => {
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(CAPTURE_DELAY_MS))
                    .await;
                cx.update(|cx| crate::ui::recording::open_audio_sheet(&app, cx));
            })
            .detach();
        }
        // Disabled in this build; the model never returns them from `activate`.
        CommandId::PublishLast | CommandId::SaveToDrive => {}
    }
}

/// Starts a screenshot once the launcher is gone: with the plan's CAP-5 delay (the pill) and
/// its one-off copy choice (LNC-1 Actions).
fn delayed_capture(app: Entity<AppModel>, mode: CaptureMode, plan: ShotPlan, cx: &mut App) {
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(Duration::from_millis(CAPTURE_DELAY_MS))
            .await;
        cx.update(|cx| {
            if app.read(cx).capture_busy {
                return; // CAP-2: ignored while another capture or delay runs.
            }
            app.update(cx, |m, _| m.copy_override = plan.copy);
            match plan.delay_s {
                Some(secs) => crate::ui::delay_pill::start(&app, mode, secs, cx),
                None => app::start_capture(&app, mode, cx),
            }
            // Spent by the capture that started. A Region or Window delay keeps it until its
            // picker opens, so only a start that did not begin a delay clears it here.
            if app.read(cx).delay_run.is_none() {
                app.update(cx, |m, _| m.copy_override = None);
            }
        });
    })
    .detach();
}

/// A keycap as the kit draws it inside launcher rows and the footer: one cap, Geist Mono 12.
fn cap(label: impl Into<SharedString>, fg: Color, bg: Color, py: f32) -> gpui::Div {
    div()
        .flex_none()
        .px(px(7.))
        .py(px(py))
        .rounded(px(scale::RADIUS_XS))
        .bg(rgba_of(bg))
        .text_size(px(type_scale::CAPTION))
        .text_color(rgba_of(fg))
        .mono()
        .child(label.into())
}

fn tile(t: &Theme, c: &Command, selected: bool) -> gpui::Div {
    let agent = c.group == Group::Agents;
    let (bg, border, fg) = if !c.is_enabled() {
        (t.raised, Some(t.line), t.mist)
    } else if selected {
        (t.rec, None, t.ink)
    } else if agent {
        (AGENT_TILE_FILL, Some(AGENT_TILE_BORDER), t.agent_text)
    } else {
        (t.raised, Some(t.line), t.text)
    };
    let glyph = match glyph_path(c.glyph) {
        Some(path) => gpui::svg()
            .path(path)
            .size(px(16.))
            .flex_none()
            .text_color(rgba_of(fg))
            .into_any_element(),
        // The logo mark: crop corners plus the bubble, in the tile's glyph color.
        None => div()
            .relative()
            .size(px(16.))
            .flex_none()
            .child(
                gpui::svg()
                    .path("brand/mark-corners.svg")
                    .size(px(16.))
                    .text_color(rgba_of(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(47. / 4. - 2.75))
                    .top(px(47. / 4. - 2.75))
                    .size(px(5.5))
                    .rounded(px(2.75))
                    .bg(rgba_of(fg)),
            )
            .into_any_element(),
    };
    div()
        .size(px(30.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_SM))
        .bg(rgba_of(bg))
        .when_some(border, |d, b| d.border_1().border_color(rgba_of(b)))
        .child(glyph)
}

impl LauncherView {
    fn row(
        &self,
        t: &Theme,
        index: usize,
        c: &Command,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let flashing = self.flash == Some(c.id);
        let enabled = c.is_enabled();
        let title_color = if enabled { t.text } else { t.mist };
        let hint_color = if flashing {
            t.warning
        } else if !enabled {
            t.mist
        } else if selected {
            t.text_3
        } else {
            t.mist
        };
        let hint: Option<gpui::AnyElement> = match (&c.disabled, &c.hint) {
            (Some(reason), _) => Some(SharedString::from(reason.clone()).into_any_element()),
            (None, Hint::Text(s)) => Some(SharedString::from(s.clone()).into_any_element()),
            (None, Hint::Agent(s)) => Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_color(rgba_of(t.agent_text))
                    .child(dot(t.agent, 7.))
                    .child(ellipsis_text(s.clone()))
                    .into_any_element(),
            ),
            (None, Hint::None) => None,
        };
        let key_cap = (enabled && !c.shortcut.is_empty()).then(|| {
            let (fg, bg) = if selected {
                (t.text_2, t.line)
            } else {
                (t.text_3, t.raised)
            };
            cap(shortcut_text(&c.shortcut, self.platform), fg, bg, 3.)
        });
        div()
            .id(SharedString::from(format!("launcher-row-{index}")))
            .role(Role::ListBoxOption)
            .aria_label(SharedString::from(c.accessible_name(self.platform)))
            .aria_selected(selected)
            .min_h(px(scale::ROW_H))
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(14.))
            .rounded(px(scale::RADIUS_MD))
            .when(selected, |d| d.bg(rgba_of(SELECTED_FILL)))
            .text_size(px(type_scale::BODY))
            .text_color(rgba_of(title_color))
            .cursor_pointer()
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                if this.model.selected_index() != Some(index) && !this.model.actions_open() {
                    this.model.select(index);
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.model.select(index);
                let o = this.model.activate();
                this.apply(o, window, cx);
            }))
            .child(tile(t, c, selected))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .when(selected && enabled, |d| d.font_weight(FontWeight::MEDIUM))
                    .child(SharedString::from(c.title.clone())),
            )
            .children(hint.map(|h| {
                div()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(type_scale::UI - 1.))
                    .text_color(rgba_of(hint_color))
                    .child(h)
            }))
            .children(key_cap)
    }

    fn list(&mut self, t: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mut children: Vec<gpui::AnyElement> = Vec::new();
        let mut selected_child = 0;
        if self.model.actions_open() {
            let title = self
                .model
                .selected()
                .map(|c| c.title.clone())
                .unwrap_or_default();
            children.push(
                header(t, &tr!("Actions for {title}", title = title), true).into_any_element(),
            );
            let sel = self.model.action_selected();
            for (i, a) in self.model.actions().iter().enumerate() {
                if i == sel {
                    selected_child = children.len();
                }
                // A disabled row is dimmed and shows its reason as the hint, like a disabled
                // command (LNC-1.AC5); ↵ on it flashes the reason in the warning color.
                let enabled = a.is_enabled();
                let flashing = self.flash_action == Some(i);
                let label_color = if enabled { t.text } else { t.mist };
                let hint_color = if flashing {
                    t.warning
                } else if enabled && i == sel {
                    t.text_3
                } else {
                    t.mist
                };
                let name = match &a.disabled {
                    Some(reason) => tr!(
                        "{label}, unavailable: {reason}",
                        label = a.label,
                        reason = reason
                    ),
                    None => match &a.hint {
                        Some(h) => tr!("{label}, {hint}", label = a.label, hint = h),
                        None => a.label.clone(),
                    },
                };
                children.push(
                    div()
                        .id(SharedString::from(format!("launcher-action-{i}")))
                        .role(Role::ListBoxOption)
                        .aria_label(SharedString::from(name))
                        .aria_selected(i == sel)
                        .h(px(scale::ROW_H))
                        .px(px(12.))
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(14.))
                        .rounded(px(scale::RADIUS_MD))
                        .when(i == sel, |d| d.bg(rgba_of(SELECTED_FILL)))
                        .text_size(px(type_scale::BODY))
                        .text_color(rgba_of(label_color))
                        .cursor_pointer()
                        .on_mouse_move(cx.listener(move |this, _, _, cx| {
                            if this.model.action_selected() != i {
                                this.model.select_action(i);
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.model.select_action(i);
                            let o = this.model.activate();
                            this.apply(o, window, cx)
                        }))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .truncate()
                                .when(i == sel && enabled, |d| d.font_weight(FontWeight::MEDIUM))
                                .child(SharedString::from(a.label.clone())),
                        )
                        .children(a.hint.clone().map(|h| {
                            div()
                                .min_w(px(0.))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(type_scale::UI - 1.))
                                .text_color(rgba_of(hint_color))
                                .child(SharedString::from(h))
                        }))
                        .into_any_element(),
                );
            }
        } else if self.model.visible_len() == 0 {
            children.push(
                div()
                    .h(px(scale::ROW_H))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .text_size(px(type_scale::UI))
                    .text_color(rgba_of(t.mist))
                    .child(tr!("No commands match"))
                    .into_any_element(),
            );
        } else {
            let sel = self.model.selected_index();
            let rows: Vec<Command> = self.model.visible().cloned().collect();
            let mut last: Option<Group> = None;
            let mut first = true;
            for (i, c) in rows.iter().enumerate() {
                if last != Some(c.group) {
                    children.push(header(t, &c.group.label(), first).into_any_element());
                    first = false;
                    last = Some(c.group);
                }
                if sel == Some(i) {
                    selected_child = children.len();
                }
                children.push(self.row(t, i, c, sel == Some(i), cx).into_any_element());
            }
        }
        if self.scroll_to_selected {
            self.scroll_to_selected = false;
            self.scroll.scroll_to_item(selected_child);
        }
        div()
            .id("launcher-list")
            .role(Role::ListBox)
            .aria_label(tr!("Commands"))
            .flex_1()
            .min_h(px(0.))
            .p(px(8.))
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(children)
            .into_any_element()
    }

    fn footer(&self, t: &Theme) -> gpui::Div {
        let (verb, enter) = if self.model.actions_open() {
            (tr_ctx!("launcher action", "Run"), true)
        } else {
            match self.model.selected() {
                Some(c) if c.is_enabled() => (c.verb.clone(), true),
                Some(_) => (tr_ctx!("launcher footer", "Unavailable"), false),
                None => (String::new(), false),
            }
        };
        let platform = self.platform;
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(12.))
            .px(px(16.))
            .py(px(10.))
            .border_t_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.sidebar))
            .text_size(px(type_scale::UI - 1.))
            .text_color(rgba_of(t.text_3))
            .child(logo(t, 16.))
            .child(div().flex_1().child("CaptureMann"))
            .child(SharedString::from(verb))
            .when(enter, |d| d.child(cap("\u{21B5}", t.text_2, t.raised, 2.)))
            .child(div().w(px(1.)).h(px(16.)).bg(rgba_of(t.line)))
            .child(if self.model.actions_open() {
                tr_ctx!("launcher footer", "Back")
            } else {
                tr_ctx!("launcher footer", "Actions")
            })
            .child(cap(
                if self.model.actions_open() {
                    "esc".to_owned()
                } else {
                    actions_shortcut(platform).to_owned()
                },
                t.text_2,
                t.raised,
                2.,
            ))
    }
}

fn header(t: &Theme, label: &str, first: bool) -> gpui::Div {
    div()
        .flex_none()
        .px(px(12.))
        .pt(px(if first { 10. } else { 14. }))
        .pb(px(6.))
        .text_size(px(type_scale::CAPTION))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba_of(t.mist))
        .child(SharedString::from(label.to_owned()))
}

impl Render for LauncherView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let focused = self.focus.is_focused(window);
        let (before, after) = self.input.split();
        let (before, after) = (before.to_owned(), after.to_owned());
        let empty = self.input.is_empty();
        let selected_all = self.input.all_selected();
        let body = if empty {
            div()
                .flex()
                .items_center()
                .when(focused, |d| {
                    d.child(div().w(px(1.)).h(px(22.)).bg(rgba_of(t.text)))
                })
                .child(
                    div()
                        .text_color(rgba_of(t.mist))
                        .child(tr!("Search commands\u{2026}")),
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
                    d.child(div().w(px(1.)).h(px(22.)).bg(rgba_of(t.text)))
                })
                .child(div().child(SharedString::from(after)))
                .into_any_element()
        };
        let search = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(14.))
            .px(px(20.))
            .py(px(18.))
            .border_b_1()
            .border_color(rgba_of(t.border))
            .child(
                gpui::svg()
                    .path("icons/search.svg")
                    .size(px(20.))
                    .flex_none()
                    .text_color(rgba_of(t.mist)),
            )
            .child(
                div()
                    .id("launcher-input")
                    .role(Role::TextInput)
                    .aria_label(tr!("Search commands"))
                    .aria_placeholder(tr!("Search commands"))
                    .aria_value(SharedString::from(self.input.text().to_owned()))
                    .flex_1()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(20.))
                    .child(body),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(8.))
                    .py(px(3.))
                    .rounded(px(scale::RADIUS_XS))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.raised))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .mono()
                    .child("esc"),
            );
        let list = self.list(&t, cx);
        let footer = self.footer(&t);
        div()
            .id("launcher")
            .role(Role::Dialog)
            .aria_label(tr!("Command launcher"))
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            // The window is taller than the card: a click on the transparent part is a click
            // outside the launcher.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, window, cx| dismiss(window, cx, true)),
            )
            .font_family(UI_FONT)
            .font_features(ui_features())
            .text_color(rgba_of(t.text))
            .text_size(px(type_scale::UI))
            .child(
                div()
                    .w_full()
                    .max_h_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded(px(scale::RADIUS_XL))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(WINDOW_FILL))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(search)
                    .child(list)
                    .child(footer),
            )
    }
}
