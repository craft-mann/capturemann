//! The launcher's command registry (LNC-1 "Commands"): which commands exist, their live hints,
//! the user's current shortcuts and whether each one can run in this build.

use cm_i18n::{tr, tr_ctx};

use crate::agent_status::McpStatus;
use crate::shortcuts::{Platform, ShortcutAction, ShortcutMap};

/// Delay between closing the launcher and starting a capture, so the closing animation is
/// gone from the screen before the overlay freezes it (LNC-1.AC6).
pub const CAPTURE_DELAY_MS: u64 = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandId {
    RecordScreen,
    RecordRegion,
    /// "Record audio" (VOC-1): opens the start sheet in Audio mode.
    RecordAudio,
    ScreenshotRegion,
    ScreenshotWindow,
    ScreenshotDisplay,
    OpenLibrary,
    OpenSettings,
    OpenPermissions,
    PublishLast,
    SaveToDrive,
    CopyMcpConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Capture,
    Publish,
    Agents,
}

impl Group {
    pub const ORDER: [Group; 3] = [Group::Capture, Group::Publish, Group::Agents];

    pub fn label(self) -> String {
        match self {
            Group::Capture => tr_ctx!("launcher group", "Capture"),
            Group::Publish => tr_ctx!("launcher group", "Publish"),
            Group::Agents => tr_ctx!("launcher group", "Agents"),
        }
    }
}

/// Icon tile glyph (assets under `icons/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Record,
    /// The microphone (Record audio).
    Mic,
    Frame,
    Window,
    Monitor,
    Library,
    Settings,
    Upload,
    Cloud,
    Terminal,
    Permissions,
}

/// The right-aligned hint of a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hint {
    None,
    Text(String),
    /// Agent-blue status with a dot (MCP status).
    Agent(String),
}

/// What a command does when it is picked from its Actions list (⌘K, LNC-1 Actions).
/// [`LaunchOption::Default`] is what ↵ does in the main list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaunchOption {
    Default,
    // Record screen + webcam
    NoWebcam,
    MicOnly,
    ScreenOnly,
    Fps60,
    Countdown5,
    // Record region
    WithWebcam,
    NoAudio,
    // Screenshots
    SaveOnly,
    CopyAndSave,
    CopyOnly,
    Delay5,
}

/// One row of a command's Actions list. The first row is always [`LaunchOption::Default`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionItem {
    pub option: LaunchOption,
    pub label: String,
    pub hint: Option<String>,
    /// Why this row cannot run right now (shown as its hint; ↵ flashes it, LNC-1.AC5).
    pub disabled: Option<String>,
}

impl OptionItem {
    fn new(option: LaunchOption, label: impl Into<String>, hint: impl Into<String>) -> Self {
        OptionItem {
            option,
            label: label.into(),
            hint: Some(hint.into()),
            disabled: None,
        }
    }

    fn off(mut self, reason: Option<String>) -> Self {
        self.disabled = reason;
        self
    }
}

/// The seconds of the "With 5 s delay" screenshot option and "Start in 5 s" recording option.
pub const OPTION_DELAY_S: u32 = 5;

