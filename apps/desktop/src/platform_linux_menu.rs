//! The Linux tray menu as plain data (CAP-2): the same items, order and enabled states as the
//! menu `platform::build_tray` makes with `tray-icon` on macOS and Windows, but described as a
//! tree that the StatusNotifierItem backend (`platform_linux`) serves over `com.canonical.dbusmenu`.
//! No D-Bus here, so the tests run on every OS.

use crate::platform::{ids, menu_text, recording_group, recording_group_start, TrayState};

/// One item of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuNode {
    /// The dbusmenu id (stable per item, see [`item_id`]).
    pub id: i32,
    /// The action id the app routes (`platform::ids`, `recent:<capture>`, `delay:<secs>`); empty
    /// for separators and submenu parents that do nothing by themselves.
    pub key: String,
    pub label: String,
    pub kind: Kind,
    pub enabled: bool,
    pub children: Vec<MenuNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Normal,
    Separator,
    /// A check mark item (the capture delay choices).
    Check(bool),
    Submenu,
}

/// What the menu depends on. The tray handle keeps one and rebuilds the tree when it changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuInputs {
    pub icon: TrayState,
    pub tooltip: String,
    /// A take is on screen: Stop leads the menu.
    pub recording: bool,
    pub record_enabled: bool,
    pub stop_enabled: bool,
    pub pause_enabled: bool,
    pub pause_label: String,
    /// Selected capture delay in seconds (CAP-5).
    pub delay_s: u32,
    /// `(capture id, title)`, newest first.
    pub recents: Vec<(String, String)>,
}

impl Default for MenuInputs {
    fn default() -> Self {
        Self {
            icon: TrayState::Idle,
            tooltip: "CaptureMann".to_owned(), // i18n-ignore: the product name
            recording: false,
            record_enabled: true,
            stop_enabled: false,
            pause_enabled: false,
            pause_label: menu_text(ids::REC_PAUSE),
            delay_s: 0,
            recents: Vec::new(),
        }
    }
}

/// The stable dbusmenu id of an action id. Ids never change while the app runs, so a click that
/// arrives just after a rebuild still names the item the user saw.
pub fn item_id(key: &str) -> i32 {
    match key {
        "root" => 0,
        ids::CAPTURE_REGION => 1,
        ids::CAPTURE_WINDOW => 2,
        ids::CAPTURE_DISPLAY => 3,
        "delay" => 4,
        ids::RECORD => 5,
        ids::RECORD_AUDIO => 6,
        ids::REC_PAUSE => 7,
        ids::REC_STOP => 8,
        ids::OPEN_LIBRARY => 9,
        ids::OPEN_LAUNCHER => 10,
        "recent" => 11,
        ids::SETTINGS => 12,
        ids::PERMISSIONS => 13,
        ids::QUIT => 14,
        _ => -1,
    }
}

const SEPARATOR_BASE: i32 = 50;
const DELAY_BASE: i32 = 100;
const RECENT_BASE: i32 = 200;
/// "Recent captures" shows at most this many entries.
pub const MAX_RECENTS: usize = 5;

fn node(key: &str, enabled: bool) -> MenuNode {
    MenuNode {
        id: item_id(key),
        key: key.to_owned(),
        label: menu_text(key),
        kind: Kind::Normal,
        enabled,
        children: Vec::new(),
    }
}

fn separator(n: i32) -> MenuNode {
    MenuNode {
        id: SEPARATOR_BASE + n,
        key: String::new(),
        label: String::new(),
        kind: Kind::Separator,
        enabled: true,
        children: Vec::new(),
    }
}

