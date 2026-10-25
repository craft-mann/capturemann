//! Display-free models for the Settings screens: the inner nav, the agent client config
//! snippet, the permission-tier labels, per-client policies and the shortcut rows. Kept free
//! of GPUI so they are unit tested (DESIGN 5.8, AGT-1, AGT-2, NFR-11).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use cm_daemon::tool_policy::{self, ToolControl, ToolSetting};
use cm_daemon::{ActionSpec, ActivityEntry, ClientPolicy, Endpoint, Tier, ACTIONS};
use cm_i18n::{tr, tr_ctx};

use crate::shortcuts::{Platform, ShortcutAction, ShortcutMap};
use crate::ui::app::Route;

// ---------------------------------------------------------------------------------------
// Settings sections (the sidebar's settings mode)
// ---------------------------------------------------------------------------------------

/// The pages of the settings sidebar: the kit's seven minus Agents & CLI, plus Privacy (decision
/// OD-9). Agents & CLI is its own sidebar item (OD-16), but `Route::Agents` still draws its page
/// through this view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsSection {
    General,
    Capture,
    Webcam,
    Destinations,
    /// Not in [`SettingsSection::ALL`]: reached from the Agents & CLI sidebar item.
    Agents,
    Permissions,
    Privacy,
    Shortcuts,
    About,
}

impl SettingsSection {
    pub const ALL: [SettingsSection; 8] = [
        SettingsSection::General,
        SettingsSection::Capture,
        SettingsSection::Webcam,
        SettingsSection::Destinations,
        SettingsSection::Permissions,
        SettingsSection::Privacy,
        SettingsSection::Shortcuts,
        SettingsSection::About,
    ];

    /// Nav label (the kit's wording).
    pub fn label(self) -> String {
        match self {
            Self::General => tr_ctx!("settings page", "General"),
            Self::Capture => tr_ctx!("settings page", "Capture & recording"),
            Self::Webcam => tr_ctx!("settings page", "Webcam & audio"),
            Self::Destinations => tr_ctx!("settings page", "Destinations"),
            Self::Agents => tr_ctx!("settings page", "Agents & CLI"),
            Self::Permissions => tr_ctx!("settings page", "Permissions"),
            Self::Privacy => tr_ctx!("settings page", "Privacy"),
            Self::Shortcuts => tr_ctx!("settings page", "Shortcuts"),
            Self::About => tr_ctx!("settings page", "About \u{00B7} Apache-2.0"),
        }
    }

    /// Page title (the nav label without the licence suffix).
    pub fn title(self) -> String {
        match self {
            Self::About => tr_ctx!("settings page", "About"),
            s => s.label(),
        }
    }

    pub fn subtitle(self) -> String {
        match self {
            Self::General => tr!("Appearance, after-capture behavior and your library folder."),
            Self::Capture => tr!("Recording quality, frame rate and what gets captured."),
            Self::Webcam => tr!("The webcam bubble, microphone and system audio."),
            Self::Destinations => {
                tr!("Where finished captures are saved, and the accounts you publish to.")
            }
            Self::Agents => {
                tr!("Every CaptureMann feature is available as an MCP tool and a CLI command.")
            }
            Self::Permissions => {
                tr!("What CaptureMann needs from your system, what is granted and how to fix the rest.")
            }
            // TRANSLATORS: FACT: owner review required (privacy)
            Self::Privacy => tr!("What CaptureMann does with your data."),
            Self::Shortcuts => tr!("Global shortcuts that work from any app."),
            Self::About => tr!("Version, licence and credits."),
        }
    }

