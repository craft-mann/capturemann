//! The state machine behind the window picker palette (board R19): query, filters, layout,
//! selection and what a key press means. Display-free; `ui::window_picker` draws it and feeds it
//! keys. The ordering, filtering and navigation rules are those of [`super::list`].

use super::list::{
    app_for_shortcut, app_groups, keep_selection, navigate, visible, AppFilter, AppGroups,
    DisplayFilter, Layout, Nav,
};
use super::{LastUsed, PickerDisplay, PickerWindow};

/// Columns of the grid at the palette's width (board: three cards per row).
pub const GRID_COLUMNS: usize = 3;

/// What a key (or a click on a button) asks the host to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing for the host to do (the state may still have changed).
    None,
    /// Enter, or a double click on a card: capture this window. The identity is for the
    /// last-used memory.
    Capture {
        window_id: u32,
        app: String,
        title: String,
    },
    /// Esc: close without capturing.
    Cancel,
    /// Tab, or "Pick on screen": close and start the hover mode.
    PickOnScreen,
}

#[derive(Debug, Clone)]
pub struct PickerState {
    windows: Vec<PickerWindow>,
    displays: Vec<PickerDisplay>,
    last_used: LastUsed,
    query: String,
    app_filter: AppFilter,
    display_filter: DisplayFilter,
    list_layout: bool,
    columns: usize,
    visible: Vec<usize>,
    selected: Option<usize>,
}

impl PickerState {
    /// The first visible window (the most recently used one, else the front-most) is selected.
    pub fn new(
        windows: Vec<PickerWindow>,
        displays: Vec<PickerDisplay>,
        last_used: LastUsed,
        list_layout: bool,
    ) -> Self {
        let mut s = PickerState {
            windows,
            displays,
            last_used,
            query: String::new(),
            app_filter: AppFilter::All,
            display_filter: DisplayFilter::All,
            list_layout,
            columns: GRID_COLUMNS,
            visible: Vec::new(),
            selected: None,
        };
        s.refresh(None);
        s
    }

    // ---- read side -------------------------------------------------------------------------

    pub fn windows(&self) -> &[PickerWindow] {
        &self.windows
    }