/// The whole menu, in the order of `platform::build_tray`.
pub fn build_menu(inp: &MenuInputs) -> Vec<MenuNode> {
    let delays = crate::delay::CHOICES
        .iter()
        .enumerate()
        .map(|(i, &secs)| MenuNode {
            id: DELAY_BASE + i as i32,
            key: crate::delay::menu_id(secs),
            label: crate::delay::choice_label(secs),
            kind: Kind::Check(secs == inp.delay_s),
            enabled: true,
            children: Vec::new(),
        })
        .collect();
    let delay = MenuNode {
        kind: Kind::Submenu,
        children: delays,
        ..node("delay", true)
    };
    let recent_children = if inp.recents.is_empty() {
        vec![MenuNode {
            id: RECENT_BASE - 1,
            key: String::new(),
            label: cm_i18n::tr!("No captures yet"),
            kind: Kind::Normal,
            enabled: false,
            children: Vec::new(),
        }]
    } else {
        inp.recents
            .iter()
            .take(MAX_RECENTS)
            .enumerate()
            .map(|(i, (id, title))| MenuNode {
                id: RECENT_BASE + i as i32,
                key: format!("recent:{id}"),
                label: title.clone(),
                kind: Kind::Normal,
                enabled: true,
                children: Vec::new(),
            })
            .collect()
    };
    let recent = MenuNode {
        kind: Kind::Submenu,
        children: recent_children,
        ..node("recent", true)
    };

    let group = |key: &str| {
        let mut n = node(key, true);
        match key {
            ids::RECORD | ids::RECORD_AUDIO => n.enabled = inp.record_enabled,
            ids::REC_STOP => n.enabled = inp.stop_enabled,
            ids::REC_PAUSE => {
                n.enabled = inp.pause_enabled;
                n.label = inp.pause_label.clone();
            }
            _ => {}
        }
        n
    };

    let mut items = vec![
        node(ids::CAPTURE_REGION, true),
        node(ids::CAPTURE_WINDOW, true),
        node(ids::CAPTURE_DISPLAY, true),
        delay,
    ];
    let at = recording_group_start(inp.recording);
    for (i, key) in recording_group(inp.recording).into_iter().enumerate() {
        items.insert(at + i, group(key));
    }
    items.extend([
        separator(0),
        node(ids::OPEN_LIBRARY, true),
        node(ids::OPEN_LAUNCHER, true),
        recent,
        separator(1),
        node(ids::SETTINGS, true),
        node(ids::PERMISSIONS, true),
        separator(2),
        node(ids::QUIT, true),
    ]);
    items
}

/// Finds the node with a dbusmenu id (depth first).
pub fn find(items: &[MenuNode], id: i32) -> Option<&MenuNode> {
    for n in items {
        if n.id == id {
            return Some(n);
        }
        if let Some(c) = find(&n.children, id) {
            return Some(c);
        }
    }
    None
}

/// The action id a click on dbusmenu item `id` routes to, or `None` for separators, disabled
/// items, submenu parents and unknown ids.
pub fn action_for(items: &[MenuNode], id: i32) -> Option<String> {
    let n = find(items, id)?;
    if !n.enabled || n.key.is_empty() || matches!(n.kind, Kind::Separator | Kind::Submenu) {
        return None;
    }
    Some(n.key.clone())
}

/// A property value of a dbusmenu item before it is turned into a D-Bus variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prop {
    Str(String),
    Bool(bool),
    Int(i32),
}

