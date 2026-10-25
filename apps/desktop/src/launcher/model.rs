//! Launcher state without any UI: the filtered list, the selection and the Actions panel
//! (LNC-1 "Search", "Groups", "Actions").

use super::commands::{Command, CommandId, Group, LaunchOption};
use super::search;

/// What a key or click asks the window to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Close the launcher, then run the command (its default behavior).
    Run(CommandId),
    /// Close the launcher, then run the command with a non-default option from its Actions list.
    RunOption(CommandId, LaunchOption),
    /// A disabled command: do nothing except flash its reason (LNC-1.AC5).
    Flash(String),
    /// Nothing to do (empty list, no actions).
    Nothing,
    Close,
    /// Redraw only.
    Changed,
}

/// Group, disabled, rank, recent-use position, default position.
type SortKey = (usize, bool, u8, usize, usize);

/// One entry of the secondary (⌘K) list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionItem {
    pub label: String,
    /// The short hint on the right; the reason when the row is disabled.
    pub hint: Option<String>,
    /// Why the row cannot run now (LNC-1.AC5); `None` when it can.
    pub disabled: Option<String>,
    pub command: CommandId,
    pub option: LaunchOption,
}

impl ActionItem {
    pub fn is_enabled(&self) -> bool {
        self.disabled.is_none()
    }

