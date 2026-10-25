//! The window list inside the Record sheet when the kind is Window (board R20, "Record sheet >
//! Window"). Display-free: parsing `list_targets`, the search state, the "Last used" and "Front
//! to back" groups, the Privacy note, keyboard moves, which rows are on screen, and the texts.
//! `ui::recording::window_list` draws it. The ranking, filtering and navigation come from
//! `crate::window_picker` (shared with the screenshot picker, board R19).
//!
//! Decisions (board R20 review):
//! - Minimized windows are not listed (macOS does not enumerate them; a minimized one that does
//!   arrive is skipped).
//! - Apps excluded in Privacy are not recordable. Their windows are left out of the rows and one
//!   muted note at the end of the list names the apps ([`hidden_note`]).
//! - Sizes are the window's physical px, like the display chips of the same sheet.

use std::ops::Range;

use cm_i18n::{tr, trn};
use serde_json::Value;

use crate::window_picker::{
    navigate, visible, AppFilter, DisplayFilter, LastUsed, Layout, Nav, PickerDisplay, PickerWindow,
};

/// The setting that remembers the windows last recorded or captured (app-only; written by the
/// sheet when a recording starts with a chosen window and by the screenshot picker).
pub const KEY_LAST_USED: &str = "app.window_picker.last_used";

// ---- geometry (board R20, points) ------------------------------------------------------------

/// The search bar above the list card (field height 34 plus the gap to the card).
pub const SEARCH_BAR_H: f32 = 34.0;
/// Gap between the search bar and the list card.
pub const SEARCH_GAP: f32 = 10.0;
/// The scrolling area of the list: fixed, board `max-height: 312px`.
pub const LIST_VIEWPORT_H: f32 = 312.0;
/// A group header: padding 9 + 14 + 4.
pub const HEADER_H: f32 = 28.0;
/// A window row: padding 7 + 40 px thumbnail + 7.
pub const ROW_H: f32 = 54.0;
/// The muted Privacy note row.
pub const NOTE_H: f32 = 40.0;
/// The card around the list (1 px border on each side).
pub const LIST_CARD_BORDER: f32 = 2.0;
/// The whole block: search bar, gap, card.
pub const LIST_BLOCK_H: f32 = SEARCH_BAR_H + SEARCH_GAP + LIST_VIEWPORT_H + LIST_CARD_BORDER;
/// The "Include" chips row.
pub const INCLUDE_ROW_H: f32 = 34.0;

// ---- thumbnails ------------------------------------------------------------------------------

// ---- parsing ---------------------------------------------------------------------------------

/// Where the window listing stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum WindowsLoad {
    /// Not asked yet (the kind is not Window).
    #[default]
    Idle,
    Loading,
    Loaded,
    Failed(String),
}

fn size_of(bounds: &Value) -> Option<(u32, u32)> {
    let size = bounds.get("size")?;
    let (w, h) = match size.as_array() {
        Some(a) => (a.first()?.as_u64()?, a.get(1)?.as_u64()?),
        None => (size.get("width")?.as_u64()?, size.get("height")?.as_u64()?),
    };
    Some((u32::try_from(w).ok()?, u32::try_from(h).ok()?))
}

/// The windows of a `list_targets` reply, front to back, minimized ones skipped.
pub fn parse_windows(v: &Value) -> Vec<PickerWindow> {
    v.get("windows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|w| w.get("minimized").and_then(Value::as_bool) != Some(true))
        .filter_map(|w| {
            let (width, height) = size_of(w.get("bounds")?)?;
            Some(PickerWindow {
                id: u32::try_from(w.get("id")?.as_u64()?).ok()?,
                title: w
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                app: w
                    .get("app")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                width,
                height,
                display_id: w
                    .get("display_id")
                    .and_then(Value::as_u64)
                    .and_then(|d| u32::try_from(d).ok()),
            })
        })
        .collect()
}

/// The displays of a `list_targets` reply, with `is_builtin` (absent counts as external).
pub fn parse_picker_displays(v: &Value) -> Vec<PickerDisplay> {
    v.get("displays")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|d| {
            Some(PickerDisplay {
                id: u32::try_from(d.get("id")?.as_u64()?).ok()?,
                name: d.get("name")?.as_str()?.to_owned(),
                builtin: d.get("is_builtin").and_then(Value::as_bool) == Some(true),
            })
        })
        .collect()
}

// ---- Privacy exclusions ----------------------------------------------------------------------

