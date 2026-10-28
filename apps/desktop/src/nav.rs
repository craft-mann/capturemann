//! Sidebar navigation model (OD-16, Oct 23, 2026): the items of the LIBRARY and INTELLIGENCE
//! groups plus Settings, the sidebar mode each route uses (app, rail, settings), the rail layout
//! and the capture dock with its keycap hints read from the shortcut map. Pure data so it is unit
//! tested without a display.

use cm_i18n::{tr, tr_ctx};

use crate::library::Filter;
use crate::shortcuts::{Platform, ShortcutAction, ShortcutMap};
use crate::ui::app::Route;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavItem {
    /// Every capture, newest first (`Route::Library`, the default landing page).
    AllCaptures,
    Recordings,
    /// Audio-only recordings (`Route::Audio`, VOC-1.AC10).
    Audio,
    Screenshots,
    Published,
    /// The speech-model manager (`Route::Ai`).
    AiModels,
    /// MCP status, clients and the CLI (`Route::Agents`).
    Agents,
    Settings,
}

/// Real counts shown beside nav items. `None` means unknown (library not loaded or
/// unavailable): no number is shown rather than a made-up one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NavCounts {
    pub screenshots: Option<usize>,
    pub recordings: Option<usize>,
    /// Published videos (the Published page loads the library itself).
    pub published: Option<usize>,
    /// Audio-only recordings.
    pub audio: Option<usize>,
}

impl NavItem {
    pub const ALL: [NavItem; 8] = [
        NavItem::AllCaptures,
        NavItem::Recordings,
        NavItem::Audio,
        NavItem::Screenshots,
        NavItem::Published,
        NavItem::AiModels,
        NavItem::Agents,
        NavItem::Settings,
    ];

    /// The LIBRARY group of the app sidebar.
    pub const LIBRARY: [NavItem; 5] = [
        NavItem::AllCaptures,
        NavItem::Recordings,
        NavItem::Audio,
        NavItem::Screenshots,
        NavItem::Published,
    ];

    /// The INTELLIGENCE group of the app sidebar.
    pub const INTELLIGENCE: [NavItem; 2] = [NavItem::AiModels, NavItem::Agents];

    pub fn label(self) -> String {
        match self {
            NavItem::AllCaptures => tr!("All captures"),
            NavItem::Recordings => tr_ctx!("navigation", "Recordings"),
            NavItem::Audio => tr_ctx!("navigation", "Audio"),
            NavItem::Screenshots => tr_ctx!("navigation", "Screenshots"),
            NavItem::Published => tr_ctx!("navigation", "Published"),
            NavItem::AiModels => tr!("AI models"),
            NavItem::Agents => tr!("Agents & CLI"),
            NavItem::Settings => tr_ctx!("navigation", "Settings"),
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            NavItem::AllCaptures => "nav-all",
            NavItem::Recordings => "nav-recordings",
            NavItem::Audio => "nav-audio",
            NavItem::Screenshots => "nav-screenshots",
            NavItem::Published => "nav-published",
            NavItem::AiModels => "nav-ai",
            NavItem::Agents => "nav-agents",
            NavItem::Settings => "nav-settings",
        }
    }

