//! The Studio window: sidebar plus the routed main area (library, editor placeholder,
//! settings). Studio mode is flat (DESIGN section 1): tone separates panels, no shadows.

use gpui::{
    actions, div, prelude::*, px, AnyElement, App, Context, Entity, FocusHandle, FontWeight,
    KeyBinding, Menu, MenuItem, Render, Role, SharedString, Subscription, Window,
};

use cm_i18n::{tr, tr_ctx, trn};

use crate::agent_status::{self, McpStatus};
use crate::backend::Backend;
use crate::nav;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::ai_view::AiPage;
use crate::ui::app::{self, AppHandle, AppModel, Command, Route};
use crate::ui::editor_view::EditorView;
use crate::ui::library_view::LibraryView;
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::permissions_view::PermissionsScreen;
use crate::ui::publish_sheet::PublishPage;
use crate::ui::published_view::PublishedView;
use crate::ui::recording::details::RecordingDetails;
use crate::ui::recording_editor::RecordingEditor;
use crate::ui::settings_view::SettingsView;
use crate::ui::sidebar::{sidebar, SidebarProps};

/// The active Studio theme (light or dark), read by every Studio view.
#[derive(Clone, Copy)]
pub struct ThemeGlobal(pub Theme);

actions!(
    cm_desktop,
    [
        FocusSearch,
        ShowLibrary,
        ShowSettings,
        QuitApp,
        CloseWindow,
        FocusNext,
        FocusPrev,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        OpenSelected,
    ]
);

pub const SIDEBAR_WIDTH: f32 = scale::SIDEBAR_W;

/// Key bindings and the app menu. `secondary` is Cmd on macOS and Ctrl on Windows.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-f", FocusSearch, None),
        KeyBinding::new("secondary-1", ShowLibrary, None),
        KeyBinding::new("secondary-,", ShowSettings, None),
        KeyBinding::new("secondary-q", QuitApp, None),
        KeyBinding::new("secondary-w", CloseWindow, None),
        KeyBinding::new("tab", FocusNext, None),
        KeyBinding::new("shift-tab", FocusPrev, None),
        KeyBinding::new("left", SelectLeft, Some("LibraryList")),
        KeyBinding::new("right", SelectRight, Some("LibraryList")),
        KeyBinding::new("up", SelectUp, Some("LibraryList")),
        KeyBinding::new("down", SelectDown, Some("LibraryList")),
        KeyBinding::new("enter", OpenSelected, Some("LibraryList")),
    ]);
    cx.set_menus(app_menus());
}

/// The menu bar in the current UI language. Rebuilt by `app::apply_language` on a language switch.
pub fn app_menus() -> Vec<Menu> {
    vec![Menu::new("CaptureMann").items([
        MenuItem::action(tr_ctx!("menu", "Library"), ShowLibrary),
        MenuItem::action(tr_ctx!("menu", "Settings\u{2026}"), ShowSettings),
        MenuItem::separator(),
        MenuItem::action(tr!("Quit CaptureMann"), QuitApp),
    ])]
}

/// Registers the global action handlers that work with no window open (menu bar items).
pub fn install_app_actions(cx: &mut App) {
    cx.on_action(
        |_: &QuitApp, cx| match cx.try_global::<AppHandle>().map(|h| h.0.clone()) {
            Some(app) => app::quit(&app, cx),
            None => cx.quit(),
        },
    );
    cx.on_action(|_: &ShowLibrary, cx| {
        if let Some(app) = cx.try_global::<AppHandle>().map(|h| h.0.clone()) {
            app::handle_command(&app, Command::ShowLibrary, cx);
        }
    });
    cx.on_action(|_: &ShowSettings, cx| {
        if let Some(app) = cx.try_global::<AppHandle>().map(|h| h.0.clone()) {
            app::handle_command(&app, Command::ShowSettings, cx);
        }
    });
}