fn alnum_lower(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Does the app display name `app` ("Google Chrome") belong to the bundle id `bundle_id`
/// ("com.google.Chrome")? A window only carries the app's display name, never its bundle id, so
/// this is a best-effort match, not an identity check. Both are folded to lowercase letters and
/// digits; they match when the name equals the last one, two or three dot-separated parts of the
/// id joined ("googlechrome" is "google" + "chrome"), or any single part ("1password" in
/// "com.1password.1password"). A name that does not follow its bundle id ("Visual Studio Code"
/// is `com.microsoft.VSCode`) is not matched: that window stays in the list, and the recording
/// still leaves the app out because the engine matches by bundle id.
pub fn app_matches_bundle(app: &str, bundle_id: &str) -> bool {
    let name = alnum_lower(app);
    if name.is_empty() {
        return false;
    }
    let parts: Vec<String> = bundle_id
        .split('.')
        .map(alnum_lower)
        .filter(|p| !p.is_empty())
        .collect();
    if parts.contains(&name) {
        return true;
    }
    (2..=3).any(|k| parts.len() >= k && parts[parts.len() - k..].concat() == name)
}

/// Is a window of `app` hidden by the Privacy exclusions?
pub fn is_excluded(app: &str, excluded_bundle_ids: &[String]) -> bool {
    excluded_bundle_ids
        .iter()
        .any(|b| app_matches_bundle(app, b))
}

/// The muted note at the end of the list, or `None` when no listed window is hidden. `apps` are
/// the display names, each once, in list order.
pub fn hidden_note(apps: &[String]) -> Option<String> {
    if apps.is_empty() {
        return None;
    }
    let names = apps.join(", ");
    Some(trn!(
        "{apps} is hidden by your Privacy settings.",
        "{apps} are hidden by your Privacy settings.",
        apps.len(),
        apps = names
    ))
}

// ---- the list --------------------------------------------------------------------------------

/// The two groups of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    LastUsed,
    FrontToBack,
}

/// One child of the scrolling list, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListItem {
    Header(Group),
    /// `window`: an index into [`WindowList::windows`].
    Row {
        window: usize,
        external: bool,
    },
    Note(String),
}

impl ListItem {
    pub fn height(&self) -> f32 {
        match self {
            ListItem::Header(_) => HEADER_H,
            ListItem::Row { .. } => ROW_H,
            ListItem::Note(_) => NOTE_H,
        }
    }
}

/// Keys the list reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKey {
    Up,
    Down,
    Home,
    End,
    Space,
    Enter,
}

/// What a key did beyond moving the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOutcome {
    Nothing,
    /// Choose this window.
    Select(u32),
    /// Enter on the window that is already chosen: start the recording.
    Start,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WindowList {
    pub load: WindowsLoad,
    /// Recordable windows, front to back (minimized and Privacy-hidden ones are not here).
    pub windows: Vec<PickerWindow>,
    pub displays: Vec<PickerDisplay>,
    /// App names hidden by Privacy that currently have windows, each once.
    pub hidden_apps: Vec<String>,
    pub query: String,
    pub last_used: LastUsed,
    /// The keyboard cursor: the id of the window the arrow keys stand on. Choosing happens with
    /// Space or Enter, so this can differ from the chosen window.
    pub cursor: Option<u32>,
}

impl WindowList {
    /// The listing arrived. Privacy-hidden apps are separated out; the cursor stays on its
    /// window when that is still listed.
    pub fn apply(&mut self, reply: &Value, excluded_bundle_ids: &[String]) {
        let all = parse_windows(reply);
        self.displays = parse_picker_displays(reply);
        self.hidden_apps.clear();
        let mut windows = Vec::with_capacity(all.len());
        for w in all {
            if is_excluded(&w.app, excluded_bundle_ids) {
                if !self.hidden_apps.contains(&w.app) {
                    self.hidden_apps.push(w.app.clone());
                }
            } else {
                windows.push(w);
            }
        }
        self.windows = windows;
        self.load = WindowsLoad::Loaded;
        self.settle_cursor();
    }

