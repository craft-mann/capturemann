//! Hooks the daemon core does not own: clipboard, settings storage and the engines behind
//! Confirm-tier actions. The desktop app (or tests) injects implementations.

use async_trait::async_trait;
use cm_core::CmError;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use crate::action::Action;
use crate::error::DaemonError;

/// Copies a PNG to the system clipboard (CAP-4; `arboard` lives in `apps/desktop`).
pub trait Clipboard: Send + Sync {
    fn copy_png(&self, png: &[u8]) -> Result<(), CmError>;
}

/// No clipboard available (headless daemon): copying reports an error and the capture still
/// succeeds with `copied_to_clipboard: false` and a warning.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoClipboard;

impl Clipboard for NoClipboard {
    fn copy_png(&self, _png: &[u8]) -> Result<(), CmError> {
        Err(CmError::invalid_input(
            "no clipboard is available in this daemon",
        ))
    }
}

/// Settings storage behind `get_settings` / `set_settings`. Keys are dotted
/// (`recording.fps`). Implementations persist; the daemon validates keys first.
pub trait SettingsStore: Send + Sync {
    /// All settings.
    fn all(&self) -> Result<Map<String, Value>, CmError>;
    /// Applies every pair or none (atomic), returning the full effective settings.
    fn set(&self, values: &Map<String, Value>) -> Result<Map<String, Value>, CmError>;
}

/// In-memory settings (headless default and tests).
#[derive(Debug, Default)]
pub struct MemorySettings {
    map: Mutex<BTreeMap<String, Value>>,
}

impl MemorySettings {
    pub fn with_defaults(defaults: Map<String, Value>) -> Self {
        MemorySettings {
            map: Mutex::new(defaults.into_iter().collect()),
        }
    }
}

impl SettingsStore for MemorySettings {
    fn all(&self) -> Result<Map<String, Value>, CmError> {
        let g = self.map.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(g.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }

    fn set(&self, values: &Map<String, Value>) -> Result<Map<String, Value>, CmError> {
        let mut g = self.map.lock().unwrap_or_else(PoisonError::into_inner);
        for (k, v) in values {
            g.insert(k.clone(), v.clone());
        }
        Ok(g.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }
}

/// Substrings that mark a setting key as holding a secret (Never tier: tokens and keys are
/// never exposed, R2-4). Matched case-insensitively against the **whole** key, not per dotted
/// segment, so `auth.access_token`, `youtube.refresh_token`, `db.password_hash` and
/// `youtube.api_key_id` are all refused.
pub(crate) const SECRET_SUBSTRINGS: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "api_key",
    "apikey",
    "credential",
    "private_key",
    "bearer",
];

/// Keys that contain a [`SECRET_SUBSTRINGS`] word but hold no secret: `ai.text.context_tokens` is a
/// count of model tokens (AI-7).
pub(crate) const NOT_SECRET_KEYS: &[&str] = &["ai.text.context_tokens"];

/// Setting keys (or `prefix.*` families) that only the app itself may write (R2-3). A socket
/// client (always an agent) is refused with `user_declined` / `data.reason =
/// "app_only_setting"`; the in-process GUI (`Actor::User`) can write them.
///
/// Why each one: `library.path` redirects every later capture (AGT-2 makes moving the library a
/// Confirm-tier operation); `privacy.*` holds the excluded-apps list; `shortcuts.*` rebinds
/// global hotkeys; `permissions.*` and `agents.*` are the setup state and the per-client
/// policies; `storage.*` and `export.*` change what leaves the machine or runs unattended
/// (`storage.pause_uploads_while_recording`, `export.auto_on_stop`); `recording.fps60_selftest`
/// is the self-test verdict, `recording.hud_positions` is window geometry; `mcp.*` and `auth.*`
/// are server and account configuration; `app.*` (and a bare `launch_at_login`) is the shell's
/// own state, such as login-item and window preferences; `capture.save_folder_enabled`,
/// `capture.save_folder` and `capture.save_pattern` make the app write PNG files outside the
/// library (CAP-4), so an agent cannot point them anywhere. `camera.position` and the rest of `recording.*`,
/// `camera.*`, `audio.*`, `editor.*`, `appearance.*` stay writable: an agent configuring a
/// recording is the point of `set_settings`.
pub const APP_ONLY_SETTINGS: &[&str] = &[
    "library.*",
    "privacy.*",
    "shortcuts.*",
    "permissions.*",
    "agents.*",
    "storage.*",
    "export.*",
    "ai.*",
    "mcp.*",
    "auth.*",
    "app.*",
    "appearance.language",
    "launch_at_login",
    "recording.fps60_selftest",
    "recording.hud_positions",
    "capture.save_folder_enabled",
    "capture.save_folder",
    "capture.save_pattern",
];

/// The settings families an agent may write (everything in [`APP_ONLY_SETTINGS`] stays refused
/// inside them). Any other family is not a setting of this app: a deny list alone would let a
/// future family, or a typo, be written by an agent without anyone deciding that it may.
pub const AGENT_WRITABLE_FAMILIES: &[&str] = &[
    "recording",
    "camera",
    "audio",
    "editor",
    "cursor",
    "capture",
    "appearance",
    // `profile.display_name`: the name the user's own words get in headset mode (AI-8.AC7).
    "profile",
];

/// Longest settings key.
pub const MAX_SETTING_KEY_BYTES: usize = 128;
/// Largest serialized value of one setting (16 KiB).
pub const MAX_SETTING_VALUE_BYTES: usize = 16 * 1024;
/// Deepest nesting of a setting value.
pub const MAX_SETTING_DEPTH: usize = 8;
/// Most keys in one `set_settings` call.
pub const MAX_SETTINGS_PER_CALL: usize = 64;

/// Whether `key` is in [`APP_ONLY_SETTINGS`].
pub fn is_app_only_setting(key: &str) -> bool {
    APP_ONLY_SETTINGS
        .iter()
        .any(|p| match p.strip_suffix(".*") {
            Some(prefix) => key
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('.')),
            None => *p == key,
        })
}