    /// The leading icon in the settings sidebar (`design/icons/icon-map.md`, "Settings nav").
    pub fn icon(self) -> &'static str {
        match self {
            Self::General => "sliders-horizontal",
            Self::Capture => "cm-capture-region",
            Self::Webcam => "webcam",
            Self::Destinations => "folder-output",
            Self::Agents => "bot",
            Self::Permissions => "shield-check",
            Self::Privacy => "eye-off",
            Self::Shortcuts => "keyboard",
            Self::About => "info",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::General => "settings-general",
            Self::Capture => "settings-capture",
            Self::Webcam => "settings-webcam",
            Self::Destinations => "settings-destinations",
            Self::Agents => "settings-agents",
            Self::Permissions => "settings-permissions",
            Self::Privacy => "settings-privacy",
            Self::Shortcuts => "settings-shortcuts",
            Self::About => "settings-about",
        }
    }

    /// Destinations has its own route (it opens straight from Settings and from a few links);
    /// Agents & CLI has one because it is a sidebar item; every other page lives under
    /// `Route::Settings`.
    pub fn route(self) -> Route {
        match self {
            Self::Destinations => Route::Destinations,
            Self::Agents => Route::Agents,
            _ => Route::Settings,
        }
    }

    /// The page shown for a route; `remembered` is the last page picked under
    /// `Route::Settings`. `None` for routes that are not settings pages.
    pub fn for_route(route: &Route, remembered: SettingsSection) -> Option<SettingsSection> {
        match route {
            Route::Destinations => Some(Self::Destinations),
            Route::Agents => Some(Self::Agents),
            Route::Settings => Some(match remembered {
                Self::Destinations | Self::Agents => Self::General,
                s => s,
            }),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------
// Agent client config
// ---------------------------------------------------------------------------------------

/// File name of the program a client runs for MCP: `capturemann mcp` (AGT-1).
pub fn mcp_binary_name() -> String {
    if cfg!(windows) {
        "capturemann.exe".to_owned()
    } else {
        "capturemann".to_owned()
    }
}

/// Where `capturemann` was found, if anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCommand {
    /// The command to put in the config: the absolute path when found, else the bare name.
    pub command: String,
    pub found: bool,
}

/// Looks next to the running executable first (bundled install), then in `PATH`.
pub fn locate_mcp(current_exe: Option<&Path>, path_env: Option<OsString>) -> McpCommand {
    let name = mcp_binary_name();
    let beside = current_exe
        .and_then(Path::parent)
        .map(|d| d.join(&name))
        .filter(|p| p.is_file());
    let on_path = || {
        path_env.as_ref().and_then(|paths| {
            std::env::split_paths(paths)
                .map(|d| d.join(&name))
                .find(|p| p.is_file())
        })
    };
    match beside.or_else(on_path) {
        Some(p) => McpCommand {
            command: p.display().to_string(),
            found: true,
        },
        None => McpCommand {
            command: name,
            found: false,
        },
    }
}

/// The socket or pipe argument for `--socket`.
pub fn endpoint_arg(endpoint: Option<&Endpoint>) -> Option<String> {
    match endpoint? {
        Endpoint::Path(p) => Some(p.display().to_string()),
        Endpoint::Pipe(p) => Some(p.clone()),
    }
}

/// The MCP client config (the shape Claude Desktop, Cursor and similar clients read), pretty
/// printed with 2-space indent. `socket` is omitted only when the app serves no endpoint.
pub fn client_config(command: &str, socket: Option<&str>) -> String {
    // Hand-laid so `args` stays on one line; every string goes through serde_json for escaping.
    let q = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_owned());
    let args = match socket {
        Some(s) => format!("[\"mcp\", \"--socket\", {}]", q(s)),
        None => "[\"mcp\"]".to_owned(),
    };
    format!(
        "{{\n  \"mcpServers\": {{\n    \"capturemann\": {{\n      \"command\": {},\n      \"args\": {}\n    }}\n  }}\n}}",
        q(command),
        args
    )
}

// ---------------------------------------------------------------------------------------
// Permission tiers
// ---------------------------------------------------------------------------------------

/// The kit's label for a permission tier (specs/02-agent-interface.md, "Two controls, one
/// model"): Free = Allow, Confirm = Ask first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionLabel {
    Allow,
    AskFirst,
}

impl PermissionLabel {
    pub fn text(self) -> String {
        match self {
            Self::Allow => tr_ctx!("permission tier", "Allow"),
            Self::AskFirst => tr_ctx!("permission tier", "Ask first"),
        }
    }
}

/// `None` for the Never tier (not exposed, so never listed).
pub fn permission_label(tier: Tier) -> Option<PermissionLabel> {
    match tier {
        Tier::Free => Some(PermissionLabel::Allow),
        Tier::Confirm => Some(PermissionLabel::AskFirst),
        Tier::Never => None,
    }
}

/// Why a row cannot be changed (the tooltip of its lock icon).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockReason {
    /// A tool that only reads.
    Reading,
    /// Publishing and deleting (NFR-11).
    PublishDelete,
    /// The 60 fps self-test records the screen.
    SelfTest,
    /// Downloading and deleting speech models.
    Models,
}

impl LockReason {
    pub fn text(self) -> String {
        match self {
            Self::Reading => tr!("Reading never needs asking."),
            // TRANSLATORS: FACT: owner review required (confirmation guarantee)
            Self::PublishDelete => tr!("Publishing and deleting always ask first."),
            Self::SelfTest => tr!("The 60 fps test records your screen, so it always asks first."),
            Self::Models => tr!("Downloading and deleting speech models always ask first."),
        }
    }
}

/// One capability row: a real tool from the daemon's `ACTIONS` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRow {
    pub title: String,
    pub tool: &'static str,
    /// What the tool is set to now.
    pub label: PermissionLabel,
    /// What it is set to when the user changed nothing.
    pub default: PermissionLabel,
    /// The row cannot be changed (reading tools, and publishing, deleting ...: AGT-2, NFR-11).
    pub locked: bool,
    pub lock_reason: Option<LockReason>,
    /// Differs from its default (the "Changed" marker).
    pub changed: bool,
    pub mutating: bool,
}

impl CapabilityRow {
    /// `start_recording` set to Allow: the one loosening, shown with an amber note.
    pub fn is_loosened(&self) -> bool {
        self.tool == "start_recording" && self.label == PermissionLabel::Allow
    }
}

fn label_of(s: ToolSetting) -> PermissionLabel {
    match s {
        ToolSetting::Allow => PermissionLabel::Allow,
        ToolSetting::Ask => PermissionLabel::AskFirst,
    }
}

fn lock_reason(spec: &ActionSpec) -> Option<LockReason> {
    match tool_policy::control_of(spec) {
        ToolControl::LockedRead => Some(LockReason::Reading),
        ToolControl::LockedAsk => Some(match spec.name {
            "run_fps_selftest" => LockReason::SelfTest,
            "download_model" | "remove_model" => LockReason::Models,
            _ => LockReason::PublishDelete,
        }),
        _ => None,
    }
}