    pub fn set_failed(&mut self, message: &str) {
        self.windows.clear();
        self.hidden_apps.clear();
        self.cursor = None;
        self.load = WindowsLoad::Failed(message.trim().to_owned());
    }

    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_owned();
        self.settle_cursor();
    }

    pub fn is_searching(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// Indices into `windows` in display order (the picker's ranking).
    pub fn visible(&self) -> Vec<usize> {
        visible(
            &self.windows,
            &self.displays,
            &self.query,
            &AppFilter::All,
            DisplayFilter::All,
            &self.last_used,
        )
    }

    /// Is the first visible window the remembered one? Only with an empty search.
    fn has_last_used_group(&self, vis: &[usize]) -> bool {
        !self.is_searching()
            && vis.first().is_some_and(|&i| {
                self.last_used
                    .rank_of(&self.windows[i].app, &self.windows[i].title)
                    .is_some()
            })
    }

    fn is_external(&self, w: &PickerWindow) -> bool {
        w.display_id
            .is_some_and(|id| self.displays.iter().any(|d| d.id == id && !d.builtin))
    }

    /// The children of the scrolling list. A search shows one flat, ranked list without group
    /// headers. The Privacy note is last, and only while not searching or when it is the only
    /// thing to say.
    pub fn items(&self) -> Vec<ListItem> {
        let vis = self.visible();
        let last_used = self.has_last_used_group(&vis);
        let mut out = Vec::new();
        let row = |i: usize| ListItem::Row {
            window: i,
            external: self.is_external(&self.windows[i]),
        };
        if last_used {
            out.push(ListItem::Header(Group::LastUsed));
            out.push(row(vis[0]));
            if vis.len() > 1 {
                out.push(ListItem::Header(Group::FrontToBack));
                out.extend(vis[1..].iter().map(|&i| row(i)));
            }
        } else if self.is_searching() {
            out.extend(vis.iter().map(|&i| row(i)));
        } else if !vis.is_empty() {
            out.push(ListItem::Header(Group::FrontToBack));
            out.extend(vis.iter().map(|&i| row(i)));
        }
        if let Some(note) = hidden_note(&self.hidden_apps) {
            out.push(ListItem::Note(note));
        }
        out
    }

    /// What the list area says when there is no row: loading, a failure, nothing open, no match.
    pub fn empty_text(&self) -> Option<String> {
        match &self.load {
            WindowsLoad::Idle | WindowsLoad::Loading => {
                return Some(tr!("Looking for windows\u{2026}"))
            }
            WindowsLoad::Failed(m) if m.is_empty() => {
                return Some(tr!("The windows could not be listed."))
            }
            WindowsLoad::Failed(m) => return Some(m.clone()),
            WindowsLoad::Loaded => {}
        }
        if !self.visible().is_empty() {
            return None;
        }
        Some(if self.windows.is_empty() {
            tr!("No windows are open.")
        } else {
            tr!(
                "No window matches \u{201C}{query}\u{201D}.",
                query = self.query.trim()
            )
        })
    }

    /// The placeholder of the search box, with the number of recordable windows.
    pub fn placeholder(&self) -> String {
        let n = self.windows.len();
        trn!(
            "Search {n} window by title or app",
            "Search {n} windows by title or app",
            n,
            n = n
        )
    }

    pub fn window(&self, id: u32) -> Option<&PickerWindow> {
        self.windows.iter().find(|w| w.id == id)
    }

    /// Window ids in display order.
    fn visible_ids(&self) -> Vec<u32> {
        self.visible()
            .into_iter()
            .map(|i| self.windows[i].id)
            .collect()
    }

    /// After the list or the query changed: keep the cursor on its window if it is still shown,
    /// else drop it (the next arrow key starts at the top).
    fn settle_cursor(&mut self) {
        let vis = self.visible();
        self.cursor = self
            .cursor
            .filter(|&id| vis.iter().any(|&i| self.windows[i].id == id));
    }

    /// Puts the cursor on the chosen window (when the sheet opens or a pick comes back).
    pub fn cursor_to(&mut self, id: Option<u32>) {
        self.cursor = id.filter(|&id| self.visible_ids().contains(&id));
    }

    /// The position of the cursor in `items()`, for scrolling it into view.
    pub fn cursor_item_index(&self) -> Option<usize> {
        let id = self.cursor?;
        self.items().iter().position(
            |it| matches!(it, ListItem::Row { window, .. } if self.windows[*window].id == id),
        )
    }

    /// Handles a key. The cursor moves for arrows, Home and End; Space and Enter choose the
    /// window under the cursor, and Enter on the window that is already `chosen` starts the
    /// recording (so Enter still means "Start recording" once the choice is made).
    pub fn key(&mut self, key: ListKey, chosen: Option<u32>) -> KeyOutcome {
        let ids = self.visible_ids();
        let pos = self.cursor.and_then(|c| ids.iter().position(|&i| i == c));
        let nav = match key {
            ListKey::Up => Some(Nav::Up),
            ListKey::Down => Some(Nav::Down),
            ListKey::Home => Some(Nav::Home),
            ListKey::End => Some(Nav::End),
            ListKey::Space | ListKey::Enter => None,
        };
        if let Some(nav) = nav {
            // With no cursor, a first Down or Up starts at the chosen window if it is listed.
            let start = pos.or_else(|| chosen.and_then(|c| ids.iter().position(|&i| i == c)));
            let next = navigate(start, ids.len(), Layout::List, nav);
            let moved = match (pos, start, nav) {
                // Standing on nothing: arrows land on the chosen window first.
                (None, Some(s), Nav::Up | Nav::Down) => Some(s),
                _ => next,
            };
            self.cursor = moved.map(|p| ids[p]);
            return KeyOutcome::Nothing;
        }
        match (self.cursor, chosen) {
            (Some(c), Some(ch)) if c == ch => match key {
                ListKey::Enter => KeyOutcome::Start,
                _ => KeyOutcome::Nothing,
            },
            (Some(c), _) => KeyOutcome::Select(c),
            (None, Some(_)) if key == ListKey::Enter => KeyOutcome::Start,
            (None, _) => KeyOutcome::Nothing,
        }
    }

    /// The window the cursor should take after the visible list changed, by `keep_selection`.
    #[cfg(test)]
    fn kept(&self, old: Option<u32>) -> Option<usize> {
        crate::window_picker::keep_selection(old, &self.windows, &self.visible())
    }

    /// Ids of the windows whose rows are in `range` of [`items`](Self::items): the ones that
    /// need a thumbnail.
    pub fn visible_window_ids(&self, range: Range<usize>) -> Vec<u32> {
        self.items()
            .iter()
            .enumerate()
            .filter(|(i, _)| range.contains(i))
            .filter_map(|(_, it)| match it {
                ListItem::Row { window, .. } => Some(self.windows[*window].id),
                _ => None,
            })
            .collect()
    }

    /// Records the chosen window in the Last used memory and returns the value to store under
    /// [`KEY_LAST_USED`]. `None` when the window is not in the list.
    pub fn remember(&mut self, id: u32) -> Option<Value> {
        let w = self.window(id)?.clone();
        self.last_used.touch(&w.app, &w.title);
        Some(self.last_used.to_value())
    }
}