/// Validates a settings key: lowercase dotted path of `[a-z0-9_]` segments, and not a secret.
pub fn validate_setting_key(key: &str) -> Result<(), CmError> {
    let ok = !key.is_empty()
        && key.len() <= MAX_SETTING_KEY_BYTES
        && key.split('.').all(|seg| {
            !seg.is_empty()
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        });
    if !ok {
        return Err(CmError::invalid_field(
            "values",
            format!("`{key}` is not a valid setting key (use dotted lowercase names)"),
        ));
    }
    let lower = key.to_ascii_lowercase();
    if !NOT_SECRET_KEYS.contains(&lower.as_str())
        && SECRET_SUBSTRINGS.iter().any(|w| lower.contains(w))
    {
        return Err(CmError::invalid_field(
            "values",
            format!("`{key}` is not available to clients"),
        ));
    }
    Ok(())
}

fn json_depth(v: &Value) -> usize {
    match v {
        Value::Array(a) => 1 + a.iter().map(json_depth).max().unwrap_or(0),
        Value::Object(o) => 1 + o.values().map(json_depth).max().unwrap_or(0),
        _ => 0,
    }
}

/// Size and depth limits of one setting value (R2-3).
pub fn validate_setting_value(key: &str, value: &Value) -> Result<(), CmError> {
    if json_depth(value) > MAX_SETTING_DEPTH {
        return Err(CmError::invalid_field(
            "values",
            format!("the value of `{key}` is nested deeper than {MAX_SETTING_DEPTH} levels"),
        ));
    }
    let size = serde_json::to_vec(value).map_or(usize::MAX, |v| v.len());
    if size > MAX_SETTING_VALUE_BYTES {
        return Err(CmError::invalid_field(
            "values",
            format!(
                "the value of `{key}` is larger than {} KiB",
                MAX_SETTING_VALUE_BYTES / 1024
            ),
        ));
    }
    Ok(())
}

/// The daemon-level policy of `set_settings`: key and value validation for everyone, and the
/// [`APP_ONLY_SETTINGS`] deny list for agents (socket clients). Returns the offending key's
/// refusal as a [`DaemonError`] so the app-only case carries `data.reason`.
pub fn check_setting_writes(agent: bool, values: &Map<String, Value>) -> Result<(), DaemonError> {
    if values.len() > MAX_SETTINGS_PER_CALL {
        return Err(CmError::invalid_field(
            "values",
            format!("at most {MAX_SETTINGS_PER_CALL} settings per call"),
        )
        .into());
    }
    for (k, v) in values {
        validate_setting_key(k)?;
        validate_setting_value(k, v)?;
        if agent && is_app_only_setting(k) {
            return Err(DaemonError::Declined {
                reason: "app_only_setting",
                message: format!("`{k}` can only be changed in the CaptureMann app (Settings)"),
            });
        }
        // AGT-2.AC10: the per-tool permissions are validated on every write. (An agent never gets
        // here: `agents.*` is app-only above.)
        if k == crate::tool_policy::KEY {
            crate::tool_policy::parse_strict(v)?;
        }
        if agent && !AGENT_WRITABLE_FAMILIES.contains(&k.split('.').next().unwrap_or_default()) {
            return Err(CmError::invalid_field(
                "values",
                format!("`{k}` is not a setting an agent can change"),
            )
            .into());
        }
    }
    Ok(())
}

/// Engines behind Confirm-tier actions (`start_recording`, `publish_video`,
/// `delete_capture`), implemented by the recording, publishing and library-management layers
/// as they land. The daemon calls `execute` only after the confirmation gate approved (or the
/// caller is the user).
#[async_trait]
pub trait ExternalActions: Send + Sync {
    /// Whether this build can run `method`. Unsupported actions are refused with
    /// `not_implemented` *before* any dialog opens.
    fn supports(&self, method: &str) -> bool;

