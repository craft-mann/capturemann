//! Search, filters, the app rail and keyboard navigation of the window picker.
//!
//! Search: every whitespace-separated query token must match the window title or the app name,
//! case- and accent-insensitive (see [`fold`]). Per token, lower is better: 0 title starts with
//! it, 1 a title word starts with it, 2 title subsequence, 3 app name starts with it or a word
//! of it does, 4 app name subsequence. A window's rank is its worst token's rank.

use super::last_used::LastUsed;
use super::{PickerDisplay, PickerWindow};

// ---------------------------------------------------------------------------------------------
// Folding

const FOLD_TABLE: &[(char, &str)] = &[
    ('a', "àáâãäåāăąǎảạấầẩẫậắằẳẵặ"),
    ('c', "çćĉċč"),
    ('d', "ďđ"),
    ('e', "èéêëēĕėęěẻẽẹếềểễệ"),
    ('g', "ĝğġģ"),
    ('h', "ĥħ"),
    ('i', "ìíîïĩīĭįıỉịǐ"),
    ('j', "ĵ"),
    ('k', "ķ"),
    ('l', "ĺļľŀł"),
    ('n', "ñńņňŉ"),
    ('o', "òóôõöøōŏőơǒỏọốồổỗộớờởỡợ"),
    ('r', "ŕŗř"),
    ('s', "śŝşšș"),
    ('t', "ţťŧț"),
    ('u', "ùúûüũūŭůűųưǔủụứừửữự"),
    ('w', "ŵ"),
    ('y', "ýÿŷỳỷỹỵ"),
    ('z', "źżž"),
];

fn fold_char(c: char) -> Option<char> {
    if ('\u{0300}'..='\u{036F}').contains(&c) {
        return None; // a combining mark (decomposed input)
    }
    if c.is_ascii() {
        return Some(c.to_ascii_lowercase());
    }
    let lower = c.to_lowercase().next().unwrap_or(c);
    for (base, set) in FOLD_TABLE {
        if set.contains(lower) {
            return Some(*base);
        }
    }
    Some(lower)
}

/// Lower-cases and strips accents, so "Tài liệu" folds to "tai lieu". Covers Latin letters
/// including the Vietnamese ones and `đ`; precomposed and decomposed (NFD) input fold alike.
/// Other scripts are only lower-cased.
pub fn fold(s: &str) -> String {
    s.chars().filter_map(fold_char).collect()
}

fn is_subsequence(needle: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|c| it.any(|h| h == c))
}

fn word_starts(s: &str) -> impl Iterator<Item = &str> {
    s.char_indices()
        .filter(|(i, c)| {
            c.is_alphanumeric()
                && (*i == 0
                    || s[..*i]
                        .chars()
                        .next_back()
                        .is_some_and(|p| !p.is_alphanumeric()))
        })
        .map(|(i, _)| &s[i..])
}

fn token_rank(token: &str, title: &str, app: &str) -> Option<u8> {
    if title.starts_with(token) {
        return Some(0);
    }
    if word_starts(title).any(|w| w.starts_with(token)) {
        return Some(1);
    }
    if is_subsequence(token, title) {
        return Some(2);
    }
    if app.starts_with(token) || word_starts(app).any(|w| w.starts_with(token)) {
        return Some(3);
    }
    if is_subsequence(token, app) {
        return Some(4);
    }
    None
}

/// Rank of a window for a query (0 best, 4 worst), or `None` when some token matches neither
/// the title nor the app. A blank query matches everything at rank 0.
pub fn rank_window(query: &str, title: &str, app: &str) -> Option<u8> {
    let q = fold(query);
    let (title, app) = (fold(title), fold(app));
    let mut worst = 0;
    for token in q.split_whitespace() {
        worst = worst.max(token_rank(token, &title, &app)?);
    }
    Some(worst)
}