/// A short human title for a tool.
pub(crate) fn tool_title(name: &str) -> String {
    match name {
        "list_targets" => tr!("List displays and windows"),
        "capture_screenshot" => tr!("Take screenshots"),
        "annotate" => tr!("Annotate captures"),
        "get_capture" => tr!("Read a capture"),
        "suggest_redactions" => tr!("Suggest redactions"),
        "get_settings" => tr!("Read settings"),
        "set_settings" => tr!("Change settings"),
        "undo" => tr!("Undo"),
        "redo" => tr!("Redo"),
        "list_library" => tr!("List the library"),
        "rename_capture" => tr!("Rename a capture"),
        "get_job" => tr!("Check a job"),
        "list_jobs" => tr!("List jobs"),
        "cancel_job" => tr!("Cancel a job"),
        "retry_job" => tr!("Retry a job"),
        "export_video" => tr!("Export a recording to MP4"),
        "start_recording" => tr!("Start recording"),
        "pause_recording" => tr!("Pause a recording"),
        "resume_recording" => tr!("Resume a recording"),
        "set_mic_muted" => tr!("Mute or unmute the microphone"),
        "stop_recording" => tr!("Stop a recording"),
        "get_recording_status" => tr!("Read recording status"),
        "list_devices" => tr!("List cameras and microphones"),
        "publish_video" => tr!("Publish a video"),
        "delete_capture" => tr!("Delete a capture"),
        "move_library" => tr!("Change the library folder"),
        "run_fps_selftest" => tr!("Run the 60 fps self-test"),
        "trim" => tr!("Trim a recording"),
        "cut_segments" => tr!("Cut parts of a recording"),
        "restore_segments" => tr!("Restore cut parts"),
        "split" => tr!("Split a recording"),
        "set_speed" => tr!("Change playback speed"),
        "set_background" => tr!("Set the background frame"),
        "set_audio_mix" => tr!("Change the audio mix"),
        "set_camera_layout" => tr!("Change the camera layout"),
        "set_zooms" => tr!("Add zoom regions"),
        "set_cursor_style" => tr!("Style the cursor"),
        "set_reframe" => tr!("Reframe for vertical or square"),
        "list_models" => tr!("List speech models"),
        "download_model" => tr!("Download a speech model"),
        "import_model" => tr!("Install a speech model from a file"),
        "select_model" => tr!("Choose the speech model"),
        "remove_model" => tr!("Delete a speech model"),
        "transcribe" => tr!("Transcribe a recording"),
        "get_transcript" => tr!("Read a transcript"),
        "get_captions" => tr!("Read caption lines"),
        "set_captions" => tr!("Edit captions"),
        "replace_in_captions" => tr!("Replace text in captions"),
        "set_caption_options" => tr!("Show or hide captions"),
        "export_captions" => tr!("Export captions (SRT, VTT, TXT)"),
        "suggest_zooms" => tr!("Suggest zoom regions"),
        "suggest_cuts" => tr!("Suggest cuts"),
        "list_destinations" => tr!("List save destinations"),
        "upload" => tr!("Copy a capture to a destination"),
        "list_publish_accounts" => tr!("List publish accounts"),
        _ => tr!("Other action"),
    }
}

/// Capabilities in table order, Free rows first, then the Confirm-tier rows (the order does not
/// change when a setting does). `entries` is the stored `agents.tool_policy` (read with
/// [`tool_policy::parse_lenient`]). Internal plumbing actions are not tools and are left out.
pub fn capability_rows(entries: &BTreeMap<String, ToolSetting>) -> Vec<CapabilityRow> {
    let mut rows: Vec<CapabilityRow> = ACTIONS
        .iter()
        .filter(|s| !cm_daemon::is_internal(s.name))
        .filter(|s| s.tier != Tier::Never)
        .map(|s| {
            let control = tool_policy::control_of(s);
            let now = tool_policy::setting_of(s, entries);
            let default = match control {
                ToolControl::Editable { default } => default,
                _ => now,
            };
            CapabilityRow {
                title: tool_title(s.name),
                tool: s.name,
                label: label_of(now),
                default: label_of(default),
                locked: !matches!(control, ToolControl::Editable { .. }),
                lock_reason: lock_reason(s),
                changed: now != default,
                mutating: s.mutating,
            }
        })
        .collect();
    // Stable: by the table's tier, never by the current setting.
    rows.sort_by_key(|r| tier_of_tool(r.tool) == Some(Tier::Confirm));
    rows
}

fn tier_of_tool(tool: &str) -> Option<Tier> {
    cm_daemon::spec_for(tool).map(|s| s.tier)
}

/// The tools whose setting differs from the default, in table order.
pub fn changed_tools(entries: &BTreeMap<String, ToolSetting>) -> Vec<&'static str> {
    capability_rows(entries)
        .into_iter()
        .filter(|r| r.changed)
        .map(|r| r.tool)
        .collect()
}

/// More changed rows than this and "Reset to defaults" asks first.
pub const RESET_ASKS_ABOVE: usize = 3;

/// Whether "Reset to defaults" shows the app's confirm dialog before it writes.
pub fn reset_needs_confirm(changed: usize) -> bool {
    changed > RESET_ASKS_ABOVE
}

/// Setting `tool` to Allow opens the "Let AI agents start recordings without asking?" dialog
/// first; only `start_recording` can be loosened.
pub fn needs_loosen_confirm(tool: &str, setting: ToolSetting) -> bool {
    tool == "start_recording" && setting == ToolSetting::Allow
}

/// The entries after setting `tool` to `setting`: only the tools that differ from their default
/// stay (so the stored value never lists a default), and an entry the rules do not allow is
/// dropped. `None` when `tool` cannot be changed.
pub fn with_setting(
    entries: &BTreeMap<String, ToolSetting>,
    tool: &str,
    setting: ToolSetting,
) -> Option<BTreeMap<String, ToolSetting>> {
    let spec = cm_daemon::spec_for(tool)?;
    let ToolControl::Editable { .. } = tool_policy::control_of(spec) else {
        return None;
    };
    let mut next = entries.clone();
    next.insert(tool.to_owned(), setting);
    Some(only_changed(&next))
}