/// A command's Actions list (LNC-1 Actions). Only the record and screenshot commands have
/// options beyond the single default; the rest show just "Run". The commands themselves are
/// disabled as a whole while recording or capture cannot run, so these rows only describe
/// what is possible once they are enabled.
fn options_for(id: CommandId, verb: &str, env: &Env<'_>) -> Vec<OptionItem> {
    use LaunchOption as O;
    match id {
        CommandId::RecordScreen => vec![
            OptionItem::new(O::Default, verb, tr!("Webcam on, your saved audio choices")),
            OptionItem::new(
                O::NoWebcam,
                tr!("Without webcam"),
                tr!("Screen and your saved audio"),
            ),
            OptionItem::new(
                O::MicOnly,
                tr!("Screen + microphone only"),
                tr!("No webcam, no system audio"),
            ),
            OptionItem::new(
                O::ScreenOnly,
                tr!("Screen only (no audio)"),
                tr!("No webcam, no sound"),
            ),
            OptionItem::new(O::Fps60, tr!("Record at 60 fps"), tr!("Smoother motion"))
                .off((!env.fps60_ready).then(|| tr!("Run the 60 fps test in Settings first"))),
            OptionItem::new(
                O::Countdown5,
                tr!("Start in 5 s"),
                tr!("A 5 second countdown"),
            ),
        ],
        CommandId::RecordRegion => vec![
            OptionItem::new(O::Default, verb, tr!("Drag to select, then the sheet")),
            OptionItem::new(
                O::WithWebcam,
                tr!("Include webcam"),
                tr!("Region with the bubble"),
            ),
            OptionItem::new(
                O::NoAudio,
                tr!("Without audio"),
                tr!("No microphone, no system audio"),
            ),
        ],
        CommandId::ScreenshotRegion
        | CommandId::ScreenshotWindow
        | CommandId::ScreenshotDisplay => {
            let (default_hint, second) = if env.copy_after_capture {
                (
                    tr!("Copy + save"),
                    OptionItem::new(
                        O::SaveOnly,
                        tr!("Save only (don't copy)"),
                        tr!("Leave the clipboard alone"),
                    ),
                )
            } else {
                (
                    tr_ctx!("launcher hint", "Save"),
                    OptionItem::new(
                        O::CopyAndSave,
                        tr!("Copy and save"),
                        tr!("Also copy to the clipboard"),
                    ),
                )
            };
            vec![
                OptionItem::new(O::Default, verb, default_hint),
                second,
                OptionItem::new(
                    O::CopyOnly,
                    tr!("Copy only (don't save)"),
                    tr!("Not available yet"),
                )
                .off(Some(tr!(
                    "Not available yet: every screenshot is saved to the library"
                ))),
                OptionItem::new(
                    O::Delay5,
                    tr!("With 5 s delay"),
                    tr!("A countdown pill, Esc cancels"),
                ),
            ]
        }
        _ => vec![OptionItem {
            option: O::Default,
            label: tr_ctx!("launcher action", "Run"),
            hint: None,
            disabled: None,
        }],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub id: CommandId,
    pub group: Group,
    pub title: String,
    /// What the search also matches: the keywords in the current language, then the English ones
    /// (so "record" still finds a command in any language) and the English title.
    pub keywords: Vec<String>,
    pub glyph: Glyph,
    pub hint: Hint,
    /// The user's current binding as keycap labels (LNC-1.AC3); empty when unbound.
    pub shortcut: Vec<String>,
    /// Why the command cannot run, shown as the hint (LNC-1.AC5).
    pub disabled: Option<String>,
    /// Primary verb in the footer (LNC-1.AC4).
    pub verb: String,
    /// The Actions list (⌘K): the first row is the default.
    pub options: Vec<OptionItem>,
}

impl Command {
    pub fn is_enabled(&self) -> bool {
        self.disabled.is_none()
    }

    /// The hint text a screen reader or a flash shows: the reason when disabled.
    pub fn hint_text(&self) -> Option<&str> {
        match (&self.disabled, &self.hint) {
            (Some(r), _) => Some(r),
            (None, Hint::Text(t) | Hint::Agent(t)) => Some(t),
            (None, Hint::None) => None,
        }
    }

    /// LNC-1.AC10: accessible name = title, hint and shortcut.
    pub fn accessible_name(&self, platform: Platform) -> String {
        let title = &self.title;
        let hint = self.hint_text();
        if self.is_enabled() && !self.shortcut.is_empty() {
            let shortcut = shortcut_text(&self.shortcut, platform);
            match hint {
                Some(hint) => tr!(
                    "{title}, {hint}, shortcut {shortcut}",
                    title = title,
                    hint = hint,
                    shortcut = shortcut
                ),
                None => tr!(
                    "{title}, shortcut {shortcut}",
                    title = title,
                    shortcut = shortcut
                ),
            }
        } else if !self.is_enabled() {
            match hint {
                Some(hint) => tr!("{title}, {hint}, unavailable", title = title, hint = hint),
                None => tr!("{title}, unavailable", title = title),
            }
        } else {
            match hint {
                Some(hint) => tr!("{title}, {hint}", title = title, hint = hint),
                None => title.clone(),
            }
        }
    }
}

/// Keycap text for one shortcut as a single cap (kit: "⌥⇧R"): symbols joined on macOS,
/// words joined with "+" on Windows.
pub fn shortcut_text(labels: &[String], platform: Platform) -> String {
    match platform {
        Platform::Mac => labels.concat(),
        Platform::Windows | Platform::Linux => labels.join("+"),
    }
}

/// The "Actions" keycap in the footer (LNC-1 Footer).
pub fn actions_shortcut(platform: Platform) -> &'static str {
    match platform {
        Platform::Mac => "\u{2318}K",
        Platform::Windows | Platform::Linux => "Ctrl+K",
    }
}

