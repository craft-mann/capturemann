//! Persistent settings (`settings.json`) behind the daemon's `SettingsStore`, plus the library
//! location rules of LIB-2.
//!
//! Keys are dotted (`library.path`, `appearance.theme`, `capture.copy_to_clipboard`,
//! `shortcuts.*`, `agents.client_policies`). Absent keys mean "default"; defaults are not
//! written to disk.
//!
//! # `agents.client_policies` (AGT-2.AC4)
//!
//! A JSON object `{ "<client name>": "allowed" | "read_only" | "blocked" }`: the per-client
//! policy of Settings -> Agents & CLI. **Every explicit choice is stored, `allowed` included**:
//! an explicit "allowed" is how a user pre-approves a client that has not connected yet, and it
//! keeps the client listed. A client that was never configured has no entry (and is allowed).
//! Client names only; no secrets (NFR-10). Names follow the `session.hello` rule
//! ([`validate_client_name`]). Malformed input is never fatal: a non-object value is ignored,
//! and entries with an invalid name or an unknown policy are dropped (the next write removes
//! them from the file); the app logs one line per load.
//!
//! The key is private to the desktop app: the daemon-facing store ([`crate::backend::SharedSettings`])
//! hides it from `get_settings` and refuses it in `set_settings`, so an agent can neither read
//! nor rewrite the policies that restrict it.
//!
//! # A corrupt `settings.json` (R4-16)
//!
//! An unparsable file is moved to `settings.broken-<unix seconds>-<n>.json` (the newest
//! [`KEEP_BROKEN`] are kept), defaults are restored, and the app tells the user once
//! ([`LoadReport`]). The restrictive client policies must not be lost with it: they are also kept
//! in `client_policies.json` beside it and restored from there. When the broken file held policies
//! and that copy is gone, [`LoadReport::policies_unreadable`] is set and Settings -> Agents shows
//! a banner asking the user to review every client.

use cm_core::CmError;
use cm_daemon::{ClientPolicy, SettingsStore};
use cm_i18n::tr;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

pub const KEY_LIBRARY_PATH: &str = "library.path";
pub const KEY_THEME: &str = "appearance.theme";
/// The UI language: a code from [`cm_i18n::Lang::ALL`] (`en`, `vi`). Default `en`: the app stays
/// English until the user picks another language (no automatic switch to the OS language). Like
/// the theme it is not in the daemon's app-only list; an agent that writes it takes effect at the
/// next start, the live switch is the Settings row's. A stored value that is not a shipped
/// language reads as `en` ([`language_from_settings`]).
pub const KEY_LANGUAGE: &str = "appearance.language";

/// The language a settings map selects; missing or unknown values are English.
pub fn language_from_settings(settings: &Map<String, Value>) -> cm_i18n::Lang {
    cm_i18n::Lang::from_setting(settings.get(KEY_LANGUAGE).and_then(Value::as_str))
}
pub const KEY_COPY_AFTER_CAPTURE: &str = "capture.copy_to_clipboard";
/// STO-4: copy the share link to the clipboard when an upload finishes (default on). App-only like
/// the rest of `storage.*`: a setting that decides what lands on the clipboard stays the user's.
pub const KEY_COPY_LINK_WHEN_DONE: &str = "storage.copy_link_when_done";
/// CAP-5: the remembered capture delay in seconds (0, 3, 5 or 10).
pub const KEY_CAPTURE_DELAY: &str = crate::delay::KEY;
/// CAP-4: also save a PNG of each capture to a folder (app-only: it writes outside the library).
pub const KEY_SAVE_FOLDER_ENABLED: &str = "capture.save_folder_enabled";
/// CAP-4: the folder for that PNG (app-only).
pub const KEY_SAVE_FOLDER: &str = "capture.save_folder";
/// CAP-4: the file name pattern (app-only, no UI: edit `settings.json`).
pub const KEY_SAVE_PATTERN: &str = "capture.save_pattern";
pub const KEY_CLIENT_POLICIES: &str = "agents.client_policies";
/// AGT-2.AC9 to AC14: `{ "<tool>": "allow" | "ask" }`, only the tools that differ from their
/// default (app-only for agents; the daemon reads it on every agent call).
pub const KEY_TOOL_POLICY: &str = cm_daemon::tool_policy::KEY;
/// Unknown agent clients are blocked until the user reviews the list (set when the policy store
/// was unreadable and could not be restored). App-internal, hidden from agents.
pub const KEY_DENY_UNKNOWN: &str = "agents.deny_unknown_clients";
/// The first-run permissions window was completed once (PRM-1.AC5). App-internal.
pub const KEY_PERMISSIONS_ONBOARDED: &str = "permissions.onboarded";