fn only_changed(entries: &BTreeMap<String, ToolSetting>) -> BTreeMap<String, ToolSetting> {
    entries
        .iter()
        .filter(|(tool, setting)| {
            cm_daemon::spec_for(tool).is_some_and(|s| match tool_policy::control_of(s) {
                ToolControl::Editable { default } => **setting != default,
                _ => false,
            })
        })
        .map(|(k, v)| (k.clone(), *v))
        .collect()
}

/// The JSON stored under `agents.tool_policy` for these entries.
pub fn tool_policy_value(entries: &BTreeMap<String, ToolSetting>) -> serde_json::Value {
    serde_json::Value::Object(
        only_changed(entries)
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::String(v.as_str().to_owned())))
            .collect(),
    )
}

// ---------------------------------------------------------------------------------------
// Client policies
// ---------------------------------------------------------------------------------------

pub const POLICIES: [ClientPolicy; 3] = [
    ClientPolicy::Allowed,
    ClientPolicy::ReadOnly,
    ClientPolicy::Blocked,
];

pub fn policy_label(p: ClientPolicy) -> String {
    match p {
        ClientPolicy::Allowed => tr_ctx!("client policy", "Allowed"),
        ClientPolicy::ReadOnly => tr_ctx!("client policy", "Read-only"),
        ClientPolicy::Blocked => tr_ctx!("client policy", "Blocked"),
    }
}