/// Live values the hints and disabled reasons are derived from.
pub struct Env<'a> {
    pub platform: Platform,
    pub shortcuts: &'a ShortcutMap,
    pub mcp: &'a McpStatus,
    /// The library opened, so there is somewhere to save captures.
    pub capture_available: bool,
    pub copy_after_capture: bool,
    /// Why recording cannot start right now (library unavailable, a recording is running), or
    /// `None` when it can.
    pub recording_off: Option<String>,
    /// A 60 fps self-test passed on this machine and is still current, so "Record at 60 fps"
    /// can be offered (REC-3). The engine makes the final decision when the recording starts.
    pub fps60_ready: bool,
}

/// Whether a 60 fps self-test passed on this machine, is not older than the daemon's staleness
/// window and was run by this app version (REC-3), so "Record at 60 fps" is offered. It is a hint
/// only: the engine decides for the real display, size and codec when the recording starts and
/// the start fails with its own message when it says no.
pub fn fps60_ready(
    settings: &serde_json::Map<String, serde_json::Value>,
    now: time::OffsetDateTime,
) -> bool {
    use cm_daemon::recording::selftest::{load_entries, STALE_AFTER_DAYS};
    use time::format_description::well_known::Rfc3339;
    load_entries(settings).iter().any(|e| {
        e.passed
            && e.app_version == env!("CARGO_PKG_VERSION")
            && time::OffsetDateTime::parse(&e.at, &Rfc3339).is_ok_and(|at| {
                let age = now - at;
                age <= time::Duration::days(STALE_AFTER_DAYS) && age >= -time::Duration::days(1)
            })
    })
}

/// All commands in default (group, then table) order.
pub fn build(env: &Env<'_>) -> Vec<Command> {
    let mut cmds = build_rows(env);
    for c in &mut cmds {
        c.options = options_for(c.id, &c.verb, env);
    }
    cmds
}

/// The keywords of one command: each translated (context "launcher keyword"), then the English
/// ones that are not already there. Search matches all of them.
fn keywords(translated: Vec<String>, english_title: &str, english: &[&str]) -> Vec<String> {
    let mut all = translated;
    for e in std::iter::once(&english_title).chain(english.iter()) {
        if !all.iter().any(|k| k.eq_ignore_ascii_case(e)) {
            all.push((*e).to_owned());
        }
    }
    all
}