/// The daemon's limit on a client name in `session.hello` (cm-daemon `rpc.rs`).
pub const MAX_CLIENT_NAME: usize = 64;

/// Validates a client name by the rule `session.hello` enforces: after trimming it must be
/// non-empty, at most [`MAX_CLIENT_NAME`] bytes and free of control characters. Returns the
/// trimmed name the daemon would store. (`cm-mcp` truncates and sanitizes what a client
/// *sends*; a name typed here must already be exactly what `session.hello` accepts, or it
/// would never match.)
pub fn validate_client_name(name: &str) -> Result<String, ClientNameError> {
    let name = name.trim();
    if name.is_empty() {
        Err(ClientNameError::Empty)
    } else if name.len() > MAX_CLIENT_NAME {
        Err(ClientNameError::TooLong)
    } else if name.chars().any(char::is_control) {
        Err(ClientNameError::Control)
    } else {
        Ok(name.to_owned())
    }
}

/// Why a typed client name was refused. Kept as a reason and worded when drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientNameError {
    Empty,
    TooLong,
    Control,
}

impl ClientNameError {
    pub fn text(self) -> String {
        match self {
            ClientNameError::Empty => tr!("Enter the client\u{2019}s name."),
            ClientNameError::TooLong => tr!("A client name is at most 64 characters."),
            ClientNameError::Control => {
                tr!("A client name can\u{2019}t contain control characters.")
            }
        }
    }
}

fn policy_key(p: ClientPolicy) -> &'static str {
    match p {
        ClientPolicy::Allowed => "allowed",
        ClientPolicy::ReadOnly => "read_only",
        ClientPolicy::Blocked => "blocked",
    }
}

fn parse_policy(s: &str) -> Option<ClientPolicy> {
    match s {
        "allowed" => Some(ClientPolicy::Allowed),
        "read_only" => Some(ClientPolicy::ReadOnly),
        "blocked" => Some(ClientPolicy::Blocked),
        _ => None,
    }
}

/// The stored policies and how many entries were unusable.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LoadedPolicies {
    /// Sorted by name.
    pub policies: Vec<(String, ClientPolicy)>,
    /// Entries dropped (bad name or unknown policy), plus 1 if the whole value was not an object.
    pub dropped: usize,
}

/// Reads `agents.client_policies` leniently (see the module docs). `None` is "not set".
pub fn parse_client_policies(value: Option<&Value>) -> LoadedPolicies {
    let mut out = LoadedPolicies::default();
    let Some(value) = value else {
        return out;
    };
    let Value::Object(map) = value else {
        out.dropped = 1;
        return out;
    };
    for (name, v) in map {
        let parsed = validate_client_name(name)
            .ok()
            .filter(|n| n == name)
            .zip(v.as_str().and_then(parse_policy));
        match parsed {
            Some(entry) => out.policies.push(entry),
            None => out.dropped += 1,
        }
    }
    out.policies.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The JSON object stored under `agents.client_policies`.
pub fn client_policies_value(policies: &[(String, ClientPolicy)]) -> Value {
    Value::Object(
        policies
            .iter()
            .map(|(n, p)| (n.clone(), Value::String(policy_key(*p).to_owned())))
            .collect(),
    )
}

/// App folder name inside the platform data and config directories.
pub const APP_DIR: &str = "CaptureMann";

/// The config directory: `CAPTUREMANN_CONFIG_DIR`, else the platform one.
pub fn config_dir() -> PathBuf {
    std::env::var_os("CAPTUREMANN_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::config_dir().map(|d| d.join(APP_DIR)))
        .unwrap_or_else(|| std::env::temp_dir().join(APP_DIR))
}

/// LIB-2 default: `~/Movies/CaptureMann` (macOS) or `%USERPROFILE%\Videos\CaptureMann`
/// (Windows), i.e. the platform video folder; the home folder when that is unknown.
pub fn default_library_dir(video_dir: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    video_dir
        .or_else(|| home.map(|h| h.join("Videos")))
        .map(|d| d.join(APP_DIR))
}

/// Where the library lives: `CAPTUREMANN_LIBRARY` (as the headless daemon), then
/// `library.path`, then the LIB-2 default.
pub fn resolve_library_path(
    env_override: Option<PathBuf>,
    settings: &Map<String, Value>,
    default: Option<PathBuf>,
) -> Option<PathBuf> {
    env_override
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| {
            settings
                .get(KEY_LIBRARY_PATH)
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
        })
        .or(default)
}

/// LIB-2.AC2: the internal-disk folder used while the configured library is unavailable.
pub fn fallback_library_dir(data_dir: Option<PathBuf>) -> PathBuf {
    data_dir
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIR)
        .join("Fallback Library")
}