/// Which children of a list are on screen: `heights` are the item heights top to bottom, `offset`
/// the distance scrolled (>= 0) and `viewport` the visible height. The range includes every item
/// that overlaps the viewport by at least a pixel, extended by `overscan` items on each side.
pub fn visible_range(heights: &[f32], offset: f32, viewport: f32, overscan: usize) -> Range<usize> {
    if heights.is_empty() || viewport <= 0.0 {
        return 0..0;
    }
    let (top, bottom) = (offset.max(0.0), offset.max(0.0) + viewport);
    let mut y = 0.0;
    let (mut first, mut last) = (None, None);
    for (i, h) in heights.iter().enumerate() {
        let (a, b) = (y, y + h);
        if b > top + 0.5 && a < bottom - 0.5 {
            first.get_or_insert(i);
            last = Some(i);
        }
        y = b;
    }
    match (first, last) {
        (Some(f), Some(l)) => f.saturating_sub(overscan)..(l + 1 + overscan).min(heights.len()),
        _ => 0..0,
    }
}

/// The scroll offset that brings item `index` fully into view with the least movement.
pub fn offset_to_reveal(heights: &[f32], index: usize, offset: f32, viewport: f32) -> f32 {
    let top: f32 = heights.iter().take(index).sum();
    let bottom = top + heights.get(index).copied().unwrap_or(0.0);
    if top < offset {
        top
    } else if bottom > offset + viewport {
        bottom - viewport
    } else {
        offset
    }
}

// ---- row details -----------------------------------------------------------------------------

/// The letter and palette slot of the app tile (board: a small coloured square with the app's
/// initial). Stable per app name.
pub fn app_badge(app: &str) -> (char, usize) {
    let letter = app
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().next().unwrap_or(c))
        .unwrap_or('?');
    // FNV-1a: stable across runs and platforms (no randomised hasher).
    let mut h: u32 = 0x811C_9DC5;
    for b in app.to_lowercase().bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    (letter, (h % BADGE_PALETTE.len() as u32) as usize)
}

/// Pastels from the board, dark text on each.
pub const BADGE_PALETTE: [u32; 6] = [0x8FB0FF, 0x7FB6E8, 0xC9C9CF, 0xD7A6E0, 0x9EC7F0, 0xF2C98A];

/// The text at the right of a row: "1680 × 1050", or "1440 × 900 · External".
pub fn size_text(w: &PickerWindow, external: bool) -> String {
    let size = crate::window_picker::format_size(w.width, w.height);
    if external {
        tr!("{size} \u{00B7} External", size = size)
    } else {
        size
    }
}

/// The spoken label of a row.
pub fn row_label(w: &PickerWindow, external: bool) -> String {
    let size = crate::window_picker::format_size(w.width, w.height);
    if external {
        tr!(
            "{title}, {app}, {size}, external display",
            title = w.title,
            app = w.app,
            size = size
        )
    } else {
        tr!(
            "{title}, {app}, {size}",
            title = w.title,
            app = w.app,
            size = size
        )
    }
}