    /// The leading icon (`design/icons/icon-map.md`, "Studio sidebar").
    pub fn icon(self) -> &'static str {
        match self {
            NavItem::AllCaptures => "layout-grid",
            NavItem::Recordings => "film",
            NavItem::Audio => "audio-lines",
            NavItem::Screenshots => "image",
            NavItem::Published => "send",
            NavItem::AiModels => "sparkles",
            NavItem::Agents => "bot",
            NavItem::Settings => "settings",
        }
    }

    pub fn route(self) -> Route {
        match self {
            NavItem::AllCaptures => Route::Library,
            NavItem::Recordings => Route::Recordings,
            NavItem::Audio => Route::Audio,
            NavItem::Screenshots => Route::Screenshots,
            NavItem::Published => Route::Published,
            NavItem::AiModels => Route::Ai,
            NavItem::Agents => Route::Agents,
            NavItem::Settings => Route::Settings,
        }
    }

    /// The nav item a route belongs to (the boards: details and the recording editor under
    /// Recordings, the screenshot editor under Screenshots, the publish page under Published,
    /// the permissions setup screen under Settings).
    pub fn of_route(route: &Route) -> NavItem {
        match route {
            Route::Library => NavItem::AllCaptures,
            Route::Audio => NavItem::Audio,
            Route::Screenshots | Route::Editor(_) => NavItem::Screenshots,
            Route::Recordings | Route::Recording(_) | Route::RecordingEditor(_) => {
                NavItem::Recordings
            }
            Route::Published | Route::Publish(_) => NavItem::Published,
            Route::Ai => NavItem::AiModels,
            Route::Agents => NavItem::Agents,
            Route::Settings | Route::Destinations | Route::Permissions => NavItem::Settings,
        }
    }

    /// The right-aligned count: the real library counts, and the number of published videos.
    /// "All captures" shows none (the kinds are counted on their own rows).
    pub fn count(self, counts: &NavCounts) -> Option<usize> {
        match self {
            NavItem::Recordings => counts.recordings,
            NavItem::Audio => counts.audio,
            NavItem::Screenshots => counts.screenshots,
            NavItem::Published => counts.published,
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavEntry {
    pub item: NavItem,
    pub active: bool,
    pub count: Option<usize>,
}

fn entries_of(items: &[NavItem], route: &Route, counts: &NavCounts) -> Vec<NavEntry> {
    let current = NavItem::of_route(route);
    items
        .iter()
        .map(|&item| NavEntry {
            item,
            active: item == current,
            count: item.count(counts),
        })
        .collect()
}

/// Every item, in order, with the active one marked.
pub fn entries(route: &Route, counts: &NavCounts) -> Vec<NavEntry> {
    entries_of(&NavItem::ALL, route, counts)
}

/// The route the sidebar highlight follows: the page's own, except that the editor of an audio
/// recording belongs under Audio, not Recordings.
pub fn highlight_route(route: &Route, audio_open: bool) -> Route {
    match (route, audio_open) {
        (Route::RecordingEditor(_), true) => Route::Audio,
        (r, _) => r.clone(),
    }
}

/// The LIBRARY group.
pub fn library_entries(route: &Route, counts: &NavCounts) -> Vec<NavEntry> {
    entries_of(&NavItem::LIBRARY, route, counts)
}

/// The INTELLIGENCE group.
pub fn intelligence_entries(route: &Route, counts: &NavCounts) -> Vec<NavEntry> {
    entries_of(&NavItem::INTELLIGENCE, route, counts)
}

/// Which kind of captures a library route shows; `None` for every other route.
pub fn library_filter(route: &Route) -> Option<Filter> {
    match route {
        Route::Library => Some(Filter::All),
        Route::Recordings => Some(Filter::Recordings),
        Route::Audio => Some(Filter::Audio),
        Route::Screenshots => Some(Filter::Screenshots),
        _ => None,
    }
}

/// The count beside a library page's title: the kind's real count, "All captures" the sum of the
/// two kinds when both are known, Published its own. `None` while unknown (no placeholder).
pub fn route_count(route: &Route, counts: &NavCounts) -> Option<usize> {
    match route {
        // Audio joins the sum when its count is known (an older daemon has none: then 0).
        Route::Library => counts
            .recordings
            .zip(counts.screenshots)
            .map(|(r, s)| r + s + counts.audio.unwrap_or(0)),
        Route::Recordings => counts.recordings,
        Route::Audio => counts.audio,
        Route::Screenshots => counts.screenshots,
        Route::Published => counts.published,
        _ => None,
    }
}

/// The pages "Back to library" can return to: the four LIBRARY destinations themselves (not the
/// details, editor or publish pages that belong to them).
pub fn is_library_page(route: &Route) -> bool {
    matches!(
        route,
        Route::Library | Route::Screenshots | Route::Recordings | Route::Audio | Route::Published
    )
}

// ---------------------------------------------------------------------------------------
// Sidebar modes
// ---------------------------------------------------------------------------------------

/// The three faces of the one sidebar component (`design/redesign/CLAUDE.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarMode {
    /// 248 px: brand, capture dock, LIBRARY, INTELLIGENCE, Settings, MCP card.
    App,
    /// 64 px, icons only: the full-window pages (details, editors, publish).
    Rail,
    /// 248 px: "Back to library" and the 8 settings sections.
    Settings,
}

impl SidebarMode {
    pub fn of_route(route: &Route) -> SidebarMode {
        match route {
            Route::Recording(_)
            | Route::RecordingEditor(_)
            | Route::Editor(_)
            | Route::Publish(_) => SidebarMode::Rail,
            Route::Settings | Route::Destinations => SidebarMode::Settings,
            // The setup screen keeps the app sidebar (dimmed by the shell).
            Route::Library
            | Route::Screenshots
            | Route::Recordings
            | Route::Audio
            | Route::Published
            | Route::Ai
            | Route::Agents
            | Route::Permissions => SidebarMode::App,
        }
    }

    pub fn width(self) -> f32 {
        match self {
            SidebarMode::Rail => crate::theme::scale::RAIL_W,
            _ => crate::theme::scale::SIDEBAR_W,
        }
    }
}

/// One slot of the rail, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailEntry {
    Brand,
    Record,
    Region,
    /// The mic button: Record audio (VOC-1, Option-Shift-V).
    RecordAudio,
    Divider,
    Nav(NavItem),
    Spacer,
    Mcp,
}