/// A human reason for "Library unavailable" (LIB-2): names the volume or folder to connect.
pub fn unavailable_hint(path: &Path) -> String {
    // `/Volumes/<name>/...` on macOS, `D:\...` on Windows: the first component after the
    // mount root is what the user has to reconnect.
    let comps: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if comps.len() >= 3 && comps[1] == "Volumes" {
        return comps[2].clone();
    }
    if let Some(first) = comps.first() {
        if first.len() >= 2 && first.as_bytes()[1] == b':' {
            return first.clone();
        }
    }
    path.display().to_string()
}

/// How many `settings.broken-*.json` backups are kept.
pub const KEEP_BROKEN: usize = 5;
/// The second copy of the client policies, beside `settings.json`.
pub const POLICIES_FILE: &str = "client_policies.json";

/// What loading the settings file found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadReport {
    /// The file was unreadable and was moved here (`None`: it loaded, or did not exist).
    pub backup: Option<PathBuf>,
    /// The client policies were in the unreadable file and could not be restored. Until the user
    /// reviews the clients in Settings -> Agents, the policy table is empty.
    pub policies_unreadable: bool,
    /// The policies were restored from their second copy.
    pub policies_restored: bool,
}

/// `settings.broken-<secs>.json` in `dir`, never overwriting an earlier backup.
fn backup_path(dir: &Path, secs: u64) -> PathBuf {
    // The counter is part of every name, so names sort by age and a same-second backup never
    // overwrites (or sorts before) another.
    let prefix = format!("settings.broken-{secs:010}-");
    let next = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_prefix(&prefix)?
                .strip_suffix(".json")?
                .parse::<u32>()
                .ok()
        })
        .max()
        .map_or(0, |n| n + 1);
    dir.join(format!("{prefix}{next:02}.json"))
}

/// Deletes all but the newest `keep` backups (names sort by their zero-padded time).
fn prune_backups(dir: &Path, keep: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("settings.broken-") && n.ends_with(".json"))
        })
        .collect();
    names.sort();
    let excess = names.len().saturating_sub(keep);
    for old in names.into_iter().take(excess) {
        let _ = std::fs::remove_file(old);
    }
}

/// The one-time notice after an unreadable settings file, or `None` when nothing happened.
pub fn load_report_toast(r: &LoadReport) -> Option<crate::toast::ToastSpec> {
    use crate::toast::{ToastKind, ToastSpec};
    let backup = r.backup.as_ref()?;
    let name = backup
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut body = tr!(
        "Defaults were restored. The old file is saved as {name}.",
        name = name
    );
    if r.policies_restored {
        body.push(' ');
        body.push_str(&tr!("Your agent permissions were kept."));
    }
    if r.policies_unreadable {
        body.push(' ');
        body.push_str(&tr!(
            "Agent permissions could not be restored: review them in Settings \u{2192} Agents."
        ));
    }
    Some(ToastSpec::new(ToastKind::Warning, tr!("Your settings file was unreadable")).body(body))
}

/// JSON file store. Reads are served from memory; every `set` writes atomically
/// (temp file, then rename) so a crash never leaves half a file.
#[derive(Debug)]
pub struct FileSettings {
    path: PathBuf,
    map: Mutex<Map<String, Value>>,
    report: LoadReport,
}