    /// What activating an enabled row asks for: the default row is the plain `Run`.
    fn outcome(&self) -> Outcome {
        match self.option {
            LaunchOption::Default => Outcome::Run(self.command),
            option => Outcome::RunOption(self.command, option),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LauncherModel {
    commands: Vec<Command>,
    /// Most recently used first (this run).
    recent: Vec<CommandId>,
    query: String,
    /// Indices into `commands`, in display order.
    visible: Vec<usize>,
    selected: usize,
    actions_open: bool,
    action_selected: usize,
}

impl LauncherModel {
    pub fn new(commands: Vec<Command>, recent: Vec<CommandId>) -> Self {
        let mut m = LauncherModel {
            commands,
            recent,
            query: String::new(),
            visible: Vec::new(),
            selected: 0,
            actions_open: false,
            action_selected: 0,
        };
        m.refilter();
        m
    }

    fn refilter(&mut self) {
        let recent = &self.recent;
        let mut ranked: Vec<(usize, SortKey)> = self
            .commands
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let keywords: Vec<&str> = c.keywords.iter().map(String::as_str).collect();
                let r = search::rank(&self.query, &c.title, &keywords)?;
                let group = Group::ORDER.iter().position(|g| *g == c.group).unwrap_or(0);
                let used = recent.iter().position(|x| *x == c.id).unwrap_or(usize::MAX);
                // Group, enabled before disabled, rank, recent use, default order.
                Some((i, (group, !c.is_enabled(), r, used, i)))
            })
            .collect();
        ranked.sort_by_key(|(_, k)| *k);
        self.visible = ranked.into_iter().map(|(i, _)| i).collect();
        self.selected = 0;
        self.actions_open = false;
        self.action_selected = 0;
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// Typing replaces the list and selects the first match (LNC-1.AC2).
    pub fn set_query(&mut self, q: &str) {
        if q != self.query {
            self.query = q.to_owned();
            self.refilter();
        }
    }

    /// Visible commands in display order.
    pub fn visible(&self) -> impl Iterator<Item = &Command> {
        self.visible.iter().map(|i| &self.commands[*i])
    }

    pub fn visible_len(&self) -> usize {
        self.visible.len()
    }

    pub fn selected_index(&self) -> Option<usize> {
        (!self.visible.is_empty()).then_some(self.selected)
    }

    pub fn selected(&self) -> Option<&Command> {
        self.visible.get(self.selected).map(|i| &self.commands[*i])
    }

    /// Groups that have at least one visible command, in group order (empty groups hidden).
    pub fn groups(&self) -> Vec<Group> {
        Group::ORDER
            .into_iter()
            .filter(|g| self.visible().any(|c| c.group == *g))
            .collect()
    }

    /// ↑/↓ wrap around the list, or move inside the Actions list.
    pub fn move_selection(&mut self, delta: i32) {
        let len = if self.actions_open {
            self.actions().len()
        } else {
            self.visible.len()
        };
        if len == 0 {
            return;
        }
        let cur = if self.actions_open {
            self.action_selected
        } else {
            self.selected
        };
        let next = (cur as i64 + i64::from(delta)).rem_euclid(len as i64) as usize;
        if self.actions_open {
            self.action_selected = next;
        } else {
            self.selected = next;
        }
    }

    /// Mouse hover selects a row.
    pub fn select(&mut self, index: usize) {
        if index < self.visible.len() && !self.actions_open {
            self.selected = index;
        }
    }

    /// The options for the selected command: the record and screenshot commands list their
    /// variants (first the default), the others just "Run". Disabled commands have none.
    pub fn actions(&self) -> Vec<ActionItem> {
        match self.selected() {
            Some(c) if c.is_enabled() => c
                .options
                .iter()
                .map(|o| ActionItem {
                    label: o.label.clone(),
                    hint: o.disabled.clone().or_else(|| o.hint.clone()),
                    disabled: o.disabled.clone(),
                    command: c.id,
                    option: o.option,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Mouse hover or click on a row of the open Actions list.
    pub fn select_action(&mut self, index: usize) {
        if self.actions_open && index < self.actions().len() {
            self.action_selected = index;
        }
    }

    pub fn actions_open(&self) -> bool {
        self.actions_open
    }

    pub fn action_selected(&self) -> usize {
        self.action_selected
    }

    /// ⌘K / Ctrl+K toggles the Actions list for the selected command.
    pub fn toggle_actions(&mut self) -> Outcome {
        if self.actions_open {
            self.actions_open = false;
            return Outcome::Changed;
        }
        if self.actions().is_empty() {
            return Outcome::Nothing;
        }
        self.actions_open = true;
        self.action_selected = 0;
        Outcome::Changed
    }

    /// ↵ (or a click on the selected row).
    pub fn activate(&mut self) -> Outcome {
        if self.actions_open {
            return match self.actions().get(self.action_selected) {
                Some(a) => match &a.disabled {
                    Some(reason) => Outcome::Flash(reason.clone()),
                    None => a.outcome(),
                },
                None => Outcome::Nothing,
            };
        }
        match self.selected() {
            None => Outcome::Nothing,
            Some(c) => match &c.disabled {
                Some(reason) => Outcome::Flash(reason.clone()),
                None => Outcome::Run(c.id),
            },
        }
    }

    /// Esc leaves the Actions list first, then closes the launcher.
    pub fn escape(&mut self) -> Outcome {
        if self.actions_open {
            self.actions_open = false;
            Outcome::Changed
        } else {
            Outcome::Close
        }
    }
}

/// Moves `id` to the front of the recent list (max 8).
pub fn remember(recent: &mut Vec<CommandId>, id: CommandId) {
    recent.retain(|x| *x != id);
    recent.insert(0, id);
    recent.truncate(8);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_status::derive;
    use crate::launcher::commands::{build, Env};
    use crate::shortcuts::{Platform, ShortcutMap};

    fn model(recent: Vec<CommandId>) -> LauncherModel {
        model_with(recent, true, false, None)
    }

    fn model_with(
        recent: Vec<CommandId>,
        copy_after_capture: bool,
        fps60_ready: bool,
        recording_off: Option<&str>,
    ) -> LauncherModel {
        let map = ShortcutMap::defaults(Platform::Mac);
        let mcp = derive(true, true, None, &[]);
        let cmds = build(&Env {
            platform: Platform::Mac,
            shortcuts: &map,
            mcp: &mcp,
            capture_available: true,
            copy_after_capture,
            recording_off: recording_off.map(str::to_owned),
            fps60_ready,
        });
        LauncherModel::new(cmds, recent)
    }

    fn labels(m: &LauncherModel) -> Vec<String> {
        m.actions().iter().map(|a| a.label.clone()).collect()
    }

    /// Opens the Actions list of the command found by `query` and activates row `index`.
    fn pick(m: &mut LauncherModel, query: &str, index: usize) -> Outcome {
        m.set_query(query);
        assert_eq!(m.toggle_actions(), Outcome::Changed);
        for _ in 0..index {
            m.move_selection(1);
        }
        m.activate()
    }

    fn ids(m: &LauncherModel) -> Vec<CommandId> {
        m.visible().map(|c| c.id).collect()
    }

    /// LNC-1: an empty query shows every command in group order, enabled first within a group.
    #[test]
    fn empty_query_shows_all_in_group_order() {
        let m = model(vec![]);
        assert_eq!(m.visible_len(), 11);
        assert_eq!(m.groups(), Group::ORDER.to_vec());
        let v = ids(&m);
        // Capture: the recording commands lead (kit order), all enabled; Publish is disabled.
        assert_eq!(v[0], CommandId::RecordScreen);
        assert_eq!(v[1], CommandId::RecordRegion);
        assert_eq!(v[2], CommandId::ScreenshotRegion);
        assert_eq!(v[7], CommandId::OpenPermissions);
        assert_eq!(v[10], CommandId::CopyMcpConfig);
    }

    /// LNC-1.AC2: typing "rec" keeps only matching commands and selects the first.
    #[test]
    fn typing_rec_filters_and_selects_first() {
        let mut m = model(vec![]);
        m.move_selection(3);
        m.set_query("rec");
        let v = ids(&m);
        // The record commands lead (title prefix); looser matches follow in later groups, and
        // commands that do not match at all are gone.
        assert_eq!(v[..2], [CommandId::RecordScreen, CommandId::RecordRegion]);
        assert!(!v.contains(&CommandId::ScreenshotRegion));
        assert!(!v.contains(&CommandId::OpenLibrary));
        assert_eq!(m.selected().unwrap().id, CommandId::RecordScreen);
        assert_eq!(m.groups()[0], Group::Capture);
    }

    /// LNC-1 Groups: empty groups are hidden.
    #[test]
    fn empty_groups_are_hidden() {
        let mut m = model(vec![]);
        m.set_query("mcp");
        assert_eq!(m.groups(), vec![Group::Agents]);
        m.set_query("zzzz");
        assert!(m.groups().is_empty());
        assert!(m.selected().is_none());
        assert_eq!(m.activate(), Outcome::Nothing);
    }

    /// LNC-1.AC2: up and down move and wrap.
    #[test]
    fn arrows_wrap() {
        let mut m = model(vec![]);
        m.move_selection(-1);
        assert_eq!(m.selected().unwrap().id, CommandId::CopyMcpConfig);
        m.move_selection(1);
        assert_eq!(m.selected().unwrap().id, CommandId::RecordScreen);
        m.move_selection(1);
        assert_eq!(m.selected().unwrap().id, CommandId::RecordRegion);
    }

    /// LNC-1 Groups: ties are ordered by recent use, then default order.
    #[test]
    fn recent_use_breaks_ties() {
        let mut m = model(vec![CommandId::ScreenshotDisplay]);
        m.set_query("screenshot");
        assert_eq!(m.selected().unwrap().id, CommandId::ScreenshotDisplay);
        let m = model(vec![]);
        assert_eq!(m.selected().unwrap().id, CommandId::RecordScreen);
    }

    /// LNC-1.AC5: Enter on a disabled command runs nothing and flashes the reason.
    #[test]
    fn enter_on_disabled_flashes() {
        let mut m = model(vec![]);
        m.set_query("publish");
        assert_eq!(m.activate(), Outcome::Flash("Not available yet".into()));
        assert!(m.actions().is_empty());
        assert_eq!(m.toggle_actions(), Outcome::Nothing);
    }

    #[test]
    fn enter_runs_enabled() {
        let mut m = model(vec![]);
        m.set_query("library");
        assert_eq!(m.activate(), Outcome::Run(CommandId::OpenLibrary));
    }

    /// LNC-1 Actions: toggles with Cmd+K, Esc leaves it before closing.
    #[test]
    fn actions_panel_and_escape() {
        let mut m = model(vec![]);
        assert_eq!(m.toggle_actions(), Outcome::Changed);
        assert!(m.actions_open());
        assert_eq!(m.activate(), Outcome::Run(CommandId::RecordScreen));
        assert_eq!(m.escape(), Outcome::Changed);
        assert!(!m.actions_open());
        assert_eq!(m.escape(), Outcome::Close);
    }

    /// LNC-1 Actions: "Record screen + webcam" lists its variants; the first row is the default
    /// and does exactly what ↵ in the main list does.
    #[test]
    fn lnc1_record_screen_actions() {
        let mut m = model(vec![]);
        assert_eq!(m.selected().unwrap().id, CommandId::RecordScreen);
        let main_enter = m.activate();
        assert_eq!(main_enter, Outcome::Run(CommandId::RecordScreen));
        m.toggle_actions();
        assert_eq!(
            labels(&m),
            [
                "Start recording",
                "Without webcam",
                "Screen + microphone only",
                "Screen only (no audio)",
                "Record at 60 fps",
                "Start in 5 s"
            ]
        );
        assert_eq!(
            m.activate(),
            main_enter,
            "the default row is the main list's ↵"
        );
        let mut m = model(vec![]);
        for (i, opt) in [
            (1, LaunchOption::NoWebcam),
            (2, LaunchOption::MicOnly),
            (3, LaunchOption::ScreenOnly),
            (5, LaunchOption::Countdown5),
        ] {
            assert_eq!(
                pick(&mut m, "record screen", i),
                Outcome::RunOption(CommandId::RecordScreen, opt),
                "row {i}"
            );
            m.set_query("");
        }
    }

    /// LNC-1.AC5 for Actions: 60 fps is disabled with a reason until a self-test passed, and ↵ on
    /// it runs nothing and flashes the reason; once a test passed it runs.
    #[test]
    fn lnc1_60_fps_option_follows_the_self_test() {
        let mut m = model_with(vec![], true, false, None);
        m.set_query("record screen");
        m.toggle_actions();
        let fps = m
            .actions()
            .into_iter()
            .find(|a| a.option == LaunchOption::Fps60)
            .unwrap();
        assert_eq!(
            fps.disabled.as_deref(),
            Some("Run the 60 fps test in Settings first")
        );
        assert_eq!(fps.hint, fps.disabled, "the reason is the row's hint");
        let mut m = model_with(vec![], true, false, None);
        assert_eq!(
            pick(&mut m, "record screen", 4),
            Outcome::Flash("Run the 60 fps test in Settings first".into())
        );
        let mut m = model_with(vec![], true, true, None);
        assert_eq!(
            pick(&mut m, "record screen", 4),
            Outcome::RunOption(CommandId::RecordScreen, LaunchOption::Fps60)
        );
    }

    /// LNC-1 Actions: "Record region" offers the webcam and no-audio variants.
    #[test]
    fn lnc1_record_region_actions() {
        let mut m = model(vec![]);
        m.set_query("record region");
        m.toggle_actions();
        assert_eq!(
            labels(&m),
            ["Start recording", "Include webcam", "Without audio"]
        );
        assert_eq!(m.activate(), Outcome::Run(CommandId::RecordRegion));
        let mut m = model(vec![]);
        assert_eq!(
            pick(&mut m, "record region", 1),
            Outcome::RunOption(CommandId::RecordRegion, LaunchOption::WithWebcam)
        );
        let mut m = model(vec![]);
        assert_eq!(
            pick(&mut m, "record region", 2),
            Outcome::RunOption(CommandId::RecordRegion, LaunchOption::NoAudio)
        );
    }

    /// LNC-1 Actions: the three screenshot commands share the option list; the second row
    /// depends on whether copy-after-capture is on, "copy only" is disabled with a reason, and
    /// the delay option runs.
    #[test]
    fn lnc1_screenshot_actions() {
        for q in [
            "screenshot region",
            "screenshot window",
            "screenshot display",
        ] {
            let mut m = model_with(vec![], true, false, None);
            m.set_query(q);
            m.toggle_actions();
            assert_eq!(
                labels(&m),
                [
                    "Take screenshot",
                    "Save only (don't copy)",
                    "Copy only (don't save)",
                    "With 5 s delay"
                ],
                "{q}"
            );
            let id = m.selected().unwrap().id;
            assert_eq!(m.activate(), Outcome::Run(id));
            let mut m2 = model_with(vec![], true, false, None);
            assert_eq!(
                pick(&mut m2, q, 1),
                Outcome::RunOption(id, LaunchOption::SaveOnly)
            );
            let mut m2 = model_with(vec![], true, false, None);
            assert!(
                matches!(pick(&mut m2, q, 2), Outcome::Flash(r) if r.contains("Not available yet"))
            );
            let mut m2 = model_with(vec![], true, false, None);
            assert_eq!(
                pick(&mut m2, q, 3),
                Outcome::RunOption(id, LaunchOption::Delay5)
            );
        }
        // With copy-after-capture off the second row adds the copy instead.
        let mut m = model_with(vec![], false, false, None);
        m.set_query("screenshot display");
        m.toggle_actions();
        assert_eq!(labels(&m)[1], "Copy and save");
        let id = CommandId::ScreenshotDisplay;
        let mut m = model_with(vec![], false, false, None);
        assert_eq!(
            pick(&mut m, "screenshot display", 1),
            Outcome::RunOption(id, LaunchOption::CopyAndSave)
        );
    }

    /// LNC-1 Actions: commands with a single behavior show only "Run", and a disabled command
    /// (here: recording is off) has no Actions at all.
    #[test]
    fn lnc1_other_commands_keep_a_single_run() {
        for (q, id) in [
            ("library", CommandId::OpenLibrary),
            ("settings", CommandId::OpenSettings),
            ("mcp", CommandId::CopyMcpConfig),
        ] {
            let mut m = model(vec![]);
            m.set_query(q);
            m.toggle_actions();
            assert_eq!(labels(&m), ["Run"], "{q}");
            assert_eq!(m.activate(), Outcome::Run(id));
        }
        let mut m = model_with(vec![], true, true, Some("A recording is running"));
        m.set_query("webcam"); // only "Record screen + webcam" has this keyword
        assert_eq!(m.selected().unwrap().id, CommandId::RecordScreen);
        assert!(m.actions().is_empty());
        assert_eq!(m.toggle_actions(), Outcome::Nothing);
        assert_eq!(
            m.activate(),
            Outcome::Flash("A recording is running".into())
        );
    }

    /// LNC-1 Actions: ↑/↓ move inside the list and wrap, Esc leaves the list first and keeps the
    /// command selected, then closes.
    #[test]
    fn lnc1_actions_navigation_and_escape() {
        let mut m = model(vec![]);
        m.toggle_actions();
        m.move_selection(-1);
        assert_eq!(m.action_selected(), 5, "wraps to the last row");
        m.move_selection(1);
        assert_eq!(m.action_selected(), 0);
        m.move_selection(2);
        assert_eq!(m.escape(), Outcome::Changed);
        assert_eq!(m.selected().unwrap().id, CommandId::RecordScreen);
        assert_eq!(m.toggle_actions(), Outcome::Changed);
        assert_eq!(
            m.action_selected(),
            0,
            "reopening starts on the default row"
        );
        assert_eq!(m.escape(), Outcome::Changed);
        assert_eq!(m.escape(), Outcome::Close);
    }

    #[test]
    fn hover_selects() {
        let mut m = model(vec![]);
        m.select(2);
        assert_eq!(m.selected_index(), Some(2));
        m.select(99);
        assert_eq!(m.selected_index(), Some(2));
    }

    #[test]
    fn remember_is_most_recent_first_and_bounded() {
        let mut r = vec![];
        remember(&mut r, CommandId::OpenLibrary);
        remember(&mut r, CommandId::OpenSettings);
        remember(&mut r, CommandId::OpenLibrary);
        assert_eq!(r, vec![CommandId::OpenLibrary, CommandId::OpenSettings]);
    }
}