    /// Runs the action and returns its JSON result. `actor` is the originator (revision `by`).
    async fn execute(&self, actor: &cm_core::Actor, action: &Action) -> Result<Value, DaemonError>;
}

/// Supports nothing: the state of the daemon core before recording and publishing exist.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoExternalActions;

#[async_trait]
impl ExternalActions for NoExternalActions {
    fn supports(&self, _method: &str) -> bool {
        false
    }

    async fn execute(
        &self,
        _actor: &cm_core::Actor,
        action: &Action,
    ) -> Result<Value, DaemonError> {
        Err(DaemonError::NotImplemented(format!(
            "`{}` is not implemented yet",
            action.method()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_keys() {
        for ok in ["recording.fps", "shortcuts.capture_region", "a"] {
            assert!(validate_setting_key(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "Recording.fps",
            "a..b",
            "a b",
            "auth.token",
            "youtube.api_key",
            "x.secret.y",
            // R2-4: substring, not segment equality.
            "auth.access_token",
            "youtube.refresh_token",
            "mcp.bearer_token",
            "x.client_secret",
            "db.password_hash",
            "youtube.api_key_id",
            "a.credentials_file",
            "a.private_key_path",
            "a.passwd",
        ] {
            assert!(validate_setting_key(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn r2_3_app_only_keys_are_denied_to_agents_only() {
        for k in [
            "library.path",
            "privacy.excluded_apps",
            "shortcuts.capture_region",
            "permissions.onboarded",
            "agents.client_policies",
            "app.launch_at_login",
            "launch_at_login",
            "storage.pause_uploads_while_recording",
            "export.auto_on_stop",
            "recording.fps60_selftest",
            "recording.hud_positions",
            "capture.save_folder_enabled",
            "capture.save_folder",
            "capture.save_pattern",
        ] {
            let m: Map<String, Value> = [(k.to_owned(), Value::Bool(true))].into_iter().collect();
            let e = check_setting_writes(true, &m).unwrap_err();
            assert!(
                matches!(
                    e,
                    DaemonError::Declined {
                        reason: "app_only_setting",
                        ..
                    }
                ),
                "{k}: {e:?}"
            );
            assert!(check_setting_writes(false, &m).is_ok(), "{k} for the user");
        }
        for k in [
            "recording.fps",
            "camera.position",
            "camera.remember_position",
            "audio.noise_suppression",
            "capture.delay_s",
            "capture.copy_to_clipboard",
        ] {
            let m: Map<String, Value> = [(k.to_owned(), Value::from(1))].into_iter().collect();
            assert!(check_setting_writes(true, &m).is_ok(), "{k}");
        }
        // A family nobody decided on is not writable by an agent, whatever its key looks like.
        for k in ["nonsense.key", "ui.dark"] {
            let m: Map<String, Value> = [(k.to_owned(), Value::from(1))].into_iter().collect();
            assert!(check_setting_writes(true, &m).is_err(), "{k}");
        }
        let m: Map<String, Value> = [("ui.dark".to_owned(), Value::from(1))]
            .into_iter()
            .collect();
        assert!(check_setting_writes(false, &m).is_ok(), "the user may");
        // The whole `storage.*` family is app-only, `copy_link_when_done` included (a setting that
        // decides what lands on the user's clipboard stays the user's).
        let m: Map<String, Value> =
            [("storage.copy_link_when_done".to_owned(), Value::Bool(false))]
                .into_iter()
                .collect();
        assert!(check_setting_writes(true, &m).is_err());
        assert!(is_app_only_setting("storage.copy_link_when_done"));
        assert!(is_app_only_setting("storage.destinations"));
        // A prefix family does not match a longer first segment.
        assert!(!is_app_only_setting("library_x.path"));
    }

    #[test]
    fn r2_3_value_limits() {
        let mk = |v: Value| -> Map<String, Value> { [("a.b".to_owned(), v)].into_iter().collect() };
        assert!(check_setting_writes(true, &mk(Value::from("x".repeat(20_000)))).is_err());
        let mut deep = Value::from(1);
        for _ in 0..20 {
            deep = Value::Array(vec![deep]);
        }
        assert!(check_setting_writes(true, &mk(deep)).is_err());
        let long_key = format!("a.{}", "b".repeat(200));
        let m: Map<String, Value> = [(long_key, Value::from(1))].into_iter().collect();
        assert!(check_setting_writes(true, &m).is_err());
        let many: Map<String, Value> = (0..100)
            .map(|i| (format!("a.k{i}"), Value::from(i)))
            .collect();
        assert!(check_setting_writes(true, &many).is_err());
    }
}