impl FileSettings {
    /// Loads `path`; a missing file yields empty settings. An unparsable one is kept aside as
    /// `settings.broken-<time>.json` (see the module docs) and defaults are used; the client
    /// policies come back from their own copy when it is there.
    pub fn load(path: PathBuf) -> FileSettings {
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let mut report = LoadReport::default();
        let mut map = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(Value::Object(m)) => m,
                _ => {
                    let backup = backup_path(&dir, secs);
                    if std::fs::rename(&path, &backup).is_ok() {
                        report.backup = Some(backup);
                        prune_backups(&dir, KEEP_BROKEN);
                    }
                    report.policies_unreadable = text.contains(KEY_CLIENT_POLICIES);
                    Map::new()
                }
            },
            Err(_) => Map::new(),
        };
        let copy = dir.join(POLICIES_FILE);
        if report.backup.is_some() {
            let restored = std::fs::read_to_string(&copy)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .filter(Value::is_object);
            if let Some(v) = restored {
                map.insert(KEY_CLIENT_POLICIES.to_owned(), v);
                report.policies_unreadable = false;
                report.policies_restored = true;
            }
        } else if let Some(v) = map.get(KEY_CLIENT_POLICIES).filter(|_| !copy.exists()) {
            // An older install: make the second copy now.
            let _ = std::fs::write(&copy, v.to_string());
        }
        FileSettings {
            path,
            map: Mutex::new(map),
            report,
        }
    }

    /// What loading found (a backup of an unreadable file, lost policies).
    pub fn load_report(&self) -> &LoadReport {
        &self.report
    }

    pub fn snapshot(&self) -> Map<String, Value> {
        self.map
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Whether clients without a stored policy start out blocked: the policy store was unreadable
    /// and could not be restored (this run), or that state was not reviewed yet (an earlier run).
    pub fn deny_unknown(&self) -> bool {
        self.report.policies_unreadable
            || self
                .map
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(KEY_DENY_UNKNOWN)
                .and_then(Value::as_bool)
                .unwrap_or(false)
    }

    /// Persists the deny-unknown-clients choice.
    pub fn set_deny_unknown(&self, deny: bool) -> Result<(), CmError> {
        self.set_internal(KEY_DENY_UNKNOWN, Value::Bool(deny))
    }

    /// Persists one app-internal value (not a user-facing setting).
    pub fn set_internal(&self, key: &str, value: Value) -> Result<(), CmError> {
        let mut g = self.map.lock().unwrap_or_else(PoisonError::into_inner);
        let mut next = g.clone();
        next.insert(key.to_owned(), value);
        self.write(&next)?;
        *g = next;
        Ok(())
    }

    /// The per-tool permissions the rules allow (an unreadable or hand-edited value counts as the
    /// defaults, the same reading the daemon does at call time).
    pub fn tool_policy(
        &self,
    ) -> std::collections::BTreeMap<String, cm_daemon::tool_policy::ToolSetting> {
        cm_daemon::tool_policy::parse_lenient(
            self.map
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(KEY_TOOL_POLICY),
        )
    }

    /// Persists the per-tool permissions (validated like any write of that key). An empty map
    /// stores `{}`, which is the defaults.
    pub fn set_tool_policy(&self, value: Value) -> Result<(), CmError> {
        let values: Map<String, Value> =
            [(KEY_TOOL_POLICY.to_owned(), value)].into_iter().collect();
        SettingsStore::set(self, &values).map(|_| ())
    }

    /// Persists the client policies (app-internal; bypasses the daemon-facing key guard).
    pub fn set_client_policies(&self, policies: &[(String, ClientPolicy)]) -> Result<(), CmError> {
        let mut g = self.map.lock().unwrap_or_else(PoisonError::into_inner);
        let mut next = g.clone();
        let value = client_policies_value(policies);
        next.insert(KEY_CLIENT_POLICIES.to_owned(), value.clone());
        self.write(&next)?;
        // The second copy: a corrupt settings file must not turn every client back to allowed.
        if let Some(dir) = self.path.parent() {
            let tmp = dir.join(format!("{POLICIES_FILE}.tmp"));
            if std::fs::write(&tmp, value.to_string()).is_ok() {
                let _ = std::fs::rename(&tmp, dir.join(POLICIES_FILE));
            }
        }
        *g = next;
        Ok(())
    }

    fn write(&self, map: &Map<String, Value>) -> Result<(), CmError> {
        let io = |e: std::io::Error| CmError::internal(format!("cannot save settings: {e}"));
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(&Value::Object(map.clone()))
            .map_err(|e| CmError::internal(e.to_string()))?;
        std::fs::write(&tmp, text).map_err(io)?;
        std::fs::rename(&tmp, &self.path).map_err(io)
    }
}