fn build_rows(env: &Env<'_>) -> Vec<Command> {
    let key = |a: ShortcutAction| env.shortcuts.get(a).keycaps(env.platform);
    let capture_off = (!env.capture_available).then(|| tr!("Library unavailable"));
    let shot_hint = Hint::Text(if env.copy_after_capture {
        tr!("PNG \u{00B7} copy + save")
    } else {
        tr!("PNG \u{00B7} save")
    });
    let mcp_hint = if env.mcp.is_ready() {
        Hint::Agent(env.mcp.title().to_owned())
    } else {
        Hint::Text(env.mcp.title().to_owned())
    };
    let recording_off = || env.recording_off.clone();
    // (Row text is made here, when the launcher opens: the window lives for seconds and the
    // language cannot change meanwhile, so every open shows the current language.)
    vec![
        Command {
            id: CommandId::RecordScreen,
            group: Group::Capture,
            title: tr!("Record screen + webcam"),
            keywords: keywords(
                vec![
                    tr_ctx!("launcher keyword", "record"),
                    tr_ctx!("launcher keyword", "video"),
                    tr_ctx!("launcher keyword", "camera"),
                    tr_ctx!("launcher keyword", "bubble"),
                    tr_ctx!("launcher keyword", "webcam"),
                    tr_ctx!("launcher keyword", "screencast"),
                ],
                "Record screen + webcam", // i18n-ignore: the English title is searched too
                &[
                    "record",
                    "video",
                    "camera",
                    "bubble",
                    "webcam",
                    "screencast",
                ],
            ),
            glyph: Glyph::Record,
            hint: Hint::Text(tr!("Display, window or region")),
            shortcut: key(ShortcutAction::ToggleRecording),
            disabled: recording_off(),
            verb: tr!("Start recording"),
            options: Vec::new(),
        },
        Command {
            id: CommandId::RecordRegion,
            group: Group::Capture,
            title: tr!("Record region"),
            keywords: keywords(
                vec![
                    tr_ctx!("launcher keyword", "record"),
                    tr_ctx!("launcher keyword", "video"),
                    tr_ctx!("launcher keyword", "area"),
                    tr_ctx!("launcher keyword", "crop"),
                ],
                "Record region", // i18n-ignore: the English title is searched too
                &["record", "video", "area", "crop"],
            ),
            glyph: Glyph::Frame,
            hint: Hint::Text(tr!("Drag to select")),
            shortcut: Vec::new(),
            disabled: recording_off(),
            verb: tr!("Start recording"),
            options: Vec::new(),
        },
        Command {
            id: CommandId::RecordAudio,
            group: Group::Capture,
            title: tr!("Record audio"),
            keywords: keywords(
                vec![
                    tr_ctx!("launcher keyword", "record"),
                    tr_ctx!("launcher keyword", "audio"),
                    tr_ctx!("launcher keyword", "voice"),
                    tr_ctx!("launcher keyword", "meeting"),
                    tr_ctx!("launcher keyword", "interview"),
                    tr_ctx!("launcher keyword", "microphone"),
                ],
                "Record audio", // i18n-ignore: the English title is searched too
                &[
                    "record",
                    "audio",
                    "voice",
                    "meeting",
                    "interview",
                    "microphone",
                ],
            ),
            glyph: Glyph::Mic,
            hint: Hint::Text(tr!("No screen, microphone and calls")),
            shortcut: key(ShortcutAction::ToggleAudioRecording),
            disabled: recording_off(),
            verb: tr!("Open the sheet"),
            options: Vec::new(),
        },
        Command {
            id: CommandId::ScreenshotRegion,
            group: Group::Capture,
            title: tr!("Screenshot region"),
            keywords: keywords(
                vec![
                    tr_ctx!("launcher keyword", "capture"),
                    tr_ctx!("launcher keyword", "area"),
                    tr_ctx!("launcher keyword", "crop"),
                    tr_ctx!("launcher keyword", "image"),
                ],
                "Screenshot region", // i18n-ignore: the English title is searched too
                &["capture", "area", "crop", "image"],
            ),
            glyph: Glyph::Frame,
            hint: Hint::Text(tr!("Drag to select")),
            shortcut: key(ShortcutAction::CaptureRegion),
            disabled: capture_off.clone(),
            verb: tr!("Take screenshot"),
            options: Vec::new(),
        },
        Command {
            id: CommandId::ScreenshotWindow,
            group: Group::Capture,
            title: tr!("Screenshot window"),
            keywords: keywords(
                vec![
                    tr_ctx!("launcher keyword", "capture"),
                    tr_ctx!("launcher keyword", "app"),
                    tr_ctx!("launcher keyword", "image"),
                ],
                "Screenshot window", // i18n-ignore: the English title is searched too
                &["capture", "app", "image"],
            ),
            glyph: Glyph::Window,
            hint: shot_hint.clone(),
            shortcut: key(ShortcutAction::CaptureWindow),
            disabled: capture_off.clone(),
            verb: tr!("Take screenshot"),
            options: Vec::new(),
        },
        Command {
            id: CommandId::ScreenshotDisplay,
            group: Group::Capture,
            title: tr!("Screenshot display"),
            keywords: keywords(
                vec![
                    tr_ctx!("launcher keyword", "capture"),
                    tr_ctx!("launcher keyword", "screen"),
                    tr_ctx!("launcher keyword", "monitor"),
                    tr_ctx!("launcher keyword", "image"),
                ],
                "Screenshot display", // i18n-ignore: the English title is searched too
                &["capture", "screen", "monitor", "image"],
            ),
            glyph: Glyph::Monitor,
            hint: shot_hint,
            shortcut: key(ShortcutAction::CaptureDisplay),
            disabled: capture_off,
            verb: tr!("Take screenshot"),
            options: Vec::new(),
        },
        Command {
            id: CommandId::OpenLibrary,
            group: Group::Capture,
            title: tr!("Open library"),
            keywords: keywords(
                vec![
                    tr_ctx!("launcher keyword", "screenshots"),
                    tr_ctx!("launcher keyword", "captures"),
                    tr_ctx!("launcher keyword", "studio"),
                    tr_ctx!("launcher keyword", "editor"),
                ],
                "Open library", // i18n-ignore: the English title is searched too
                &["screenshots", "captures", "studio", "editor"],
            ),
            glyph: Glyph::Library,
            hint: Hint::Text(tr_ctx!("launcher hint", "Main window")),
            shortcut: key(ShortcutAction::OpenLibrary),
            disabled: None,
            verb: tr!("Open library"),
            options: Vec::new(),
        },
        Command {
            id: CommandId::OpenSettings,
            group: Group::Capture,
            title: tr!("Open settings"),
            keywords: keywords(
                vec![
                    tr_ctx!("launcher keyword", "preferences"),
                    tr_ctx!("launcher keyword", "shortcuts"),
                    tr_ctx!("launcher keyword", "theme"),
                    tr_ctx!("launcher keyword", "options"),
                ],
                "Open settings", // i18n-ignore: the English title is searched too
                &["preferences", "shortcuts", "theme", "options"],
            ),
            glyph: Glyph::Settings,
            hint: Hint::Text(tr_ctx!("launcher hint", "Preferences")),
            shortcut: Vec::new(),
            disabled: None,
            verb: tr!("Open settings"),
            options: Vec::new(),
        },
        Command {
            id: CommandId::OpenPermissions,
            group: Group::Capture,
            title: tr!("Permissions\u{2026}"),
            keywords: keywords(
                vec![
                    tr_ctx!("launcher keyword", "screen recording"),
                    tr_ctx!("launcher keyword", "microphone"),
                    tr_ctx!("launcher keyword", "camera"),
                    tr_ctx!("launcher keyword", "input monitoring"),
                    tr_ctx!("launcher keyword", "privacy"),
                    tr_ctx!("launcher keyword", "access"),
                    tr_ctx!("launcher keyword", "grant"),
                ],
                "Permissions\u{2026}", // i18n-ignore: the English title is searched too
                &[
                    "screen recording",
                    "microphone",
                    "camera",
                    "input monitoring",
                    "privacy",
                    "access",
                    "grant",
                ],
            ),
            glyph: Glyph::Permissions,
            hint: Hint::Text(tr!("What CaptureMann can use")),
            shortcut: Vec::new(),
            disabled: None,
            verb: tr!("Open permissions"),
            options: Vec::new(),
        },
        Command {
            id: CommandId::PublishLast,
            group: Group::Publish,
            title: tr!("Publish last recording\u{2026}"),
            keywords: keywords(
                vec![
                    tr_ctx!("launcher keyword", "youtube"),
                    tr_ctx!("launcher keyword", "tiktok"),
                    tr_ctx!("launcher keyword", "facebook"),
                    tr_ctx!("launcher keyword", "upload"),
                    tr_ctx!("launcher keyword", "share"),
                ],
                "Publish last recording\u{2026}", // i18n-ignore: the English title is searched too
                &["youtube", "tiktok", "facebook", "upload", "share"],
            ),
            glyph: Glyph::Upload,
            hint: Hint::None,
            shortcut: Vec::new(),
            disabled: Some(tr!("Not available yet")),
            verb: tr_ctx!("launcher verb", "Publish"),
            options: Vec::new(),
        },
        Command {
            id: CommandId::SaveToDrive,
            group: Group::Publish,
            title: tr!("Save to cloud drive"),
            keywords: keywords(
                vec![
                    tr_ctx!("launcher keyword", "upload"),
                    tr_ctx!("launcher keyword", "drive"),
                    tr_ctx!("launcher keyword", "dropbox"),
                    tr_ctx!("launcher keyword", "onedrive"),
                    tr_ctx!("launcher keyword", "google"),
                ],
                "Save to cloud drive", // i18n-ignore: the English title is searched too
                &["upload", "drive", "dropbox", "onedrive", "google"],
            ),
            glyph: Glyph::Cloud,
            hint: Hint::None,
            shortcut: Vec::new(),
            disabled: Some(tr!("Not available yet")),
            verb: tr!("Save to drive"),
            options: Vec::new(),
        },
        Command {
            id: CommandId::CopyMcpConfig,
            group: Group::Agents,
            title: tr!("Copy MCP server config"),
            keywords: keywords(
                vec![
                    tr_ctx!("launcher keyword", "agent"),
                    tr_ctx!("launcher keyword", "claude"),
                    tr_ctx!("launcher keyword", "cursor"),
                    tr_ctx!("launcher keyword", "json"),
                    tr_ctx!("launcher keyword", "client"),
                    tr_ctx!("launcher keyword", "ai"),
                ],
                "Copy MCP server config", // i18n-ignore: the English title is searched too
                &["agent", "claude", "cursor", "json", "client", "ai"],
            ),
            glyph: Glyph::Terminal,
            hint: mcp_hint,
            shortcut: Vec::new(),
            disabled: None,
            verb: tr!("Copy config"),
            options: Vec::new(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_status::{derive, McpState};

    fn env_for<'a>(
        platform: Platform,
        map: &'a ShortcutMap,
        mcp: &'a McpStatus,
        available: bool,
    ) -> Env<'a> {
        Env {
            platform,
            shortcuts: map,
            mcp,
            capture_available: available,
            copy_after_capture: true,
            recording_off: (!available).then(|| "Library unavailable".to_owned()),
            fps60_ready: false,
        }
    }

    fn find(cmds: &[Command], id: CommandId) -> &Command {
        cmds.iter().find(|c| c.id == id).unwrap()
    }

    /// LNC-1.AC3: the keycap is the user's current binding, including after a rebind.
    #[test]
    fn keycaps_follow_the_binding() {
        let mcp = derive(true, true, None, &[]);
        let defaults = ShortcutMap::defaults(Platform::Mac);
        let cmds = build(&env_for(Platform::Mac, &defaults, &mcp, true));
        assert_eq!(
            shortcut_text(
                &find(&cmds, CommandId::ScreenshotRegion).shortcut,
                Platform::Mac
            ),
            "\u{2325}\u{21E7}1"
        );
        let serde_json::Value::Object(m) =
            serde_json::json!({"shortcuts.capture_region": "ctrl+alt+9"})
        else {
            unreachable!()
        };
        let rebound = ShortcutMap::from_settings(&m, Platform::Windows);
        let cmds = build(&env_for(Platform::Windows, &rebound, &mcp, true));
        assert_eq!(
            shortcut_text(
                &find(&cmds, CommandId::ScreenshotRegion).shortcut,
                Platform::Windows
            ),
            "Ctrl+Alt+9"
        );
    }

    /// LNC-1.AC5: commands whose action is not built are listed, disabled, with a reason.
    #[test]
    fn unbuilt_commands_are_disabled_with_a_reason() {
        let mcp = derive(true, true, None, &[]);
        let map = ShortcutMap::defaults(Platform::Mac);
        let cmds = build(&env_for(Platform::Mac, &map, &mcp, true));
        for id in [CommandId::PublishLast, CommandId::SaveToDrive] {
            let c = find(&cmds, id);
            assert!(!c.is_enabled(), "{id:?}");
            assert_eq!(c.hint_text(), Some("Not available yet"));
        }
        for id in [
            CommandId::RecordScreen,
            CommandId::RecordRegion,
            CommandId::ScreenshotRegion,
            CommandId::OpenLibrary,
            CommandId::CopyMcpConfig,
        ] {
            assert!(find(&cmds, id).is_enabled(), "{id:?}");
        }
    }

    /// REC-1: both recording commands run now; recording shows the start/stop shortcut, and is
    /// disabled with the reason while there is no library or a recording is running.
    #[test]
    fn rec1_recording_commands_are_real() {
        let mcp = derive(true, true, None, &[]);
        let map = ShortcutMap::defaults(Platform::Mac);
        let cmds = build(&env_for(Platform::Mac, &map, &mcp, true));
        let screen = find(&cmds, CommandId::RecordScreen);
        assert!(screen.is_enabled());
        assert_eq!(
            shortcut_text(&screen.shortcut, Platform::Mac),
            "\u{2325}\u{21E7}R"
        );
        assert!(find(&cmds, CommandId::RecordRegion).is_enabled());
        let env = Env {
            recording_off: Some("A recording is running".to_owned()),
            ..env_for(Platform::Mac, &map, &mcp, true)
        };
        let busy = build(&env);
        assert_eq!(
            find(&busy, CommandId::RecordScreen).hint_text(),
            Some("A recording is running")
        );
        assert!(!find(&busy, CommandId::RecordRegion).is_enabled());
    }

    #[test]
    fn capture_needs_a_library() {
        let mcp = derive(false, true, None, &[]);
        let map = ShortcutMap::defaults(Platform::Mac);
        let cmds = build(&env_for(Platform::Mac, &map, &mcp, false));
        let c = find(&cmds, CommandId::ScreenshotWindow);
        assert_eq!(c.disabled.as_deref(), Some("Library unavailable"));
        assert!(find(&cmds, CommandId::OpenLibrary).is_enabled());
    }

    /// LNC-1.AC8: the MCP hint is the real agent status.
    #[test]
    fn mcp_hint_is_the_real_status() {
        let map = ShortcutMap::defaults(Platform::Mac);
        let ready = derive(true, true, None, &[]);
        let cmds = build(&env_for(Platform::Mac, &map, &ready, true));
        assert_eq!(
            find(&cmds, CommandId::CopyMcpConfig).hint,
            Hint::Agent("MCP ready".into())
        );
        let off = McpStatus {
            state: McpState::Offline(crate::agent_status::OfflineReason::NoEndpoint),
            last_call: None,
        };
        let cmds = build(&env_for(Platform::Mac, &map, &off, true));
        assert_eq!(
            find(&cmds, CommandId::CopyMcpConfig).hint,
            Hint::Text("MCP offline".into())
        );
    }

    /// LNC-1.AC10: the accessible name carries the hint and the shortcut.
    #[test]
    fn accessible_names() {
        let mcp = derive(true, true, None, &[]);
        let map = ShortcutMap::defaults(Platform::Windows);
        let cmds = build(&env_for(Platform::Windows, &map, &mcp, true));
        assert_eq!(
            find(&cmds, CommandId::ScreenshotWindow).accessible_name(Platform::Windows),
            "Screenshot window, PNG \u{00B7} copy + save, shortcut Ctrl+Alt+Shift+2"
        );
        assert_eq!(
            find(&cmds, CommandId::SaveToDrive).accessible_name(Platform::Windows),
            "Save to cloud drive, Not available yet, unavailable"
        );
    }

    /// LNC-1 Actions: every command's first option is the default, and only the record and
    /// screenshot commands have more than one row.
    #[test]
    fn lnc1_first_option_is_the_default_and_only_capture_commands_have_more() {
        let mcp = derive(true, true, None, &[]);
        let map = ShortcutMap::defaults(Platform::Mac);
        let cmds = build(&env_for(Platform::Mac, &map, &mcp, true));
        for c in &cmds {
            assert_eq!(c.options[0].option, LaunchOption::Default, "{:?}", c.id);
            let many = matches!(
                c.id,
                CommandId::RecordScreen
                    | CommandId::RecordRegion
                    | CommandId::ScreenshotRegion
                    | CommandId::ScreenshotWindow
                    | CommandId::ScreenshotDisplay
            );
            assert_eq!(c.options.len() > 1, many, "{:?}", c.id);
        }
        // The record and screenshot defaults carry the footer verb; the rest say "Run".
        assert_eq!(
            find(&cmds, CommandId::RecordScreen).options[0].label,
            "Start recording"
        );
        assert_eq!(
            find(&cmds, CommandId::ScreenshotWindow).options[0].label,
            "Take screenshot"
        );
        assert_eq!(find(&cmds, CommandId::OpenLibrary).options[0].label, "Run");
    }

    /// REC-3: the 60 fps option is offered only after a passing, current self-test of this app
    /// version; a failed, stale or other-version result does not count.
    #[test]
    fn rec3_fps60_ready_needs_a_current_passing_selftest() {
        use cm_daemon::recording::selftest::{store_value, SelftestEntry, SETTING_KEY};
        use time::format_description::well_known::Rfc3339;
        let now = time::OffsetDateTime::parse("2026-10-22T12:00:00Z", &Rfc3339).unwrap();
        let entry = |passed: bool, at: &str, version: &str| SelftestEntry {
            passed,
            late_pct: 0.1,
            dropped: 0,
            dropped_pct: 0.0,
            frames: 600,
            at: at.to_owned(),
            display: Some(1),
            size: [1920, 1080],
            codec: "hevc".to_owned(),
            app_version: version.to_owned(),
            duration_s: 10.0,
            stall_ms: 0,
        };
        let settings = |e: SelftestEntry| {
            let mut m = serde_json::Map::new();
            m.insert(
                SETTING_KEY.to_owned(),
                store_value(&serde_json::Map::new(), e, now),
            );
            m
        };
        let me = env!("CARGO_PKG_VERSION");
        assert!(!fps60_ready(&serde_json::Map::new(), now), "nothing stored");
        assert!(fps60_ready(
            &settings(entry(true, "2026-10-20T12:00:00Z", me)),
            now
        ));
        assert!(!fps60_ready(
            &settings(entry(false, "2026-10-20T12:00:00Z", me)),
            now
        ));
        assert!(
            !fps60_ready(&settings(entry(true, "2026-08-01T12:00:00Z", me)), now),
            "stale"
        );
        assert!(!fps60_ready(
            &settings(entry(true, "2026-10-20T12:00:00Z", "0.0.0-old")),
            now
        ));
    }

    #[test]
    fn actions_keycap_per_platform() {
        assert_eq!(actions_shortcut(Platform::Mac), "\u{2318}K");
        assert_eq!(actions_shortcut(Platform::Windows), "Ctrl+K");
    }

    /// LNC-1: every command is in one of the three groups; ids are unique.
    #[test]
    fn ids_are_unique_and_grouped() {
        let mcp = derive(true, true, None, &[]);
        let map = ShortcutMap::defaults(Platform::Mac);
        let cmds = build(&env_for(Platform::Mac, &map, &mcp, true));
        let mut ids: Vec<_> = cmds.iter().map(|c| c.id as u8).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), cmds.len());
        assert!(cmds.iter().all(|c| Group::ORDER.contains(&c.group)));
    }

    /// I18N P2: the table is built in the current language, and the search still finds a command
    /// by its English keywords and its English title (checked with the pseudo-locale, which
    /// changes every wrapped title).
    #[test]
    fn i18n_titles_translate_and_the_english_keywords_still_match() {
        use crate::launcher::model::LauncherModel;
        use cm_i18n::{with_language, Lang};
        let mcp = derive(true, true, None, &[]);
        let map = ShortcutMap::defaults(Platform::Mac);
        with_language(Lang::EnXa, || {
            let cmds = build(&env_for(Platform::Mac, &map, &mcp, true));
            let screen = find(&cmds, CommandId::RecordScreen);
            assert_ne!(screen.title, "Record screen + webcam");
            assert!(
                screen.keywords.iter().any(|k| k == "webcam"),
                "English keyword kept"
            );
            assert!(
                screen
                    .keywords
                    .iter()
                    .any(|k| k == "Record screen + webcam"),
                "English title kept"
            );
            let mut m = LauncherModel::new(cmds, vec![]);
            m.set_query("screencast");
            assert_eq!(m.selected().map(|c| c.id), Some(CommandId::RecordScreen));
            m.set_query("library");
            assert_eq!(m.selected().map(|c| c.id), Some(CommandId::OpenLibrary));
        });
        // English: byte for byte what it was, and the keywords carry no duplicates.
        let cmds = build(&env_for(Platform::Mac, &map, &mcp, true));
        let screen = find(&cmds, CommandId::RecordScreen);
        assert_eq!(screen.title, "Record screen + webcam");
        assert_eq!(screen.verb, "Start recording");
        assert_eq!(
            screen.keywords[..6],
            [
                "record",
                "video",
                "camera",
                "bubble",
                "webcam",
                "screencast"
            ]
        );
        assert_eq!(
            screen.keywords.len(),
            7,
            "the English title is the only addition"
        );
    }

    /// The accessible name keeps every placeholder in a translated language.
    #[test]
    fn i18n_accessible_name_keeps_its_parts() {
        use cm_i18n::{with_language, Lang};
        let mcp = derive(true, true, None, &[]);
        let map = ShortcutMap::defaults(Platform::Windows);
        with_language(Lang::EnXa, || {
            let cmds = build(&env_for(Platform::Windows, &map, &mcp, true));
            let name = find(&cmds, CommandId::ScreenshotWindow).accessible_name(Platform::Windows);
            assert!(name.contains("Ctrl+Alt+Shift+2"), "{name}");
        });
    }
}