/// The rail from the top: brand mark, Record, Region, a divider, the seven destinations, then
/// Settings and the MCP status button at the bottom.
pub fn rail_layout() -> Vec<RailEntry> {
    vec![
        RailEntry::Brand,
        RailEntry::Record,
        RailEntry::Region,
        RailEntry::RecordAudio,
        RailEntry::Divider,
        RailEntry::Nav(NavItem::AllCaptures),
        RailEntry::Nav(NavItem::Recordings),
        RailEntry::Nav(NavItem::Audio),
        RailEntry::Nav(NavItem::Screenshots),
        RailEntry::Nav(NavItem::Published),
        RailEntry::Nav(NavItem::AiModels),
        RailEntry::Nav(NavItem::Agents),
        RailEntry::Spacer,
        RailEntry::Nav(NavItem::Settings),
        RailEntry::Mcp,
    ]
}

// ---------------------------------------------------------------------------------------
// Capture dock
// ---------------------------------------------------------------------------------------

/// What the capture dock (and the rail's two capture buttons) start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockAction {
    /// Opens the Record sheet.
    Record,
    Region,
    Window,
    Display,
}

impl DockAction {
    pub const ALL: [DockAction; 4] = [
        DockAction::Record,
        DockAction::Region,
        DockAction::Window,
        DockAction::Display,
    ];

    /// The three tiles under the Record button.
    pub const TILES: [DockAction; 3] =
        [DockAction::Region, DockAction::Window, DockAction::Display];

    pub fn label(self) -> String {
        match self {
            DockAction::Record => tr_ctx!("capture dock", "Record\u{2026}"),
            DockAction::Region => tr_ctx!("capture dock", "Region"),
            DockAction::Window => tr_ctx!("capture dock", "Window"),
            DockAction::Display => tr_ctx!("capture dock", "Display"),
        }
    }

    /// The icon (`design/icons/icon-map.md`).
    pub fn icon(self) -> &'static str {
        match self {
            DockAction::Record => "cm-record",
            DockAction::Region => "cm-capture-region",
            DockAction::Window => "app-window",
            DockAction::Display => "monitor",
        }
    }

    /// Element ids are the ones of the header buttons this dock replaced.
    pub fn id(self) -> &'static str {
        match self {
            DockAction::Record => "rec-new",
            DockAction::Region => "cap-region",
            DockAction::Window => "cap-window",
            DockAction::Display => "cap-display",
        }
    }

    /// The global shortcut whose chord is the hint.
    pub fn shortcut(self) -> ShortcutAction {
        match self {
            DockAction::Record => ShortcutAction::ToggleRecording,
            DockAction::Region => ShortcutAction::CaptureRegion,
            DockAction::Window => ShortcutAction::CaptureWindow,
            DockAction::Display => ShortcutAction::CaptureDisplay,
        }
    }

    /// The accessible name: the words of the removed header buttons plus the live chord.
    pub fn aria(self, keys: &[String]) -> String {
        let chord = keys.join("");
        match self {
            DockAction::Record => tr!("Record, start or stop shortcut {chord}", chord = chord),
            DockAction::Region => tr!("Capture region, shortcut {chord}", chord = chord),
            DockAction::Window => tr!("Capture window, shortcut {chord}", chord = chord),
            DockAction::Display => tr!("Capture display, shortcut {chord}", chord = chord),
        }
    }
}

/// One dock button with the keycaps of its current chord.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockEntry {
    pub action: DockAction,
    /// One string per key (`⌥`, `⇧`, `R`; `Ctrl`, `Alt`, `Shift`, `3` on Windows).
    pub keys: Vec<String>,
}