pub struct Studio {
    app: Entity<AppModel>,
    library: Entity<LibraryView>,
    settings: Entity<SettingsView>,
    permissions: Entity<PermissionsScreen>,
    editor: Entity<EditorView>,
    details: Entity<RecordingDetails>,
    rec_editor: Entity<RecordingEditor>,
    publish: Entity<PublishPage>,
    ai: Entity<AiPage>,
    published: Entity<PublishedView>,
    /// The kind the library view was last set for (All, Recordings or Screenshots).
    shown: Option<crate::library::Filter>,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl Studio {
    pub fn new(app: Entity<AppModel>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let library = cx.new(|cx| LibraryView::new(app.clone(), window, cx));
        let settings = cx.new(|cx| SettingsView::new(app.clone(), cx));
        let permissions = cx.new(|cx| PermissionsScreen::new(app.clone(), cx));
        let editor = cx.new(|cx| EditorView::new(app.clone(), cx));
        let details = cx.new(|cx| RecordingDetails::new(app.clone(), cx));
        let rec_editor = cx.new(|cx| RecordingEditor::new(app.clone(), cx));
        let publish = cx.new(|cx| PublishPage::new(app.clone(), cx));
        let ai = cx.new(|cx| AiPage::new(app.clone(), cx));
        // Loads at once, so the sidebar number is real before the page is opened.
        let published = cx.new(|cx| PublishedView::new(app.clone(), cx));
        let focus = cx.focus_handle();
        let app2 = app.clone();
        let app3 = app.clone();
        let subs = vec![
            cx.observe(&app, |_, _, cx| cx.notify()),
            window.observe_window_appearance(move |_, cx| app::apply_theme(&app2, cx)),
            // R1-6: when the window comes to the front, look at the bundle folders (throttled).
            cx.observe_window_activation(window, move |_, window, cx| {
                if window.is_window_active() {
                    app::request_rescan(&app3, false, cx);
                }
            }),
        ];
        // Land on the search box, the most common first action.
        let search = library.read(cx).search_focus(cx);
        window.focus(&search, cx);
        Studio {
            app,
            library,
            settings,
            permissions,
            editor,
            details,
            rec_editor,
            publish,
            ai,
            published,
            shown: None,
            focus,
            _subs: subs,
        }
    }

    /// The minimal title row of Published (the three library pages draw their own, with the search): the title and the real count
    /// (hidden while unknown). The capture buttons are the sidebar's dock now; search, chips and
    /// sort stay in the library toolbar below (the Library restyle is Phase 2).
    fn title_row(&self, t: &Theme, route: &Route, counts: &nav::NavCounts) -> AnyElement {
        let (_, title) = nav::page_heading(route);
        let count = nav::route_count(route, counts);
        div()
            .id("page-title")
            .w_full()
            .flex_none()
            .flex()
            .items_baseline()
            .gap(px(10.))
            .px(px(24.))
            .pt(px(18.))
            .child(
                div()
                    .text_size(px(type_scale::H2))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .children(count.map(|n| {
                div()
                    .id("page-count")
                    .aria_label(SharedString::from(trn!("{n} item", "{n} items", n)))
                    .text_size(px(type_scale::UI))
                    .text_color(rgba_of(t.mist))
                    .tnum()
                    .child(SharedString::from(n.to_string()))
            }))
            .into_any_element()
    }
}

/// MCP status from the embedded daemon (see `agent_status`).
pub fn agent_status_of(backend: &Backend) -> McpStatus {
    let activity = backend
        .daemon
        .as_ref()
        .map(|d| d.activity())
        .unwrap_or_default();
    agent_status::derive(
        backend.daemon.is_some(),
        backend.endpoint.is_some(),
        backend.ipc_warning.as_deref(),
        &activity,
    )
}

impl Render for Studio {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        let (route, counts, platform, shortcuts, library_route) = {
            let m = self.app.read(cx);
            (
                m.route.clone(),
                m.counts,
                m.platform,
                m.shortcuts.clone(),
                m.library_route.clone(),
            )
        };
        // A pending "open this settings page" request is taken before the sidebar is drawn, so the
        // highlighted section and the page agree.
        if route == Route::Settings {
            self.settings.update(cx, |s, cx| s.sync_request(cx));
        }
        let section = self.settings.read(cx).current_section(&route);
        let status = agent_status_of(&self.app.read(cx).backend);
        let sidebar = sidebar(
            &t,
            &SidebarProps {
                route: route.clone(),
                counts,
                platform,
                shortcuts,
                status,
                library_route,
                section,
            },
            &self.app,
            &self.settings,
        );

        if !matches!(route, Route::Editor(_)) && self.editor.read(cx).is_open() {
            self.editor.update(cx, |e, cx| e.close(cx));
        }
        if !matches!(route, Route::RecordingEditor(_)) && self.rec_editor.read(cx).is_open() {
            self.rec_editor.update(cx, |e, cx| e.close(cx));
        }
        if !matches!(route, Route::Publish(_)) && self.publish.read(cx).is_open() {
            self.publish.update(cx, |p, cx| p.close(cx));
        }
        let main: AnyElement = match &route {
            Route::Library | Route::Screenshots | Route::Recordings => {
                // The three library pages share the grid; entering one shows its kind.
                let filter = nav::library_filter(&route).unwrap_or(crate::library::Filter::All);
                if self.shown != Some(filter) {
                    self.shown = Some(filter);
                    self.library.update(cx, |l, cx| l.show_kind(filter, cx));
                }
                // The title row, the chips and the "no speech model" hint are the library view's own.
                self.library.clone().into_any_element()
            }
            Route::Recording(id) => {
                self.details.update(cx, |d, cx| d.set_capture(id, cx));
                self.details.clone().into_any_element()
            }
            Route::RecordingEditor(id) => {
                self.rec_editor.update(cx, |e, cx| e.set_capture(id, cx));
                self.rec_editor.clone().into_any_element()
            }
            Route::Publish(id) => {
                self.publish.update(cx, |p, cx| p.set_capture(id, cx));
                self.publish.clone().into_any_element()
            }
            Route::Settings | Route::Destinations | Route::Agents => {
                self.settings.clone().into_any_element()
            }
            Route::Ai => self.ai.clone().into_any_element(),
            Route::Permissions => self.permissions.clone().into_any_element(),
            Route::Editor(id) => {
                self.editor.update(cx, |e, cx| e.set_capture(id, cx));
                self.editor.clone().into_any_element()
            }
            Route::Published => self.published.clone().into_any_element(),
        };

        let app = self.app.clone();
        let library = self.library.clone();
        // The library pages and Published get a minimal title row; every other page has its own
        // title (settings pages, AI, Agents, the toolbars of the details page and the editors).
        let title = (nav::is_library_page(&route)
            && !matches!(
                route,
                Route::Library | Route::Screenshots | Route::Recordings
            ))
        .then(|| self.title_row(&t, &route, &counts));
        let app_for_search = app.clone();
        div()
            .id("studio")
            .role(Role::Application)
            .aria_label(tr!("CaptureMann Studio"))
            .track_focus(&self.focus)
            .on_action(cx.listener(|_, _: &FocusNext, window, cx| window.focus_next(cx)))
            .on_action(cx.listener(|_, _: &FocusPrev, window, cx| window.focus_prev(cx)))
            .on_action(move |_: &FocusSearch, window, cx| {
                // The search field belongs to the library: land on All captures unless a library
                // page is already showing.
                if !nav::is_library_page(&app_for_search.read(cx).route)
                    || app_for_search.read(cx).route == Route::Published
                {
                    app::set_route(&app_for_search, Route::Library, cx);
                }
                let f = library.read(cx).search_focus(cx);
                window.focus(&f, cx);
            })
            .on_action(cx.listener(|this, _: &ShowLibrary, _, cx| {
                app::set_route(&this.app, Route::Library, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowSettings, _, cx| {
                app::set_route(&this.app, Route::Settings, cx)
            }))
            .on_action(|_: &QuitApp, _, cx| {
                match cx.try_global::<AppHandle>().map(|h| h.0.clone()) {
                    Some(app) => app::quit(&app, cx),
                    None => cx.quit(),
                }
            })
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .size_full()
            .flex()
            .bg(rgba_of(t.ground))
            .text_color(rgba_of(t.text))
            .font_family(UI_FONT)
            .font_features(ui_features())
            .text_size(px(type_scale::UI))
            // Every route has a sidebar: the full app one, the 64 px rail on the full-window
            // pages, or the settings one.
            .child(sidebar)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .flex()
                    .flex_col()
                    .children(title)
                    .child(div().flex_1().min_h(px(0.)).child(main)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// I18N P2: the menu bar is built by one function (rebuilt on a language switch); in English
    /// it has the same items as before.
    #[test]
    fn the_menu_bar_has_the_app_menu() {
        let menus = app_menus();
        assert_eq!(menus.len(), 1);
        assert_eq!(menus[0].name.as_ref(), "CaptureMann");
        assert_eq!(menus[0].items.len(), 4);
    }
}