/// The footer line for the chosen window: "Docs: Q4 launch plan · Google Chrome". An untitled
/// window shows its app only. `None` when nothing is chosen or the window is not listed.
pub fn footer_text(chosen: Option<u32>, list: &WindowList) -> Option<String> {
    let w = list.window(chosen?)?;
    Some(if w.title.trim().is_empty() {
        w.app.clone()
    } else if w.app.trim().is_empty() {
        w.title.clone()
    } else {
        tr!("{title} \u{00B7} {app}", title = w.title, app = w.app)
    })
}

/// The title shown for a row: an untitled window shows its app name.
pub fn row_title(w: &PickerWindow) -> String {
    if w.title.trim().is_empty() {
        w.app.clone()
    } else {
        w.title.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn reply() -> Value {
        let win = |id: u64, title: &str, app: &str, w: u64, h: u64, d: u64, min: bool| {
            json!({"id": id, "title": title, "app": app, "pid": 1,
                   "bounds": {"origin": {"x": 0, "y": 0}, "size": [w, h]},
                   "scale": 2.0, "display_id": d, "minimized": min})
        };
        json!({
            "displays": [
                {"id": 1, "name": "Built-in", "bounds": {"origin": {"x":0,"y":0}, "size": [3024, 1964]}, "scale": 2.0, "is_primary": true, "is_builtin": true},
                {"id": 2, "name": "Studio", "bounds": {"origin": {"x":0,"y":0}, "size": [5120, 2880]}, "scale": 2.0, "is_primary": false},
            ],
            "windows": [
                win(10, "capturemann \u{2014} editor.tsx", "Visual Studio Code", 1728, 1080, 1, false),
                win(11, "Docs: Q4 launch plan", "Google Chrome", 1680, 1050, 1, false),
                win(12, "CaptureMann \u{00B7} Pull requests", "Google Chrome", 1440, 900, 2, false),
                win(13, "Vault", "1Password", 800, 600, 1, false),
                win(14, "Old", "Finder", 900, 560, 1, true),
                win(15, "", "Terminal", 960, 600, 1, false),
            ],
        })
    }

    fn list() -> WindowList {
        let mut l = WindowList::default();
        l.apply(&reply(), &[]);
        l
    }

    fn rows(l: &WindowList) -> Vec<u32> {
        l.items()
            .iter()
            .filter_map(|it| match it {
                ListItem::Row { window, .. } => Some(l.windows[*window].id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parse_skips_minimized_and_keeps_front_to_back() {
        let l = list();
        let ids: Vec<u32> = l.windows.iter().map(|w| w.id).collect();
        assert_eq!(ids, [10, 11, 12, 13, 15]);
        assert_eq!((l.windows[1].width, l.windows[1].height), (1680, 1050));
        assert_eq!(l.windows[2].display_id, Some(2));
        assert_eq!(l.load, WindowsLoad::Loaded);
    }

    #[test]
    fn parse_survives_junk() {
        assert!(parse_windows(&json!(null)).is_empty());
        assert!(parse_windows(&json!({"windows": [1, {"id": 1}, {"bounds": {}}]})).is_empty());
        assert!(parse_picker_displays(&json!({"displays": [{"id": "x"}]})).is_empty());
        // The object form of a size is accepted too.
        let w = parse_windows(&json!({"windows": [
            {"id": 3, "title": "t", "app": "a", "bounds": {"size": {"width": 5, "height": 6}}}]}));
        assert_eq!((w[0].width, w[0].height, w[0].display_id), (5, 6, None));
    }

    #[test]
    fn external_flag_follows_is_builtin() {
        let l = list();
        let ext: Vec<bool> = l
            .items()
            .iter()
            .filter_map(|it| match it {
                ListItem::Row { external, .. } => Some(*external),
                _ => None,
            })
            .collect();
        // Rows are 10, 11, 12, 13, 15: only 12 is on the external display.
        assert_eq!(ext, [false, false, true, false, false]);
        assert_eq!(size_text(&l.windows[1], false), "1680 \u{00D7} 1050");
        assert_eq!(
            size_text(&l.windows[2], true),
            "1440 \u{00D7} 900 \u{00B7} External"
        );
    }

    #[test]
    fn without_memory_there_is_only_front_to_back() {
        let l = list();
        let items = l.items();
        assert_eq!(items[0], ListItem::Header(Group::FrontToBack));
        assert!(!items.contains(&ListItem::Header(Group::LastUsed)));
        assert_eq!(rows(&l), [10, 11, 12, 13, 15]);
    }

    #[test]
    fn last_used_group_comes_first_then_the_rest_front_to_back() {
        let mut l = list();
        l.last_used.touch("Google Chrome", "Docs: Q4 launch plan");
        let items = l.items();
        assert_eq!(items[0], ListItem::Header(Group::LastUsed));
        assert!(matches!(items[1], ListItem::Row { window: 1, .. }));
        assert_eq!(items[2], ListItem::Header(Group::FrontToBack));
        assert_eq!(rows(&l), [11, 10, 12, 13, 15]);
        // The same window renamed still matches by app.
        let mut l2 = list();
        l2.last_used.touch("Terminal", "old title");
        assert_eq!(rows(&l2)[0], 15);
        // A remembered window that is gone shows no group.
        let mut l3 = list();
        l3.last_used.touch("Mail", "Inbox");
        assert!(!l3.items().contains(&ListItem::Header(Group::LastUsed)));
    }

    #[test]
    fn last_used_only_window_has_no_front_to_back_header() {
        let mut l = WindowList::default();
        l.apply(
            &json!({"windows": [{"id": 1, "title": "T", "app": "A", "bounds": {"size": [1, 1]}}]}),
            &[],
        );
        l.last_used.touch("A", "T");
        assert_eq!(
            l.items().len(),
            2,
            "header and the row, no empty second group"
        );
    }

    #[test]
    fn search_filters_by_title_and_app_and_drops_the_groups() {
        let mut l = list();
        l.last_used.touch("Google Chrome", "Docs: Q4 launch plan");
        l.set_query("chrome");
        assert_eq!(rows(&l), [11, 12]);
        assert!(l.items().iter().all(|i| !matches!(i, ListItem::Header(_))));
        l.set_query("editor");
        assert_eq!(rows(&l), [10]);
        l.set_query("  ");
        assert!(!l.is_searching());
        assert_eq!(rows(&l).len(), 5);
        l.set_query("zzzz");
        assert!(l.items().is_empty());
        assert_eq!(
            l.empty_text().as_deref(),
            Some("No window matches \u{201C}zzzz\u{201D}.")
        );
    }

    #[test]
    fn empty_and_loading_texts() {
        let mut l = WindowList::default();
        assert_eq!(
            l.empty_text().as_deref(),
            Some("Looking for windows\u{2026}")
        );
        l.set_failed("  ");
        assert_eq!(
            l.empty_text().as_deref(),
            Some("The windows could not be listed.")
        );
        l.set_failed("daemon down");
        assert_eq!(l.empty_text().as_deref(), Some("daemon down"));
        l.apply(&json!({"windows": []}), &[]);
        assert_eq!(l.empty_text().as_deref(), Some("No windows are open."));
        assert_eq!(list().empty_text(), None);
    }

    #[test]
    fn placeholder_counts_recordable_windows() {
        assert_eq!(list().placeholder(), "Search 5 windows by title or app");
        let mut one = WindowList::default();
        one.apply(
            &json!({"windows": [{"id": 1, "title": "T", "app": "A", "bounds": {"size": [1, 1]}}]}),
            &[],
        );
        assert_eq!(one.placeholder(), "Search 1 window by title or app");
    }

    #[test]
    fn bundle_matching_is_best_effort_and_documented() {
        assert!(app_matches_bundle("Google Chrome", "com.google.Chrome"));
        assert!(app_matches_bundle("Finder", "com.apple.finder"));
        assert!(app_matches_bundle("1Password", "com.1password.1password"));
        assert!(app_matches_bundle("Safari", "COM.APPLE.SAFARI"));
        // The id's own words, not a substring: Xcode is not "Code".
        assert!(!app_matches_bundle("Code", "com.apple.dt.Xcode"));
        // A name that does not follow its id is not matched.
        assert!(!app_matches_bundle(
            "Visual Studio Code",
            "com.microsoft.VSCode"
        ));
        assert!(!app_matches_bundle("", "com.apple.finder"));
        assert!(!app_matches_bundle("Mail", ""));
    }

    #[test]
    fn privacy_hidden_apps_are_not_rows_and_get_one_note() {
        let mut l = WindowList::default();
        let ex = vec!["com.1password.1password".to_string()];
        l.apply(&reply(), &ex);
        assert_eq!(rows(&l), [10, 11, 12, 15]);
        assert_eq!(l.hidden_apps, ["1Password"]);
        assert!(l.window(13).is_none(), "not selectable");
        assert_eq!(
            l.items().last(),
            Some(&ListItem::Note(
                "1Password is hidden by your Privacy settings.".into()
            ))
        );
        // Several apps, one note, plural wording.
        let ex2 = vec![
            "com.1password.1password".to_string(),
            "com.apple.Terminal".to_string(),
        ];
        l.apply(&reply(), &ex2);
        assert_eq!(l.hidden_apps, ["1Password", "Terminal"]);
        let notes = l
            .items()
            .into_iter()
            .filter(|i| matches!(i, ListItem::Note(_)))
            .count();
        assert_eq!(notes, 1);
        assert_eq!(
            hidden_note(&l.hidden_apps).as_deref(),
            Some("1Password, Terminal are hidden by your Privacy settings.")
        );
        // An exclusion with no window open adds nothing.
        l.apply(&reply(), &["com.nobody.here".to_string()]);
        assert!(l.hidden_apps.is_empty());
        assert!(!l.items().iter().any(|i| matches!(i, ListItem::Note(_))));
        assert_eq!(hidden_note(&[]), None);
    }

    #[test]
    fn note_survives_an_empty_search_result() {
        let mut l = WindowList::default();
        l.apply(&reply(), &["com.1password.1password".to_string()]);
        l.set_query("zzzz");
        assert_eq!(l.items().len(), 1, "only the note");
        assert!(l.empty_text().is_some());
    }

    #[test]
    fn keys_move_the_cursor_and_space_or_enter_choose() {
        let mut l = list();
        assert_eq!(l.key(ListKey::Down, None), KeyOutcome::Nothing);
        assert_eq!(l.cursor, Some(10));
        l.key(ListKey::Down, None);
        assert_eq!(l.cursor, Some(11));
        l.key(ListKey::End, None);
        assert_eq!(l.cursor, Some(15));
        l.key(ListKey::Down, None);
        assert_eq!(l.cursor, Some(15), "no wrap");
        l.key(ListKey::Home, None);
        assert_eq!(l.cursor, Some(10));
        l.key(ListKey::Up, None);
        assert_eq!(l.cursor, Some(10));
        l.key(ListKey::Down, None);
        assert_eq!(l.key(ListKey::Space, None), KeyOutcome::Select(11));
        assert_eq!(l.key(ListKey::Enter, None), KeyOutcome::Select(11));
        // On the chosen window Space does nothing and Enter starts.
        assert_eq!(l.key(ListKey::Space, Some(11)), KeyOutcome::Nothing);
        assert_eq!(l.key(ListKey::Enter, Some(11)), KeyOutcome::Start);
        // No cursor and nothing chosen: nothing to select.
        let mut l2 = list();
        assert_eq!(l2.key(ListKey::Enter, None), KeyOutcome::Nothing);
        assert_eq!(l2.key(ListKey::Space, None), KeyOutcome::Nothing);
        // No cursor but a window chosen: Enter starts, the first arrow lands on it.
        assert_eq!(l2.key(ListKey::Enter, Some(12)), KeyOutcome::Start);
        l2.key(ListKey::Down, Some(12));
        assert_eq!(l2.cursor, Some(12));
        l2.key(ListKey::Down, Some(12));
        assert_eq!(l2.cursor, Some(13));
    }

    #[test]
    fn keys_follow_the_displayed_order_including_last_used() {
        let mut l = list();
        l.last_used.touch("Terminal", "x");
        // Display order: 15 (last used), 10, 11, 12, 13.
        l.key(ListKey::Down, None);
        assert_eq!(l.cursor, Some(15));
        l.key(ListKey::Down, None);
        assert_eq!(l.cursor, Some(10));
        assert_eq!(l.cursor_item_index(), Some(3), "header, row, header, row");
    }

    #[test]
    fn cursor_follows_the_search() {
        let mut l = list();
        l.cursor_to(Some(12));
        assert_eq!(l.cursor, Some(12));
        l.set_query("chrome");
        assert_eq!(l.cursor, Some(12), "still shown");
        l.set_query("editor");
        assert_eq!(l.cursor, None, "gone from the list");
        l.cursor_to(Some(999));
        assert_eq!(l.cursor, None);
        // keep_selection (the picker's rule) agrees on the first row as the fallback.
        assert_eq!(l.kept(Some(999)), Some(0));
    }

    #[test]
    fn no_keys_on_an_empty_list() {
        let mut l = WindowList::default();
        assert_eq!(l.key(ListKey::Down, None), KeyOutcome::Nothing);
        assert_eq!(l.cursor, None);
        assert_eq!(l.key(ListKey::Enter, None), KeyOutcome::Nothing);
    }

    #[test]
    fn remember_moves_the_window_to_the_front_and_stores_the_value() {
        let mut l = list();
        let v = l.remember(11).unwrap();
        assert_eq!(v[0]["app"], "Google Chrome");
        assert_eq!(v[0]["title"], "Docs: Q4 launch plan");
        l.remember(10).unwrap();
        let back = LastUsed::from_value(&l.last_used.to_value());
        assert_eq!(back.entries()[0].app, "Visual Studio Code");
        assert_eq!(back.entries()[1].app, "Google Chrome");
        assert!(l.remember(999).is_none());
        assert_eq!(KEY_LAST_USED, "app.window_picker.last_used");
    }

    #[test]
    fn footer_text_and_titles() {
        let l = list();
        assert_eq!(
            footer_text(Some(11), &l).as_deref(),
            Some("Docs: Q4 launch plan \u{00B7} Google Chrome")
        );
        assert_eq!(footer_text(None, &l), None);
        assert_eq!(footer_text(Some(999), &l), None);
        // Untitled window: the app alone.
        assert_eq!(footer_text(Some(15), &l).as_deref(), Some("Terminal"));
        assert_eq!(row_title(l.window(15).unwrap()), "Terminal");
        assert_eq!(row_title(l.window(11).unwrap()), "Docs: Q4 launch plan");
        assert_eq!(
            row_label(l.window(11).unwrap(), false),
            "Docs: Q4 launch plan, Google Chrome, 1680 \u{00D7} 1050"
        );
        assert!(row_label(l.window(12).unwrap(), true).ends_with("external display"));
    }

    #[test]
    fn visible_range_picks_rows_in_the_scroll_area() {
        // header, 5 rows
        let h = [HEADER_H, ROW_H, ROW_H, ROW_H, ROW_H, ROW_H];
        assert_eq!(visible_range(&h, 0.0, LIST_VIEWPORT_H, 0), 0..6);
        let many: Vec<f32> = std::iter::once(HEADER_H)
            .chain(std::iter::repeat_n(ROW_H, 30))
            .collect();
        // Top: the header plus the rows that start above 312.
        let r = visible_range(&many, 0.0, LIST_VIEWPORT_H, 0);
        assert_eq!(r.start, 0);
        assert!(r.end >= 6 && r.end <= 8, "{r:?}");
        // Scrolled by two rows: the first rows are gone.
        let r = visible_range(&many, HEADER_H + 2.0 * ROW_H, LIST_VIEWPORT_H, 0);
        assert_eq!(r.start, 3);
        // Overscan widens both sides but stays inside the list.
        let r2 = visible_range(&many, HEADER_H + 2.0 * ROW_H, LIST_VIEWPORT_H, 1);
        assert_eq!((r2.start, r2.end), (r.start - 1, r.end + 1));
        let total: f32 = many.iter().sum();
        let end = visible_range(&many, total - LIST_VIEWPORT_H, LIST_VIEWPORT_H, 2);
        assert!(end.end == many.len() && end.start < many.len());
        // Degenerate input.
        assert_eq!(visible_range(&[], 0.0, 100.0, 1), 0..0);
        assert_eq!(visible_range(&many, 0.0, 0.0, 1), 0..0);
        assert_eq!(visible_range(&many, -50.0, 10.0, 0), 0..1);
    }

    #[test]
    fn visible_window_ids_are_only_the_rows_in_range() {
        let l = list();
        // items: header, 10, 11, 12, 13, 15
        assert_eq!(l.visible_window_ids(0..3), [10, 11]);
        assert_eq!(l.visible_window_ids(4..6), [13, 15]);
        assert!(l.visible_window_ids(0..1).is_empty());
    }

    #[test]
    fn reveal_scrolls_the_least() {
        let h = [HEADER_H, ROW_H, ROW_H, ROW_H, ROW_H, ROW_H, ROW_H, ROW_H];
        // Already visible: unchanged.
        assert_eq!(offset_to_reveal(&h, 2, 0.0, LIST_VIEWPORT_H), 0.0);
        // Below the fold: its bottom edge meets the bottom of the viewport.
        let off = offset_to_reveal(&h, 7, 0.0, LIST_VIEWPORT_H);
        assert_eq!(off, HEADER_H + 7.0 * ROW_H - LIST_VIEWPORT_H);
        // Above the viewport: its top edge meets the top.
        assert_eq!(offset_to_reveal(&h, 1, 200.0, LIST_VIEWPORT_H), HEADER_H);
    }

    #[test]
    fn badge_is_stable_and_has_an_initial() {
        assert_eq!(app_badge("Google Chrome").0, 'G');
        assert_eq!(app_badge("google chrome"), app_badge("Google Chrome"));
        assert_eq!(app_badge("  \u{00E9}cran").0, '\u{00C9}');
        assert_eq!(app_badge("").0, '?');
        for a in ["Slack", "Finder", "Terminal", "x"] {
            assert!(app_badge(a).1 < BADGE_PALETTE.len());
        }
    }

    #[test]
    fn block_height_is_the_sum_of_its_parts() {
        assert_eq!(LIST_BLOCK_H, 34.0 + 10.0 + 312.0 + 2.0);
    }
}