/// The dock's four buttons with the live bindings: a rebound key shows at once.
pub fn dock(map: &ShortcutMap, platform: Platform) -> Vec<DockEntry> {
    DockAction::ALL
        .iter()
        .map(|&action| DockEntry {
            action,
            keys: map.get(action.shortcut()).keycaps(platform),
        })
        .collect()
}

/// Keycaps shown on a 65 px tile: the whole chord when it is three keys or fewer (the macOS
/// default), else only the last key (the Windows chords have four); the tooltip and the accessible
/// name always carry the full chord.
pub fn tile_keys(keys: &[String]) -> Vec<String> {
    if keys.len() <= 3 {
        keys.to_vec()
    } else {
        keys.last().cloned().into_iter().collect()
    }
}

/// Element id of the mic button (the dock and the rail).
pub const RECORD_AUDIO_ID: &str = "rec-audio";

/// The mic button's label.
pub fn record_audio_label() -> String {
    tr_ctx!("capture dock", "Record audio")
}

/// The chord hint of "Record audio" (VOC-1: Option-Shift-V on macOS). The shortcut map has no
/// entry for it yet (the shortcuts owner adds `RecordAudio`); until then the hint is the default
/// chord, derived from Record's modifiers so a platform keeps its own style.
pub fn record_audio_keys(map: &ShortcutMap, platform: Platform) -> Vec<String> {
    let mut keys = map.get(ShortcutAction::ToggleRecording).keycaps(platform);
    if let Some(last) = keys.last_mut() {
        *last = "V".to_owned();
    }
    keys
}

/// The accessible name of the mic button: the words and the live chord.
pub fn record_audio_aria(keys: &[String]) -> String {
    tr!("Record audio, shortcut {chord}", chord = keys.join(""))
}