/// Validates the values of the keys this app understands (the daemon validated key names).
fn validate(values: &Map<String, Value>) -> Result<(), CmError> {
    for (k, v) in values {
        match k.as_str() {
            KEY_THEME => {
                let ok = v
                    .as_str()
                    .is_some_and(|s| crate::theme::ThemePreference::parse(s).is_some());
                if !ok {
                    return Err(CmError::invalid_field(
                        k,
                        "must be `system`, `dark` or `light`",
                    ));
                }
            }
            KEY_LANGUAGE
                if !v
                    .as_str()
                    .is_some_and(|s| cm_i18n::Lang::parse(s).is_some()) =>
            {
                return Err(CmError::invalid_field(k, "must be `en` or `vi`"));
            }
            KEY_COPY_AFTER_CAPTURE if !v.is_boolean() => {
                return Err(CmError::invalid_field(k, "must be a boolean"));
            }
            KEY_COPY_LINK_WHEN_DONE if !v.is_boolean() => {
                return Err(CmError::invalid_field(k, "must be a boolean"));
            }
            KEY_SAVE_FOLDER_ENABLED if !v.is_boolean() => {
                return Err(CmError::invalid_field(k, "must be a boolean"));
            }
            KEY_SAVE_FOLDER if !v.is_string() => {
                return Err(CmError::invalid_field(k, "must be a folder path"));
            }
            KEY_SAVE_PATTERN if !v.is_string() => {
                return Err(CmError::invalid_field(k, "must be a file name pattern"));
            }
            KEY_CAPTURE_DELAY
                if crate::delay::from_setting(Some(v)) == 0 && v.as_u64() != Some(0) =>
            {
                return Err(CmError::invalid_field(k, "must be 0, 3, 5 or 10"));
            }
            KEY_TOOL_POLICY => {
                cm_daemon::tool_policy::parse_strict(v)?;
            }
            KEY_CLIENT_POLICIES if !v.is_object() => {
                return Err(CmError::invalid_field(k, "must be an object"));
            }
            KEY_LIBRARY_PATH if !v.is_string() => {
                return Err(CmError::invalid_field(k, "must be a folder path"));
            }
            key if key.starts_with("shortcuts.") => {
                let ok = v
                    .as_str()
                    .is_some_and(|s| crate::shortcuts::Chord::parse(s).is_ok());
                if !ok {
                    return Err(CmError::invalid_field(
                        k,
                        "must look like `alt+shift+1` (at least one modifier and one key)",
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

impl SettingsStore for FileSettings {
    fn all(&self) -> Result<Map<String, Value>, CmError> {
        let mut out = match json!({
            KEY_THEME: "system",
            KEY_LANGUAGE: "en",
            KEY_COPY_AFTER_CAPTURE: true,
            KEY_COPY_LINK_WHEN_DONE: true,
            KEY_CAPTURE_DELAY: 0,
            KEY_SAVE_FOLDER_ENABLED: false,
        }) {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        for (k, v) in self.snapshot() {
            out.insert(k, v);
        }
        Ok(out)
    }

    fn set(&self, values: &Map<String, Value>) -> Result<Map<String, Value>, CmError> {
        validate(values)?;
        {
            let mut g = self.map.lock().unwrap_or_else(PoisonError::into_inner);
            let mut next = g.clone();
            for (k, v) in values {
                next.insert(k.clone(), v.clone());
            }
            self.write(&next)?;
            *g = next;
        }
        self.all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    /// LIB-2: default library folder is `<Movies|Videos>/CaptureMann`.
    #[test]
    fn default_library_location() {
        assert_eq!(
            default_library_dir(Some(p("/Users/a/Movies")), Some(p("/Users/a"))),
            Some(p("/Users/a/Movies/CaptureMann"))
        );
        assert_eq!(
            default_library_dir(None, Some(p("/home/a"))),
            Some(p("/home/a/Videos/CaptureMann"))
        );
        assert_eq!(default_library_dir(None, None), None);
    }

    #[test]
    fn library_path_precedence() {
        let Value::Object(m) = json!({"library.path": "/Volumes/Big/Caps"}) else {
            unreachable!()
        };
        let d = Some(p("/def"));
        assert_eq!(
            resolve_library_path(Some(p("/env")), &m, d.clone()),
            Some(p("/env"))
        );
        assert_eq!(
            resolve_library_path(None, &m, d.clone()),
            Some(p("/Volumes/Big/Caps"))
        );
        assert_eq!(resolve_library_path(None, &Map::new(), d), Some(p("/def")));
        assert_eq!(resolve_library_path(Some(p("")), &Map::new(), None), None);
    }

    /// LIB-2: "Library unavailable - connect `<name>`" names the drive.
    #[test]
    fn unavailable_hint_names_the_volume() {
        assert_eq!(unavailable_hint(&p("/Volumes/Big Drive/Caps")), "Big Drive");
        assert_eq!(
            unavailable_hint(&p("/home/a/Videos/CaptureMann")),
            "/home/a/Videos/CaptureMann"
        );
    }

    #[test]
    fn cap4_cap5_keys_are_validated_and_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let s = FileSettings::load(file.clone());
        let obj = |v: Value| match v {
            Value::Object(m) => m,
            _ => unreachable!(),
        };
        assert_eq!(s.all().unwrap()[KEY_CAPTURE_DELAY], 0);
        assert_eq!(s.all().unwrap()[KEY_SAVE_FOLDER_ENABLED], false);
        for bad in [json!(7), json!("5"), json!(-1), json!(5.5), json!(31)] {
            assert!(s.set(&obj(json!({ KEY_CAPTURE_DELAY: bad }))).is_err());
        }
        for good in [0, 3, 5, 10] {
            s.set(&obj(json!({ KEY_CAPTURE_DELAY: good }))).unwrap();
        }
        // CAP-5: the choice survives a restart.
        assert_eq!(
            crate::delay::from_setting(FileSettings::load(file).snapshot().get(KEY_CAPTURE_DELAY)),
            10
        );
        assert!(s
            .set(&obj(json!({ KEY_SAVE_FOLDER_ENABLED: "yes" })))
            .is_err());
        assert!(s.set(&obj(json!({ KEY_SAVE_FOLDER: 4 }))).is_err());
        s.set(&obj(
            json!({ KEY_SAVE_FOLDER: "/tmp/x", KEY_SAVE_FOLDER_ENABLED: true }),
        ))
        .unwrap();
    }

    #[test]
    fn persists_atomically_and_validates() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sub/settings.json");
        let s = FileSettings::load(file.clone());
        let all = s.all().unwrap();
        assert_eq!(all[KEY_THEME], "system");
        assert_eq!(all[KEY_COPY_AFTER_CAPTURE], true);

        let Value::Object(set) =
            json!({"appearance.theme": "light", "shortcuts.capture_region": "ctrl+alt+r"})
        else {
            unreachable!()
        };
        let eff = s.set(&set).unwrap();
        assert_eq!(eff[KEY_THEME], "light");
        let reloaded = FileSettings::load(file);
        assert_eq!(
            reloaded.all().unwrap()["shortcuts.capture_region"],
            "ctrl+alt+r"
        );

        let Value::Object(bad) = json!({"appearance.theme": "neon"}) else {
            unreachable!()
        };
        assert!(s.set(&bad).is_err());
        let Value::Object(bad) = json!({"shortcuts.capture_region": "r"}) else {
            unreachable!()
        };
        assert!(s.set(&bad).is_err());
        // A rejected set changes nothing.
        assert_eq!(s.all().unwrap()[KEY_THEME], "light");
    }

    /// AGT-2.AC10: the app's own write path refuses the locked rules, keeps the last good value,
    /// survives a restart, and reads a hand-edited file leniently (publish and delete stay
    /// Ask first).
    #[test]
    fn agt_2_ac10_tool_policy_is_validated_persisted_and_read_leniently() {
        use cm_daemon::tool_policy::ToolSetting;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let s = FileSettings::load(file.clone());
        assert!(s.tool_policy().is_empty());
        s.set_tool_policy(json!({"capture_screenshot": "ask", "start_recording": "allow"}))
            .unwrap();
        assert_eq!(
            s.tool_policy().get("start_recording"),
            Some(&ToolSetting::Allow)
        );
        for bad in [
            json!("ask"),
            json!({"publish_video": "allow"}),
            json!({"delete_capture": "allow"}),
            json!({"list_library": "ask"}),
            json!({"nope": "ask"}),
            json!({"annotate": "sometimes"}),
        ] {
            assert!(s.set_tool_policy(bad.clone()).is_err(), "{bad}");
        }
        // The rejected writes changed nothing; the value survives a restart.
        assert_eq!(s.tool_policy().len(), 2);
        assert_eq!(FileSettings::load(file.clone()).tool_policy().len(), 2);
        // Reset is one write of `{}`.
        s.set_tool_policy(json!({})).unwrap();
        assert!(FileSettings::load(file.clone()).tool_policy().is_empty());
        // A hand-edited file: the locked entries are ignored, the allowed ones kept.
        std::fs::write(
            &file,
            r#"{"agents.tool_policy": {"publish_video": "allow", "delete_capture": "allow",
                "run_fps_selftest": "allow", "list_jobs": "ask", "annotate": "ask"}}"#,
        )
        .unwrap();
        let edited = FileSettings::load(file.clone());
        let got = edited.tool_policy();
        assert_eq!(
            got.keys().map(String::as_str).collect::<Vec<_>>(),
            ["annotate"]
        );
        // The daemon-side store (the same map) is not blocked from reading it either.
        assert!(SettingsStore::all(&edited)
            .unwrap()
            .contains_key(KEY_TOOL_POLICY));
    }

    /// STO-4: the link is copied by default; only a boolean can change it.
    #[test]
    fn copy_link_when_done_defaults_on_and_takes_a_boolean() {
        let dir = tempfile::tempdir().unwrap();
        let s = FileSettings::load(dir.path().join("settings.json"));
        assert_eq!(s.all().unwrap()[KEY_COPY_LINK_WHEN_DONE], true);
        let Value::Object(off) = json!({ "storage.copy_link_when_done": false }) else {
            unreachable!()
        };
        assert_eq!(s.set(&off).unwrap()[KEY_COPY_LINK_WHEN_DONE], false);
        let Value::Object(bad) = json!({ "storage.copy_link_when_done": "yes" }) else {
            unreachable!()
        };
        assert!(s.set(&bad).is_err());
    }

    /// I18N P1: the app is English until the user picks a language; only shipped languages can be
    /// stored, and a hand-edited unknown value reads as English.
    #[test]
    fn language_defaults_to_english_validates_and_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let s = FileSettings::load(file.clone());
        assert_eq!(s.all().unwrap()[KEY_LANGUAGE], "en");
        assert_eq!(language_from_settings(&s.all().unwrap()), cm_i18n::Lang::En);
        let Value::Object(vi) = json!({ "appearance.language": "vi" }) else {
            unreachable!()
        };
        assert_eq!(s.set(&vi).unwrap()[KEY_LANGUAGE], "vi");
        let reloaded = FileSettings::load(file.clone());
        assert_eq!(
            language_from_settings(&reloaded.all().unwrap()),
            cm_i18n::Lang::Vi
        );
        for bad in [json!("system"), json!("fr"), json!("en-XA"), json!(3)] {
            let Value::Object(m) = json!({ "appearance.language": bad }) else {
                unreachable!()
            };
            assert!(s.set(&m).is_err(), "{bad}");
        }
        // Edited by hand to something unknown: English.
        std::fs::write(&file, r#"{"appearance.language": "klingon"}"#).unwrap();
        let odd = FileSettings::load(file);
        assert_eq!(
            language_from_settings(&odd.all().unwrap()),
            cm_i18n::Lang::En
        );
    }

    /// AGT-2.AC4: the policy table round-trips through the settings file.
    #[test]
    fn agt_2_ac4_client_policies_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let s = FileSettings::load(file.clone());
        let want = vec![
            ("claude-desktop".to_owned(), ClientPolicy::Allowed),
            ("cursor".to_owned(), ClientPolicy::ReadOnly),
            ("zed".to_owned(), ClientPolicy::Blocked),
        ];
        s.set_client_policies(&want).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"read_only\""), "{text}");
        let reloaded = FileSettings::load(file);
        let got = parse_client_policies(reloaded.snapshot().get(KEY_CLIENT_POLICIES));
        assert_eq!(got.policies, want);
        assert_eq!(got.dropped, 0);
        // Other settings are untouched.
        assert_eq!(reloaded.all().unwrap()[KEY_THEME], "system");
    }

    /// AGT-2.AC4: malformed stored values are ignored, never fatal.
    /// Fail-closed: an unreadable policy store means unknown clients stay blocked until the
    /// user allows them, across restarts.
    #[test]
    fn deny_unknown_survives_until_the_user_allows_unknown_clients() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(
            &file,
            "{\"agents.client_policies\": {\"a\": \"blocked\"}, broken",
        )
        .unwrap();
        let s = FileSettings::load(file.clone());
        assert!(s.load_report().policies_unreadable);
        assert!(s.deny_unknown());
        s.set_deny_unknown(true).unwrap();
        let again = FileSettings::load(file.clone());
        assert!(!again.load_report().policies_unreadable);
        assert!(again.deny_unknown(), "the choice persisted");
        again.set_deny_unknown(false).unwrap();
        assert!(!FileSettings::load(file).deny_unknown());
    }

    #[test]
    fn agt_2_ac4_malformed_policies_are_dropped() {
        assert_eq!(parse_client_policies(None), LoadedPolicies::default());
        let not_object = json!(["blocked"]);
        let l = parse_client_policies(Some(&not_object));
        assert!(l.policies.is_empty());
        assert_eq!(l.dropped, 1);
        let mixed = json!({
            "ok": "blocked", "bad-policy": "maybe", "num": 3,
            "": "blocked", " padded ": "blocked", "ctl\n": "allowed",
            "long": "read_only"
        });
        let l = parse_client_policies(Some(&mixed));
        assert_eq!(
            l.policies,
            vec![
                ("long".to_owned(), ClientPolicy::ReadOnly),
                ("ok".to_owned(), ClientPolicy::Blocked)
            ]
        );
        assert_eq!(l.dropped, 5);
        let long = "x".repeat(65);
        let v = json!({ long: "blocked" });
        assert_eq!(parse_client_policies(Some(&v)).dropped, 1);
        // The store itself refuses a non-object.
        let dir = tempfile::tempdir().unwrap();
        let s = FileSettings::load(dir.path().join("s.json"));
        let Value::Object(bad) = json!({KEY_CLIENT_POLICIES: "x"}) else {
            unreachable!()
        };
        assert!(s.set(&bad).is_err());
    }

    /// The name rule is the one `session.hello` enforces.
    #[test]
    fn client_name_validation_matches_session_hello() {
        assert_eq!(
            validate_client_name("  claude-ai ").as_deref(),
            Ok("claude-ai")
        );
        assert!(validate_client_name("   ").is_err());
        assert!(validate_client_name("a\tb").is_err());
        assert!(validate_client_name(&"a".repeat(64)).is_ok());
        assert!(validate_client_name(&"a".repeat(65)).is_err());
        // The limit is in bytes, like the daemon's.
        assert!(validate_client_name(&"\u{e9}".repeat(33)).is_err());
        assert!(validate_client_name("Claude Desktop (beta)").is_ok());
    }

    #[test]
    fn r4_16_the_notice_names_the_backup() {
        assert!(load_report_toast(&LoadReport::default()).is_none());
        let t = load_report_toast(&LoadReport {
            backup: Some(PathBuf::from("/x/settings.broken-0000000001-00.json")),
            policies_unreadable: true,
            policies_restored: false,
        })
        .unwrap();
        let body = t.body.unwrap();
        assert!(body.contains("settings.broken-0000000001-00.json"));
        assert!(body.contains("Settings \u{2192} Agents"));
    }

    #[test]
    fn broken_file_is_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(&file, "{ not json").unwrap();
        let s = FileSettings::load(file);
        assert_eq!(s.all().unwrap()[KEY_THEME], "system");
        let report = s.load_report();
        let backup = report.backup.clone().expect("a backup path");
        assert!(backup.exists());
        assert!(backup
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("settings.broken-")));
        assert!(!report.policies_unreadable, "no policies were in it");
    }

    /// R4-16: backups are timestamped, never overwritten, and only the newest five stay.
    #[test]
    fn r4_16_backups_are_kept_and_pruned() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let mut seen = std::collections::HashSet::new();
        for i in 0..8 {
            std::fs::write(&file, format!("{{ broken {i}")).unwrap();
            let s = FileSettings::load(file.clone());
            let b = s.load_report().backup.clone().expect("backup");
            assert!(seen.insert(b), "an earlier backup was overwritten");
        }
        let n = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("settings.broken-")
            })
            .count();
        assert_eq!(n, KEEP_BROKEN);
    }

    /// R4-16: corruption must not reset the agent policies to "everyone allowed". They come back
    /// from their own copy; when they cannot, the report says so (the Agents page shows a banner).
    #[test]
    fn r4_16_policies_survive_a_corrupt_settings_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let s = FileSettings::load(file.clone());
        let want = vec![("evil-agent".to_owned(), ClientPolicy::Blocked)];
        s.set_client_policies(&want).unwrap();
        // The file gets corrupted.
        std::fs::write(&file, "{ \"agents.client_policies\": { \"evil-agent\": ").unwrap();
        let again = FileSettings::load(file.clone());
        let report = again.load_report();
        assert!(report.backup.is_some() && report.policies_restored);
        assert!(!report.policies_unreadable);
        let got = parse_client_policies(again.snapshot().get(KEY_CLIENT_POLICIES));
        assert_eq!(got.policies, want);
        // Both gone: the report says the policies were lost.
        std::fs::remove_file(dir.path().join(POLICIES_FILE)).unwrap();
        std::fs::write(&file, "{ \"agents.client_policies\": { \"evil").unwrap();
        let lost = FileSettings::load(file);
        assert!(lost.load_report().policies_unreadable);
        assert!(lost.snapshot().get(KEY_CLIENT_POLICIES).is_none());
    }
}