    pub fn displays(&self) -> &[PickerDisplay] {
        &self.displays
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn app_filter(&self) -> &AppFilter {
        &self.app_filter
    }

    pub fn display_filter(&self) -> DisplayFilter {
        self.display_filter
    }

    pub fn is_list_layout(&self) -> bool {
        self.list_layout
    }

    pub fn layout(&self) -> Layout {
        if self.list_layout {
            Layout::List
        } else {
            Layout::Grid {
                columns: self.columns,
            }
        }
    }

    /// The windows shown, in display order.
    pub fn visible_windows(&self) -> impl Iterator<Item = &PickerWindow> {
        self.visible.iter().map(|&i| &self.windows[i])
    }

    pub fn visible_len(&self) -> usize {
        self.visible.len()
    }

    /// Position of the selection in [`visible_windows`](Self::visible_windows).
    pub fn selected_pos(&self) -> Option<usize> {
        self.selected
    }

    pub fn selected_window(&self) -> Option<&PickerWindow> {
        self.selected
            .and_then(|p| self.visible.get(p))
            .map(|&i| &self.windows[i])
    }

    pub fn selected_id(&self) -> Option<u32> {
        self.selected_window().map(|w| w.id)
    }

    /// The rail: counts follow the display filter and the query, not the app filter.
    pub fn groups(&self) -> AppGroups {
        app_groups(
            &self.windows,
            &self.displays,
            self.display_filter,
            &self.query,
        )
    }

    /// Nothing to list at all (no capturable window).
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// Windows exist, but the query or a filter hides them all.
    pub fn is_no_match(&self) -> bool {
        !self.windows.is_empty() && self.visible.is_empty()
    }

    pub fn display_of(&self, w: &PickerWindow) -> Option<&PickerDisplay> {
        w.display_id
            .and_then(|id| self.displays.iter().find(|d| d.id == id))
    }

    /// Whether the display filter is worth showing: more than one display.
    pub fn has_several_displays(&self) -> bool {
        self.displays.len() > 1
    }

    pub fn last_used(&self) -> &LastUsed {
        &self.last_used
    }

    // ---- write side ------------------------------------------------------------------------

    fn refresh(&mut self, keep: Option<u32>) {
        self.visible = visible(
            &self.windows,
            &self.displays,
            &self.query,
            &self.app_filter,
            self.display_filter,
            &self.last_used,
        );
        self.selected = keep_selection(keep, &self.windows, &self.visible);
    }

    /// Re-filters; the selected window stays selected while it is still shown.
    fn refilter(&mut self) {
        let keep = self.selected_id();
        self.refresh(keep);
    }

    pub fn set_query(&mut self, query: &str) {
        if self.query == query {
            return;
        }
        self.query = query.to_owned();
        // A query that hides the chosen app leaves the app filter pointing at an app with no
        // row on the rail; fall back to all apps so the list is never empty for that reason.
        if let AppFilter::App(a) = &self.app_filter {
            if !self.groups().groups.iter().any(|g| &g.app == a) {
                self.app_filter = AppFilter::All;
            }
        }
        self.refilter();
    }

    pub fn set_app_filter(&mut self, filter: AppFilter) {
        if self.app_filter == filter {
            return;
        }
        self.app_filter = filter;
        self.refilter();
    }

    pub fn set_display_filter(&mut self, filter: DisplayFilter) {
        if self.display_filter == filter {
            return;
        }
        self.display_filter = filter;
        if let AppFilter::App(a) = &self.app_filter {
            if !self.groups().groups.iter().any(|g| &g.app == a) {
                self.app_filter = AppFilter::All;
            }
        }
        self.refilter();
    }

    pub fn toggle_layout(&mut self) {
        self.list_layout = !self.list_layout;
    }

    pub fn set_list_layout(&mut self, list: bool) {
        self.list_layout = list;
    }

    /// The number of cards per row the view lays out (a narrow window may fit fewer).
    pub fn set_columns(&mut self, columns: usize) {
        self.columns = columns.max(1);
    }

    /// Cmd+1..9: the n-th app of the rail (1 = the first app, not "All windows"). Returns
    /// whether the key did anything. Pressing the number of the app already chosen goes back to
    /// all windows. Cmd+0 is "All windows".
    pub fn select_app_number(&mut self, n: usize) -> bool {
        if n == 0 {
            self.set_app_filter(AppFilter::All);
            return true;
        }
        let groups = self.groups();
        let Some(g) = app_for_shortcut(&groups.groups, n) else {
            return false;
        };
        let filter = AppFilter::App(g.app.clone());
        if self.app_filter == filter {
            self.set_app_filter(AppFilter::All);
        } else {
            self.set_app_filter(filter);
        }
        true
    }

    /// Moves the selection with an arrow key, Home or End.
    pub fn move_selection(&mut self, key: Nav) {
        self.selected = navigate(self.selected, self.visible.len(), self.layout(), key);
    }

    /// Selects the card at this position of the visible list (hover or click).
    pub fn select_pos(&mut self, pos: usize) {
        if pos < self.visible.len() {
            self.selected = Some(pos);
        }
    }

    /// Enter: capture the selected window; nothing happens with no selection.
    pub fn enter(&self) -> Outcome {
        match self.selected_window() {
            Some(w) => Outcome::Capture {
                window_id: w.id,
                app: w.app.clone(),
                title: w.title.clone(),
            },
            None => Outcome::None,
        }
    }

    pub fn escape(&self) -> Outcome {
        Outcome::Cancel
    }

    pub fn tab(&self) -> Outcome {
        Outcome::PickOnScreen
    }

    /// Replaces the windows (a refresh); the selected window stays selected while it exists.
    pub fn replace_windows(&mut self, windows: Vec<PickerWindow>, displays: Vec<PickerDisplay>) {
        let keep = self.selected_id();
        self.windows = windows;
        self.displays = displays;
        self.refresh(keep);
    }

    /// Records a capture in the last-used memory and returns the new memory to persist.
    pub fn touch_last_used(&mut self, app: &str, title: &str) -> &LastUsed {
        self.last_used.touch(app, title);
        &self.last_used
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(id: u32, app: &str, title: &str, display: u32) -> PickerWindow {
        PickerWindow {
            id,
            title: title.into(),
            app: app.into(),
            width: 1000,
            height: 700,
            display_id: Some(display),
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
        ]
    }

    /// Front to back.
    fn sample() -> Vec<PickerWindow> {
        vec![
            w(10, "Safari", "Docs: Q4 launch plan", 1),
            w(11, "Finder", "Documents", 1),
            w(12, "Chrome", "Google Docs: Release notes", 2),
            w(13, "Safari", "Inbox", 2),
            w(14, "Terminal", "zsh", 1),
            w(15, "Chrome", "Pull request", 1),
        ]
    }

    fn state() -> PickerState {
        PickerState::new(sample(), displays(), LastUsed::new(), false)
    }

    fn ids(s: &PickerState) -> Vec<u32> {
        s.visible_windows().map(|w| w.id).collect()
    }

    #[test]
    fn opens_front_to_back_with_the_front_window_selected() {
        let s = state();
        assert_eq!(ids(&s), vec![10, 11, 12, 13, 14, 15]);
        assert_eq!(s.selected_id(), Some(10));
        assert_eq!(s.selected_pos(), Some(0));
        assert_eq!(s.groups().total, 6);
        assert!(!s.is_empty() && !s.is_no_match());
    }

    #[test]
    fn the_last_used_window_comes_first_and_is_selected() {
        let mut lu = LastUsed::new();
        lu.touch("Terminal", "zsh");
        let s = PickerState::new(sample(), displays(), lu, false);
        assert_eq!(ids(&s), vec![14, 10, 11, 12, 13, 15]);
        assert_eq!(s.selected_id(), Some(14));
    }

    #[test]
    fn typing_filters_at_once_and_the_board_example_matches_three() {
        let mut s = state();
        s.set_query("doc");
        assert_eq!(ids(&s), vec![10, 11, 12]);
        assert_eq!(s.visible_len(), 3);
        assert_eq!(s.groups().total, 3);
        // The rail follows the query.
        let apps: Vec<_> = s.groups().groups.into_iter().map(|g| g.app).collect();
        assert_eq!(apps, vec!["Safari", "Finder", "Chrome"]);
    }

    #[test]
    fn the_selection_stays_on_its_window_across_filters() {
        let mut s = state();
        s.move_selection(Nav::Right);
        s.move_selection(Nav::Right); // Google Docs (12)
        assert_eq!(s.selected_id(), Some(12));
        s.set_query("doc");
        assert_eq!(s.selected_id(), Some(12));
        assert_eq!(s.selected_pos(), Some(2));
        s.set_query("");
        assert_eq!(s.selected_id(), Some(12));
        s.set_display_filter(DisplayFilter::BuiltIn); // 12 is on the external display
        assert_eq!(s.selected_id(), Some(10));
        s.set_display_filter(DisplayFilter::All);
        assert_eq!(s.selected_id(), Some(10));
    }

    #[test]
    fn display_filter_built_in_and_external() {
        let mut s = state();
        s.set_display_filter(DisplayFilter::External);
        assert_eq!(ids(&s), vec![12, 13]);
        s.set_display_filter(DisplayFilter::BuiltIn);
        assert_eq!(ids(&s), vec![10, 11, 14, 15]);
        assert_eq!(s.groups().total, 4);
        assert!(s.has_several_displays());
    }

    #[test]
    fn cmd_number_jumps_to_an_app_and_again_goes_back() {
        let mut s = state();
        // Rail order is by front-most window: Safari, Finder, Chrome, Terminal.
        assert!(s.select_app_number(3));
        assert_eq!(s.app_filter(), &AppFilter::App("Chrome".into()));
        assert_eq!(ids(&s), vec![12, 15]);
        assert_eq!(s.selected_id(), Some(12));
        assert!(s.select_app_number(3));
        assert_eq!(s.app_filter(), &AppFilter::All);
        assert!(s.select_app_number(2));
        assert_eq!(ids(&s), vec![11]);
        assert!(s.select_app_number(0));
        assert_eq!(s.app_filter(), &AppFilter::All);
        assert_eq!(s.visible_len(), 6);
    }

    #[test]
    fn cmd_number_past_the_rail_does_nothing() {
        let mut s = state();
        assert!(!s.select_app_number(5));
        assert!(!s.select_app_number(9));
        assert!(!s.select_app_number(10));
        assert_eq!(s.app_filter(), &AppFilter::All);
        assert_eq!(s.visible_len(), 6);
    }

    #[test]
    fn a_query_that_hides_the_chosen_app_falls_back_to_all_apps() {
        let mut s = state();
        s.select_app_number(4); // Terminal
        assert_eq!(ids(&s), vec![14]);
        s.set_query("doc");
        assert_eq!(s.app_filter(), &AppFilter::All);
        assert_eq!(ids(&s), vec![10, 11, 12]);
    }

    #[test]
    fn a_display_filter_that_hides_the_chosen_app_falls_back_too() {
        let mut s = state();
        s.select_app_number(4); // Terminal, on the built-in display
        s.set_display_filter(DisplayFilter::External);
        assert_eq!(s.app_filter(), &AppFilter::All);
        assert_eq!(ids(&s), vec![12, 13]);
    }

    #[test]
    fn grid_navigation_uses_the_laid_out_columns_and_list_ignores_them() {
        let mut s = state();
        s.set_columns(2);
        s.move_selection(Nav::Down);
        assert_eq!(s.selected_pos(), Some(2));
        s.move_selection(Nav::Right);
        assert_eq!(s.selected_pos(), Some(3));
        s.move_selection(Nav::Up);
        assert_eq!(s.selected_pos(), Some(1));
        s.toggle_layout();
        assert!(s.is_list_layout());
        s.move_selection(Nav::Down);
        assert_eq!(s.selected_pos(), Some(2));
        s.move_selection(Nav::Right); // no-op in a list
        assert_eq!(s.selected_pos(), Some(2));
        s.move_selection(Nav::End);
        assert_eq!(s.selected_pos(), Some(5));
        s.move_selection(Nav::Down); // no wrap
        assert_eq!(s.selected_pos(), Some(5));
        s.move_selection(Nav::Home);
        assert_eq!(s.selected_pos(), Some(0));
        s.toggle_layout();
        assert!(!s.is_list_layout());
    }

    #[test]
    fn zero_columns_count_as_one() {
        let mut s = state();
        s.set_columns(0);
        assert_eq!(s.layout(), Layout::Grid { columns: 1 });
    }

    #[test]
    fn selecting_by_position_ignores_a_stale_position() {
        let mut s = state();
        s.select_pos(4);
        assert_eq!(s.selected_id(), Some(14));
        s.select_pos(99);
        assert_eq!(s.selected_id(), Some(14));
    }

    #[test]
    fn enter_captures_the_selection_with_its_identity() {
        let mut s = state();
        s.set_query("inbox");
        assert_eq!(
            s.enter(),
            Outcome::Capture {
                window_id: 13,
                app: "Safari".into(),
                title: "Inbox".into()
            }
        );
    }

    #[test]
    fn escape_cancels_and_tab_hands_over() {
        let s = state();
        assert_eq!(s.escape(), Outcome::Cancel);
        assert_eq!(s.tab(), Outcome::PickOnScreen);
    }

    #[test]
    fn no_match_state_has_no_selection_and_enter_does_nothing() {
        let mut s = state();
        s.set_query("zzzz");
        assert!(s.is_no_match() && !s.is_empty());
        assert_eq!(s.selected_id(), None);
        assert_eq!(s.enter(), Outcome::None);
        s.move_selection(Nav::Down);
        assert_eq!(s.selected_pos(), None);
        // Clearing the query brings the list and a selection back.
        s.set_query("");
        assert_eq!(s.selected_id(), Some(10));
    }

    #[test]
    fn empty_state_has_nothing_to_capture() {
        let mut s = PickerState::new(vec![], vec![], LastUsed::new(), false);
        assert!(s.is_empty() && !s.is_no_match());
        assert_eq!(s.enter(), Outcome::None);
        assert_eq!(s.escape(), Outcome::Cancel);
        assert_eq!(s.tab(), Outcome::PickOnScreen);
        s.move_selection(Nav::End);
        assert_eq!(s.selected_pos(), None);
        assert!(!s.has_several_displays());
    }

    #[test]
    fn a_refresh_keeps_the_selected_window_or_falls_to_the_first() {
        let mut s = state();
        s.select_pos(3); // 13
        let mut fewer = sample();
        fewer.retain(|w| w.id != 10);
        s.replace_windows(fewer.clone(), displays());
        assert_eq!(s.selected_id(), Some(13));
        fewer.retain(|w| w.id != 13);
        s.replace_windows(fewer, displays());
        assert_eq!(s.selected_id(), Some(11));
    }

    #[test]
    fn touching_last_used_puts_that_window_first_next_time() {
        let mut s = state();
        let lu = s.touch_last_used("Finder", "Documents").clone();
        let again = PickerState::new(sample(), displays(), lu, false);
        assert_eq!(again.selected_id(), Some(11));
    }

    #[test]
    fn the_stored_layout_choice_is_the_starting_layout() {
        let s = PickerState::new(sample(), displays(), LastUsed::new(), true);
        assert!(s.is_list_layout());
        assert_eq!(s.layout(), Layout::List);
    }

    #[test]
    fn display_of_finds_the_card_display() {
        let s = state();
        let first = s.windows()[0].clone();
        assert_eq!(s.display_of(&first).map(|d| d.id), Some(1));
        let mut orphan = first;
        orphan.display_id = None;
        assert!(s.display_of(&orphan).is_none());
    }
}