/// Crumb and title of a route. The page headers are gone (OD-16); the title feeds the library
/// pages' title row and the Published page.
pub fn page_heading(route: &Route) -> (String, String) {
    match route {
        Route::Library => (tr_ctx!("breadcrumb", "Library /"), tr!("All captures")),
        Route::Screenshots => (
            tr_ctx!("breadcrumb", "Library /"),
            tr_ctx!("navigation", "Screenshots"),
        ),
        Route::Editor(_) => (
            tr_ctx!("breadcrumb", "Screenshots /"),
            tr_ctx!("page title", "Screenshot"),
        ),
        Route::Recordings => (
            tr_ctx!("breadcrumb", "Library /"),
            tr_ctx!("navigation", "Recordings"),
        ),
        Route::Audio => (
            tr_ctx!("breadcrumb", "Library /"),
            tr_ctx!("navigation", "Audio"),
        ),
        Route::Recording(_) => (
            tr_ctx!("breadcrumb", "Recordings /"),
            tr_ctx!("page title", "Recording"),
        ),
        Route::RecordingEditor(_) => (
            tr_ctx!("breadcrumb", "Recordings /"),
            tr_ctx!("page title", "Edit"),
        ),
        Route::Publish(_) => (
            tr_ctx!("breadcrumb", "Recordings /"),
            tr_ctx!("page title", "Publish"),
        ),
        Route::Published => (
            tr_ctx!("breadcrumb", "Library /"),
            tr_ctx!("navigation", "Published"),
        ),
        Route::Destinations => (
            tr_ctx!("breadcrumb", "Settings /"),
            tr_ctx!("page title", "Destinations"),
        ),
        Route::Agents => (tr_ctx!("breadcrumb", "Intelligence /"), tr!("Agents & CLI")),
        Route::Ai => (tr_ctx!("breadcrumb", "Intelligence /"), tr!("AI models")),
        Route::Settings => (
            tr_ctx!("breadcrumb", "Studio /"),
            tr_ctx!("navigation", "Settings"),
        ),
        Route::Permissions => (
            tr_ctx!("breadcrumb", "Setup /"),
            tr_ctx!("page title", "Permissions"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_routes() -> Vec<Route> {
        vec![
            Route::Library,
            Route::Screenshots,
            Route::Recordings,
            Route::Audio,
            Route::Published,
            Route::Destinations,
            Route::Agents,
            Route::Editor("cap_1".into()),
            Route::Recording("rec_1".into()),
            Route::RecordingEditor("rec_1".into()),
            Route::Publish("rec_1".into()),
            Route::Settings,
            Route::Ai,
            Route::Permissions,
        ]
    }

    #[test]
    fn eight_items_in_order_in_two_groups() {
        let labels: Vec<_> = NavItem::ALL.iter().map(|i| i.label()).collect();
        assert_eq!(
            labels,
            [
                "All captures",
                "Recordings",
                "Audio",
                "Screenshots",
                "Published",
                "AI models",
                "Agents & CLI",
                "Settings"
            ]
        );
        let library: Vec<_> = NavItem::LIBRARY.iter().map(|i| i.label()).collect();
        assert_eq!(
            library,
            [
                "All captures",
                "Recordings",
                "Audio",
                "Screenshots",
                "Published"
            ]
        );
        let intelligence: Vec<_> = NavItem::INTELLIGENCE.iter().map(|i| i.label()).collect();
        assert_eq!(intelligence, ["AI models", "Agents & CLI"]);
        // No AI & Agents menu or tabs any more (OD-16 supersedes OD-15).
        assert!(!labels
            .iter()
            .any(|l| l == "AI & Agents" || l == "Destinations"));
        let ids: std::collections::HashSet<_> = NavItem::ALL.iter().map(|i| i.id()).collect();
        assert_eq!(ids.len(), NavItem::ALL.len());
    }

    #[test]
    fn exactly_one_active_and_route_round_trips() {
        for item in NavItem::ALL {
            let route = item.route();
            let e = entries(&route, &NavCounts::default());
            assert_eq!(e.iter().filter(|x| x.active).count(), 1);
            assert!(e.iter().any(|x| x.item == item && x.active));
            assert_eq!(NavItem::of_route(&route), item);
        }
        // Every route highlights exactly one item, and the groups agree with the full list.
        for route in all_routes() {
            let counts = NavCounts::default();
            assert_eq!(
                entries(&route, &counts).iter().filter(|x| x.active).count(),
                1,
                "{route:?}"
            );
            let grouped = library_entries(&route, &counts)
                .into_iter()
                .chain(intelligence_entries(&route, &counts))
                .filter(|x| x.active)
                .count();
            let settings = usize::from(NavItem::of_route(&route) == NavItem::Settings);
            assert_eq!(grouped + settings, 1, "{route:?}");
        }
    }

    /// The highlight follows the boards.
    #[test]
    fn highlight_follows_the_boards() {
        use NavItem as N;
        let of = |r: Route| NavItem::of_route(&r);
        assert_eq!(of(Route::Library), N::AllCaptures);
        assert_eq!(of(Route::Recording("rec_1".into())), N::Recordings);
        assert_eq!(of(Route::RecordingEditor("rec_1".into())), N::Recordings);
        assert_eq!(of(Route::Editor("cap_1".into())), N::Screenshots);
        assert_eq!(of(Route::Screenshots), N::Screenshots);
        assert_eq!(of(Route::Ai), N::AiModels);
        assert_eq!(of(Route::Agents), N::Agents);
        assert_eq!(of(Route::Permissions), N::Settings);
        assert_eq!(of(Route::Destinations), N::Settings);
    }

    /// The editor of an audio recording highlights Audio; a screen recording's editor keeps
    /// Recordings (seen on Oct 28, 2026: audio recordings lit up Recordings).
    #[test]
    fn an_audio_recordings_editor_highlights_audio() {
        let editor = Route::RecordingEditor("rec_1".into());
        let counts = NavCounts::default();
        let active = |route: &Route| {
            library_entries(route, &counts)
                .into_iter()
                .filter(|e| e.active)
                .map(|e| e.item)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            active(&highlight_route(&editor, true)),
            vec![NavItem::Audio]
        );
        assert_eq!(
            active(&highlight_route(&editor, false)),
            vec![NavItem::Recordings]
        );
        // Other pages are not touched by the flag.
        assert_eq!(highlight_route(&Route::Ai, true), Route::Ai);
        assert_eq!(highlight_route(&Route::Recordings, true), Route::Recordings);
    }

    /// PUB-2 and the board: the publish page highlights Published (it used to say Recordings).
    #[test]
    fn the_publish_page_highlights_published() {
        let r = Route::Publish("rec_1".into());
        assert_eq!(NavItem::of_route(&r), NavItem::Published);
        assert_eq!(page_heading(&r).1, "Publish");
        let active: Vec<_> = entries(&r, &NavCounts::default())
            .into_iter()
            .filter(|e| e.active)
            .map(|e| e.item)
            .collect();
        assert_eq!(active, [NavItem::Published]);
    }

    #[test]
    fn counts_are_real_or_absent() {
        let none = NavCounts::default();
        assert_eq!(NavItem::Screenshots.count(&none), None);
        assert_eq!(NavItem::Recordings.count(&none), None);
        let c = NavCounts {
            screenshots: Some(138),
            recordings: Some(0),
            published: Some(3),
            audio: Some(5),
        };
        assert_eq!(NavItem::Screenshots.count(&c), Some(138));
        assert_eq!(NavItem::Recordings.count(&c), Some(0));
        assert_eq!(NavItem::Audio.count(&c), Some(5));
        assert_eq!(NavItem::Audio.count(&none), None);
        assert_eq!(
            NavItem::Published.count(&none),
            None,
            "unknown until loaded"
        );
        assert_eq!(NavItem::Published.count(&c), Some(3));
        assert_eq!(NavItem::AllCaptures.count(&c), None);
        assert_eq!(NavItem::AiModels.count(&c), None);
        assert_eq!(NavItem::Agents.count(&c), None);
        assert_eq!(NavItem::Settings.count(&c), None);
        // Unknown counts produce no number on any row.
        assert!(entries(&Route::Library, &none)
            .iter()
            .all(|e| e.count.is_none()));
    }

    /// Every item and dock button has a label and an icon from the registry.
    #[test]
    fn every_item_has_a_registered_icon() {
        let registered = |name: &str| {
            let path = format!("icons/{name}.svg");
            crate::ui::paint::ASSETS.iter().any(|(p, _)| *p == path)
        };
        for item in NavItem::ALL {
            assert!(!item.label().is_empty());
            assert!(registered(item.icon()), "{}", item.icon());
        }
        for a in DockAction::ALL {
            assert!(registered(a.icon()), "{}", a.icon());
        }
    }

    /// All captures is the default landing page and shows every kind; Recordings and Screenshots
    /// are their own routes with their own filter.
    #[test]
    fn all_captures_and_screenshots_have_their_own_filters() {
        assert_eq!(library_filter(&Route::Library), Some(Filter::All));
        assert_eq!(library_filter(&Route::Recordings), Some(Filter::Recordings));
        assert_eq!(
            library_filter(&Route::Screenshots),
            Some(Filter::Screenshots)
        );
        assert_eq!(library_filter(&Route::Published), None);
        assert_eq!(library_filter(&Route::Editor("cap_1".into())), None);
        assert_ne!(Route::Library, Route::Screenshots);
        assert_eq!(page_heading(&Route::Library).1, "All captures");
        assert_eq!(page_heading(&Route::Screenshots).1, "Screenshots");
    }

    #[test]
    fn title_counts_are_real_or_absent() {
        let none = NavCounts::default();
        for r in [
            Route::Library,
            Route::Recordings,
            Route::Audio,
            Route::Screenshots,
            Route::Published,
        ] {
            assert_eq!(route_count(&r, &none), None);
        }
        let c = NavCounts {
            screenshots: Some(11),
            recordings: Some(24),
            published: Some(2),
            audio: Some(6),
        };
        assert_eq!(route_count(&Route::Library, &c), Some(41));
        assert_eq!(route_count(&Route::Audio, &c), Some(6));
        assert_eq!(route_count(&Route::Recordings, &c), Some(24));
        assert_eq!(route_count(&Route::Screenshots, &c), Some(11));
        assert_eq!(route_count(&Route::Published, &c), Some(2));
        // One kind unknown: no made-up total.
        let half = NavCounts {
            recordings: Some(3),
            ..NavCounts::default()
        };
        assert_eq!(route_count(&Route::Library, &half), None);
        assert_eq!(route_count(&Route::Ai, &c), None);
    }

    /// "Back to library" returns to a library page, never to a details or editor page.
    #[test]
    fn only_the_library_destinations_are_library_pages() {
        for r in [
            Route::Library,
            Route::Screenshots,
            Route::Recordings,
            Route::Audio,
            Route::Published,
        ] {
            assert!(is_library_page(&r), "{r:?}");
            assert!(NavItem::LIBRARY.contains(&NavItem::of_route(&r)));
        }
        for r in [
            Route::Recording("rec_1".into()),
            Route::Editor("cap_1".into()),
            Route::Publish("rec_1".into()),
            Route::Ai,
            Route::Agents,
            Route::Settings,
            Route::Destinations,
            Route::Permissions,
        ] {
            assert!(!is_library_page(&r), "{r:?}");
        }
    }

    #[test]
    fn mode_per_route() {
        use SidebarMode as M;
        for (route, mode) in [
            (Route::Library, M::App),
            (Route::Screenshots, M::App),
            (Route::Recordings, M::App),
            (Route::Audio, M::App),
            (Route::Published, M::App),
            (Route::Ai, M::App),
            (Route::Agents, M::App),
            (Route::Permissions, M::App),
            (Route::Recording("rec_1".into()), M::Rail),
            (Route::RecordingEditor("rec_1".into()), M::Rail),
            (Route::Editor("cap_1".into()), M::Rail),
            (Route::Publish("rec_1".into()), M::Rail),
            (Route::Settings, M::Settings),
            (Route::Destinations, M::Settings),
        ] {
            assert_eq!(M::of_route(&route), mode, "{route:?}");
        }
        for route in all_routes() {
            let w = M::of_route(&route).width();
            assert!(w == 248. || w == 64.);
        }
        assert_eq!(M::App.width(), 248.);
        assert_eq!(M::Settings.width(), 248.);
        assert_eq!(M::Rail.width(), 64.);
    }

    #[test]
    fn rail_items_and_their_order() {
        let rail = rail_layout();
        assert_eq!(rail.first(), Some(&RailEntry::Brand));
        assert_eq!(rail.last(), Some(&RailEntry::Mcp));
        let nav: Vec<_> = rail
            .iter()
            .filter_map(|e| match e {
                RailEntry::Nav(i) => Some(*i),
                _ => None,
            })
            .collect();
        assert_eq!(
            nav,
            [
                NavItem::AllCaptures,
                NavItem::Recordings,
                NavItem::Audio,
                NavItem::Screenshots,
                NavItem::Published,
                NavItem::AiModels,
                NavItem::Agents,
                NavItem::Settings,
            ]
        );
        let pos = |e: RailEntry| rail.iter().position(|x| *x == e).unwrap();
        assert!(pos(RailEntry::Record) < pos(RailEntry::Region));
        // The mic button sits with the capture buttons, above the divider (board UiSidebar).
        assert!(pos(RailEntry::Region) < pos(RailEntry::RecordAudio));
        assert!(pos(RailEntry::RecordAudio) < pos(RailEntry::Divider));
        assert!(pos(RailEntry::Divider) < pos(RailEntry::Nav(NavItem::AllCaptures)));
        assert!(pos(RailEntry::Nav(NavItem::Agents)) < pos(RailEntry::Spacer));
        // Settings and the MCP button are pinned below the spacer.
        assert!(pos(RailEntry::Spacer) < pos(RailEntry::Nav(NavItem::Settings)));
        assert!(pos(RailEntry::Nav(NavItem::Settings)) < pos(RailEntry::Mcp));
    }

    #[test]
    fn dock_hints_follow_the_shortcut_map() {
        let keys = |m: &ShortcutMap, p| -> Vec<(DockAction, String)> {
            dock(m, p)
                .into_iter()
                .map(|e| (e.action, e.keys.join("")))
                .collect()
        };
        let mac = ShortcutMap::defaults(Platform::Mac);
        assert_eq!(
            keys(&mac, Platform::Mac),
            [
                (DockAction::Record, "\u{2325}\u{21E7}R".to_owned()),
                (DockAction::Region, "\u{2325}\u{21E7}1".to_owned()),
                (DockAction::Window, "\u{2325}\u{21E7}2".to_owned()),
                (DockAction::Display, "\u{2325}\u{21E7}3".to_owned()),
            ]
        );
        // The Windows defaults differ.
        let win = ShortcutMap::defaults(Platform::Windows);
        let win_keys = dock(&win, Platform::Windows);
        assert_eq!(win_keys[1].keys, ["Ctrl", "Alt", "Shift", "1"]);
        assert_eq!(win_keys[0].keys, ["Ctrl", "Alt", "Shift", "R"]);
        // A rebound key shows at once.
        let chord = crate::shortcuts::Chord::parse("alt+shift+9").unwrap();
        let rebound = mac.with(ShortcutAction::CaptureRegion, chord);
        let region = &dock(&rebound, Platform::Mac)[1];
        assert_eq!(region.action, DockAction::Region);
        assert_eq!(region.keys.join(""), "\u{2325}\u{21E7}9");
        let rec_chord = crate::shortcuts::Chord::parse("ctrl+f5").unwrap();
        let rebound = mac.with(ShortcutAction::ToggleRecording, rec_chord);
        assert_eq!(dock(&rebound, Platform::Mac)[0].keys, ["\u{2303}", "F5"]);
    }

    #[test]
    fn dock_buttons_keep_their_ids_and_names() {
        let mac = ShortcutMap::defaults(Platform::Mac);
        let d = dock(&mac, Platform::Mac);
        assert_eq!(
            d.iter().map(|e| e.action.id()).collect::<Vec<_>>(),
            ["rec-new", "cap-region", "cap-window", "cap-display"]
        );
        assert_eq!(
            d[0].action.aria(&d[0].keys),
            "Record, start or stop shortcut \u{2325}\u{21E7}R"
        );
        assert_eq!(
            d[1].action.aria(&d[1].keys),
            "Capture region, shortcut \u{2325}\u{21E7}1"
        );
        assert_eq!(DockAction::Record.label(), "Record\u{2026}");
        assert_eq!(
            DockAction::TILES.map(DockAction::label),
            ["Region", "Window", "Display"]
        );
    }

    #[test]
    fn tiles_show_the_whole_chord_when_it_fits() {
        let mac = ShortcutMap::defaults(Platform::Mac);
        let region = &dock(&mac, Platform::Mac)[1];
        assert_eq!(tile_keys(&region.keys), region.keys);
        let win = ShortcutMap::defaults(Platform::Windows);
        let region = &dock(&win, Platform::Windows)[1];
        assert_eq!(tile_keys(&region.keys), ["1"]);
    }

    /// LIB-1: a recording's details card belongs to the Recordings section.
    #[test]
    fn recording_details_belong_to_recordings() {
        assert_eq!(
            NavItem::of_route(&Route::Recording("rec_1".into())),
            NavItem::Recordings
        );
        assert_eq!(
            page_heading(&Route::Recording("rec_1".into())).1,
            "Recording"
        );
    }

    #[test]
    fn editor_belongs_to_screenshots() {
        assert_eq!(
            NavItem::of_route(&Route::Editor("cap_1".into())),
            NavItem::Screenshots
        );
    }

    /// OD-16: AI models and Agents & CLI are two items of their own; Destinations is reached only
    /// through Settings.
    #[test]
    fn ai_and_agents_are_two_items_and_destinations_belong_to_settings() {
        assert_eq!(NavItem::AiModels.route(), Route::Ai);
        assert_eq!(NavItem::Agents.route(), Route::Agents);
        assert_ne!(
            NavItem::of_route(&Route::Ai),
            NavItem::of_route(&Route::Agents)
        );
        assert_eq!(NavItem::of_route(&Route::Destinations), NavItem::Settings);
        assert_eq!(page_heading(&Route::Ai).1, "AI models");
        assert_eq!(page_heading(&Route::Agents).1, "Agents & CLI");
    }

    /// VOC-1.AC10 and the board: the mic button says Record audio with the chord of Record and
    /// the key V; the Audio item sits between Recordings and Screenshots and has its own route.
    #[test]
    fn the_mic_button_and_the_audio_item() {
        let mac = ShortcutMap::defaults(Platform::Mac);
        assert_eq!(
            record_audio_keys(&mac, Platform::Mac),
            ["\u{2325}", "\u{21E7}", "V"]
        );
        let win = ShortcutMap::defaults(Platform::Windows);
        assert_eq!(
            record_audio_keys(&win, Platform::Windows).last().unwrap(),
            "V"
        );
        assert_eq!(record_audio_label(), "Record audio");
        assert_eq!(
            record_audio_aria(&record_audio_keys(&mac, Platform::Mac)),
            "Record audio, shortcut \u{2325}\u{21E7}V"
        );
        assert_eq!(NavItem::Audio.route(), Route::Audio);
        assert_eq!(library_filter(&Route::Audio), Some(Filter::Audio));
        assert_eq!(Filter::Audio.daemon_kind(), Some("audio"));
        assert_eq!(page_heading(&Route::Audio).1, "Audio");
        assert_eq!(NavItem::Audio.icon(), "audio-lines");
    }
}