pub fn policy_hint(p: ClientPolicy) -> String {
    match p {
        ClientPolicy::Allowed => tr!("Runs as the permissions above say."),
        ClientPolicy::ReadOnly => tr!("Can look but not change anything."),
        ClientPolicy::Blocked => tr!("Every request is refused."),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientRow {
    pub name: String,
    pub policy: ClientPolicy,
    /// Most recent tool this client was gated or refused on, if any.
    pub last_tool: Option<String>,
}

/// Clients the daemon has heard of: those with a stored policy plus those in the activity log
/// (an unknown client is Allowed). Sorted by name.
pub fn client_rows(
    policies: &[(String, ClientPolicy)],
    activity: &[ActivityEntry],
) -> Vec<ClientRow> {
    let mut map: BTreeMap<String, ClientRow> = BTreeMap::new();
    for (name, policy) in policies {
        map.insert(
            name.clone(),
            ClientRow {
                name: name.clone(),
                policy: *policy,
                last_tool: None,
            },
        );
    }
    for e in activity {
        let row = map.entry(e.client.clone()).or_insert_with(|| ClientRow {
            name: e.client.clone(),
            policy: ClientPolicy::default(),
            last_tool: None,
        });
        row.last_tool = Some(e.tool.clone());
    }
    map.into_values().collect()
}

// ---------------------------------------------------------------------------------------
// Shortcuts
// ---------------------------------------------------------------------------------------

/// A shortcut row for display: one per [`ShortcutAction`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutRow {
    pub action: ShortcutAction,
    pub key: &'static str,
    pub label: String,
    pub keycaps: Vec<String>,
}

/// The translated name of a shortcut action (`ShortcutAction::label` stays English for logs).
pub fn shortcut_label(a: ShortcutAction) -> String {
    match a {
        ShortcutAction::CaptureRegion => tr_ctx!("shortcut", "Capture region"),
        ShortcutAction::CaptureWindow => tr_ctx!("shortcut", "Capture window"),
        ShortcutAction::CaptureDisplay => tr_ctx!("shortcut", "Capture display"),
        ShortcutAction::ToggleRecording => tr_ctx!("shortcut", "Start or stop recording"),
        ShortcutAction::PauseRecording => tr_ctx!("shortcut", "Pause or resume recording"),
        ShortcutAction::OpenLibrary => tr_ctx!("shortcut", "Open library"),
        ShortcutAction::OpenLauncher => tr_ctx!("shortcut", "Open launcher"),
        ShortcutAction::DiscardRecording => tr_ctx!("shortcut", "Discard recording"),
        ShortcutAction::RestartRecording => tr_ctx!("shortcut", "Restart recording"),
        ShortcutAction::AnnotateLast => tr_ctx!("shortcut", "Annotate last capture"),
    }
}

pub fn shortcut_rows(map: &ShortcutMap, platform: Platform) -> Vec<ShortcutRow> {
    ShortcutAction::ALL
        .iter()
        .map(|a| ShortcutRow {
            action: *a,
            key: a.key(),
            label: shortcut_label(*a),
            keycaps: map.get(*a).keycaps(platform),
        })
        .collect()
}

/// The two groups of the Shortcuts page: (title, note, actions). Every action is a global
/// shortcut that is registered all the time (CAP-2, REC-5), so neither group says "only while
/// recording". Each action is in exactly one group.
pub fn shortcut_groups() -> [(String, String, Vec<ShortcutAction>); 2] {
    [
        (
            tr_ctx!("shortcut group", "Capture"),
            tr_ctx!("shortcut scope", "Global"),
            vec![
                ShortcutAction::CaptureRegion,
                ShortcutAction::CaptureWindow,
                ShortcutAction::CaptureDisplay,
                ShortcutAction::OpenLauncher,
                ShortcutAction::OpenLibrary,
                ShortcutAction::AnnotateLast,
            ],
        ),
        (
            tr_ctx!("shortcut group", "Recording"),
            tr_ctx!("shortcut scope", "Global"),
            vec![
                ShortcutAction::ToggleRecording,
                ShortcutAction::PauseRecording,
                ShortcutAction::RestartRecording,
                ShortcutAction::DiscardRecording,
            ],
        ),
    ]
}

// ---------------------------------------------------------------------------------------
// Privacy
// ---------------------------------------------------------------------------------------

/// The four fact tiles of Settings -> Privacy: (icon, title, body). The facts are the app's, not
/// the board's: nothing claims the app "works offline" (the Whisper model download and the OAuth
/// sign-in use the network when the user starts them), and the agent tile keeps the confirmation
/// and recording-indicator guarantees (AGT-2).
pub fn privacy_facts() -> [(&'static str, String, String); 4] {
    [
        (
            "eye-off",
            // TRANSLATORS: FACT: owner review required
            tr!("No telemetry"),
            // TRANSLATORS: FACT: owner review required
            tr!("CaptureMann collects no usage data and sends no analytics or crash reports."),
        ),
        (
            "hard-drive",
            // TRANSLATORS: FACT: owner review required
            tr!("Local-first"),
            // TRANSLATORS: FACT: owner review required
            tr!("Captures are saved as folders in your library on this computer, and you can open them without the app."),
        ),
        (
            "globe",
            // TRANSLATORS: FACT: owner review required
            tr!("No network traffic by default"),
            // TRANSLATORS: FACT: owner review required
            tr!("Nothing leaves this computer unless you connect a destination yourself."),
        ),
        (
            "laptop",
            // TRANSLATORS: FACT: owner review required
            tr!("Agents stay on this computer"),
            // TRANSLATORS: FACT: owner review required
            tr!("The CLI and MCP server reach the app over a local socket. An agent starting a recording, publishing or deleting always asks you first; the recording indicator can\u{2019}t be hidden."),
        ),
    ]
}

/// What the excluded-apps list does, and what it does not (the recording engine reads
/// `privacy.excluded_apps`; a screenshot leaves out only CaptureMann's own windows).
// TRANSLATORS: FACT: owner review required
pub fn excluded_note() -> String {
    tr!("Left out of recordings, not of screenshots")
}

/// The caption under the excluded-apps list.
// TRANSLATORS: FACT: owner review required
pub fn excluded_caption() -> String {
    tr!("A listed app\u{2019}s windows are left out of the recording (the desktop shows through). Screenshots leave out only CaptureMann\u{2019}s own windows. CaptureMann\u{2019}s own windows are always left out and can\u{2019}t be removed. Changes apply to the next recording.")
}

// ---------------------------------------------------------------------------------------
// Destinations
// ---------------------------------------------------------------------------------------

/// Cloud drives (STO-2, decision D3).
// i18n-ignore: product names
pub const DRIVES: [(&str, &str); 3] = [("Google Drive", "G"), ("OneDrive", "O"), ("Dropbox", "D")];

/// Publish targets with the kit's one-line copy (Facebook is Pages only: OD-7).
/// The copy is the English source: show [`platform_detail`], which translates it.
pub const PLATFORMS: [(&str, &str); 3] = [
    // i18n-ignore: product name; the copy is translated by platform_detail()
    (
        "YouTube",
        // i18n-ignore
        "Upload to your channel with a default visibility.",
    ),
    // i18n-ignore
    ("TikTok", "Vertical 9:16 export with auto-reframed bubble."),
    // i18n-ignore
    ("Facebook", "Post to a Page you manage."),
];

/// What an unbuilt or not-yet-offered row says (owner decision Oct 23, 2026: v0.1 ships Google Drive
/// and YouTube; the other providers and platforms are "Coming soon").
pub fn later_milestone() -> String {
    tr!("Coming soon")
}

/// The one-line copy of a publish platform from [`PLATFORMS`] in the current language (`None` for
/// a name that is not in the list).
pub fn platform_detail(name: &str) -> Option<String> {
    Some(match name {
        "YouTube" => tr!("Upload to your channel with a default visibility."),
        "TikTok" => tr!("Vertical 9:16 export with auto-reframed bubble."),
        "Facebook" => tr!("Post to a Page you manage."),
        _ => return None,
    })
}

/// Where to find the binary when it is not found (shown under the config snippet).
pub fn missing_mcp_hint(exe: Option<&Path>) -> String {
    let dir: PathBuf = exe
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    tr!(
        "{name} was not found next to the app ({dir}) or on PATH. Build or install it, then use its full path as the command.",
        name = mcp_binary_name(),
        dir = dir.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use time::OffsetDateTime;

    #[test]
    fn nav_has_the_settings_pages_without_ai_and_agents() {
        let labels: Vec<_> = SettingsSection::ALL.iter().map(|s| s.label()).collect();
        assert_eq!(
            labels,
            [
                "General",
                "Capture & recording",
                "Webcam & audio",
                "Destinations",
                "Permissions",
                "Privacy",
                "Shortcuts",
                "About \u{00B7} Apache-2.0"
            ]
        );
        // AI models and Agents & CLI are sidebar items of their own, not Settings pages.
        assert!(!labels.iter().any(|l| l == "AI" || l == "Agents & CLI"));
        assert_eq!(SettingsSection::About.title(), "About");
    }

    /// The settings sidebar draws one row per section with the icon of the kit's map.
    #[test]
    fn sections_have_the_kit_icons_and_they_are_registered() {
        let icons: Vec<_> = SettingsSection::ALL.iter().map(|s| s.icon()).collect();
        assert_eq!(
            icons,
            [
                "sliders-horizontal",
                "cm-capture-region",
                "webcam",
                "folder-output",
                "shield-check",
                "eye-off",
                "keyboard",
                "info"
            ]
        );
        for s in SettingsSection::ALL {
            let path = format!("icons/{}.svg", s.icon());
            assert!(
                crate::ui::paint::ASSETS.iter().any(|(p, _)| *p == path),
                "{path}"
            );
        }
    }

    #[test]
    fn sections_map_to_routes() {
        use SettingsSection as S;
        assert_eq!(S::Destinations.route(), Route::Destinations);
        assert_eq!(S::Agents.route(), Route::Agents);
        assert_eq!(S::Shortcuts.route(), Route::Settings);
        assert_eq!(S::for_route(&Route::Agents, S::About), Some(S::Agents));
        assert_eq!(S::for_route(&Route::Settings, S::Privacy), Some(S::Privacy));
        // A stale Destinations pick never shows under the Settings route.
        assert_eq!(
            S::for_route(&Route::Settings, S::Destinations),
            Some(S::General)
        );
        assert_eq!(S::for_route(&Route::Library, S::General), None);
        assert_eq!(S::for_route(&Route::Screenshots, S::General), None);
        let ids: std::collections::HashSet<_> = S::ALL.iter().map(|s| s.id()).collect();
        assert_eq!(ids.len(), S::ALL.len());
    }

    #[test]
    fn config_snippet_uses_real_command_and_socket() {
        let s = client_config("/Apps/CaptureMann/capturemann", Some("/tmp/cm.sock"));
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let srv = &v["mcpServers"]["capturemann"];
        assert_eq!(srv["command"], "/Apps/CaptureMann/capturemann");
        assert_eq!(srv["args"], json!(["mcp", "--socket", "/tmp/cm.sock"]));
        assert!(!s.contains("PATH TO CLI"));
        assert!(s.contains("\n  \"mcpServers\""));
    }

    #[test]
    fn config_snippet_escapes_and_handles_no_socket() {
        let s = client_config(r"C:\Program Files\capturemann.exe", None);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(
            v["mcpServers"]["capturemann"]["command"],
            r"C:\Program Files\capturemann.exe"
        );
        assert_eq!(v["mcpServers"]["capturemann"]["args"], json!(["mcp"]));
    }

    #[test]
    fn endpoint_argument() {
        assert_eq!(endpoint_arg(None), None);
        assert_eq!(
            endpoint_arg(Some(&Endpoint::Path("/tmp/a.sock".into()))).as_deref(),
            Some("/tmp/a.sock")
        );
        assert_eq!(
            endpoint_arg(Some(&Endpoint::Pipe(r"\\.\pipe\cm".into()))).as_deref(),
            Some(r"\\.\pipe\cm")
        );
    }

    #[test]
    fn locates_mcp_beside_exe_then_path() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        let other = dir.path().join("other");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let exe = bin.join("cm-desktop");
        // Nothing yet.
        let none = locate_mcp(Some(&exe), None);
        assert!(!none.found);
        assert_eq!(none.command, mcp_binary_name());
        // On PATH.
        std::fs::write(other.join(mcp_binary_name()), b"").unwrap();
        let path = std::env::join_paths([&other]).unwrap();
        let p = locate_mcp(Some(&exe), Some(path.clone()));
        assert!(p.found);
        assert_eq!(PathBuf::from(&p.command), other.join(mcp_binary_name()));
        // Beside the executable wins.
        std::fs::write(bin.join(mcp_binary_name()), b"").unwrap();
        let b = locate_mcp(Some(&exe), Some(path));
        assert_eq!(PathBuf::from(&b.command), bin.join(mcp_binary_name()));
    }

    #[test]
    fn tiers_map_to_kit_labels() {
        assert_eq!(permission_label(Tier::Free), Some(PermissionLabel::Allow));
        assert_eq!(
            permission_label(Tier::Confirm),
            Some(PermissionLabel::AskFirst)
        );
        assert_eq!(permission_label(Tier::Never), None);
        assert_eq!(PermissionLabel::AskFirst.text(), "Ask first");
    }

    fn none() -> BTreeMap<String, ToolSetting> {
        BTreeMap::new()
    }

    fn only(tool: &str, s: ToolSetting) -> BTreeMap<String, ToolSetting> {
        [(tool.to_owned(), s)].into_iter().collect()
    }

    /// AGT-2.AC7 / NFR-11: with nothing changed the page shows what the tiers say, and publish,
    /// delete and the other always-ask tools are locked at Ask first.
    #[test]
    fn agt_2_ac7_default_rows() {
        let rows = capability_rows(&none());
        for t in [
            "publish_video",
            "delete_capture",
            "run_fps_selftest",
            "download_model",
            "remove_model",
            "move_library",
        ] {
            let r = rows.iter().find(|r| r.tool == t).unwrap();
            assert_eq!(r.label, PermissionLabel::AskFirst, "{t}");
            assert!(r.locked, "{t} must be locked (NFR-11)");
            assert!(r.lock_reason.is_some() && !r.changed, "{t}");
        }
        // start_recording: Ask first by default, and the one Confirm row that can be changed.
        let start = rows.iter().find(|r| r.tool == "start_recording").unwrap();
        assert_eq!(start.label, PermissionLabel::AskFirst);
        assert!(!start.locked && !start.changed && start.lock_reason.is_none());
        // Reading tools: Allow, locked, with the reason.
        for t in [
            "list_library",
            "get_settings",
            "get_recording_status",
            "get_job",
        ] {
            let r = rows.iter().find(|r| r.tool == t).unwrap();
            assert_eq!(r.label, PermissionLabel::Allow, "{t}");
            assert!(
                r.locked && r.lock_reason == Some(LockReason::Reading),
                "{t}"
            );
        }
        // Every Free tool that changes something is editable and starts at Allow.
        for r in rows
            .iter()
            .filter(|r| r.mutating && r.tool != "start_recording")
        {
            if r.label == PermissionLabel::Allow {
                assert!(!r.locked, "{}", r.tool);
            }
        }
        assert!(rows
            .iter()
            .any(|r| r.tool == "capture_screenshot" && !r.locked));
        // Internal plumbing absent, every exposed tool listed once, no placeholder titles.
        assert!(rows.iter().all(|r| !cm_daemon::is_internal(r.tool)));
        assert!(rows.iter().all(|r| r.title != "Other action"));
        let n = ACTIONS
            .iter()
            .filter(|s| !cm_daemon::is_internal(s.name))
            .count();
        assert_eq!(rows.len(), n);
        // Free-tier rows first, then the Confirm-tier ones.
        let first_confirm = rows
            .iter()
            .position(|r| cm_daemon::spec_for(r.tool).unwrap().tier == Tier::Confirm)
            .unwrap();
        assert!(rows[first_confirm..]
            .iter()
            .all(|r| cm_daemon::spec_for(r.tool).unwrap().tier == Tier::Confirm));
    }

    /// AGT-2.AC7: a changed row shows the new label and the "Changed" marker; the order of the
    /// rows does not move.
    #[test]
    fn agt_2_ac7_changed_rows_and_stable_order() {
        let base: Vec<_> = capability_rows(&none()).iter().map(|r| r.tool).collect();
        let e = with_setting(&none(), "capture_screenshot", ToolSetting::Ask).unwrap();
        let rows = capability_rows(&e);
        assert_eq!(rows.iter().map(|r| r.tool).collect::<Vec<_>>(), base);
        let r = rows
            .iter()
            .find(|r| r.tool == "capture_screenshot")
            .unwrap();
        assert_eq!(r.label, PermissionLabel::AskFirst);
        assert_eq!(r.default, PermissionLabel::Allow);
        assert!(r.changed && !r.locked);
        let e = with_setting(&e, "start_recording", ToolSetting::Allow).unwrap();
        let rows = capability_rows(&e);
        let s = rows.iter().find(|r| r.tool == "start_recording").unwrap();
        assert!(s.changed && s.is_loosened());
        assert_eq!(s.default, PermissionLabel::AskFirst);
        assert_eq!(changed_tools(&e), ["capture_screenshot", "start_recording"]);
        // Setting a tool back to its default clears it: nothing is stored for it.
        let back = with_setting(&e, "capture_screenshot", ToolSetting::Allow).unwrap();
        assert_eq!(changed_tools(&back), ["start_recording"]);
        assert_eq!(
            tool_policy_value(&back),
            serde_json::json!({"start_recording": "allow"})
        );
        assert_eq!(tool_policy_value(&none()), serde_json::json!({}));
    }

    /// AGT-2.AC10 in the page model: locked tools cannot be changed, and a stored value for one
    /// (a hand-edited file) does not show as a change.
    #[test]
    fn agt_2_ac10_locked_rows_cannot_change() {
        for t in [
            "publish_video",
            "delete_capture",
            "run_fps_selftest",
            "list_library",
            "freeze_display",
            "no_such_tool",
        ] {
            assert!(
                with_setting(&none(), t, ToolSetting::Allow).is_none(),
                "{t}"
            );
            assert!(with_setting(&none(), t, ToolSetting::Ask).is_none(), "{t}");
        }
        let hand_edited = cm_daemon::tool_policy::parse_lenient(Some(&serde_json::json!({
            "publish_video": "allow", "delete_capture": "allow", "list_library": "ask"
        })));
        assert!(changed_tools(&hand_edited).is_empty());
        let rows = capability_rows(&hand_edited);
        let p = rows.iter().find(|r| r.tool == "publish_video").unwrap();
        assert_eq!(p.label, PermissionLabel::AskFirst);
    }

    /// AGT-2.AC13 / AC14: the dialogs the page opens.
    #[test]
    fn agt_2_ac13_only_loosening_start_recording_asks() {
        assert!(needs_loosen_confirm("start_recording", ToolSetting::Allow));
        assert!(!needs_loosen_confirm("start_recording", ToolSetting::Ask));
        assert!(!needs_loosen_confirm(
            "capture_screenshot",
            ToolSetting::Ask
        ));
        // Reset asks only when more than three rows differ.
        assert!(!reset_needs_confirm(0));
        assert!(!reset_needs_confirm(3));
        assert!(reset_needs_confirm(4));
    }

    #[test]
    fn lock_reasons_are_told_apart() {
        let rows = capability_rows(&none());
        let why = |t: &str| rows.iter().find(|r| r.tool == t).unwrap().lock_reason;
        assert_eq!(why("publish_video"), Some(LockReason::PublishDelete));
        assert_eq!(why("delete_capture"), Some(LockReason::PublishDelete));
        assert_eq!(why("run_fps_selftest"), Some(LockReason::SelfTest));
        assert_eq!(why("remove_model"), Some(LockReason::Models));
        assert_eq!(why("list_jobs"), Some(LockReason::Reading));
        assert_eq!(why("capture_screenshot"), None);
        assert_eq!(
            LockReason::PublishDelete.text(),
            "Publishing and deleting always ask first."
        );
        assert_eq!(LockReason::Reading.text(), "Reading never needs asking.");
        let _ = only("annotate", ToolSetting::Ask);
    }

    fn entry(client: &str, tool: &str) -> ActivityEntry {
        ActivityEntry {
            at: OffsetDateTime::UNIX_EPOCH,
            client: client.to_owned(),
            tool: tool.to_owned(),
            summary: String::new(),
            decision: cm_daemon::ActivityDecision::Declined,
            approval: None,
        }
    }

    #[test]
    fn client_rows_merge_policies_and_activity() {
        let policies = vec![
            ("zed".to_owned(), ClientPolicy::Blocked),
            ("cursor".to_owned(), ClientPolicy::ReadOnly),
        ];
        let log = [
            entry("claude-desktop", "start_recording"),
            entry("cursor", "delete_capture"),
            entry("claude-desktop", "publish_video"),
        ];
        let rows = client_rows(&policies, &log);
        let names: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["claude-desktop", "cursor", "zed"]);
        assert_eq!(rows[0].policy, ClientPolicy::Allowed);
        assert_eq!(rows[0].last_tool.as_deref(), Some("publish_video"));
        assert_eq!(rows[1].policy, ClientPolicy::ReadOnly);
        assert_eq!(rows[2].last_tool, None);
        assert!(client_rows(&[], &[]).is_empty());
    }

    /// AGT-2.AC4: a stored policy lists the client before it ever connects, and an
    /// activity-only client stays Allowed.
    #[test]
    fn agt_2_ac4_stored_policies_show_without_activity() {
        let loaded = crate::settings::parse_client_policies(Some(&serde_json::json!({
            "never-seen": "blocked", "claude-desktop": "allowed", "junk": 7
        })));
        let rows = client_rows(&loaded.policies, &[entry("late-comer", "annotate")]);
        let view: Vec<_> = rows
            .iter()
            .map(|r| (r.name.as_str(), r.policy, r.last_tool.is_some()))
            .collect();
        assert_eq!(
            view,
            [
                ("claude-desktop", ClientPolicy::Allowed, false),
                ("late-comer", ClientPolicy::Allowed, true),
                ("never-seen", ClientPolicy::Blocked, false),
            ]
        );
    }

    #[test]
    fn policy_labels() {
        let l: Vec<_> = POLICIES.iter().map(|p| policy_label(*p)).collect();
        assert_eq!(l, ["Allowed", "Read-only", "Blocked"]);
    }

    /// The launcher is a real shortcut action (key `shortcuts.open_launcher`); every action has
    /// exactly one row.
    #[test]
    fn every_shortcut_action_has_one_row() {
        let map = ShortcutMap::defaults(Platform::Mac);
        let rows = shortcut_rows(&map, Platform::Mac);
        assert_eq!(rows.len(), ShortcutAction::ALL.len());
        let l = rows
            .iter()
            .find(|r| r.key == "shortcuts.open_launcher")
            .unwrap();
        assert_eq!(l.keycaps, ["\u{2325}", "\u{21E7}", "Space"]);
        let w = shortcut_rows(&ShortcutMap::defaults(Platform::Windows), Platform::Windows);
        let l = w
            .iter()
            .find(|r| r.action == ShortcutAction::OpenLauncher)
            .unwrap();
        assert_eq!(l.keycaps, ["Ctrl", "Alt", "Shift", "Space"]);
        assert!(!rows.iter().any(|r| r.key == "shortcuts.launcher"));
    }

    #[test]
    fn shortcut_groups_hold_every_action_once_and_no_invented_ones() {
        let groups = shortcut_groups();
        let all: Vec<ShortcutAction> = groups.iter().flat_map(|g| g.2.clone()).collect();
        assert_eq!(all.len(), ShortcutAction::ALL.len());
        for a in ShortcutAction::ALL {
            assert_eq!(all.iter().filter(|x| **x == a).count(), 1, "{}", a.key());
        }
        for (_, note, _) in &groups {
            assert!(!note.to_lowercase().contains("only active"), "{note}");
        }
    }

    #[test]
    fn privacy_facts_are_true_ones() {
        let all: String = privacy_facts()
            .iter()
            .map(|(_, t, b)| format!("{t} {b}"))
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        // The app is not offline-only: model downloads and sign-in use the network.
        assert!(!all.contains("offline"), "{all}");
        assert!(all.contains("no telemetry"));
        assert!(all.contains("always asks you first"));
        assert!(all.contains("indicator can\u{2019}t be hidden"));
        assert!(all.contains("unless you connect"));
    }

    #[test]
    fn the_excluded_list_does_not_claim_screenshots() {
        for text in [excluded_note(), excluded_caption()] {
            assert!(
                !text.contains("screenshots and recordings"),
                "screenshots do not honour privacy.excluded_apps: {text}"
            );
        }
        assert!(excluded_note().contains("not of screenshots"));
    }

    #[test]
    fn drives_and_platforms_are_the_decided_ones() {
        let d: Vec<_> = DRIVES.iter().map(|d| d.0).collect();
        assert_eq!(d, ["Google Drive", "OneDrive", "Dropbox"]);
        assert!(PLATFORMS[2].1.contains("Page"));
        assert!(!PLATFORMS[2].1.contains("profile"));
    }
}