// ---------------------------------------------------------------------------------------------
// Filters

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AppFilter {
    #[default]
    All,
    App(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayFilter {
    #[default]
    All,
    BuiltIn,
    /// Any display that is not built in.
    External,
    Display(u32),
}

/// Does a window pass the display filter? A window with no known display (or one whose display
/// is missing from `displays`) passes only `All`.
fn display_ok(w: &PickerWindow, displays: &[PickerDisplay], f: DisplayFilter) -> bool {
    if f == DisplayFilter::All {
        return true;
    }
    let Some(id) = w.display_id else { return false };
    match f {
        DisplayFilter::All => true,
        DisplayFilter::Display(d) => id == d,
        DisplayFilter::BuiltIn => displays.iter().any(|d| d.id == id && d.builtin),
        DisplayFilter::External => displays.iter().any(|d| d.id == id && !d.builtin),
    }
}

/// Indices into `windows` that the picker shows, in display order.
///
/// Empty (blank) query: the most recently used window (best [`LastUsed::rank_of`], ties to the
/// front-most) goes first, the rest stay front to back. With a query: by rank, then front to
/// back. The app and display filters apply either way.
pub fn visible(
    windows: &[PickerWindow],
    displays: &[PickerDisplay],
    query: &str,
    app_filter: &AppFilter,
    display_filter: DisplayFilter,
    last_used: &LastUsed,
) -> Vec<usize> {
    let mut hits: Vec<(u8, usize)> = windows
        .iter()
        .enumerate()
        .filter(|(_, w)| match app_filter {
            AppFilter::All => true,
            AppFilter::App(a) => &w.app == a,
        })
        .filter(|(_, w)| display_ok(w, displays, display_filter))
        .filter_map(|(i, w)| rank_window(query, &w.title, &w.app).map(|r| (r, i)))
        .collect();
    if query.trim().is_empty() {
        let first = hits
            .iter()
            .filter_map(|&(_, i)| {
                last_used
                    .rank_of(&windows[i].app, &windows[i].title)
                    .map(|r| (r, i))
            })
            .min();
        let mut out: Vec<usize> = hits.iter().map(|&(_, i)| i).collect();
        if let Some((_, i)) = first {
            out.retain(|&x| x != i);
            out.insert(0, i);
        }
        return out;
    }
    hits.sort(); // (rank, index): index order is front to back
    hits.into_iter().map(|(_, i)| i).collect()
}

// ---------------------------------------------------------------------------------------------
// App rail

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppGroup {
    pub app: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppGroups {
    /// The "All windows" count: windows passing the display filter and the query.
    pub total: usize,
    /// Apps with at least one such window (apps with none are left out), ordered by their
    /// front-most window.
    pub groups: Vec<AppGroup>,
}

/// The rail: counts respect the display filter and the query (not the app filter itself).
pub fn app_groups(
    windows: &[PickerWindow],
    displays: &[PickerDisplay],
    display_filter: DisplayFilter,
    query: &str,
) -> AppGroups {
    let mut out = AppGroups::default();
    for w in windows {
        if !display_ok(w, displays, display_filter)
            || rank_window(query, &w.title, &w.app).is_none()
        {
            continue;
        }
        out.total += 1;
        match out.groups.iter_mut().find(|g| g.app == w.app) {
            Some(g) => g.count += 1,
            None => out.groups.push(AppGroup {
                app: w.app.clone(),
                count: 1,
            }),
        }
    }
    out
}

/// Cmd+1..9: `n` = 1 is the first app (not "All windows"). `None` for 0, past the end or above 9.
pub fn app_for_shortcut(groups: &[AppGroup], n: usize) -> Option<&AppGroup> {
    if (1..=9).contains(&n) {
        groups.get(n - 1)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------------------------
// Keyboard navigation

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Grid { columns: usize },
    List,
}

/// New selected position (an index into the visible list) after a key. Rules, no wrap anywhere:
/// - empty list: `None`; nothing selected: `End` selects the last, any other key the first.
/// - Home / End: first / last.
/// - List: Up / Down move by one and stop at the ends; Left / Right do nothing.
/// - Grid: Left / Right move by one (across row ends, stopping at first and last); Up moves
///   one row up, staying put on the top row; Down moves one row down, and from a row above a
///   shorter last row lands on the last item; on the last row it stays put.
///
/// `columns` of 0 counts as 1.
pub fn navigate(selected: Option<usize>, len: usize, layout: Layout, key: Nav) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    let Some(cur) = selected.map(|s| s.min(last)) else {
        return Some(if key == Nav::End { last } else { 0 });
    };
    let cols = match layout {
        Layout::Grid { columns } => columns.max(1),
        Layout::List => 1,
    };
    let list = layout == Layout::List;
    Some(
        match key {
            Nav::Home => 0,
            Nav::End => last,
            Nav::Left if list => cur,
            Nav::Right if list => cur,
            Nav::Left => cur.saturating_sub(1),
            Nav::Right => (cur + 1).min(last),
            Nav::Up if cur >= cols => cur - cols,
            Nav::Up => cur,
            Nav::Down => {
                if cur + cols <= last {
                    cur + cols
                } else if cur / cols < last / cols {
                    last
                } else {
                    cur
                }
            }
        }
        .min(last),
    )
}

/// Selection after the visible list changed: the window with `old_selected_id` if it is still
/// visible, else the first visible item, else `None`. `visible` holds indices into `windows`;
/// the result is a position in `visible`.
pub fn keep_selection(
    old_selected_id: Option<u32>,
    windows: &[PickerWindow],
    visible: &[usize],
) -> Option<usize> {
    if visible.is_empty() {
        return None;
    }
    old_selected_id
        .and_then(|id| {
            visible
                .iter()
                .position(|&i| windows.get(i).is_some_and(|w| w.id == id))
        })
        .or(Some(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(id: u32, app: &str, title: &str, display: Option<u32>) -> PickerWindow {
        PickerWindow {
            id,
            title: title.into(),
            app: app.into(),
            width: 800,
            height: 600,
            display_id: display,
        }
    }

    fn displays() -> Vec<PickerDisplay> {
        vec![
            PickerDisplay {
                id: 1,
                name: "Built-in".into(),
                builtin: true,
            },
            PickerDisplay {
                id: 2,
                name: "LG".into(),
                builtin: false,
            },
            PickerDisplay {
                id: 3,
                name: "Dell".into(),
                builtin: false,
            },
        ]
    }

    fn sample() -> Vec<PickerWindow> {
        vec![
            w(10, "Safari", "Docs: Q4 launch plan", Some(1)),
            w(11, "Finder", "Documents", Some(1)),
            w(12, "Chrome", "Google Docs: Release notes", Some(2)),
            w(13, "Safari", "Inbox", Some(2)),
            w(14, "Terminal", "zsh", None),
        ]
    }

    fn vis(
        ws: &[PickerWindow],
        q: &str,
        a: &AppFilter,
        d: DisplayFilter,
        lu: &LastUsed,
    ) -> Vec<usize> {
        visible(ws, &displays(), q, a, d, lu)
    }

    // ---- fold and search

    #[test]
    fn fold_strips_vietnamese_and_case() {
        assert_eq!(fold("Tài liệu"), "tai lieu");
        assert_eq!(fold("ĐƯỜNG Đi"), "duong di");
        assert_eq!(fold("Việt Nam ỞỮỰ"), "viet nam ouu");
        assert_eq!(fold("Café Ñandú"), "cafe nandu");
        assert_eq!(fold(""), "");
        // decomposed input: "a" + combining grave
        assert_eq!(fold("a\u{0300}"), "a");
        assert_eq!(fold("日本語 Ab"), "日本語 ab");
    }

    #[test]
    fn accent_insensitive_both_ways() {
        assert!(rank_window("tai lieu", "Tài liệu", "Pages").is_some());
        assert!(rank_window("tài liệu", "Tai lieu", "Pages").is_some());
        assert!(rank_window("TÀI", "tai lieu", "Pages").is_some());
    }

    #[test]
    fn board_example_doc() {
        let ws = sample();
        let v = vis(
            &ws,
            "doc",
            &AppFilter::All,
            DisplayFilter::All,
            &LastUsed::new(),
        );
        // Docs (title prefix), Documents (title prefix), Google Docs (word start); Safari "Inbox" no.
        assert_eq!(v, vec![0, 1, 2]);
    }

    #[test]
    fn rank_tiers() {
        assert_eq!(rank_window("doc", "Docs", "X"), Some(0));
        assert_eq!(rank_window("doc", "Google Docs", "X"), Some(1));
        assert_eq!(rank_window("dcs", "Docs", "X"), Some(2));
        assert_eq!(rank_window("saf", "Inbox", "Safari"), Some(3));
        assert_eq!(rank_window("sfr", "Inbox", "Safari"), Some(4));
        assert_eq!(rank_window("zzz", "Inbox", "Safari"), None);
    }

    #[test]
    fn app_name_is_searched() {
        let ws = sample();
        let v = vis(
            &ws,
            "finder",
            &AppFilter::All,
            DisplayFilter::All,
            &LastUsed::new(),
        );
        assert_eq!(v, vec![1]);
    }

    #[test]
    fn multi_token_all_must_match() {
        let ws = sample();
        let lu = LastUsed::new();
        let v = vis(
            &ws,
            "google notes",
            &AppFilter::All,
            DisplayFilter::All,
            &lu,
        );
        assert_eq!(v, vec![2]);
        let v = vis(
            &ws,
            "google inbox",
            &AppFilter::All,
            DisplayFilter::All,
            &lu,
        );
        assert!(v.is_empty());
        // token matching the app plus a token matching the title
        let v = vis(
            &ws,
            "safari inbox",
            &AppFilter::All,
            DisplayFilter::All,
            &lu,
        );
        assert_eq!(v, vec![3]);
    }

    #[test]
    fn no_match_and_empty_list() {
        let lu = LastUsed::new();
        assert!(vis(&sample(), "qqq", &AppFilter::All, DisplayFilter::All, &lu).is_empty());
        assert!(vis(&[], "", &AppFilter::All, DisplayFilter::All, &lu).is_empty());
        assert!(vis(&[], "x", &AppFilter::All, DisplayFilter::All, &lu).is_empty());
    }

    #[test]
    fn query_orders_by_rank_then_front_to_back() {
        let ws = vec![
            w(1, "A", "My docs", None),
            w(2, "A", "Docs two", None),
            w(3, "A", "Docs one", None),
        ];
        let v = vis(
            &ws,
            "docs",
            &AppFilter::All,
            DisplayFilter::All,
            &LastUsed::new(),
        );
        assert_eq!(v, vec![1, 2, 0]);
    }

    #[test]
    fn blank_query_is_whitespace_tolerant() {
        let ws = sample();
        let v = vis(
            &ws,
            "   ",
            &AppFilter::All,
            DisplayFilter::All,
            &LastUsed::new(),
        );
        assert_eq!(v, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn unicode_title_search() {
        let ws = vec![
            w(1, "Pages", "Tài liệu mới", None),
            w(2, "Pages", "Other", None),
        ];
        let v = vis(
            &ws,
            "tai lieu",
            &AppFilter::All,
            DisplayFilter::All,
            &LastUsed::new(),
        );
        assert_eq!(v, vec![0]);
    }

    // ---- visible: filters and last used

    #[test]
    fn empty_query_puts_last_used_first() {
        let ws = sample();
        let mut lu = LastUsed::new();
        lu.touch("Chrome", "Google Docs: Release notes");
        let v = vis(&ws, "", &AppFilter::All, DisplayFilter::All, &lu);
        assert_eq!(v, vec![2, 0, 1, 3, 4]);
    }

    #[test]
    fn last_used_survives_title_change_and_picks_most_recent() {
        let ws = sample();
        let mut lu = LastUsed::new();
        lu.touch("Finder", "Something old");
        lu.touch("Safari", "Gone window");
        // Safari entry is the most recent; the front-most Safari window wins the tie.
        let v = vis(&ws, "", &AppFilter::All, DisplayFilter::All, &lu);
        assert_eq!(v, vec![0, 1, 2, 3, 4]);
        lu.touch("Finder", "Documents");
        let v = vis(&ws, "", &AppFilter::All, DisplayFilter::All, &lu);
        assert_eq!(v[0], 1);
    }

    #[test]
    fn last_used_ignored_when_filtered_out() {
        let ws = sample();
        let mut lu = LastUsed::new();
        lu.touch("Chrome", "Google Docs: Release notes");
        let v = vis(
            &ws,
            "",
            &AppFilter::App("Safari".into()),
            DisplayFilter::All,
            &lu,
        );
        assert_eq!(v, vec![0, 3]);
    }

    #[test]
    fn last_used_does_not_reorder_with_query() {
        let ws = sample();
        let mut lu = LastUsed::new();
        lu.touch("Finder", "Documents");
        let v = vis(&ws, "doc", &AppFilter::All, DisplayFilter::All, &lu);
        assert_eq!(v, vec![0, 1, 2]);
    }

    #[test]
    fn display_filters() {
        let ws = sample();
        let lu = LastUsed::new();
        let a = AppFilter::All;
        assert_eq!(vis(&ws, "", &a, DisplayFilter::BuiltIn, &lu), vec![0, 1]);
        assert_eq!(vis(&ws, "", &a, DisplayFilter::External, &lu), vec![2, 3]);
        assert_eq!(vis(&ws, "", &a, DisplayFilter::Display(2), &lu), vec![2, 3]);
        assert_eq!(
            vis(&ws, "", &a, DisplayFilter::Display(3), &lu),
            Vec::<usize>::new()
        );
        // window without a display only passes All
        assert!(vis(&ws, "zsh", &a, DisplayFilter::All, &lu).contains(&4));
        assert!(!vis(&ws, "zsh", &a, DisplayFilter::External, &lu).contains(&4));
    }

    #[test]
    fn unknown_display_id_fails_builtin_and_external() {
        let ws = vec![w(1, "A", "t", Some(99))];
        let lu = LastUsed::new();
        let a = AppFilter::All;
        assert!(vis(&ws, "", &a, DisplayFilter::BuiltIn, &lu).is_empty());
        assert!(vis(&ws, "", &a, DisplayFilter::External, &lu).is_empty());
        assert_eq!(vis(&ws, "", &a, DisplayFilter::Display(99), &lu), vec![0]);
    }

    #[test]
    fn app_filter_exact_name() {
        let ws = sample();
        let v = vis(
            &ws,
            "",
            &AppFilter::App("Safari".into()),
            DisplayFilter::All,
            &LastUsed::new(),
        );
        assert_eq!(v, vec![0, 3]);
        let v = vis(
            &ws,
            "",
            &AppFilter::App("safari".into()),
            DisplayFilter::All,
            &LastUsed::new(),
        );
        assert!(v.is_empty());
    }

    #[test]
    fn duplicate_titles_keep_front_to_back() {
        let ws = vec![
            w(1, "Terminal", "zsh", None),
            w(2, "Terminal", "zsh", None),
            w(3, "Terminal", "zsh", None),
        ];
        let v = vis(
            &ws,
            "zsh",
            &AppFilter::All,
            DisplayFilter::All,
            &LastUsed::new(),
        );
        assert_eq!(v, vec![0, 1, 2]);
        let mut lu = LastUsed::new();
        lu.touch("Terminal", "zsh");
        let v = vis(&ws, "", &AppFilter::All, DisplayFilter::All, &lu);
        assert_eq!(v, vec![0, 1, 2]);
    }

    #[test]
    fn hundred_windows() {
        let ws: Vec<_> = (0..100)
            .map(|i| {
                w(
                    i,
                    &format!("App{}", i % 7),
                    &format!("Window {i}"),
                    Some(1 + i % 2),
                )
            })
            .collect();
        let lu = LastUsed::new();
        let all = vis(&ws, "", &AppFilter::All, DisplayFilter::All, &lu);
        assert_eq!(all, (0..100).collect::<Vec<_>>());
        let v = vis(&ws, "window 99", &AppFilter::All, DisplayFilter::All, &lu);
        assert_eq!(v[0], 99);
        let g = app_groups(&ws, &displays(), DisplayFilter::All, "");
        assert_eq!(g.total, 100);
        assert_eq!(g.groups.len(), 7);
        assert_eq!(g.groups.iter().map(|x| x.count).sum::<usize>(), 100);
    }

    // ---- app rail

    #[test]
    fn groups_order_front_most_first_with_counts() {
        let g = app_groups(&sample(), &displays(), DisplayFilter::All, "");
        assert_eq!(g.total, 5);
        let names: Vec<_> = g.groups.iter().map(|x| (x.app.as_str(), x.count)).collect();
        assert_eq!(
            names,
            [("Safari", 2), ("Finder", 1), ("Chrome", 1), ("Terminal", 1)]
        );
    }

    #[test]
    fn groups_respect_display_filter_and_query() {
        let g = app_groups(&sample(), &displays(), DisplayFilter::External, "");
        assert_eq!(g.total, 2);
        assert_eq!(g.groups.len(), 2);
        assert_eq!(
            g.groups[0],
            AppGroup {
                app: "Chrome".into(),
                count: 1
            }
        );
        let g = app_groups(&sample(), &displays(), DisplayFilter::All, "doc");
        assert_eq!(g.total, 3);
        assert_eq!(g.groups.len(), 3);
        let g = app_groups(&sample(), &displays(), DisplayFilter::All, "nomatch");
        assert_eq!(g, AppGroups::default());
        assert_eq!(
            app_groups(&[], &[], DisplayFilter::All, ""),
            AppGroups::default()
        );
    }

    #[test]
    fn shortcut_numbers() {
        let g = app_groups(&sample(), &displays(), DisplayFilter::All, "");
        assert_eq!(app_for_shortcut(&g.groups, 1).unwrap().app, "Safari");
        assert_eq!(app_for_shortcut(&g.groups, 4).unwrap().app, "Terminal");
        assert!(app_for_shortcut(&g.groups, 0).is_none());
        assert!(app_for_shortcut(&g.groups, 5).is_none());
        assert!(app_for_shortcut(&g.groups, 10).is_none());
        assert!(app_for_shortcut(&[], 1).is_none());
    }

    #[test]
    fn shortcut_stops_at_nine() {
        let groups: Vec<_> = (0..12)
            .map(|i| AppGroup {
                app: format!("A{i}"),
                count: 1,
            })
            .collect();
        assert_eq!(app_for_shortcut(&groups, 9).unwrap().app, "A8");
        assert!(app_for_shortcut(&groups, 10).is_none());
    }

    // ---- navigation

    const G3: Layout = Layout::Grid { columns: 3 };

    #[test]
    fn nav_empty_and_unselected() {
        assert_eq!(navigate(None, 0, G3, Nav::Down), None);
        assert_eq!(navigate(Some(2), 0, G3, Nav::Down), None);
        assert_eq!(navigate(None, 5, G3, Nav::Down), Some(0));
        assert_eq!(navigate(None, 5, G3, Nav::Up), Some(0));
        assert_eq!(navigate(None, 5, G3, Nav::End), Some(4));
        assert_eq!(navigate(None, 5, Layout::List, Nav::Home), Some(0));
    }

    #[test]
    fn nav_grid_rules() {
        // 8 items, 3 columns: rows [0 1 2] [3 4 5] [6 7]
        let n = |s, k| navigate(Some(s), 8, G3, k).unwrap();
        assert_eq!(n(4, Nav::Down), 7);
        assert_eq!(n(5, Nav::Down), 7); // shorter last row: land on last item
        assert_eq!(n(7, Nav::Down), 7); // last row: stay
        assert_eq!(n(6, Nav::Down), 6);
        assert_eq!(n(3, Nav::Up), 0);
        assert_eq!(n(2, Nav::Right), 3); // crosses the row end
        assert_eq!(n(3, Nav::Left), 2);
        assert_eq!(n(7, Nav::Right), 7); // no wrap
        assert_eq!(n(0, Nav::Left), 0);
        assert_eq!(n(5, Nav::Home), 0);
        assert_eq!(n(0, Nav::End), 7);
    }

    #[test]
    fn nav_grid_top_row_up_stays() {
        let n = |s| navigate(Some(s), 8, G3, Nav::Up).unwrap();
        assert_eq!(n(0), 0);
        assert_eq!(n(2), 2); // top row: stays put
        assert_eq!(n(1), 1);
    }

    #[test]
    fn nav_list() {
        let n = |s, k| navigate(Some(s), 4, Layout::List, k).unwrap();
        assert_eq!(n(0, Nav::Down), 1);
        assert_eq!(n(3, Nav::Down), 3);
        assert_eq!(n(0, Nav::Up), 0);
        assert_eq!(n(2, Nav::Up), 1);
        assert_eq!(n(2, Nav::Left), 2);
        assert_eq!(n(2, Nav::Right), 2);
        assert_eq!(n(2, Nav::End), 3);
    }

    #[test]
    fn nav_edge_cases() {
        assert_eq!(navigate(Some(0), 1, G3, Nav::Down), Some(0));
        assert_eq!(
            navigate(Some(0), 1, Layout::Grid { columns: 0 }, Nav::Down),
            Some(0)
        );
        assert_eq!(
            navigate(Some(0), 3, Layout::Grid { columns: 0 }, Nav::Down),
            Some(1)
        );
        // stale selection beyond the list is clamped
        assert_eq!(navigate(Some(50), 3, G3, Nav::Left), Some(1));
    }

    #[test]
    fn keep_selection_rules() {
        let ws = sample();
        assert_eq!(keep_selection(Some(12), &ws, &[0, 1, 2]), Some(2));
        assert_eq!(keep_selection(Some(12), &ws, &[2, 0]), Some(0));
        assert_eq!(keep_selection(Some(12), &ws, &[0, 1]), Some(0)); // gone: first
        assert_eq!(keep_selection(None, &ws, &[3, 1]), Some(0));
        assert_eq!(keep_selection(Some(12), &ws, &[]), None);
        assert_eq!(keep_selection(Some(999), &ws, &[1]), Some(0));
    }
}