/// The dbusmenu properties of a node (only those that differ from the protocol defaults, plus
/// `label`). `names` limits them when the host asked for specific ones (empty = all).
pub fn props(n: &MenuNode, names: &[String]) -> Vec<(&'static str, Prop)> {
    let mut all: Vec<(&'static str, Prop)> = Vec::new();
    match n.kind {
        Kind::Separator => all.push(("type", Prop::Str("separator".into()))),
        Kind::Normal | Kind::Check(_) | Kind::Submenu => {
            all.push(("label", Prop::Str(n.label.clone())));
            if !n.enabled {
                all.push(("enabled", Prop::Bool(false)));
            }
        }
    }
    if let Kind::Check(on) = n.kind {
        all.push(("toggle-type", Prop::Str("checkmark".into())));
        all.push(("toggle-state", Prop::Int(i32::from(on))));
    }
    if n.kind == Kind::Submenu {
        all.push(("children-display", Prop::Str("submenu".into())));
    }
    if names.is_empty() {
        all
    } else {
        all.into_iter()
            .filter(|(k, _)| names.iter().any(|w| w == k))
            .collect()
    }
}

/// Premultiplication-free ARGB32, network byte order, as StatusNotifierItem `IconPixmap` wants.
pub fn rgba_to_argb(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for p in rgba.as_chunks::<4>().0 {
        out.extend_from_slice(&[p[3], p[0], p[1], p[2]]);
    }
    out
}

/// The `Status` property of the item: Attention asks the host to draw attention.
pub fn sni_status(state: TrayState) -> &'static str {
    if state == TrayState::Attention {
        "NeedsAttention"
    } else {
        "Active"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(items: &[MenuNode]) -> Vec<&str> {
        items.iter().map(|n| n.key.as_str()).collect()
    }

    /// CAP-2: the idle menu has the same entries, in the same order, as the macOS/Windows tray.
    #[test]
    fn idle_menu_matches_the_native_tray() {
        let m = build_menu(&MenuInputs::default());
        assert_eq!(
            keys(&m),
            [
                "capture_region",
                "capture_window",
                "capture_display",
                "delay",
                "record",
                "record_audio",
                "rec_pause",
                "rec_stop",
                "",
                "open_library",
                "open_launcher",
                "recent",
                "",
                "settings",
                "permissions",
                "",
                "quit"
            ]
        );
        assert_eq!(m[8].kind, Kind::Separator);
    }

    /// CAP-2.AC4: while recording, Stop is the first item.
    #[test]
    fn cap2_ac4_stop_leads_while_recording() {
        let m = build_menu(&MenuInputs {
            recording: true,
            record_enabled: false,
            stop_enabled: true,
            pause_enabled: true,
            ..MenuInputs::default()
        });
        assert_eq!(
            keys(&m)[..8],
            [
                "rec_stop",
                "rec_pause",
                "record",
                "record_audio",
                "capture_region",
                "capture_window",
                "capture_display",
                "delay"
            ]
        );
        assert!(m[0].enabled && m[1].enabled && !m[2].enabled && !m[3].enabled);
    }

    #[test]
    fn idle_disables_stop_and_pause_and_resume_relabels() {
        let m = build_menu(&MenuInputs::default());
        let by = |k: &str| m.iter().find(|n| n.key == k).unwrap();
        assert!(!by("rec_stop").enabled && !by("rec_pause").enabled && by("record").enabled);
        let m = build_menu(&MenuInputs {
            pause_enabled: true,
            pause_label: "Resume recording".into(),
            ..MenuInputs::default()
        });
        let p = m.iter().find(|n| n.key == "rec_pause").unwrap();
        assert_eq!(p.label, "Resume recording");
        assert!(p.enabled);
    }

    #[test]
    fn ids_are_unique_and_stable() {
        let all = |inp: &MenuInputs| {
            let mut v = Vec::new();
            fn walk(items: &[MenuNode], v: &mut Vec<i32>) {
                for n in items {
                    v.push(n.id);
                    walk(&n.children, v);
                }
            }
            walk(&build_menu(inp), &mut v);
            v
        };
        let a = all(&MenuInputs::default());
        let mut s = a.clone();
        s.sort_unstable();
        s.dedup();
        assert_eq!(s.len(), a.len(), "unique: {a:?}");
        assert!(a.iter().all(|&i| i > 0));
        // Recording moves items but does not renumber them.
        let b = build_menu(&MenuInputs {
            recording: true,
            ..MenuInputs::default()
        });
        for key in [ids::RECORD, ids::REC_STOP, ids::QUIT] {
            assert_eq!(find(&b, item_id(key)).unwrap().key, key);
        }
    }

    #[test]
    fn delay_has_one_check_and_routes_like_the_tray() {
        let m = build_menu(&MenuInputs {
            delay_s: 5,
            ..MenuInputs::default()
        });
        let delay = m.iter().find(|n| n.key == "delay").unwrap();
        assert_eq!(delay.kind, Kind::Submenu);
        let on: Vec<_> = delay
            .children
            .iter()
            .filter(|c| c.kind == Kind::Check(true))
            .collect();
        assert_eq!(on.len(), 1);
        assert_eq!(crate::delay::from_menu_id(&on[0].key), Some(5));
        let three = delay
            .children
            .iter()
            .find(|c| crate::delay::from_menu_id(&c.key) == Some(3))
            .unwrap();
        assert_eq!(action_for(&m, three.id), Some(three.key.clone()));
    }

    #[test]
    fn recents_are_capped_and_route_with_the_recent_prefix() {
        let recents: Vec<_> = (0..8)
            .map(|i| (format!("c{i}"), format!("Shot {i}")))
            .collect();
        let m = build_menu(&MenuInputs {
            recents,
            ..MenuInputs::default()
        });
        let r = m.iter().find(|n| n.key == "recent").unwrap();
        assert_eq!(r.children.len(), MAX_RECENTS);
        assert_eq!(
            action_for(&m, r.children[1].id).as_deref(),
            Some("recent:c1")
        );
        // Empty: one disabled placeholder that routes nowhere.
        let m = build_menu(&MenuInputs::default());
        let r = m.iter().find(|n| n.key == "recent").unwrap();
        assert_eq!(r.children.len(), 1);
        assert!(!r.children[0].enabled);
        assert_eq!(action_for(&m, r.children[0].id), None);
    }

    #[test]
    fn clicks_route_only_to_enabled_actions() {
        let m = build_menu(&MenuInputs::default());
        assert_eq!(
            action_for(&m, item_id(ids::CAPTURE_REGION)).as_deref(),
            Some("capture_region")
        );
        assert_eq!(action_for(&m, item_id(ids::QUIT)).as_deref(), Some("quit"));
        assert_eq!(action_for(&m, item_id(ids::REC_STOP)), None, "disabled");
        assert_eq!(action_for(&m, item_id("delay")), None, "submenu parent");
        assert_eq!(action_for(&m, SEPARATOR_BASE), None);
        assert_eq!(action_for(&m, 9999), None);
    }

    /// Every action the app routes (`Command::TrayMenu`) can be reached from this menu.
    #[test]
    fn every_native_tray_action_is_present() {
        let m = build_menu(&MenuInputs::default());
        for key in [
            ids::CAPTURE_REGION,
            ids::CAPTURE_WINDOW,
            ids::CAPTURE_DISPLAY,
            ids::RECORD,
            ids::RECORD_AUDIO,
            ids::REC_PAUSE,
            ids::REC_STOP,
            ids::OPEN_LIBRARY,
            ids::OPEN_LAUNCHER,
            ids::SETTINGS,
            ids::PERMISSIONS,
            ids::QUIT,
        ] {
            let n = find(&m, item_id(key)).unwrap_or_else(|| panic!("{key} missing"));
            assert_eq!(n.key, key);
        }
    }

    #[test]
    fn dbusmenu_props() {
        let m = build_menu(&MenuInputs {
            delay_s: 3,
            ..MenuInputs::default()
        });
        let stop = find(&m, item_id(ids::REC_STOP)).unwrap();
        let p = props(stop, &[]);
        assert!(p.contains(&("enabled", Prop::Bool(false))));
        assert!(p.iter().any(|(k, _)| *k == "label"));
        assert_eq!(props(stop, &["label".to_owned()]).len(), 1);
        let sep = &m[8];
        assert_eq!(props(sep, &[]), [("type", Prop::Str("separator".into()))]);
        let three = find(&m, DELAY_BASE + 1).unwrap();
        let p = props(three, &[]);
        assert!(p.contains(&("toggle-type", Prop::Str("checkmark".into()))));
        assert!(p.contains(&("toggle-state", Prop::Int(1))));
        let d = find(&m, item_id("delay")).unwrap();
        assert!(props(d, &[]).contains(&("children-display", Prop::Str("submenu".into()))));
    }

    #[test]
    fn argb_and_status() {
        assert_eq!(
            rgba_to_argb(&[1, 2, 3, 4, 5, 6, 7, 8]),
            [4, 1, 2, 3, 8, 5, 6, 7]
        );
        assert_eq!(sni_status(TrayState::Attention), "NeedsAttention");
        assert_eq!(sni_status(TrayState::Recording), "Active");
    }
}
